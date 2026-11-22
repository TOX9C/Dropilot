use std::fs;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::{AppHandle, Manager, State};
use tracing::{info, warn};
use url::Url;

use crate::auth::login_window::{
    clear_webview_browsing_data, close_kick_bridge_window, close_twitch_bridge_window,
    open_drops_inventory_window, open_login_window, twitch_bridge_fetch_campaign_details,
};
use crate::autostart::sync_autostart;
use crate::history::{history_path, HistoryExportResult, HistoryStats, HistoryStore};
use crate::logging::{log_dir, LogEntry, LogLevel};
use crate::net::allowlist::{validate_url, AllowlistKind};
use crate::platforms::twitch::apply_campaign_details;
use crate::platforms::twitch::models::parse_campaign_details;
use crate::platforms::{AppSnapshot, Campaign, PlatformId};
use crate::remote_config::RemoteConfigStatus;
use crate::secrets::SecretStore;
use crate::settings::{appdata_dir, PriorityItem, Settings, CURRENT_DISCLAIMER_VERSION};
use crate::state::{AppState, MinerControl};
use crate::update_check::{check_signed_update, install_signed_update, UpdateStatus};
use crate::webhooks::{WebhookChannel, WebhookDispatcher, WebhookSecretsStatus};
use crate::windows::apply_window_theme;

#[tauri::command]
pub async fn get_snapshot(state: State<'_, Arc<AppState>>) -> Result<AppSnapshot, String> {
    Ok(state.full_snapshot().await)
}

#[tauri::command]
pub async fn login(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
) -> Result<(), String> {
    if state.mock_active.load(Ordering::Relaxed) {
        state
            .send_miner(platform, MinerControl::LoginComplete)
            .await;
        return Ok(());
    }
    open_login_window(&app, state.inner().clone(), platform)
        .await
        .map_err(|e| e.user_message())
}

