use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager, State};
use tracing::{info, warn};
use url::Url;

use crate::auth::login_window::{
    clear_profile_webview_browsing_data, clear_webview_browsing_data, close_profile_windows,
    open_drops_inventory_window_for_profile, open_epic_claim_window, open_login_window_for_profile,
    twitch_bridge_fetch_campaign_details_for_profile,
};
use crate::autostart::sync_autostart;
use crate::edition::{
    current_edition, ensure_platform_enabled, ensure_profile_allowed, is_multi_account_enabled,
    EditionInfo,
};
use crate::history::{history_path, HistoryExportResult, HistoryStats, HistoryStore};
use crate::logging::{log_dir, LogEntry, LogLevel};
use crate::miner::engine::{build_platform_backend, spawn_miner_supervisor};
use crate::net::allowlist::{validate_url, AllowlistKind};
use crate::platforms::twitch::apply_campaign_details;
use crate::platforms::twitch::models::parse_campaign_details;
use crate::platforms::{AppSnapshot, Campaign, PlatformId, PlatformSnapshot};
use crate::remote_config::RemoteConfigStatus;
use crate::rewards::epic::{
    launcher_url_for_game, notify_newly_live_games, EpicFeedSnapshot,
    EPIC_FALLBACK_FREE_GAMES_URL, EPIC_LAUNCHER_FREE_GAMES_URL,
};
use crate::secrets::SecretStore;
use crate::settings::{
    appdata_dir, is_default_profile, AccountProfile, PlatformSettings, PriorityItem, Settings,
    CURRENT_DISCLAIMER_VERSION,
};
use crate::state::{AppState, MinerControl};
use crate::update_check::{check_signed_update, install_signed_update, UpdateStatus};
use crate::webhooks::{WebhookChannel, WebhookDispatcher, WebhookSecretsStatus};
use crate::windows::{apply_window_theme, profile_webview_data_dir};

static PROFILE_SEQ: AtomicU64 = AtomicU64::new(1);

async fn resolve_profile_id(
    state: &AppState,
    platform: PlatformId,
    profile_id: Option<String>,
) -> Result<String, String> {
    ensure_platform_enabled(platform)?;
    let guard = state.settings.read().await;
    let resolved = match profile_id {
        Some(pid) if !pid.trim().is_empty() => {
            let prof = guard
                .profile_by_id(&pid)
                .ok_or_else(|| format!("Profile '{pid}' not found"))?;
            if prof.platform != platform {
                return Err(format!(
                    "Profile '{pid}' belongs to {:?}, not {:?}",
                    prof.platform, platform
                ));
            }
            pid
        }
        _ => guard.active_profile_id(platform).to_string(),
    };
    ensure_profile_allowed(platform, &resolved)?;
    Ok(resolved)
}

#[tauri::command]
pub fn get_edition_info() -> EditionInfo {
    current_edition()
}

fn generate_profile_id(platform: PlatformId, existing: &[AccountProfile]) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    loop {
        let seq = PROFILE_SEQ.fetch_add(1, Ordering::Relaxed);
        let candidate = format!("{}-{ts:x}-{seq}", platform.as_str());
        if !existing.iter().any(|p| p.id == candidate) {
            return candidate;
        }
    }
}

#[tauri::command]
pub async fn get_snapshot(state: State<'_, Arc<AppState>>) -> Result<AppSnapshot, String> {
    Ok(state.full_snapshot().await)
}

#[tauri::command]
pub async fn add_account_profile(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
    label: String,
) -> Result<Settings, String> {
    ensure_platform_enabled(platform)?;
    if !is_multi_account_enabled() {
        return Err("Multiple accounts are not included in Dropilot Free Edition.".to_string());
    }
    let (profile, updated) = {
        let mut guard = state.settings.write().await;
        let existing_count = guard.profiles_for(platform).len();
        let trimmed = label.trim().to_string();
        let clean_label = if trimmed.is_empty() {
            format!("Account {}", existing_count + 1)
        } else {
            trimmed
        };
        let id = generate_profile_id(platform, &guard.profiles);
        let profile = AccountProfile {
            id: id.clone(),
            label: clean_label,
            platform,
            settings: PlatformSettings::default(),
        };
        guard.profiles.push(profile.clone());
        guard.sanitize();
        (profile, guard.clone())
    };

    state.persist_settings().await?;
    state
        .update_profile_snapshot(&profile.id, platform, PlatformSnapshot::logged_out(platform))
        .await;

    let backend = build_platform_backend(&profile.id, platform, &app, state.inner());
    let tx = spawn_miner_supervisor(
        profile.id.clone(),
        platform,
        0,
        app.clone(),
        state.inner().clone(),
        backend,
    );
    state.register_miner_for_profile(&profile.id, tx).await;
    state.publish_snapshot(&app, true).await;
    info!(
        platform = platform.as_str(),
        profile_id = %profile.id,
        label = %profile.label,
        "Added account profile"
    );
    Ok(updated)
}