#[tauri::command]
pub async fn logout(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
) -> Result<(), String> {
    {
        let mut guard = state.settings.write().await;
        match platform {
            PlatformId::Twitch => guard.twitch.pinned_channel = None,
            PlatformId::Kick => guard.kick.pinned_channel = None,
        }
    }
    let _ = state.persist_settings().await;
    state.send_miner(platform, MinerControl::Logout).await;
    if !state.mock_active.load(Ordering::Relaxed) {
        let _ = SecretStore::delete_token(platform);
        if let Err(err) = clear_webview_browsing_data(&app).await {
            warn!(error = %err, "Could not clear WebView2 browsing data on logout");
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
) -> Result<Settings, String> {
    let updated = {
        let mut guard = state.settings.write().await;
        let plat = match platform {
            PlatformId::Twitch => &mut guard.twitch,
            PlatformId::Kick => &mut guard.kick,
        };
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
        .send_miner(platform, MinerControl::SettingsUpdated)
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
) -> Result<Settings, String> {
    let trimmed_name = game_name.trim().to_string();
    if farm && trimmed_name.is_empty() {
        return Err("Game name is required".to_string());
    }

    let updated = {
        let mut guard = state.settings.write().await;
        let plat = match platform {
            PlatformId::Twitch => &mut guard.twitch,
            PlatformId::Kick => &mut guard.kick,
        };
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
        .send_miner(platform, MinerControl::SettingsUpdated)
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
) -> Result<Settings, String> {
    let updated = {
        let mut guard = state.settings.write().await;
        let plat = match platform {
            PlatformId::Twitch => &mut guard.twitch,
            PlatformId::Kick => &mut guard.kick,
        };
        plat.priority = items;
        guard.sanitize();
        guard.clone()
    };

    state.persist_settings().await?;
    state
        .send_miner(platform, MinerControl::SettingsUpdated)
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
) -> Result<Settings, String> {
    let updated = {
        let mut guard = state.settings.write().await;
        let plat = match platform {
            PlatformId::Twitch => &mut guard.twitch,
            PlatformId::Kick => &mut guard.kick,
        };
        plat.prioritize_ending_soon = enabled;
        guard.sanitize();
        guard.clone()
    };

    state.persist_settings().await?;
    state
        .send_miner(platform, MinerControl::SettingsUpdated)
        .await;
    state.publish_snapshot(&app, true).await;
    Ok(updated)
}

#[tauri::command]
pub async fn pause(state: State<'_, Arc<AppState>>, platform: PlatformId) -> Result<(), String> {
    state.send_miner(platform, MinerControl::Pause).await;
    Ok(())
}

#[tauri::command]
pub async fn resume(state: State<'_, Arc<AppState>>, platform: PlatformId) -> Result<(), String> {
    state.send_miner(platform, MinerControl::Resume).await;
    Ok(())
}

#[tauri::command]
pub async fn switch_channel(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
) -> Result<(), String> {
    {
        let mut guard = state.settings.write().await;
        match platform {
            PlatformId::Twitch => guard.twitch.pinned_channel = None,
            PlatformId::Kick => guard.kick.pinned_channel = None,
        }
    }
    let _ = state.persist_settings().await;
    {
        let mut snap = state.snapshot_lock(platform).write().await;
        snap.pinned_channel = None;
    }
    state.publish_snapshot(&app, true).await;
    state
        .send_miner(platform, MinerControl::SwitchChannel)
        .await;
    Ok(())
}

#[tauri::command]
pub async fn pin_channel(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
    channel_login: Option<String>,
) -> Result<Settings, String> {
    let updated = {
        let mut guard = state.settings.write().await;
        let plat = match platform {
            PlatformId::Twitch => &mut guard.twitch,
            PlatformId::Kick => &mut guard.kick,
        };
        plat.pinned_channel = channel_login.clone();
        guard.clone()
    };

    {
        let mut snap = state.snapshot_lock(platform).write().await;
        snap.pinned_channel = channel_login.clone();
    }
    state.persist_settings().await?;
    state.publish_snapshot(&app, true).await;
    state
        .send_miner(platform, MinerControl::PinChannel(channel_login))
        .await;
    Ok(updated)
}

#[tauri::command]
pub async fn retry_platform(
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
) -> Result<(), String> {
    state.send_miner(platform, MinerControl::RetryNow).await;
    Ok(())
}

#[tauri::command]
pub async fn open_drops_inventory(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
) -> Result<(), String> {
    open_drops_inventory_window(&app, state.inner().clone(), platform)
        .await
        .map_err(|e| e.user_message())
}

#[tauri::command]
pub async fn fetch_campaign_details(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    platform: PlatformId,
    campaign_id: String,
) -> Result<Campaign, String> {
    let mut camp = {
        let snap = state.snapshot_lock(platform).read().await;
        snap.campaigns
            .iter()
            .find(|c| c.id == campaign_id)
            .cloned()
            .ok_or_else(|| "Campaign not found".to_string())?
    };

    if platform != PlatformId::Twitch
        || state.mock_active.load(Ordering::Relaxed)
        || (camp.link_verified && !camp.drops.is_empty())
    {
        return Ok(camp);
    }

    let payload = {
        let _guard = state.twitch_bridge_lock.lock().await;
        twitch_bridge_fetch_campaign_details(
            &app,
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
            let mut snap = state.twitch_snapshot.write().await;
            if let Some(target) = snap.campaigns.iter_mut().find(|c| c.id == camp.id) {
                *target = camp.clone();
            }
            crate::miner::engine::propagate_linked_games(&mut snap.campaigns);
            if let Some(synced) = snap.campaigns.iter().find(|c| c.id == camp.id) {
                camp = synced.clone();
            }
        }
        state
            .send_miner(
                PlatformId::Twitch,
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
    state.send_all_miners(MinerControl::Logout).await;
    let _ = SecretStore::delete_token(PlatformId::Twitch);
    let _ = SecretStore::delete_token(PlatformId::Kick);
    WebhookDispatcher::clear_all_secrets();
    let _ = clear_webview_browsing_data(&app).await;

    let disclaimer_accepted_version = state.settings.read().await.disclaimer_accepted_version;
    let fresh = Settings {
        disclaimer_accepted_version,
        ..Default::default()
    };
    {
        let mut guard = state.settings.write().await;
        *guard = fresh;
    }
    {
        let mut hist = state.history.write().await;
        *hist = HistoryStore::default();
    }
    state.recent_claims.write().await.clear();
    state.remote_config.reset_to_bundled().await;
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
    if parsed.scheme() != "https" {
        return Err("Only https:// URLs may be opened".to_string());
    }

    let campaign_link_ok = {
        let tw = state.twitch_snapshot.read().await;
        let kk = state.kick_snapshot.read().await;
        tw.campaigns
            .iter()
            .chain(kk.campaigns.iter())
            .filter_map(|c| c.link_url.as_deref())
            .any(|link| link == url)
    };

    let allowlist_ok = campaign_link_ok || validate_url(&parsed, AllowlistKind::ExternalOpen).is_ok();
    if !allowlist_ok {
        return Err(format!(
            "Refused to open unverified external URL host: {}",
            parsed.host_str().unwrap_or("unknown")
        ));
    }

    tauri_plugin_opener::open_url(parsed.as_str(), None::<&str>)
        .map_err(|e| format!("Failed opening URL in system browser: {e}"))
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
/// with ~11 MB RAM and 0 background WebView2 processes.
#[tauri::command]
pub async fn hide_to_tray(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        if let Some(bounds) = crate::windows::capture_bounds_sync(&win.as_ref().window()) {
            let mut guard = state.settings.write().await;
            guard.window_bounds = Some(bounds);
        }
        let _ = state.persist_settings().await;
        state.events.reset_cache().await;
        let _ = win.destroy();
    }
    close_kick_bridge_window(&app);
    close_twitch_bridge_window(&app);
    info!("UI closed to system tray (ultra-light mode; WebView2 destroyed)");
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
    telegram_token: Option<String>,
) -> Result<WebhookSecretsStatus, String> {
    if let Some(url) = discord_url {
        WebhookDispatcher::save_discord_url(&url)?;
    }
    if let Some(tok) = telegram_token {
        WebhookDispatcher::save_telegram_token(&tok)?;
    }
    Ok(WebhookDispatcher::secrets_status())
}

#[tauri::command]
pub async fn test_webhook(
    state: State<'_, Arc<AppState>>,
    channel: WebhookChannel,
    telegram_chat_id: Option<String>,
) -> Result<(), String> {
    let chat_id = match telegram_chat_id {
        Some(id) if !id.trim().is_empty() => id,
        _ => state.settings.read().await.webhooks.telegram_chat_id.clone(),
    };
    WebhookDispatcher::send_test(&state.http, channel, &chat_id).await
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