#[tauri::command]
pub async fn remove_account_profile(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    profile_id: String,
) -> Result<Settings, String> {
    let platform = {
        let guard = state.settings.read().await;
        let prof = guard
            .profile_by_id(&profile_id)
            .ok_or_else(|| format!("Profile '{profile_id}' not found"))?;
        if guard.profiles_for(prof.platform).len() <= 1 {
            return Err("Cannot remove the last remaining account for this platform".to_string());
        }
        prof.platform
    };

    state
        .send_profile_miner(&profile_id, MinerControl::Shutdown)
        .await;
    state.unregister_miner_for_profile(&profile_id).await;
    close_profile_windows(&app, platform, &profile_id);

    if !state.mock_active.load(Ordering::Relaxed) {
        let _ = SecretStore::delete_profile_token(platform, &profile_id);
    }

    if !is_default_profile(&profile_id) {
        let dir = profile_webview_data_dir(&profile_id);
        if dir.exists() {
            let _ = fs::remove_dir_all(&dir);
        }
    }

    let updated = {
        let mut guard = state.settings.write().await;
        guard.profiles.retain(|p| p.id != profile_id);
        if guard.active_profile_id(platform) == profile_id {
            if let Some(first) = guard.profiles.iter().find(|p| p.platform == platform) {
                let next_id = first.id.clone();
                guard.set_active_profile_id(platform, next_id);
            }
        }
        guard.sanitize();
        guard.clone()
    };

    state.persist_settings().await?;
    state.sync_active_platform_snapshot(platform).await;
    state.publish_snapshot(&app, true).await;
    info!(
        platform = platform.as_str(),
        profile_id = %profile_id,
        "Removed account profile"
    );
    Ok(updated)
}

#[tauri::command]
pub async fn set_active_profile(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
    profile_id: String,
) -> Result<Settings, String> {
    ensure_profile_allowed(platform, &profile_id)?;
    let updated = {
        let mut guard = state.settings.write().await;
        let prof = guard
            .profile_by_id(&profile_id)
            .ok_or_else(|| format!("Profile '{profile_id}' not found"))?;
        if prof.platform != platform {
            return Err(format!(
                "Profile '{profile_id}' does not belong to {}",
                platform.display_name()
            ));
        }
        guard.set_active_profile_id(platform, profile_id.clone());
        guard.sanitize();
        guard.clone()
    };

    state.persist_settings().await?;
    state.sync_active_platform_snapshot(platform).await;
    state.publish_snapshot(&app, true).await;
    Ok(updated)
}

#[tauri::command]
pub async fn rename_account_profile(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    profile_id: String,
    label: String,
) -> Result<Settings, String> {
    let trimmed = label.trim().to_string();
    if trimmed.is_empty() {
        return Err("Profile label cannot be empty".to_string());
    }

    let updated = {
        let mut guard = state.settings.write().await;
        let prof = guard
            .profile_by_id_mut(&profile_id)
            .ok_or_else(|| format!("Profile '{profile_id}' not found"))?;
        prof.label = trimmed;
        guard.sanitize();
        guard.clone()
    };

    state.persist_settings().await?;
    state.publish_snapshot(&app, true).await;
    Ok(updated)
}

#[tauri::command]
pub async fn login(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
    profile_id: Option<String>,
) -> Result<(), String> {
    let pid = resolve_profile_id(state.inner(), platform, profile_id).await?;
    if state.mock_active.load(Ordering::Relaxed) {
        state
            .send_profile_miner(&pid, MinerControl::LoginComplete)
            .await;
        return Ok(());
    }
    open_login_window_for_profile(&app, state.inner().clone(), platform, &pid)
        .await
        .map_err(|e| e.user_message())
}

#[tauri::command]
pub async fn logout(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
    profile_id: Option<String>,
) -> Result<(), String> {
    let pid = resolve_profile_id(state.inner(), platform, profile_id).await?;
    {
        let mut guard = state.settings.write().await;
        if let Some(plat) = guard.profile_settings_mut(&pid) {
            plat.pinned_channel = None;
        }
        guard.sync_legacy_mirrors();
    }
    let _ = state.persist_settings().await;
    state
        .send_profile_miner(&pid, MinerControl::Logout)
        .await;
    if !state.mock_active.load(Ordering::Relaxed) {
        let _ = SecretStore::delete_profile_token(platform, &pid);
        if let Err(err) = clear_profile_webview_browsing_data(&app, platform, &pid).await {
            warn!(profile_id = %pid, error = %err, "Could not clear WebView2 browsing data on logout");
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn set_campaign_farm(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
    campaign_id: String,
    farm: bool,
    profile_id: Option<String>,
) -> Result<Settings, String> {
    let pid = resolve_profile_id(state.inner(), platform, profile_id).await?;
    let updated = {
        let mut guard = state.settings.write().await;
        let plat = guard
            .profile_settings_mut(&pid)
            .ok_or_else(|| format!("Profile '{pid}' not found"))?;
        if farm {
            plat.excluded_campaigns.retain(|id| id != &campaign_id);
            if !plat.farm_campaigns.contains(&campaign_id) {
                plat.farm_campaigns.push(campaign_id.clone());
            }
        } else {
            plat.farm_campaigns.retain(|id| id != &campaign_id);
            if !plat.excluded_campaigns.contains(&campaign_id) {
                plat.excluded_campaigns.push(campaign_id.clone());
            }
            plat.priority.retain(|item| match item {
                PriorityItem::Campaign { campaign_id: id, .. } => id != &campaign_id,
                _ => true,
            });
        }
        guard.sanitize();
        guard.clone()
    };

    state.persist_settings().await?;
    state
        .send_profile_miner(&pid, MinerControl::SettingsUpdated)
        .await;
    state.publish_snapshot(&app, true).await;
    Ok(updated)
}

fn priority_game_matches(item: &PriorityItem, game_id: &str, game_name: &str) -> bool {
    match item {
        PriorityItem::Game {
            game_id: gid,
            game_name: gname,
        } => {
            (!gid.is_empty() && !game_id.is_empty() && gid == game_id)
                || (!gname.trim().is_empty()
                    && !game_name.trim().is_empty()
                    && gname.trim().eq_ignore_ascii_case(game_name.trim()))
        }
        _ => false,
    }
}

#[tauri::command]
pub async fn set_game_farm(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
    game_id: String,
    game_name: String,
    farm: bool,
    profile_id: Option<String>,
) -> Result<Settings, String> {
    let trimmed_name = game_name.trim().to_string();
    if farm && trimmed_name.is_empty() {
        return Err("Game name is required".to_string());
    }
    let pid = resolve_profile_id(state.inner(), platform, profile_id).await?;

    let updated = {
        let mut guard = state.settings.write().await;
        let plat = guard
            .profile_settings_mut(&pid)
            .ok_or_else(|| format!("Profile '{pid}' not found"))?;
        if farm {
            if let Some(existing) = plat
                .priority
                .iter_mut()
                .find(|item| priority_game_matches(item, &game_id, &trimmed_name))
            {
                if let PriorityItem::Game {
                    game_id: gid,
                    game_name: gname,
                } = existing
                {
                    if gid.is_empty() && !game_id.is_empty() {
                        *gid = game_id;
                    }
                    *gname = trimmed_name;
                }
            } else {
                plat.priority.push(PriorityItem::Game {
                    game_id,
                    game_name: trimmed_name,
                });
            }
        } else {
            plat.priority
                .retain(|item| !priority_game_matches(item, &game_id, &trimmed_name));
        }
        guard.sanitize();
        guard.clone()
    };

    state.persist_settings().await?;
    state
        .send_profile_miner(&pid, MinerControl::SettingsUpdated)
        .await;
    state.publish_snapshot(&app, true).await;
    Ok(updated)
}

#[tauri::command]
pub async fn set_priority(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
    items: Vec<PriorityItem>,
    profile_id: Option<String>,
) -> Result<Settings, String> {
    let pid = resolve_profile_id(state.inner(), platform, profile_id).await?;
    let updated = {
        let mut guard = state.settings.write().await;
        let plat = guard
            .profile_settings_mut(&pid)
            .ok_or_else(|| format!("Profile '{pid}' not found"))?;
        plat.priority = items;
        guard.sanitize();
        guard.clone()
    };

    state.persist_settings().await?;
    state
        .send_profile_miner(&pid, MinerControl::SettingsUpdated)
        .await;
    state.publish_snapshot(&app, true).await;
    Ok(updated)
}

#[tauri::command]
pub async fn set_prioritize_ending_soon(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
    enabled: bool,
    profile_id: Option<String>,
) -> Result<Settings, String> {
    let pid = resolve_profile_id(state.inner(), platform, profile_id).await?;
    let updated = {
        let mut guard = state.settings.write().await;
        let plat = guard
            .profile_settings_mut(&pid)
            .ok_or_else(|| format!("Profile '{pid}' not found"))?;
        plat.prioritize_ending_soon = enabled;
        guard.sanitize();
        guard.clone()
    };

    state.persist_settings().await?;
    state
        .send_profile_miner(&pid, MinerControl::SettingsUpdated)
        .await;
    state.publish_snapshot(&app, true).await;
    Ok(updated)
}

#[tauri::command]
pub async fn pause(
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
    profile_id: Option<String>,
) -> Result<(), String> {
    let pid = resolve_profile_id(state.inner(), platform, profile_id).await?;
    state.send_profile_miner(&pid, MinerControl::Pause).await;
    Ok(())
}

#[tauri::command]
pub async fn resume(
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
    profile_id: Option<String>,
) -> Result<(), String> {
    let pid = resolve_profile_id(state.inner(), platform, profile_id).await?;
    state.send_profile_miner(&pid, MinerControl::Resume).await;
    Ok(())
}

#[tauri::command]
pub async fn switch_channel(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
    profile_id: Option<String>,
) -> Result<(), String> {
    let pid = resolve_profile_id(state.inner(), platform, profile_id).await?;
    let is_active = {
        let mut guard = state.settings.write().await;
        if let Some(plat) = guard.profile_settings_mut(&pid) {
            plat.pinned_channel = None;
        }
        guard.sync_legacy_mirrors();
        guard.active_profile_id(platform) == pid
    };
    let _ = state.persist_settings().await;
    {
        let mut map = state.profile_snapshots.write().await;
        if let Some(snap) = map.get_mut(&pid) {
            snap.pinned_channel = None;
        }
    }
    if is_active {
        let mut snap = state.snapshot_lock(platform).write().await;
        snap.pinned_channel = None;
    }
    state.publish_snapshot(&app, true).await;
    state
        .send_profile_miner(&pid, MinerControl::SwitchChannel)
        .await;
    Ok(())
}

#[tauri::command]
pub async fn pin_channel(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
    channel_login: Option<String>,
    profile_id: Option<String>,
) -> Result<Settings, String> {
    let pid = resolve_profile_id(state.inner(), platform, profile_id).await?;
    let (updated, is_active) = {
        let mut guard = state.settings.write().await;
        let plat = guard
            .profile_settings_mut(&pid)
            .ok_or_else(|| format!("Profile '{pid}' not found"))?;
        plat.pinned_channel = channel_login.clone();
        guard.sync_legacy_mirrors();
        let active = guard.active_profile_id(platform) == pid;
        (guard.clone(), active)
    };

    {
        let mut map = state.profile_snapshots.write().await;
        if let Some(snap) = map.get_mut(&pid) {
            snap.pinned_channel = channel_login.clone();
        }
    }
    if is_active {
        let mut snap = state.snapshot_lock(platform).write().await;
        snap.pinned_channel = channel_login.clone();
    }
    state.persist_settings().await?;
    state.publish_snapshot(&app, true).await;
    state
        .send_profile_miner(&pid, MinerControl::PinChannel(channel_login))
        .await;
    Ok(updated)
}

#[tauri::command]
pub async fn retry_platform(
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
    profile_id: Option<String>,
) -> Result<(), String> {
    let pid = resolve_profile_id(state.inner(), platform, profile_id).await?;
    state.send_profile_miner(&pid, MinerControl::RetryNow).await;
    Ok(())
}

#[tauri::command]
pub async fn open_drops_inventory(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
    profile_id: Option<String>,
) -> Result<(), String> {
    let pid = resolve_profile_id(state.inner(), platform, profile_id).await?;
    open_drops_inventory_window_for_profile(&app, state.inner().clone(), platform, &pid)
        .await
        .map_err(|e| e.user_message())
}

#[tauri::command]
pub async fn fetch_campaign_details(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
    campaign_id: String,
    profile_id: Option<String>,
) -> Result<Campaign, String> {
    let pid = resolve_profile_id(state.inner(), platform, profile_id).await?;
    let mut camp = {
        let map = state.profile_snapshots.read().await;
        if let Some(snap) = map.get(&pid) {
            snap.campaigns
                .iter()
                .find(|c| c.id == campaign_id)
                .cloned()
        } else {
            None
        }
    }
    .ok_or_else(|| "Campaign not found".to_string())?;

    if platform != PlatformId::Twitch
        || state.mock_active.load(Ordering::Relaxed)
        || (camp.link_verified && !camp.drops.is_empty())
    {
        return Ok(camp);
    }

    // 1. Fast pure-Rust reward preview lookup via public live directory + AvailableDrops (~120ms,
    //    no hidden WebView2 hang when Twitch DropCampaignDetails is integrity-gated).
    if camp.drops.is_empty() {
        if let Some(public_drops) =
            crate::platforms::twitch::gql::fetch_campaign_drops_via_public_directory(
                &state.http,
                &camp.game.id,
                &camp.game.name,
                &camp.id,
            )
            .await
        {
            if !public_drops.is_empty() {
                camp.drops = public_drops;
                {
                    let mut map = state.profile_snapshots.write().await;
                    if let Some(snap) = map.get_mut(&pid) {
                        if let Some(target) = snap.campaigns.iter_mut().find(|c| c.id == camp.id) {
                            if target.drops.is_empty() {
                                target.drops = camp.drops.clone();
                            }
                            camp = target.clone();
                        }
                    }
                }
                state.sync_active_platform_snapshot(PlatformId::Twitch).await;
                state
                    .send_profile_miner(
                        &pid,
                        MinerControl::CampaignDetailsLoaded(Box::new(camp.clone())),
                    )
                    .await;
                state.publish_snapshot(&app, true).await;
                return Ok(camp);
            }
        }
    }

    let payload = {
        let _guard = state.twitch_bridge_lock.lock().await;
        twitch_bridge_fetch_campaign_details_for_profile(
            &app,
            &pid,
            &camp.id,
            &camp.name,
            &camp.game.id,
            &camp.game.name,
        )
        .await
        .map_err(|e| e.user_message())?
    };

    if let Some(detailed) = parse_campaign_details(&payload).map_err(|e| e.user_message())? {
        apply_campaign_details(&mut camp, detailed);
        {
            let mut map = state.profile_snapshots.write().await;
            if let Some(snap) = map.get_mut(&pid) {
                if let Some(target) = snap.campaigns.iter_mut().find(|c| c.id == camp.id) {
                    *target = camp.clone();
                }
                crate::miner::engine::propagate_linked_games(&mut snap.campaigns);
                if let Some(synced) = snap.campaigns.iter().find(|c| c.id == camp.id) {
                    camp = synced.clone();
                }
            }
        }
        state.sync_active_platform_snapshot(PlatformId::Twitch).await;
        state
            .send_profile_miner(
                &pid,
                MinerControl::CampaignDetailsLoaded(Box::new(camp.clone())),
            )
            .await;
        state.publish_snapshot(&app, true).await;
    }

    Ok(camp)
}

#[tauri::command]
pub async fn get_settings(state: State<'_, Arc<AppState>>) -> Result<Settings, String> {
    Ok(state.settings.read().await.clone())
}

#[tauri::command]
pub async fn update_settings(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    mut new_settings: Settings,
) -> Result<Settings, String> {
    let (prev_autostart, prev_bounds, prev_theme, prev_log_level, prev_mock, prev_remote_cfg) = {
        let cur = state.settings.read().await;
        (
            cur.autostart,
            cur.window_bounds.clone(),
            cur.theme,
            cur.log_level,
            cur.mock_mode,
            cur.remote_config_enabled,
        )
    };

    if new_settings.window_bounds.is_none() {
        new_settings.window_bounds = prev_bounds;
    }
    new_settings.sanitize();

    if new_settings.autostart != prev_autostart {
        sync_autostart(&app, new_settings.autostart)?;
    }

    if new_settings.theme != prev_theme {
        apply_window_theme(&app, new_settings.theme);
    }

    if new_settings.log_level != prev_log_level {
        if let Ok(guard) = state.log_handle.lock() {
            if let Some(ref handle) = *guard {
                handle.set_level(new_settings.log_level);
            }
        }
    }

    if cfg!(feature = "mock") && new_settings.mock_mode != prev_mock {
        state
            .mock_active
            .store(new_settings.mock_mode, Ordering::Relaxed);
    }

    if new_settings.remote_config_enabled != prev_remote_cfg {
        if !new_settings.remote_config_enabled {
            state.remote_config.reset_to_bundled().await;
        } else {
            let rc = state.remote_config.clone();
            let http = state.http.clone();
            tauri::async_runtime::spawn(async move {
                let _ = rc.refresh_now(&http).await;
            });
        }
    }

    state
        .close_to_tray
        .store(new_settings.close_to_tray, Ordering::Relaxed);

    {
        let mut guard = state.settings.write().await;
        *guard = new_settings.clone();
    }

    state.persist_settings().await?;
    state.send_all_miners(MinerControl::SettingsUpdated).await;
    state.publish_snapshot(&app, true).await;
    Ok(new_settings)
}

#[tauri::command]
pub async fn accept_disclaimer(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<Settings, String> {
    let updated = {
        let mut guard = state.settings.write().await;
        guard.disclaimer_accepted_version = CURRENT_DISCLAIMER_VERSION;
        guard.clone()
    };

    info!("User accepted Dropilot terms disclaimer");
    state.persist_settings().await?;
    state.send_all_miners(MinerControl::SettingsUpdated).await;
    state.publish_snapshot(&app, true).await;
    Ok(updated)
}

#[tauri::command]
pub async fn clear_app_data(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let existing_profiles = state.settings.read().await.profiles.clone();
    for prof in &existing_profiles {
        if is_default_profile(&prof.id) {
            state
                .send_profile_miner(&prof.id, MinerControl::Logout)
                .await;
        } else {
            state
                .send_profile_miner(&prof.id, MinerControl::Shutdown)
                .await;
            state.unregister_miner_for_profile(&prof.id).await;
            close_profile_windows(&app, prof.platform, &prof.id);
        }
        let _ = SecretStore::delete_profile_token(prof.platform, &prof.id);
    }
    let _ = SecretStore::delete_token(PlatformId::Twitch);
    let _ = SecretStore::delete_token(PlatformId::Kick);
    WebhookDispatcher::clear_all_secrets();
    let _ = clear_webview_browsing_data(&app).await;
    let profiles_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("Dropilot")
        .join("WebView2-Profiles");
    if profiles_dir.exists() {
        let _ = fs::remove_dir_all(&profiles_dir);
    }

    let disclaimer_accepted_version = state.settings.read().await.disclaimer_accepted_version;
    let fresh = Settings {
        disclaimer_accepted_version,
        ..Default::default()
    };
    state
        .close_to_tray
        .store(fresh.close_to_tray, Ordering::Relaxed);
    {
        let mut guard = state.settings.write().await;
        *guard = fresh;
    }
    state.sync_active_platform_snapshot(PlatformId::Twitch).await;
    state.sync_active_platform_snapshot(PlatformId::Kick).await;
    {
        let mut hist = state.history.write().await;
        *hist = HistoryStore::default();
    }
    state.recent_claims.write().await.clear();
    state.remote_config.reset_to_bundled().await;
    state.epic.clear_cache().await;
    let _ = fs::remove_file(history_path());
    let _ = fs::remove_file(appdata_dir().join("settings.json"));
    state.persist_settings().await?;
    state.send_all_miners(MinerControl::SettingsUpdated).await;
    state.publish_snapshot(&app, true).await;
    info!("Cleared Dropilot accounts, WebView2 data, and reset settings");
    Ok(())
}

#[tauri::command]
pub async fn get_logs(
    state: State<'_, Arc<AppState>>,
    level: Option<LogLevel>,
    platform: Option<PlatformId>,
) -> Result<Vec<LogEntry>, String> {
    Ok(state.logs.list(level, platform))
}

#[tauri::command]
pub async fn open_url(state: State<'_, Arc<AppState>>, url: String) -> Result<(), String> {
    let parsed = Url::parse(&url).map_err(|e| format!("Invalid URL: {e}"))?;
    if parsed.scheme() != "https" && parsed.scheme() != "com.epicgames.launcher" {
        return Err("Only https:// or com.epicgames.launcher:// URLs may be opened".to_string());
    }

    let campaign_link_ok = if parsed.scheme() == "https" {
        let map = state.profile_snapshots.read().await;
        let in_profiles = map
            .values()
            .flat_map(|s| s.campaigns.iter())
            .filter_map(|c| c.link_url.as_deref())
            .any(|link| link == url);
        if in_profiles {
            true
        } else {
            let tw = state.twitch_snapshot.read().await;
            let kk = state.kick_snapshot.read().await;
            tw.campaigns
                .iter()
                .chain(kk.campaigns.iter())
                .filter_map(|c| c.link_url.as_deref())
                .any(|link| link == url)
        }
    } else {
        false
    };

    let allowlist_ok = campaign_link_ok || validate_url(&parsed, AllowlistKind::ExternalOpen).is_ok();
    if !allowlist_ok {
        return Err(format!(
            "Refused to open unverified external URL host: {}",
            parsed.host_str().unwrap_or("unknown")
        ));
    }

    tauri_plugin_opener::open_url(parsed.as_str(), None::<&str>)
        .map_err(|e| format!("Failed opening URL in system handler: {e}"))
}

/// Opens the Epic Games Launcher directly via `com.epicgames.launcher://store/...`.
/// Returns `Ok(true)` if the launcher URI opened, or `Ok(false)` if it gracefully fell
/// back to opening the official Epic Games Store page in the default browser.
#[tauri::command]
pub async fn open_epic_launcher(
    state: State<'_, Arc<AppState>>,
    game_id: Option<String>,
) -> Result<bool, String> {
    let (launcher_url, fallback_web_url) = match game_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(id) => match state.epic.find_game(id).await {
            Some(game) => (launcher_url_for_game(&game), game.store_url),
            None => (
                EPIC_LAUNCHER_FREE_GAMES_URL.to_string(),
                EPIC_FALLBACK_FREE_GAMES_URL.to_string(),
            ),
        },
        None => (
            EPIC_LAUNCHER_FREE_GAMES_URL.to_string(),
            EPIC_FALLBACK_FREE_GAMES_URL.to_string(),
        ),
    };

    let parsed_launcher = Url::parse(&launcher_url)
        .map_err(|e| format!("Invalid Epic launcher URI: {e}"))?;
    validate_url(&parsed_launcher, AllowlistKind::ExternalOpen)
        .map_err(|e| e.user_message())?;

    if let Err(err) = tauri_plugin_opener::open_url(parsed_launcher.as_str(), None::<&str>) {
        warn!(
            url = %parsed_launcher.as_str(),
            error = %err,
            "Failed opening Epic Games Launcher URI; falling back to default browser"
        );
        let parsed_web = Url::parse(&fallback_web_url)
            .map_err(|e| format!("Invalid fallback Epic Store URL: {e}"))?;
        validate_url(&parsed_web, AllowlistKind::ExternalOpen)
            .map_err(|e| e.user_message())?;
        tauri_plugin_opener::open_url(parsed_web.as_str(), None::<&str>)
            .map_err(|e| format!("Failed opening Epic Games Launcher or browser fallback: {e}"))?;
        return Ok(false);
    }

    Ok(true)
}

#[tauri::command]
pub async fn open_log_folder() -> Result<(), String> {
    let dir = log_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed creating log folder: {e}"))?;
    tauri_plugin_opener::open_path(dir, None::<&str>)
        .map_err(|e| format!("Failed opening log folder: {e}"))
}

#[tauri::command]
pub async fn open_data_folder() -> Result<(), String> {
    let dir = appdata_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed creating data folder: {e}"))?;
    tauri_plugin_opener::open_path(dir, None::<&str>)
        .map_err(|e| format!("Failed opening data folder: {e}"))
}

#[tauri::command]
pub async fn check_update(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<UpdateStatus, String> {
    check_signed_update(&app, &state.http)
        .await
        .map_err(|e| e.user_message())
}

#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    install_signed_update(&app, state.inner()).await
}

#[tauri::command]
pub fn is_mock_available() -> bool {
    cfg!(feature = "mock")
}

#[tauri::command]
pub async fn report_user_agent(
    state: State<'_, Arc<AppState>>,
    user_agent: String,
) -> Result<(), String> {
    if user_agent.len() > 512 {
        return Err("Invalid user-agent length".to_string());
    }
    state.http.set_user_agent(user_agent).await;
    Ok(())
}

/// Closes & destroys the UI WebView2 window so Dropilot keeps farming in the system tray
/// with ~11 MB RAM and 0 background WebView2 processes (unless an auxiliary window is open).
#[tauri::command]
pub async fn hide_to_tray(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let bounds = app
        .get_webview_window("main")
        .and_then(|win| crate::windows::capture_bounds_sync(&win.as_ref().window()))
        .or_else(crate::windows::cached_main_bounds);
    crate::windows::destroy_main_to_tray(&app, state.inner(), bounds).await;
    Ok(())
}

#[tauri::command]
pub async fn get_remote_config_status(
    state: State<'_, Arc<AppState>>,
) -> Result<RemoteConfigStatus, String> {
    Ok(state.remote_config.status().await)
}

#[tauri::command]
pub async fn refresh_remote_config(
    state: State<'_, Arc<AppState>>,
) -> Result<RemoteConfigStatus, String> {
    let enabled = state.settings.read().await.remote_config_enabled;
    if !enabled {
        state.remote_config.reset_to_bundled().await;
        return Ok(state.remote_config.status().await);
    }
    Ok(state.remote_config.refresh_now(&state.http).await)
}

#[tauri::command]
pub async fn get_webhook_secrets_status() -> Result<WebhookSecretsStatus, String> {
    Ok(WebhookDispatcher::secrets_status())
}

#[tauri::command]
pub async fn save_webhook_secrets(
    discord_url: Option<String>,
) -> Result<WebhookSecretsStatus, String> {
    if !current_edition().webhooks_enabled {
        return Err("Discord webhooks are not available in this build.".to_string());
    }
    if let Some(url) = discord_url {
        WebhookDispatcher::save_discord_url(&url)?;
    }
    Ok(WebhookDispatcher::secrets_status())
}

#[tauri::command]
pub async fn test_webhook(
    state: State<'_, Arc<AppState>>,
    channel: WebhookChannel,
) -> Result<String, String> {
    if !current_edition().webhooks_enabled {
        return Err("Discord webhooks are not available in this build.".to_string());
    }
    WebhookDispatcher::send_test(&state.http, channel).await
}

#[tauri::command]
pub async fn get_history_stats(
    state: State<'_, Arc<AppState>>,
) -> Result<HistoryStats, String> {
    Ok(state.history.read().await.build_stats())
}

#[tauri::command]
pub async fn export_history(
    state: State<'_, Arc<AppState>>,
    format: String,
) -> Result<HistoryExportResult, String> {
    let guard = state.history.read().await;
    guard.export_to_disk(&format)
}

#[tauri::command]
pub async fn clear_history(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    {
        let mut hist = state.history.write().await;
        *hist = HistoryStore::default();
        let _ = hist.save();
    }
    state.recent_claims.write().await.clear();
    state.publish_snapshot(&app, true).await;
    info!("Cleared persistent drop claim history and stats");
    Ok(())
}

#[tauri::command]
pub async fn get_epic_free_games(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<EpicFeedSnapshot, String> {
    if state.epic.is_stale().await {
        let (snap, newly_live) = state.epic.refresh_now(&state.http).await;
        notify_newly_live_games(&app, state.inner(), &newly_live).await;
        return Ok(snap);
    }
    Ok(state.epic.snapshot().await)
}

#[tauri::command]
pub async fn refresh_epic_free_games(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<EpicFeedSnapshot, String> {
    let (snap, newly_live) = state.epic.refresh_now(&state.http).await;
    notify_newly_live_games(&app, state.inner(), &newly_live).await;
    Ok(snap)
}

#[tauri::command]
pub async fn open_epic_claim(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    game_id: String,
) -> Result<(), String> {
    let game = state
        .epic
        .find_game(&game_id)
        .await
        .ok_or_else(|| "Epic free game offer not found; try refreshing".to_string())?;
    open_epic_claim_window(&app, state.inner().clone(), &game.store_url, &game.title)
        .await
        .map_err(|e| e.user_message())
}

#[tauri::command]
pub async fn mark_epic_game_claimed(
    state: State<'_, Arc<AppState>>,
    game_id: String,
    claimed: bool,
) -> Result<EpicFeedSnapshot, String> {
    let updated_game = state.epic.set_claimed(&game_id, claimed).await;
    if claimed {
        if let Some(ref game) = updated_game {
            state.record_epic_claim(game).await;
        }
    } else {
        state.remove_epic_claim(&game_id).await;
    }
    Ok(state.epic.snapshot().await)
}
