use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;
use tokio::sync::mpsc;
use tokio::time::{sleep, Instant};
use tracing::{error, info, warn};

use crate::auth::login_window::{
    close_compat_watch_window_for_profile, close_kick_bridge_window_for_profile,
    close_twitch_bridge_window_for_profile,
};
use crate::history::{ClaimHistoryEntry, ClaimRecordStatus};
use crate::miner::scheduler::{
    duration_until_quiet_end, format_minutes_hhmm, is_in_quiet_hours_now, jittered_duration,
    jittered_duration_with_mode, local_minutes_since_midnight, ACTIVE_PROGRESS_POLL_SECS,
    ERROR_RETRY_SECS, IDLE_RETRY_SECS, LIVE_CHECK_SECS, MIN_CAMPAIGN_REFRESH_SECS,
    MOCK_WATCH_TICK_SECS, PROGRESS_POLL_SECS, WATCH_TICK_SECS,
};
use crate::miner::selector::{
    evaluate_switch, game_names_match, ordered_target_campaigns, select_best_channel,
    SwitchContext, SwitchDecision,
};
use crate::net::backoff::Backoff;
use crate::platforms::kick::models::is_kick_fallback_case_id;
use crate::platforms::kick::KickPlatform;
#[cfg(feature = "mock")]
use crate::platforms::mock::MockPlatform;
use crate::platforms::twitch::apply_campaign_details;
use crate::platforms::twitch::models::{GAME_LINKED_ID_PREFIX, GAME_LINKED_NAME_PREFIX};
use crate::platforms::twitch::TwitchPlatform;
use crate::platforms::{
    Account, AppError, Campaign, CampaignStatus, Channel, ClaimedBenefit, Drop, DropProgress,
    MinerState, Platform, PlatformId, PlatformSnapshot, RecentClaim, Reward, WatchTickResult,
};
use crate::secrets::SecretStore;
use crate::settings::is_default_profile;
use crate::state::{AppState, MinerControl};
use crate::tray::update_tray_from_state;
use crate::webhooks::WebhookEvent;

#[derive(Clone)]
pub struct PlatformBackend {
    pub real: Arc<dyn Platform>,
    pub mock: Option<Arc<dyn Platform>>,
}

impl PlatformBackend {
    fn active(&self, state: &AppState) -> &Arc<dyn Platform> {
        if state.mock_active.load(Ordering::Relaxed) {
            if let Some(ref m) = self.mock {
                return m;
            }
        }
        &self.real
    }
}

pub fn build_platform_backend(
    profile_id: &str,
    platform_id: PlatformId,
    app: &AppHandle,
    state: &Arc<AppState>,
) -> PlatformBackend {
    let real: Arc<dyn Platform> = match platform_id {
        PlatformId::Twitch => Arc::new(TwitchPlatform::new_for_profile(
            profile_id.to_string(),
            state.http.clone(),
            Some(app.clone()),
            state.twitch_bridge_lock.clone(),
        )),
        PlatformId::Kick => Arc::new(KickPlatform::new_for_profile(
            profile_id.to_string(),
            state.http.clone(),
            Some(app.clone()),
            state.kick_bridge_waiters.clone(),
        )),
    };

    #[cfg(feature = "mock")]
    let mock = Some(Arc::new(MockPlatform::new(platform_id)) as Arc<dyn Platform>);
    #[cfg(not(feature = "mock"))]
    let mock = None;

    PlatformBackend { real, mock }
}

pub fn spawn_miner_supervisor(
    profile_id: String,
    platform_id: PlatformId,
    startup_delay_ms: u64,
    app: AppHandle,
    state: Arc<AppState>,
    backend: PlatformBackend,
) -> mpsc::Sender<MinerControl> {
    let (tx, mut rx) = mpsc::channel::<MinerControl>(32);

    tauri::async_runtime::spawn(async move {
        if startup_delay_ms > 0 {
            tokio::select! {
                _ = sleep(Duration::from_millis(startup_delay_ms)) => {}
                cmd = rx.recv() => {
                    if matches!(cmd, None | Some(MinerControl::Shutdown)) {
                        info!(platform = platform_id.as_str(), profile_id = %profile_id, "Miner stopped cleanly before start");
                        return;
                    }
                }
            }
        }

        loop {
            let result = run_miner_loop(
                &profile_id,
                platform_id,
                app.clone(),
                state.clone(),
                backend.clone(),
                &mut rx,
            )
            .await;

            match result {
                Ok(ExitReason::Shutdown) => {
                    info!(platform = platform_id.as_str(), profile_id = %profile_id, "Miner stopped cleanly");
                    break;
                }
                Ok(ExitReason::Restart) => {
                    warn!(platform = platform_id.as_str(), profile_id = %profile_id, "Restarting miner loop");
                    sleep(Duration::from_secs(3)).await;
                }
                Err(err) => {
                    error!(
                        platform = platform_id.as_str(),
                        profile_id = %profile_id,
                        error = %err,
                        "Miner supervisor caught fatal error; restarting in 5s"
                    );
                    sleep(Duration::from_secs(5)).await;
                }
            }
        }
    });

    tx
}

enum ExitReason {
    Shutdown,
    Restart,
}

struct RuntimeContext {
    token: Option<String>,
    account: Option<Account>,
    state: MinerState,
    state_reason: Option<String>,
    last_error: Option<String>,
    campaigns: Vec<Campaign>,
    candidates: Vec<Channel>,
    watching: Option<Channel>,
    current_campaign_id: Option<String>,
    current_drop_id: Option<String>,
    watching_since: Option<DateTime<Utc>>,
    progress_synced_at: Option<DateTime<Utc>>,
    next_action_at: Option<DateTime<Utc>>,
    last_campaign_fetch: Option<Instant>,
    last_progress_fetch: Option<Instant>,
    last_completion_bridge_reset: Option<Instant>,
    last_live_check: Option<Instant>,
    last_watch_tick: Option<Instant>,
    last_progress_advance: DateTime<Utc>,
    last_voluntary_switch: Option<DateTime<Utc>>,
    last_seen_total_minutes: u32,
    progress_baseline_initialized: bool,
    confirmed_progress_at: Option<DateTime<Utc>>,
    auto_compat_fallback: bool,
    backoff: Backoff,
    retry_not_before: Option<Instant>,
    paused: bool,
    hydrated_campaign_ids: HashSet<String>,
    claimed_benefits: Vec<ClaimedBenefit>,
    no_live_channels_until: HashMap<String, Instant>,
    avoid_channels_until: HashMap<String, Instant>,
}

impl RuntimeContext {
    fn new() -> Self {
        Self {
            token: None,
            account: None,
            state: MinerState::LoggedOut,
            state_reason: Some("Not logged in".to_string()),
            last_error: None,
            campaigns: Vec::new(),
            candidates: Vec::new(),
            watching: None,
            current_campaign_id: None,
            current_drop_id: None,
            watching_since: None,
            progress_synced_at: None,
            next_action_at: None,
            last_campaign_fetch: None,
            last_progress_fetch: None,
            last_completion_bridge_reset: None,
            last_live_check: None,
            last_watch_tick: None,
            last_progress_advance: Utc::now(),
            last_voluntary_switch: None,
            last_seen_total_minutes: 0,
            progress_baseline_initialized: false,
            confirmed_progress_at: None,
            auto_compat_fallback: false,
            backoff: Backoff::new(),
            retry_not_before: None,
            paused: false,
            hydrated_campaign_ids: HashSet::new(),
            claimed_benefits: Vec::new(),
            no_live_channels_until: HashMap::new(),
            avoid_channels_until: HashMap::new(),
        }
    }

    fn reset_session(&mut self, reason: &str) {
        self.token = None;
        self.account = None;
        self.state = MinerState::LoggedOut;
        self.state_reason = Some(reason.to_string());
        self.watching = None;
        self.current_campaign_id = None;
        self.current_drop_id = None;
        self.watching_since = None;
        self.progress_synced_at = None;
        self.candidates.clear();
        self.campaigns.clear();
        self.hydrated_campaign_ids.clear();
        self.claimed_benefits.clear();
        self.no_live_channels_until.clear();
        self.avoid_channels_until.clear();
        self.last_campaign_fetch = None;
        self.last_progress_fetch = None;
        self.last_completion_bridge_reset = None;
        self.last_live_check = None;
        self.last_watch_tick = None;
        self.retry_not_before = None;
        self.next_action_at = None;
        self.last_seen_total_minutes = 0;
        self.progress_baseline_initialized = false;
        self.confirmed_progress_at = None;
        self.auto_compat_fallback = false;
        self.backoff.reset();
    }

    fn prune_cooldowns(&mut self) {
        let now_inst = Instant::now();
        self.no_live_channels_until.retain(|_, until| *until > now_inst);
        self.avoid_channels_until.retain(|_, until| *until > now_inst);
    }

    fn to_snapshot(&self, platform: PlatformId, pinned_channel: Option<String>) -> PlatformSnapshot {
        let now = Utc::now();
        PlatformSnapshot {
            platform,
            state: self.state,
            state_reason: self.state_reason.clone(),
            account: self.account.clone(),
            watching: self.watching.clone(),
            current_campaign_id: self.current_campaign_id.clone(),
            current_drop_id: self.current_drop_id.clone(),
            watching_since: self.watching_since,
            progress_synced_at: self.progress_synced_at,
            next_action_at: self.next_action_at,
            last_error: self.last_error.clone(),
            campaigns: self
                .campaigns
                .iter()
                .filter(|c| c.status != CampaignStatus::Expired && c.ends_at > now)
                .cloned()
                .collect(),
            candidate_channels: self
                .candidates
                .iter()
                .filter(|c| c.is_live && c.drops_enabled)
                .cloned()
                .collect(),
            pinned_channel,
        }
    }
}

async fn clear_pinned_channel(state: &AppState, profile_id: &str, platform: PlatformId) {
    let changed = {
        let mut guard = state.settings.write().await;
        let cleared = if let Some(plat) = guard.profile_settings_mut(profile_id) {
            plat.pinned_channel.take().is_some()
        } else {
            let plat = match platform {
                PlatformId::Twitch => &mut guard.twitch,
                PlatformId::Kick => &mut guard.kick,
            };
            plat.pinned_channel.take().is_some()
        };
        if cleared {
            guard.sync_legacy_mirrors();
        }
        cleared
    };
    if changed {
        let _ = state.persist_settings().await;
    }
}

async fn run_miner_loop(
    profile_id: &str,
    platform_id: PlatformId,
    app: AppHandle,
    state: Arc<AppState>,
    backend: PlatformBackend,
    rx: &mut mpsc::Receiver<MinerControl>,
) -> Result<ExitReason, String> {
    let mut ctx = RuntimeContext::new();

    // Load initial token from Credential Manager (or mock session when mock mode is active).
    load_initial_token(profile_id, platform_id, &state, &backend, &mut ctx).await;
    sync_snapshot(&app, &state, profile_id, platform_id, &ctx, true).await;

    loop {
        // Check if we need to reload token when switching between real and mock mode.
        let is_mock = state.mock_active.load(Ordering::Relaxed);
        if is_mock && ctx.token.is_none() {
            load_initial_token(profile_id, platform_id, &state, &backend, &mut ctx).await;
            sync_snapshot(&app, &state, profile_id, platform_id, &ctx, true).await;
        }

        // Wait either for a MinerControl command or the next step tick.
        let humanized_jitter = state.settings.read().await.schedule.humanized_jitter;
        let wait_dur = compute_next_wake_delay(&state, &ctx, humanized_jitter);
        tokio::select! {
            maybe_cmd = rx.recv() => {
                let Some(cmd) = maybe_cmd else {
                    return Ok(ExitReason::Restart);
                };
                match handle_control(profile_id, platform_id, &app, &state, &backend, &mut ctx, cmd).await {
                    ControlOutcome::Continue => {}
                    ControlOutcome::Shutdown => return Ok(ExitReason::Shutdown),
                }
                sync_snapshot(&app, &state, profile_id, platform_id, &ctx, true).await;
            }
            _ = sleep(wait_dur) => {
                step_miner(profile_id, platform_id, &app, &state, &backend, &mut ctx).await;
                sync_snapshot(&app, &state, profile_id, platform_id, &ctx, false).await;
            }
        }
    }
}

enum ControlOutcome {
    Continue,
    Shutdown,
}

async fn load_initial_token(
    profile_id: &str,
    platform_id: PlatformId,
    state: &AppState,
    backend: &PlatformBackend,
    ctx: &mut RuntimeContext,
) {
    if state.mock_active.load(Ordering::Relaxed) {
        ctx.token = Some("mock-session".to_string());
        if let Ok(acct) = backend.active(state).validate_session("mock-session").await {
            ctx.account = Some(acct);
            ctx.state = MinerState::Idle;
            ctx.state_reason = Some("Ready (Mock mode)".to_string());
        }
        return;
    }

    match SecretStore::load_profile_token(platform_id, profile_id) {
        Ok(Some(secret)) => {
            let tok = secret.expose().to_string();
            match backend.active(state).validate_session(&tok).await {
                Ok(acct) => {
                    ctx.token = Some(tok);
                    ctx.account = Some(acct);
                    ctx.state = MinerState::Idle;
                    ctx.state_reason = Some("Loading campaigns...".to_string());
                    info!(
                        platform = platform_id.as_str(),
                        profile_id,
                        "Session restored from Credential Manager"
                    );
                }
                Err(AppError::Auth(_)) => {
                    let _ = SecretStore::delete_profile_token(platform_id, profile_id);
                    ctx.reset_session("Session expired; please log in again");
                }
                Err(err) => {
                    ctx.token = Some(tok);
                    ctx.state = MinerState::Idle;
                    ctx.state_reason = Some(err.user_message());
                }
            }
        }
        Ok(None) => {
            ctx.reset_session("Not logged in");
        }
        Err(err) => {
            warn!(
                platform = platform_id.as_str(),
                profile_id,
                error = %err,
                "Failed reading Credential Manager"
            );
            ctx.reset_session("Not logged in");
        }
    }
}

async fn handle_control(
    profile_id: &str,
    platform_id: PlatformId,
    app: &AppHandle,
    state: &AppState,
    backend: &PlatformBackend,
    ctx: &mut RuntimeContext,
    cmd: MinerControl,
) -> ControlOutcome {
    match cmd {
        MinerControl::Shutdown => {
            backend.active(state).stop_watching().await;
            close_compat_watch_window_for_profile(app, platform_id, profile_id);
            ctx.auto_compat_fallback = false;
            ctx.confirmed_progress_at = None;
            match platform_id {
                PlatformId::Twitch => close_twitch_bridge_window_for_profile(app, profile_id),
                PlatformId::Kick => close_kick_bridge_window_for_profile(app, profile_id),
            }
            return ControlOutcome::Shutdown;
        }
        MinerControl::LoginComplete => {
            ctx.paused = false;
            ctx.last_error = None;
            ctx.backoff.reset();
            load_initial_token(profile_id, platform_id, state, backend, ctx).await;
            ctx.last_campaign_fetch = None;
            ctx.retry_not_before = None;
            step_miner(profile_id, platform_id, app, state, backend, ctx).await;
        }
        MinerControl::Logout => {
            let plat = backend.active(state);
            let _ = plat.logout(ctx.token.as_deref()).await;
            ctx.paused = false;
            ctx.last_error = None;
            ctx.reset_session("Logged out");
            info!(platform = platform_id.as_str(), profile_id, "Logged out");
        }
        MinerControl::Pause => {
            backend.active(state).stop_watching().await;
            close_compat_watch_window_for_profile(app, platform_id, profile_id);
            if platform_id == PlatformId::Kick {
                close_kick_bridge_window_for_profile(app, profile_id);
            }
            ctx.paused = true;
            ctx.watching = None;
            ctx.watching_since = None;
            ctx.auto_compat_fallback = false;
            ctx.confirmed_progress_at = None;
            ctx.state = MinerState::Paused;
            ctx.state_reason = Some("Paused by user".to_string());
            ctx.next_action_at = None;
            info!(platform = platform_id.as_str(), profile_id, "Mining paused");
        }
        MinerControl::Resume => {
            ctx.paused = false;
            ctx.retry_not_before = None;
            ctx.no_live_channels_until.clear();
            ctx.avoid_channels_until.clear();
            ctx.backoff.reset();
            if ctx.token.is_some() {
                ctx.state = MinerState::Idle;
                ctx.state_reason = Some("Resuming...".to_string());
                step_miner(profile_id, platform_id, app, state, backend, ctx).await;
            }
            info!(platform = platform_id.as_str(), profile_id, "Mining resumed");
        }
        MinerControl::SwitchChannel => {
            clear_pinned_channel(state, profile_id, platform_id).await;
            if ctx.token.is_some() && !ctx.paused {
                if let Some(ref current) = ctx.watching {
                    let current_login = current.login.clone();
                    ctx.candidates.retain(|c| !c.login.eq_ignore_ascii_case(&current_login));
                    ctx.avoid_channels_until.insert(
                        current_login.to_ascii_lowercase(),
                        Instant::now() + Duration::from_secs(IDLE_RETRY_SECS),
                    );
                }
                ctx.watching = None;
                ctx.last_voluntary_switch = Some(Utc::now());
                step_miner(profile_id, platform_id, app, state, backend, ctx).await;
            }
        }
        MinerControl::PinChannel(pin) => {
            if let Some(ref pin_login) = pin {
                info!(
                    platform = platform_id.as_str(),
                    profile_id,
                    channel = %pin_login,
                    "Pinned channel updated"
                );
                ctx.avoid_channels_until
                    .remove(&pin_login.to_ascii_lowercase());
            }
            ctx.last_voluntary_switch = None;
            if ctx.token.is_some() && !ctx.paused {
                step_miner(profile_id, platform_id, app, state, backend, ctx).await;
            }
        }
        MinerControl::RetryNow => {
            ctx.retry_not_before = None;
            ctx.last_error = None;
            ctx.no_live_channels_until.clear();
            ctx.avoid_channels_until.clear();
            ctx.backoff.reset();
            backend.active(state).reset_progress_sync_cooldown();
            if ctx.token.is_some() && !ctx.paused {
                step_miner(profile_id, platform_id, app, state, backend, ctx).await;
            }
        }
        MinerControl::SettingsUpdated => {
            ctx.no_live_channels_until.clear();
            ctx.last_voluntary_switch = None;
            for camp in &mut ctx.campaigns {
                for drop in &mut camp.drops {
                    drop.needs_manual_claim = false;
                }
            }
            let is_mock = state.mock_active.load(Ordering::Relaxed);
            if is_mock && ctx.token.as_deref() != Some("mock-session") {
                ctx.reset_session("Switching to mock mode");
                load_initial_token(profile_id, platform_id, state, backend, ctx).await;
            } else if !is_mock && ctx.token.as_deref() == Some("mock-session") {
                ctx.reset_session("Switching to live mode");
                load_initial_token(profile_id, platform_id, state, backend, ctx).await;
            }
            if ctx.token.is_some() && !ctx.paused {
                step_miner(profile_id, platform_id, app, state, backend, ctx).await;
            }
        }
        MinerControl::RefreshNow => {
            ctx.last_campaign_fetch = None;
            ctx.last_progress_fetch = None;
            ctx.last_completion_bridge_reset = None;
            ctx.no_live_channels_until.clear();
            ctx.avoid_channels_until.clear();
            for camp in &mut ctx.campaigns {
                for drop in &mut camp.drops {
                    drop.needs_manual_claim = false;
                }
            }
            backend.active(state).reset_progress_sync_cooldown();
            if ctx.token.is_some() && !ctx.paused {
                step_miner(profile_id, platform_id, app, state, backend, ctx).await;
            }
        }
        MinerControl::CampaignDetailsLoaded(detailed) => {
            ctx.hydrated_campaign_ids.insert(detailed.id.clone());
            if let Some(camp) = ctx.campaigns.iter_mut().find(|c| c.id == detailed.id) {
                apply_campaign_details(camp, *detailed);
            }
            reconcile_claimed_benefits(&mut ctx.campaigns, &ctx.claimed_benefits);
            propagate_linked_games(&mut ctx.campaigns);
        }
    }
    ControlOutcome::Continue
}

fn compute_next_wake_delay(
    state: &AppState,
    ctx: &RuntimeContext,
    humanized_jitter: bool,
) -> Duration {
    if ctx.token.is_none() || ctx.paused {
        return Duration::from_secs(30);
    }

    if let Some(not_before) = ctx.retry_not_before {
        let now = Instant::now();
        if not_before > now {
            return (not_before - now).min(Duration::from_secs(15));
        }
        return Duration::from_millis(200);
    }

    if ctx.last_campaign_fetch.is_none() {
        return Duration::from_millis(250);
    }

    if ctx.watching.is_some() {
        let tick = if state.mock_active.load(Ordering::Relaxed) {
            MOCK_WATCH_TICK_SECS
        } else {
            WATCH_TICK_SECS
        };
        return jittered_duration_with_mode(tick, humanized_jitter).min(Duration::from_secs(75));
    }

    Duration::from_secs(10)
}

async fn step_miner(
    profile_id: &str,
    platform_id: PlatformId,
    app: &AppHandle,
    state: &AppState,
    backend: &PlatformBackend,
    ctx: &mut RuntimeContext,
) {
    let settings = state.settings.read().await.clone();
    let plat_settings = settings
        .profile_settings(profile_id)
        .cloned()
        .unwrap_or_else(|| match platform_id {
            PlatformId::Twitch => settings.twitch.clone(),
            PlatformId::Kick => settings.kick.clone(),
        });

    if !plat_settings.enabled {
        if ctx.watching.is_some() || ctx.auto_compat_fallback {
            backend.active(state).stop_watching().await;
            close_compat_watch_window_for_profile(app, platform_id, profile_id);
        }
        ctx.watching = None;
        ctx.watching_since = None;
        ctx.auto_compat_fallback = false;
        ctx.confirmed_progress_at = None;
        ctx.state = MinerState::Paused;
        ctx.state_reason = Some("Platform disabled in settings".to_string());
        return;
    }

    if !settings.disclaimer_accepted() {
        ctx.watching = None;
        ctx.state = MinerState::Idle;
        ctx.state_reason = Some("Accept disclaimer to start mining".to_string());
        return;
    }

    if ctx.paused {
        ctx.state = MinerState::Paused;
        return;
    }

    let remote_cfg = state.remote_config.current().await;
    let platform_remotely_enabled = match platform_id {
        PlatformId::Twitch => remote_cfg.twitch_enabled,
        PlatformId::Kick => remote_cfg.kick_enabled,
    };
    if !platform_remotely_enabled && !state.mock_active.load(Ordering::Relaxed) {
        if ctx.watching.is_some() {
            backend.active(state).stop_watching().await;
            close_compat_watch_window_for_profile(app, platform_id, profile_id);
            ctx.watching = None;
            ctx.watching_since = None;
        }
        ctx.state = MinerState::Paused;
        ctx.state_reason = Some(
            remote_cfg
                .maintenance_banner
                .unwrap_or_else(|| format!("{} temporarily paused for maintenance", platform_id.display_name())),
        );
        return;
    }

    if is_in_quiet_hours_now(&settings.schedule) {
        if ctx.watching.is_some() {
            backend.active(state).stop_watching().await;
            close_compat_watch_window_for_profile(app, platform_id, profile_id);
            if platform_id == PlatformId::Kick {
                close_kick_bridge_window_for_profile(app, profile_id);
            } else {
                close_twitch_bridge_window_for_profile(app, profile_id);
            }
            ctx.watching = None;
            ctx.watching_since = None;
            ctx.auto_compat_fallback = false;
            ctx.confirmed_progress_at = None;
            ctx.current_campaign_id = None;
            ctx.current_drop_id = None;
        }
        let until_end =
            duration_until_quiet_end(&settings.schedule, local_minutes_since_midnight());
        ctx.state = MinerState::Paused;
        ctx.state_reason = Some(format!(
            "Quiet hours until {}",
            format_minutes_hhmm(settings.schedule.quiet_end_minutes)
        ));
        ctx.next_action_at =
            Some(Utc::now() + ChronoDuration::from_std(until_end).unwrap_or_default());
        return;
    }

    let Some(token) = ctx.token.clone() else {
        ctx.state = MinerState::LoggedOut;
        let should_fetch_public = ctx
            .last_campaign_fetch
            .map(|t| t.elapsed() >= Duration::from_secs(30 * 60))
            .unwrap_or(true);
        if should_fetch_public {
            let plat = backend.active(state).clone();
            if let Ok(list) = plat.fetch_public_campaigns().await {
                if !list.is_empty() {
                    ctx.campaigns = list;
                }
            }
            ctx.last_campaign_fetch = Some(Instant::now());
        }
        return;
    };

    if let Some(not_before) = ctx.retry_not_before {
        if Instant::now() < not_before {
            return;
        }
        ctx.retry_not_before = None;
    }

    let plat = backend.active(state).clone();
    let is_mock = state.mock_active.load(Ordering::Relaxed);

    // 1. Validate session if account info missing.
    if ctx.account.is_none() {
        match plat.validate_session(&token).await {
            Ok(acct) => {
                ctx.account = Some(acct);
                ctx.backoff.reset();
            }
            Err(err) => {
                handle_platform_error(profile_id, platform_id, app, state, ctx, err).await;
                return;
            }
        }
    }

    // 2. Refresh campaigns periodically.
    let refresh_secs = (settings.refresh_minutes as u64 * 60).max(MIN_CAMPAIGN_REFRESH_SECS);
    let need_campaigns = ctx
        .last_campaign_fetch
        .map(|t| t.elapsed() >= Duration::from_secs(refresh_secs))
        .unwrap_or(true);

    if need_campaigns {
        match plat.fetch_campaigns(&token).await {
            Ok(mut fetched) => {
                for next_c in &mut fetched {
                    if let Some(prev_c) = ctx.campaigns.iter().find(|old| old.id == next_c.id) {
                        if prev_c.linked && prev_c.link_verified && !next_c.linked {
                            next_c.linked = true;
                            next_c.link_verified = true;
                        }
                        if (next_c.drops.is_empty() && !prev_c.drops.is_empty())
                            || (is_kick_fallback_case_id(&next_c.id)
                                && prev_c.ends_at == next_c.ends_at)
                        {
                            apply_campaign_details(next_c, prev_c.clone());
                        }
                    }
                    if next_c.drops.is_empty() {
                        if let Some(cached_drops) = plat.cached_campaign_drops(&next_c.id) {
                            next_c.drops = cached_drops;
                        }
                    }
                }
                reconcile_claimed_benefits(&mut fetched, &ctx.claimed_benefits);
                propagate_linked_games(&mut fetched);
                ctx.campaigns = fetched;
                ctx.last_campaign_fetch = Some(Instant::now());
                ctx.backoff.reset();
            }
            Err(err) => {
                handle_platform_error(profile_id, platform_id, app, state, ctx, err).await;
                return;
            }
        }
    }

    // 3. Refresh progress periodically (more frequently while actively watching or in mock mode).
    let progress_secs = if is_mock {
        6
    } else if ctx.watching.is_some() {
        ACTIVE_PROGRESS_POLL_SECS
    } else {
        PROGRESS_POLL_SECS
    };
    let need_progress = ctx
        .last_progress_fetch
        .map(|t| t.elapsed() >= Duration::from_secs(progress_secs))
        .unwrap_or(true);

    if need_progress {
        match plat.fetch_progress(&token).await {
            Ok(progress_list) => {
                if !progress_list.is_empty() {
                    merge_claimed_benefits(&mut ctx.claimed_benefits, &progress_list);
                    apply_progress(&mut ctx.campaigns, &progress_list);
                    reconcile_claimed_benefits(&mut ctx.campaigns, &ctx.claimed_benefits);
                    ctx.progress_synced_at = Some(Utc::now());
                } else if ctx.watching.is_some() || ctx.progress_synced_at.is_none() {
                    // Always re-anchor sync timestamp when watching so UI interpolation does not
                    // drift endlessly upward when the platform has not yet credited minutes.
                    ctx.progress_synced_at = Some(Utc::now());
                }
                ctx.last_progress_fetch = Some(Instant::now());
                ctx.backoff.reset();

                let total_mins = total_watched_minutes(&ctx.campaigns);
                if ctx.progress_baseline_initialized && total_mins > ctx.last_seen_total_minutes {
                    let delta = total_mins - ctx.last_seen_total_minutes;
                    state.record_watch_minutes(delta).await;
                    ctx.last_seen_total_minutes = total_mins;
                    ctx.last_progress_advance = Utc::now();
                    ctx.confirmed_progress_at = Some(Utc::now());
                } else if !ctx.progress_baseline_initialized {
                    ctx.last_seen_total_minutes = total_mins;
                    ctx.progress_baseline_initialized = true;
                } else if total_mins < ctx.last_seen_total_minutes {
                    // Campaign expired/removed; reset high-water mark so future deltas still count.
                    ctx.last_seen_total_minutes = total_mins;
                } else if let Some(ref ch) = ctx.watching {
                    let stalled_secs = (Utc::now() - ctx.last_progress_advance).num_seconds();
                    if !is_mock && stalled_secs >= 4 * 60 {
                        warn!(
                            platform = platform_id.as_str(),
                            channel = %ch.login,
                            elapsed_secs = stalled_secs,
                            "Platform has not confirmed any drop progress yet while watching"
                        );
                    }
                }
            }
            Err(err) => {
                handle_platform_error(profile_id, platform_id, app, state, ctx, err).await;
                return;
            }
        }
    }

    // Prune any campaigns whose end time has passed so ended drops disappear immediately
    // and never trigger stale manual-claim banners.
    let now = Utc::now();
    ctx.campaigns
        .retain(|c| c.status != CampaignStatus::Expired && c.ends_at > now);
    if let Some(ref cur_id) = ctx.current_campaign_id {
        if !ctx.campaigns.iter().any(|c| &c.id == cur_id) {
            plat.stop_watching().await;
            close_compat_watch_window_for_profile(app, platform_id, profile_id);
            ctx.watching = None;
            ctx.watching_since = None;
            ctx.current_campaign_id = None;
            ctx.current_drop_id = None;
            ctx.candidates.clear();
        }
    }

    // 4. Auto-claim any claimable drops.
    if settings.auto_claim {
        claim_ready_drops(
            profile_id,
            platform_id,
            app,
            state,
            &plat,
            &token,
            ctx,
            settings.notifications,
        )
        .await;
    }

    // 5. Build ordered priority queue of eligible campaigns.
    let now = Utc::now();
    ctx.prune_cooldowns();
    let ordered_queue: Vec<Campaign> =
        ordered_target_campaigns(&ctx.campaigns, &settings, &plat_settings, now)
            .into_iter()
            .cloned()
            .collect();

    if ordered_queue.is_empty() {
        plat.stop_watching().await;
        close_compat_watch_window_for_profile(app, platform_id, profile_id);
        ctx.watching = None;
        ctx.watching_since = None;
        ctx.auto_compat_fallback = false;
        ctx.confirmed_progress_at = None;
        ctx.current_campaign_id = None;
        ctx.current_drop_id = None;
        ctx.state = MinerState::Idle;
        ctx.state_reason = Some(
            "No eligible campaigns selected or active; checking again later".to_string(),
        );
        let next_dur = Duration::from_secs(IDLE_RETRY_SECS);
        ctx.retry_not_before = Some(Instant::now() + next_dur);
        ctx.next_action_at = Some(Utc::now() + ChronoDuration::from_std(next_dur).unwrap_or_default());
        return;
    }

    // 6. Check current watching state & switch rules.
    if let Some(mut current_ch) = ctx.watching.clone() {
        if should_escalate_to_compat_player(
            platform_id,
            is_mock,
            settings.compat_watch_mode,
            ctx.auto_compat_fallback,
            ctx.watching_since,
            ctx.confirmed_progress_at,
            now,
        ) {
            ctx.auto_compat_fallback = true;
            ctx.last_progress_advance = now;
            ctx.last_watch_tick = None;
            plat.reset_progress_sync_cooldown();
            info!(
                platform = platform_id.as_str(),
                profile_id,
                channel = %current_ch.login,
                "Twitch lightweight watcher saw 0 confirmed drop progress after 3m; switching to hidden muted Compatibility Player (160p30) for reliable drop credit"
            );
        }

        let current_camp = ctx
            .current_campaign_id
            .as_deref()
            .and_then(|id| ctx.campaigns.iter().find(|c| c.id == id))
            .cloned()
            .unwrap_or_else(|| ordered_queue[0].clone());

        let need_live_check = ctx
            .last_live_check
            .map(|t| t.elapsed() >= Duration::from_secs(if is_mock { 10 } else { LIVE_CHECK_SECS }))
            .unwrap_or(true);

        if need_live_check {
            match plat.check_live(&token, &current_ch).await {
                Ok(mut updated) => {
                    if updated.is_live {
                        if let Ok(false) = plat
                            .is_channel_participating(&token, &updated, &current_camp)
                            .await
                        {
                            info!(
                                platform = platform_id.as_str(),
                                profile_id,
                                channel = %updated.login,
                                campaign = %current_camp.name,
                                "Channel is no longer participating in active drop campaign"
                            );
                            updated.drops_enabled = false;
                        }
                    }
                    current_ch = updated;
                    ctx.watching = Some(current_ch.clone());
                    ctx.last_live_check = Some(Instant::now());
                }
                Err(AppError::NotFound(_)) => {
                    current_ch.is_live = false;
                }
                Err(err) => {
                    handle_platform_error(profile_id, platform_id, app, state, ctx, err).await;
                    return;
                }
            }
        }

        // Only consider switching up to a higher-priority campaign if it is not currently
        // in a no-live-channels cooldown.
        let current_rank = ordered_queue
            .iter()
            .position(|c| c.id == current_camp.id);
        let higher_priority = ordered_queue
            .iter()
            .take(current_rank.unwrap_or(ordered_queue.len()))
            .find(|c| !ctx.no_live_channels_until.contains_key(&c.id));

        let top_for_switch = higher_priority.unwrap_or(&current_camp);

        let pinned_if_live = plat_settings.pinned_channel.as_deref().and_then(|pin| {
            ctx.candidates
                .iter()
                .any(|c| c.is_live && c.drops_enabled && c.login.eq_ignore_ascii_case(pin))
                .then_some(pin)
        });

        let decision = evaluate_switch(SwitchContext {
            current_channel: &current_ch,
            current_campaign: &current_camp,
            top_campaign: Some(top_for_switch),
            pinned_login: pinned_if_live,
            now,
            last_progress_advance: ctx.last_progress_advance,
            last_voluntary_switch: ctx.last_voluntary_switch,
        });

        match decision {
            SwitchDecision::Keep => {}
            SwitchDecision::SwitchImmediate(reason) => {
                info!(platform = platform_id.as_str(), reason = %reason, "Switching channel immediately");
                if reason.contains("rotating channel") {
                    ctx.avoid_channels_until.insert(
                        current_ch.login.to_ascii_lowercase(),
                        Instant::now() + Duration::from_secs(IDLE_RETRY_SECS),
                    );
                }
                ctx.watching = None;
                ctx.candidates.clear();
            }
            SwitchDecision::SwitchVoluntary(reason) => {
                if let Some(hi_camp) = higher_priority {
                    if hi_camp.id != current_camp.id {
                        // Probe the higher-priority campaign BEFORE dropping the active stream
                        // so we never interrupt farming if the higher-priority drop has no live channels.
                        match plat.find_candidate_channels(&token, hi_camp).await {
                            Ok(candidates) => {
                                // Switching to a different campaign always resets channel selection to Auto.
                                if let Some(best) = select_best_channel(hi_camp, &candidates, None).cloned() {
                                    clear_pinned_channel(state, profile_id, platform_id).await;
                                    info!(
                                        platform = platform_id.as_str(),
                                        reason = %reason,
                                        channel = %best.login,
                                        game = %hi_camp.game.name,
                                        "Switching to higher-priority participating stream"
                                    );
                                    ctx.candidates = candidates;
                                    ctx.watching = Some(best);
                                    ctx.watching_since = Some(now);
                                    ctx.current_campaign_id = Some(hi_camp.id.clone());
                                    ctx.current_drop_id =
                                        active_drop_for(hi_camp).map(|d| d.id.clone());
                                    ctx.last_live_check = Some(Instant::now());
                                    ctx.last_watch_tick = None;
                                    ctx.last_progress_advance = now;
                                    ctx.last_voluntary_switch = Some(now);
                                } else {
                                    info!(
                                        platform = platform_id.as_str(),
                                        campaign = %hi_camp.name,
                                        current = %current_camp.name,
                                        "Higher-priority campaign has no live participating channels; continuing current drop"
                                    );
                                    ctx.no_live_channels_until.insert(
                                        hi_camp.id.clone(),
                                        Instant::now() + Duration::from_secs(IDLE_RETRY_SECS),
                                    );
                                }
                            }
                            Err(AppError::NoCandidates(_)) => {
                                ctx.no_live_channels_until.insert(
                                    hi_camp.id.clone(),
                                    Instant::now() + Duration::from_secs(IDLE_RETRY_SECS),
                                );
                            }
                            Err(_) => {
                                // Transient probe failure: keep watching current stream.
                                ctx.no_live_channels_until.insert(
                                    hi_camp.id.clone(),
                                    Instant::now() + Duration::from_secs(120),
                                );
                            }
                        }
                    } else {
                        info!(platform = platform_id.as_str(), reason = %reason, "Switching channel");
                        ctx.watching = None;
                        ctx.candidates.clear();
                        ctx.last_voluntary_switch = Some(now);
                    }
                } else {
                    info!(platform = platform_id.as_str(), reason = %reason, "Switching channel");
                    ctx.watching = None;
                    ctx.candidates.clear();
                    ctx.last_voluntary_switch = Some(now);
                }
            }
        }
    }

    // 7. Find a participating live channel across the ordered priority queue.
    if ctx.watching.is_none() {
        let non_cooldown: Vec<&Campaign> = ordered_queue
            .iter()
            .filter(|c| !ctx.no_live_channels_until.contains_key(&c.id))
            .collect();
        let try_order: Vec<&Campaign> = if non_cooldown.is_empty() {
            ordered_queue.iter().collect()
        } else {
            non_cooldown
        };

        let mut last_empty_name: Option<String> = None;

        for cand_camp in &try_order {
            ctx.state = MinerState::Selecting;
            ctx.state_reason = Some(format!(
                "Finding participating stream for {} ({})",
                cand_camp.name, cand_camp.game.name
            ));
            sync_snapshot(app, state, profile_id, platform_id, ctx, false).await;

            match plat.find_candidate_channels(&token, cand_camp).await {
                Ok(candidates) => {
                    let non_avoided: Vec<Channel> = candidates
                        .iter()
                        .filter(|c| {
                            !ctx.avoid_channels_until
                                .contains_key(&c.login.to_ascii_lowercase())
                        })
                        .cloned()
                        .collect();
                    let pool = if non_avoided.is_empty() {
                        &candidates
                    } else {
                        &non_avoided
                    };

                    let same_campaign = ctx
                        .current_campaign_id
                        .as_deref()
                        .map(|id| id == cand_camp.id)
                        .unwrap_or(false);
                    let effective_pin = if same_campaign {
                        plat_settings.pinned_channel.as_deref()
                    } else {
                        None
                    };

                    if let Some(best) = select_best_channel(cand_camp, pool, effective_pin).cloned()
                    {
                        if !same_campaign {
                            clear_pinned_channel(state, profile_id, platform_id).await;
                        }
                        info!(
                            platform = platform_id.as_str(),
                            channel = %best.login,
                            campaign = %cand_camp.name,
                            game = %cand_camp.game.name,
                            "Selected participating stream"
                        );
                        ctx.candidates = candidates;
                        ctx.watching = Some(best);
                        ctx.watching_since = Some(Utc::now());
                        ctx.current_campaign_id = Some(cand_camp.id.clone());
                        // Hydrate drop rewards & timers discovered during channel verification
                        // so Dashboard & NowWatching immediately show every reward & total ETA.
                        if let Some(cached_drops) = plat.cached_campaign_drops(&cand_camp.id) {
                            if let Some(target_c) =
                                ctx.campaigns.iter_mut().find(|c| c.id == cand_camp.id)
                            {
                                if target_c.drops.is_empty() {
                                    target_c.drops = cached_drops;
                                }
                            }
                        }
                        let active_camp_ref = ctx
                            .campaigns
                            .iter()
                            .find(|c| c.id == cand_camp.id)
                            .unwrap_or(cand_camp);
                        ctx.current_drop_id = active_drop_for(active_camp_ref).map(|d| d.id.clone());
                        ctx.no_live_channels_until.remove(&cand_camp.id);
                        ctx.last_live_check = Some(Instant::now());
                        ctx.last_watch_tick = None;
                        ctx.last_progress_advance = Utc::now();
                        break;
                    } else {
                        info!(
                            platform = platform_id.as_str(),
                            campaign = %cand_camp.name,
                            game = %cand_camp.game.name,
                            "No live participating channels for campaign; trying next in priority list"
                        );
                        ctx.no_live_channels_until.insert(
                            cand_camp.id.clone(),
                            Instant::now() + Duration::from_secs(IDLE_RETRY_SECS),
                        );
                        last_empty_name = Some(cand_camp.game.name.clone());
                    }
                }
                Err(AppError::NoCandidates(_)) => {
                    info!(
                        platform = platform_id.as_str(),
                        campaign = %cand_camp.name,
                        game = %cand_camp.game.name,
                        "No live participating channels for campaign; trying next in priority list"
                    );
                    ctx.no_live_channels_until.insert(
                        cand_camp.id.clone(),
                        Instant::now() + Duration::from_secs(IDLE_RETRY_SECS),
                    );
                    last_empty_name = Some(cand_camp.game.name.clone());
                }
                Err(err) => {
                    handle_platform_error(profile_id, platform_id, app, state, ctx, err).await;
                    return;
                }
            }
        }

        if ctx.watching.is_none() {
            plat.stop_watching().await;
            close_compat_watch_window_for_profile(app, platform_id, profile_id);
            ctx.watching_since = None;
            ctx.auto_compat_fallback = false;
            ctx.confirmed_progress_at = None;
            ctx.current_campaign_id = None;
            ctx.current_drop_id = None;
            ctx.candidates.clear();
            ctx.state = MinerState::Idle;
            let label = if try_order.len() > 1 {
                "any prioritized drop".to_string()
            } else {
                last_empty_name.unwrap_or_else(|| ordered_queue[0].game.name.clone())
            };
            ctx.state_reason = Some(format!(
                "No live participating channels for {label}, retrying in 5 min"
            ));
            let next_dur = jittered_duration(IDLE_RETRY_SECS);
            ctx.retry_not_before = Some(Instant::now() + next_dur);
            ctx.next_action_at =
                Some(Utc::now() + ChronoDuration::from_std(next_dur).unwrap_or_default());
            return;
        }
    }

    // 8. Perform watch tick on the selected stream.
    if let Some(ref channel) = ctx.watching.clone() {
        let active_camp = ctx
            .current_campaign_id
            .as_deref()
            .and_then(|id| ctx.campaigns.iter().find(|c| c.id == id))
            .cloned()
            .unwrap_or_else(|| ordered_queue[0].clone());

        let tick_interval = if is_mock {
            Duration::from_secs(MOCK_WATCH_TICK_SECS)
        } else {
            Duration::from_secs(WATCH_TICK_SECS)
        };
        let should_tick = ctx
            .last_watch_tick
            .map(|t| t.elapsed() >= tick_interval)
            .unwrap_or(true);

        let effective_compat_mode = settings.compat_watch_mode || ctx.auto_compat_fallback;

        ctx.state = MinerState::Mining;
        ctx.current_campaign_id = Some(active_camp.id.clone());
        ctx.current_drop_id = active_drop_for(&active_camp).map(|d| d.id.clone());
        ctx.state_reason = Some(if effective_compat_mode && platform_id == PlatformId::Twitch {
            format!(
                "Watching {} ({}) [Compatibility Player]",
                channel.display_name, active_camp.game.name
            )
        } else {
            format!(
                "Watching {} ({})",
                channel.display_name, active_camp.game.name
            )
        });
        ctx.last_error = None;
        ctx.next_action_at =
            Some(Utc::now() + ChronoDuration::from_std(tick_interval).unwrap_or_default());

        if should_tick {
            match plat
                .watch_tick(&token, channel, effective_compat_mode)
                .await
            {
                Ok(WatchTickResult::Watched) => {
                    ctx.last_watch_tick = Some(Instant::now());
                    ctx.backoff.reset();

                    // For fallback synthetic Kick reward cases (when Kick's gamification API
                    // is not providing live progress directly), watching ANY Kick stream progresses
                    // all active daily/weekly/sunset cases concurrently.
                    if platform_id == PlatformId::Kick {
                        let mut advanced_fallback = false;
                        for camp_mut in ctx
                            .campaigns
                            .iter_mut()
                            .filter(|c| is_kick_fallback_case_id(&c.id))
                        {
                            if let Some(drop_mut) = camp_mut.drops.first_mut() {
                                if !drop_mut.claimed
                                    && drop_mut.current_minutes < drop_mut.required_minutes
                                {
                                    drop_mut.current_minutes += 1;
                                    advanced_fallback = true;
                                    if drop_mut.current_minutes >= drop_mut.required_minutes {
                                        drop_mut.claimable = true;
                                    }
                                }
                            }
                        }
                        if advanced_fallback {
                            ctx.progress_synced_at = Some(Utc::now());
                            if is_kick_fallback_case_id(&active_camp.id) {
                                ctx.last_progress_advance = Utc::now();
                            }
                        }
                    }

                    // If a drop is right at completion threshold, fetch progress right away on next cycle
                    // (throttled to at most once every 2 minutes so unlinked drops never spam the bridge).
                    if let Some(drop) = active_drop_for(&active_camp) {
                        if drop.current_minutes + 1 >= drop.required_minutes
                            && ctx
                                .last_completion_bridge_reset
                                .map(|t| t.elapsed() >= Duration::from_secs(120))
                                .unwrap_or(true)
                        {
                            ctx.last_completion_bridge_reset = Some(Instant::now());
                            plat.reset_progress_sync_cooldown();
                            ctx.last_progress_fetch = None;
                        }
                    }
                }
                Ok(WatchTickResult::StreamOffline) => {
                    info!(
                        platform = platform_id.as_str(),
                        channel = %channel.login,
                        "Watch tick detected stream offline"
                    );
                    ctx.watching = None;
                    ctx.candidates.clear();
                }
                Err(err) => {
                    handle_platform_error(profile_id, platform_id, app, state, ctx, err).await;
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn claim_ready_drops(
    profile_id: &str,
    platform_id: PlatformId,
    app: &AppHandle,
    state: &AppState,
    plat: &Arc<dyn Platform>,
    token: &str,
    ctx: &mut RuntimeContext,
    notify_enabled: bool,
) {
    let mut to_claim: Vec<(String, String, String, Drop)> = Vec::new();
    for camp in &ctx.campaigns {
        for drop in &camp.drops {
            if drop.claimable && !drop.claimed && !drop.needs_manual_claim {
                to_claim.push((
                    camp.id.clone(),
                    camp.game.name.clone(),
                    camp.name.clone(),
                    drop.clone(),
                ));
            }
        }
    }

    if to_claim.is_empty() {
        return;
    }

    ctx.state = MinerState::Claiming;
    sync_snapshot(app, state, profile_id, platform_id, ctx, false).await;

    for (camp_id, game_name, camp_name, drop) in to_claim {
        let Some(camp_clone) = ctx.campaigns.iter().find(|c| c.id == camp_id).cloned() else {
            continue;
        };
        let display_drop_name = drop
            .rewards
            .first()
            .map(|r| r.name.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or(&drop.name)
            .to_string();
        let claim_id = if is_default_profile(profile_id) {
            format!("{}-{}", camp_id, drop.id)
        } else {
            format!("{}-{}-{}", profile_id, camp_id, drop.id)
        };
        match plat.claim(token, &camp_clone, &drop).await {
            Ok(claim_res) => {
                let final_drop_name = match claim_res
                    .reward_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    Some(won) if !won.eq_ignore_ascii_case(&display_drop_name) => {
                        format!("{display_drop_name} — {won}")
                    }
                    _ => display_drop_name.clone(),
                };
                let final_reward_img = claim_res
                    .reward_image_url
                    .clone()
                    .or_else(|| drop.rewards.first().and_then(|r| r.image_url.clone()));

                info!(
                    platform = platform_id.as_str(),
                    profile_id,
                    drop = %final_drop_name,
                    game = %game_name,
                    "Claimed drop"
                );
                let mut campaign_now_complete = false;
                let mut campaign_drops_count = 0usize;
                if let Some(camp) = ctx.campaigns.iter_mut().find(|c| c.id == camp_id) {
                    if let Some(d) = camp.drops.iter_mut().find(|d| d.id == drop.id) {
                        d.claimed = true;
                        d.claimable = false;
                        d.needs_manual_claim = false;
                        if final_drop_name != display_drop_name {
                            d.name = final_drop_name.clone();
                            if let Some(first_reward) = d.rewards.first_mut() {
                                first_reward.name = final_drop_name.clone();
                                if let Some(ref img) = claim_res.reward_image_url {
                                    first_reward.image_url = Some(img.clone());
                                }
                            } else {
                                d.rewards.push(crate::platforms::Reward {
                                    id: None,
                                    name: final_drop_name.clone(),
                                    image_url: final_reward_img.clone(),
                                });
                            }
                        } else if let Some(ref img) = claim_res.reward_image_url {
                            if let Some(first_reward) = d.rewards.first_mut() {
                                first_reward.image_url = Some(img.clone());
                            }
                        }
                    }
                    campaign_drops_count = camp.drops.len();
                    campaign_now_complete =
                        !camp.drops.is_empty() && camp.drops.iter().all(|d| d.claimed);
                }
                state
                    .record_claim(RecentClaim {
                        id: claim_id,
                        platform: platform_id,
                        game_name: game_name.clone(),
                        campaign_name: camp_name.clone(),
                        drop_name: final_drop_name.clone(),
                        reward_image_url: final_reward_img.clone(),
                        claimed_at: Utc::now(),
                    })
                    .await;

                state
                    .dispatch_webhook(WebhookEvent::DropClaimed {
                        platform: platform_id,
                        game_name: game_name.clone(),
                        campaign_name: camp_name.clone(),
                        drop_name: final_drop_name.clone(),
                        reward_image_url: final_reward_img,
                    })
                    .await;

                if campaign_now_complete {
                    state
                        .dispatch_webhook(WebhookEvent::CampaignCompleted {
                            platform: platform_id,
                            game_name: game_name.clone(),
                            campaign_name: camp_name.clone(),
                            drops_count: campaign_drops_count,
                        })
                        .await;
                }

                if notify_enabled {
                    send_toast(
                        app,
                        &format!("{} Drop Claimed", platform_id.display_name()),
                        &format!("{} ({})", final_drop_name, game_name),
                    );
                }
            }
            Err(AppError::Blocked(msg)) => {
                warn!(
                    platform = platform_id.as_str(),
                    profile_id,
                    drop = %display_drop_name,
                    reason = %msg,
                    "Claim needs manual action in official window"
                );
                if let Some(camp) = ctx.campaigns.iter_mut().find(|c| c.id == camp_id) {
                    if let Some(d) = camp.drops.iter_mut().find(|d| d.id == drop.id) {
                        d.needs_manual_claim = true;
                    }
                }
                let reward_img = drop.rewards.first().and_then(|r| r.image_url.clone());
                state
                    .record_manual_claim(ClaimHistoryEntry {
                        id: claim_id,
                        platform: platform_id.into(),
                        game_name: game_name.clone(),
                        campaign_name: camp_name.clone(),
                        drop_name: display_drop_name.clone(),
                        reward_image_url: reward_img,
                        claimed_at: Utc::now(),
                        status: ClaimRecordStatus::ManualRequired,
                    })
                    .await;
                state
                    .dispatch_webhook(WebhookEvent::ActionRequired {
                        platform: platform_id,
                        category: format!("manual_claim:{}", drop.id),
                        title: format!("Manual claim required for {}", display_drop_name),
                        detail: format!(
                            "{} requested manual verification on the {} drops inventory page ({}).",
                            platform_id.display_name(),
                            platform_id.display_name(),
                            game_name
                        ),
                    })
                    .await;
                send_toast(
                    app,
                    "Claim needs you",
                    &format!(
                        "{} requires manual claim on {}'s drops page",
                        display_drop_name,
                        platform_id.display_name()
                    ),
                );
            }
            Err(err) => {
                warn!(
                    platform = platform_id.as_str(),
                    profile_id,
                    drop = %display_drop_name,
                    error = %err,
                    "Claim attempt failed"
                );
            }
        }
    }
    sync_snapshot(app, state, profile_id, platform_id, ctx, true).await;
}

async fn handle_platform_error(
    profile_id: &str,
    platform_id: PlatformId,
    app: &AppHandle,
    state: &AppState,
    ctx: &mut RuntimeContext,
    err: AppError,
) {
    match err {
        AppError::Auth(msg) => {
            warn!(
                platform = platform_id.as_str(),
                profile_id,
                reason = %msg,
                "Authentication invalid or expired"
            );
            if !state.mock_active.load(Ordering::Relaxed) {
                let _ = SecretStore::delete_profile_token(platform_id, profile_id);
            }
            ctx.reset_session("Session expired; please log in again");
            ctx.last_error = Some(format!("Session expired ({msg})"));
            state
                .dispatch_webhook(WebhookEvent::ActionRequired {
                    platform: platform_id,
                    category: "session_expired".to_string(),
                    title: format!("{} session expired", platform_id.display_name()),
                    detail: "Open Dropilot and log in again to resume farming.".to_string(),
                })
                .await;
            send_toast(
                app,
                "Login required",
                &format!("Please log in to {} again", platform_id.display_name()),
            );
        }
        AppError::Blocked(msg) => {
            warn!(platform = platform_id.as_str(), reason = %msg, "Blocked by platform check");
            ctx.watching = None;
            ctx.state = MinerState::Error;
            ctx.state_reason = Some("Platform security check requires your action".to_string());
            ctx.last_error = Some(msg.clone());
            let dur = Duration::from_secs(ERROR_RETRY_SECS);
            ctx.retry_not_before = Some(Instant::now() + dur);
            ctx.next_action_at = Some(Utc::now() + ChronoDuration::from_std(dur).unwrap_or_default());
            state
                .dispatch_webhook(WebhookEvent::ActionRequired {
                    platform: platform_id,
                    category: "security_check".to_string(),
                    title: format!("{} security check required", platform_id.display_name()),
                    detail: msg,
                })
                .await;
            send_toast(
                app,
                &format!("{} action required", platform_id.display_name()),
                "Please verify in the official window",
            );
        }
        AppError::PlatformChanged(detail) => {
            error!(platform = platform_id.as_str(), detail = %detail, "Platform endpoint/schema changed");
            if state.settings.read().await.remote_config_enabled {
                let rc = state.remote_config.clone();
                let http = state.http.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = rc.refresh_now(&http).await;
                });
            }
            ctx.watching = None;
            ctx.state = MinerState::Error;
            ctx.state_reason = Some(format!(
                "{} changed something; an update may be needed",
                platform_id.display_name()
            ));
            ctx.last_error = Some(detail);
            let dur = Duration::from_secs(ERROR_RETRY_SECS);
            ctx.retry_not_before = Some(Instant::now() + dur);
            ctx.next_action_at = Some(Utc::now() + ChronoDuration::from_std(dur).unwrap_or_default());
        }
        AppError::RateLimited { retry_after_secs, message } => {
            let delay = ctx.backoff.record_failure(retry_after_secs);
            warn!(
                platform = platform_id.as_str(),
                delay_secs = delay.as_secs(),
                detail = %message,
                "Rate limited; backing off"
            );
            apply_backoff_or_error(ctx, delay, message);
        }
        AppError::NoCandidates(msg) | AppError::NotFound(msg) => {
            ctx.watching = None;
            ctx.state = MinerState::Idle;
            ctx.state_reason = Some(format!("{msg}; retrying in 5 min"));
            let dur = jittered_duration(IDLE_RETRY_SECS);
            ctx.retry_not_before = Some(Instant::now() + dur);
            ctx.next_action_at = Some(Utc::now() + ChronoDuration::from_std(dur).unwrap_or_default());
        }
        other => {
            let msg = other.user_message();
            let delay = ctx.backoff.record_failure(None);
            warn!(
                platform = platform_id.as_str(),
                failures = ctx.backoff.failures(),
                delay_secs = delay.as_secs(),
                error = %msg,
                "Platform request failed"
            );
            apply_backoff_or_error(ctx, delay, msg);
        }
    }
}

fn apply_backoff_or_error(ctx: &mut RuntimeContext, delay: Duration, message: String) {
    if ctx.backoff.exceeded_threshold() {
        ctx.watching = None;
        ctx.state = MinerState::Error;
        ctx.state_reason = Some("Repeated failures; automatic retry in 15 min".to_string());
        ctx.last_error = Some(message);
        let dur = Duration::from_secs(ERROR_RETRY_SECS);
        ctx.retry_not_before = Some(Instant::now() + dur);
        ctx.next_action_at = Some(Utc::now() + ChronoDuration::from_std(dur).unwrap_or_default());
    } else {
        ctx.state = MinerState::Idle;
        ctx.state_reason = Some(format!("{message}; retrying in {}s", delay.as_secs()));
        ctx.last_error = Some(message);
        ctx.retry_not_before = Some(Instant::now() + delay);
        ctx.next_action_at = Some(Utc::now() + ChronoDuration::from_std(delay).unwrap_or_default());
    }
}

fn publisher_link_host(link_url: Option<&str>) -> Option<String> {
    let raw = link_url?.trim();
    if raw.is_empty() {
        return None;
    }
    let host = url::Url::parse(raw).ok()?.host_str()?.to_ascii_lowercase();
    if host.ends_with("twitch.tv") || host.ends_with("kick.com") {
        return None;
    }
    Some(host)
}

/// Propagates confirmed connected status across all campaigns for the same game or publisher
/// account-link host (e.g., if Overwatch Collegiate Finals is connected via Battle.net, all
/// other Overwatch campaigns immediately show as Connected too).
pub fn propagate_linked_games(campaigns: &mut [Campaign]) {
    let mut linked_game_ids: HashSet<String> = HashSet::new();
    let mut linked_game_names: HashSet<String> = HashSet::new();
    let mut linked_hosts: HashSet<String> = HashSet::new();

    for c in campaigns.iter() {
        let has_progress = c.drops.iter().any(|d| d.current_minutes > 0 || d.claimed);
        let confirmed_linked = (c.requires_link && c.linked && c.link_verified) || has_progress;
        if confirmed_linked {
            if !c.game.id.trim().is_empty() {
                linked_game_ids.insert(c.game.id.trim().to_string());
            }
            if !c.game.name.trim().is_empty() {
                linked_game_names.insert(c.game.name.trim().to_ascii_lowercase());
            }
            if let Some(host) = publisher_link_host(c.link_url.as_deref()) {
                linked_hosts.insert(host);
            }
        }
    }

    if linked_game_ids.is_empty() && linked_game_names.is_empty() && linked_hosts.is_empty() {
        return;
    }

    for c in campaigns.iter_mut() {
        if c.linked && c.link_verified {
            continue;
        }
        let by_id = !c.game.id.trim().is_empty() && linked_game_ids.contains(c.game.id.trim());
        let by_name = !c.game.name.trim().is_empty()
            && linked_game_names.contains(&c.game.name.trim().to_ascii_lowercase());
        let by_host = publisher_link_host(c.link_url.as_deref())
            .map(|h| linked_hosts.contains(&h))
            .unwrap_or(false);
        if by_id || by_name || by_host {
            c.linked = true;
            c.link_verified = true;
        }
    }
}

fn apply_progress(campaigns: &mut [Campaign], progress_list: &[DropProgress]) {
    for p in progress_list {
        // Handle game-level link confirmations from Twitch Inventory `gameEventDrops` history.
        if let Some(gid) = p.campaign_id.strip_prefix(GAME_LINKED_ID_PREFIX) {
            for camp in campaigns.iter_mut() {
                if !camp.game.id.is_empty() && camp.game.id == gid {
                    camp.linked = true;
                    camp.link_verified = true;
                }
            }
            continue;
        }
        if let Some(gname) = p.campaign_id.strip_prefix(GAME_LINKED_NAME_PREFIX) {
            for camp in campaigns.iter_mut() {
                if camp.game.name.trim().eq_ignore_ascii_case(gname) {
                    camp.linked = true;
                    camp.link_verified = true;
                }
            }
            continue;
        }

        let is_session_context_only = p.campaign_id.is_empty();
        let camp_opt = if !is_session_context_only {
            campaigns.iter_mut().find(|c| c.id == p.campaign_id)
        } else {
            campaigns
                .iter_mut()
                .find(|c| c.drops.iter().any(|d| d.id == p.drop_id))
        };

        if let Some(camp) = camp_opt {
            if p.linked == Some(true) {
                camp.linked = true;
                camp.link_verified = true;
            }
            let mut session_advanced_minutes: Option<u32> = None;
            if let Some(drop) = camp.drops.iter_mut().find(|d| d.id == p.drop_id) {
                let incoming_mins = if let Some(bps) = p.progress_bps {
                    ((bps as f64 / 10_000.0) * drop.required_minutes as f64).round() as u32
                } else {
                    p.current_minutes
                };
                let next_mins = drop
                    .current_minutes
                    .max(incoming_mins)
                    .min(drop.required_minutes);
                if is_session_context_only && next_mins > drop.current_minutes {
                    session_advanced_minutes = Some(next_mins);
                }
                drop.current_minutes = next_mins;
                if p.instance_id.is_some() {
                    drop.instance_id = p.instance_id.clone();
                }
                if p.claimed {
                    drop.claimed = true;
                    drop.claimable = false;
                    drop.needs_manual_claim = false;
                } else if p.claimable
                    || (p.progress_bps.is_some()
                        && drop.required_minutes > 0
                        && drop.current_minutes >= drop.required_minutes)
                {
                    drop.claimable = true;
                }
            } else if let Some(ref hydrated) = p.drop {
                camp.drops.push(hydrated.clone());
            }

            // When Twitch DropCurrentSessionContext advances one drop in a parallel multi-drop
            // campaign (like Tarkov's 10 concurrent tiers), keep other active parallel drops
            // in sync so the Whole Drop ETA stays accurate between full Inventory syncs.
            if let Some(new_mins) = session_advanced_minutes {
                for other in &mut camp.drops {
                    if other.id != p.drop_id && !other.claimed && other.current_minutes > 0 {
                        other.current_minutes = other
                            .current_minutes
                            .max(new_mins)
                            .min(other.required_minutes);
                    }
                }
            }
        }
    }

    let claimed_from_progress: Vec<ClaimedBenefit> = progress_list
        .iter()
        .filter_map(|p| p.claimed_benefit.clone())
        .collect();
    if !claimed_from_progress.is_empty() {
        reconcile_claimed_benefits(campaigns, &claimed_from_progress);
    }

    propagate_linked_games(campaigns);
}

fn merge_claimed_benefits(existing: &mut Vec<ClaimedBenefit>, progress_list: &[DropProgress]) {
    for p in progress_list {
        let Some(ref cb) = p.claimed_benefit else {
            continue;
        };
        if let Some(slot) = existing.iter_mut().find(|e| {
            match (e.benefit_id.as_deref(), cb.benefit_id.as_deref()) {
                (Some(a), Some(b)) => a == b,
                _ => {
                    !e.name.is_empty()
                        && e.name.eq_ignore_ascii_case(&cb.name)
                        && e.game_id == cb.game_id
                }
            }
        }) {
            if cb.last_awarded_at >= slot.last_awarded_at {
                *slot = cb.clone();
            }
        } else {
            existing.push(cb.clone());
        }
    }
}

fn claimed_benefit_matches_game(cb: &ClaimedBenefit, game: &crate::platforms::Game) -> bool {
    let same_game_id = !game.id.is_empty() && cb.game_id.as_deref() == Some(game.id.as_str());
    let same_game_name = cb
        .game_name
        .as_deref()
        .map(|gn| game_names_match(&game.name, gn))
        .unwrap_or(false);
    same_game_id || same_game_name
}

fn claimed_benefit_matches_reward(
    cb: &ClaimedBenefit,
    reward: &Reward,
    game: &crate::platforms::Game,
) -> bool {
    if let (Some(cb_id), Some(r_id)) = (cb.benefit_id.as_deref(), reward.id.as_deref()) {
        if cb_id == r_id {
            return true;
        }
    }
    if !claimed_benefit_matches_game(cb, game) {
        return false;
    }
    let r_name = reward.name.trim();
    let cb_name = cb.name.trim();
    !r_name.is_empty() && !cb_name.is_empty() && r_name.eq_ignore_ascii_case(cb_name)
}

fn reconcile_claimed_benefits(campaigns: &mut [Campaign], claimed_benefits: &[ClaimedBenefit]) {
    if claimed_benefits.is_empty() {
        return;
    }

    for camp in campaigns.iter_mut() {
        if camp.drops.is_empty() {
            continue;
        }
        let window_start = camp.starts_at - ChronoDuration::hours(1);
        let window_end = camp.ends_at + ChronoDuration::hours(1);
        let window_claims: Vec<&ClaimedBenefit> = claimed_benefits
            .iter()
            .filter(|cb| cb.last_awarded_at >= window_start && cb.last_awarded_at <= window_end)
            .collect();
        if window_claims.is_empty() {
            continue;
        }

        let game = camp.game.clone();
        let mut newly_marked = 0usize;
        for drop in &mut camp.drops {
            if drop.claimed {
                continue;
            }
            let all_claimed = if !drop.rewards.is_empty() {
                drop.rewards.iter().all(|reward| {
                    window_claims
                        .iter()
                        .any(|cb| claimed_benefit_matches_reward(cb, reward, &game))
                })
            } else {
                let d_name = drop.name.trim();
                !d_name.is_empty()
                    && window_claims.iter().any(|cb| {
                        claimed_benefit_matches_game(cb, &game)
                            && cb.name.trim().eq_ignore_ascii_case(d_name)
                    })
            };

            if all_claimed {
                drop.claimed = true;
                drop.claimable = false;
                drop.needs_manual_claim = false;
                drop.current_minutes = drop.required_minutes;
                newly_marked += 1;
            }
        }

        if newly_marked > 0 {
            if camp.requires_link {
                camp.linked = true;
                camp.link_verified = true;
            }
            info!(
                campaign = %camp.name,
                campaign_id = %camp.id,
                marked_drops = newly_marked,
                "Marked {newly_marked} already-claimed Twitch drops from Inventory history (campaign={})",
                camp.name
            );
        }
    }
}

const AUTO_COMPAT_ESCALATION_SECS: i64 = 120;

fn should_escalate_to_compat_player(
    platform_id: PlatformId,
    is_mock: bool,
    compat_watch_mode: bool,
    auto_compat_fallback: bool,
    watching_since: Option<DateTime<Utc>>,
    confirmed_progress_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> bool {
    if platform_id != PlatformId::Twitch || is_mock || compat_watch_mode || auto_compat_fallback {
        return false;
    }
    let Some(since) = watching_since else {
        return false;
    };
    if (now - since).num_seconds() < AUTO_COMPAT_ESCALATION_SECS {
        return false;
    }
    confirmed_progress_at.map(|at| at < since).unwrap_or(true)
}

fn total_watched_minutes(campaigns: &[Campaign]) -> u32 {
    campaigns
        .iter()
        .flat_map(|c| c.drops.iter())
        .map(|d| d.current_minutes)
        .sum()
}

fn active_drop_for(campaign: &Campaign) -> Option<&Drop> {
    campaign
        .drops
        .iter()
        .find(|d| !d.claimed && !d.needs_manual_claim)
}

async fn sync_snapshot(
    app: &AppHandle,
    state: &AppState,
    profile_id: &str,
    platform_id: PlatformId,
    ctx: &RuntimeContext,
    force_emit: bool,
) {
    let pinned_channel = {
        let guard = state.settings.read().await;
        guard
            .profile_settings(profile_id)
            .and_then(|p| p.pinned_channel.clone())
            .or_else(|| match platform_id {
                PlatformId::Twitch => guard.twitch.pinned_channel.clone(),
                PlatformId::Kick => guard.kick.pinned_channel.clone(),
            })
    };
    let snap = ctx.to_snapshot(platform_id, pinned_channel);
    state
        .update_profile_snapshot(profile_id, platform_id, snap)
        .await;
    state.publish_snapshot(app, force_emit).await;
    update_tray_from_state(app, state).await;
}

fn send_toast(app: &AppHandle, title: &str, body: &str) {
    let _ = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::miner::selector::select_target_campaign;
    use crate::platforms::mock::MockPlatform;
    use crate::settings::{IdleBehavior, Settings};

    #[tokio::test]
    async fn mock_platform_advances_progress_and_claims() {
        let mock = MockPlatform::new(PlatformId::Twitch);
        let acct = mock.validate_session("mock-session").await.expect("acct");
        assert!(!acct.login.is_empty());

        let mut campaigns = mock.fetch_campaigns("mock-session").await.expect("campaigns");
        assert!(!campaigns.is_empty());

        let mut settings = Settings::default();
        settings.disclaimer_accepted_version = 1;
        settings.idle_behavior = IdleBehavior::FarmAny;

        let target = select_target_campaign(&campaigns, &settings, &settings.twitch, Utc::now())
            .expect("target campaign")
            .clone();

        let candidates = mock
            .find_candidate_channels("mock-session", &target)
            .await
            .expect("candidates");
        let ch = select_best_channel(&target, &candidates, None).expect("channel");

        for _ in 0..8 {
            let res = mock
                .watch_tick("mock-session", ch, false)
                .await
                .expect("watch tick");
            assert_eq!(res, WatchTickResult::Watched);
        }

        let prog = mock.fetch_progress("mock-session").await.expect("progress");
        apply_progress(&mut campaigns, &prog);

        let updated_target = campaigns
            .iter()
            .find(|c| c.id == target.id)
            .expect("updated target");
        let ready_drop = updated_target
            .drops
            .iter()
            .find(|d| d.claimable)
            .expect("drop became claimable in mock mode");

        mock.claim("mock-session", updated_target, ready_drop)
            .await
            .expect("claim succeeds");
    }

    #[test]
    fn propagates_linked_status_across_same_game_and_publisher_host() {
        let now = Utc::now();
        let make_camp = |id: &str, game_id: &str, game_name: &str, link_url: &str, linked: bool| Campaign {
            id: id.to_string(),
            platform: PlatformId::Twitch,
            game: crate::platforms::Game {
                id: game_id.to_string(),
                name: game_name.to_string(),
                slug: None,
                box_art_url: None,
            },
            name: id.to_string(),
            starts_at: now - ChronoDuration::hours(1),
            ends_at: now + ChronoDuration::hours(12),
            status: crate::platforms::CampaignStatus::Active,
            requires_link: true,
            linked,
            link_verified: true,
            link_url: Some(link_url.to_string()),
            allowed_channels: None,
            drops: Vec::new(),
        };

        let mut camps = vec![
            make_camp("ow-collegiate", "515025", "Overwatch", "https://account.battle.net/connections", true),
            make_camp("ow-owcs", "515025", "Overwatch", "https://esports.overwatch.com/link", false),
            make_camp("wow-drop", "18122", "World of Warcraft", "https://account.battle.net/connections", false),
            make_camp("rust-drop", "263490", "Rust", "https://twitch.facepunch.com", false),
        ];

        propagate_linked_games(&mut camps);
        assert!(camps[0].linked);
        assert!(camps[1].linked, "same game (Overwatch) should inherit Connected");
        assert!(camps[2].linked, "same publisher host (account.battle.net) should inherit Connected");
        assert!(!camps[3].linked, "unrelated game (Rust) should remain Not connected");
    }

    #[test]
    fn ignores_ended_inventory_drops_and_clears_manual_claim_flag_when_claimed() {
        let now = Utc::now();
        let drop = Drop {
            id: "drop-1".to_string(),
            instance_id: Some("inst-1".to_string()),
            name: "Drop 1".to_string(),
            rewards: vec![crate::platforms::Reward {
                id: None,
                name: "Common".to_string(),
                image_url: None,
            }],
            required_minutes: 60,
            current_minutes: 60,
            claimable: true,
            claimed: false,
            needs_manual_claim: true,
        };
        let mut campaigns = vec![Campaign {
            id: "active-camp".to_string(),
            platform: PlatformId::Twitch,
            game: crate::platforms::Game {
                id: "491931".to_string(),
                name: "Escape from Tarkov".to_string(),
                slug: None,
                box_art_url: None,
            },
            name: "Tarkov City day 7".to_string(),
            starts_at: now - ChronoDuration::hours(1),
            ends_at: now + ChronoDuration::hours(23),
            status: CampaignStatus::Active,
            requires_link: true,
            linked: true,
            link_verified: true,
            link_url: None,
            allowed_channels: None,
            drops: vec![drop.clone()],
        }];

        // Ended campaign not in active list must NOT be re-injected into UI campaigns.
        let ended_progress = vec![DropProgress {
            campaign_id: "ended-camp".to_string(),
            drop_id: "ended-drop".to_string(),
            instance_id: Some("inst-ended".to_string()),
            current_minutes: 60,
            progress_bps: None,
            claimable: true,
            claimed: false,
            linked: Some(true),
            drop: Some(drop),
            campaign_name: Some("Tarkov City day 6".to_string()),
            game_name: Some("Escape from Tarkov".to_string()),
            claimed_benefit: None,
        }];
        apply_progress(&mut campaigns, &ended_progress);
        assert_eq!(campaigns.len(), 1);
        assert_eq!(campaigns[0].id, "active-camp");

        // Claiming clears needs_manual_claim as well.
        let claimed_progress = vec![DropProgress {
            campaign_id: "active-camp".to_string(),
            drop_id: "drop-1".to_string(),
            instance_id: Some("inst-1".to_string()),
            current_minutes: 60,
            progress_bps: None,
            claimable: false,
            claimed: true,
            linked: Some(true),
            drop: None,
            campaign_name: None,
            game_name: None,
            claimed_benefit: None,
        }];
        apply_progress(&mut campaigns, &claimed_progress);
        assert!(campaigns[0].drops[0].claimed);
        assert!(!campaigns[0].drops[0].claimable);
        assert!(!campaigns[0].drops[0].needs_manual_claim);
    }

    #[test]
    fn auto_compat_escalation_triggers_after_120s_without_progress_and_resets_on_logout() {
        let now = Utc::now();
        let since_119 = Some(now - ChronoDuration::seconds(119));
        let since_120 = Some(now - ChronoDuration::seconds(120));

        // Not yet 120s -> false.
        assert!(!should_escalate_to_compat_player(
            PlatformId::Twitch,
            false,
            false,
            false,
            since_119,
            None,
            now,
        ));

        // 120s elapsed with 0 confirmed progress -> true.
        assert!(should_escalate_to_compat_player(
            PlatformId::Twitch,
            false,
            false,
            false,
            since_120,
            None,
            now,
        ));

        // Confirmed progress from an earlier watch before `since` -> still escalates.
        assert!(should_escalate_to_compat_player(
            PlatformId::Twitch,
            false,
            false,
            false,
            since_120,
            Some(now - ChronoDuration::seconds(300)),
            now,
        ));

        // Confirmed progress after `since` -> does not escalate.
        assert!(!should_escalate_to_compat_player(
            PlatformId::Twitch,
            false,
            false,
            false,
            since_120,
            Some(now - ChronoDuration::seconds(60)),
            now,
        ));

        // Already in compat mode, already escalated, mock mode, or Kick -> false.
        assert!(!should_escalate_to_compat_player(
            PlatformId::Twitch,
            false,
            true,
            false,
            since_120,
            None,
            now,
        ));
        assert!(!should_escalate_to_compat_player(
            PlatformId::Twitch,
            false,
            false,
            true,
            since_120,
            None,
            now,
        ));
        assert!(!should_escalate_to_compat_player(
            PlatformId::Twitch,
            true,
            false,
            false,
            since_120,
            None,
            now,
        ));
        assert!(!should_escalate_to_compat_player(
            PlatformId::Kick,
            false,
            false,
            false,
            since_120,
            None,
            now,
        ));

        // RuntimeContext::reset_session clears auto_compat_fallback and confirmed_progress_at.
        let mut ctx = RuntimeContext::new();
        ctx.auto_compat_fallback = true;
        ctx.confirmed_progress_at = Some(now);
        ctx.reset_session("Logged out");
        assert!(!ctx.auto_compat_fallback);
        assert!(ctx.confirmed_progress_at.is_none());
    }

    fn make_twitch_drop(id: &str, benefit_id: Option<&str>, reward_name: &str, mins: u32) -> Drop {
        Drop {
            id: id.to_string(),
            instance_id: None,
            name: reward_name.to_string(),
            rewards: vec![Reward {
                id: benefit_id.map(str::to_string),
                name: reward_name.to_string(),
                image_url: None,
            }],
            required_minutes: mins,
            current_minutes: 0,
            claimable: false,
            claimed: false,
            needs_manual_claim: false,
        }
    }

    fn make_claimed_progress(
        benefit_id: Option<&str>,
        name: &str,
        game_id: Option<&str>,
        game_name: Option<&str>,
        last_awarded_at: DateTime<Utc>,
    ) -> DropProgress {
        DropProgress {
            campaign_id: format!(
                "{}{}",
                crate::platforms::twitch::models::CLAIMED_BENEFIT_PREFIX,
                benefit_id.unwrap_or(name)
            ),
            drop_id: String::new(),
            instance_id: None,
            current_minutes: 0,
            progress_bps: None,
            claimable: false,
            claimed: true,
            linked: Some(true),
            drop: None,
            campaign_name: None,
            game_name: game_name.map(str::to_string),
            claimed_benefit: Some(ClaimedBenefit {
                benefit_id: benefit_id.map(str::to_string),
                name: name.to_string(),
                game_id: game_id.map(str::to_string),
                game_name: game_name.map(str::to_string),
                last_awarded_at,
            }),
        }
    }

    #[test]
    fn reconciles_already_claimed_twitch_campaign_from_game_event_drops_and_skips_it() {
        let now = Utc::now();
        let starts_at = now - ChronoDuration::days(22);
        let ends_at = now + ChronoDuration::days(6);
        let awarded_at = now - ChronoDuration::days(20);

        let ow_rewards = [
            ("b-river", "River Spray", 60),
            ("b-fika", "Lounging Fika Spray", 120),
            ("b-sakura", "Sakura Love Spray", 180),
            ("b-loot", "RoT Lootbox", 240),
            ("b-starry", "Starry Peace Spray", 300),
            ("b-epic", "RoT Epic Lootbox", 360),
        ];
        let ow_camp = Campaign {
            id: "ow-talon-s5".to_string(),
            platform: PlatformId::Twitch,
            game: crate::platforms::Game {
                id: "515025".to_string(),
                name: "Overwatch".to_string(),
                slug: None,
                box_art_url: None,
            },
            name: "Reign of Talon S5 Mid".to_string(),
            starts_at,
            ends_at,
            status: CampaignStatus::Active,
            requires_link: true,
            linked: true,
            link_verified: true,
            link_url: None,
            allowed_channels: None,
            drops: ow_rewards
                .iter()
                .enumerate()
                .map(|(i, (bid, rname, mins))| {
                    make_twitch_drop(&format!("ow-d-{i}"), Some(bid), rname, *mins)
                })
                .collect(),
        };

        let rust_camp = Campaign {
            id: "rust-next".to_string(),
            platform: PlatformId::Twitch,
            game: crate::platforms::Game {
                id: "263490".to_string(),
                name: "Rust".to_string(),
                slug: None,
                box_art_url: None,
            },
            name: "Rust Next".to_string(),
            starts_at: now - ChronoDuration::hours(2),
            ends_at: now + ChronoDuration::days(8),
            status: CampaignStatus::Active,
            requires_link: false,
            linked: true,
            link_verified: true,
            link_url: None,
            allowed_channels: None,
            drops: vec![make_twitch_drop("rust-d-1", Some("b-rust"), "Rust Door", 60)],
        };

        let mut campaigns = vec![ow_camp, rust_camp];
        let progress: Vec<DropProgress> = ow_rewards
            .iter()
            .map(|(bid, rname, _)| {
                make_claimed_progress(Some(bid), rname, Some("515025"), Some("Overwatch"), awarded_at)
            })
            .collect();

        apply_progress(&mut campaigns, &progress);

        assert!(campaigns[0].drops.iter().all(|d| d.claimed && !d.claimable && d.current_minutes == d.required_minutes));
        assert!(!campaigns[1].drops[0].claimed);

        let mut settings = Settings::default();
        settings.disclaimer_accepted_version = 1;
        settings.idle_behavior = IdleBehavior::FarmAny;

        let target = select_target_campaign(&campaigns, &settings, &settings.twitch, now)
            .expect("should pick next eligible campaign");
        assert_eq!(target.id, "rust-next");
    }

    #[test]
    fn reconciles_claimed_benefits_by_game_and_reward_name_fallback_when_id_absent() {
        let now = Utc::now();
        let mut drop_empty_rewards = make_twitch_drop("d-empty", None, "Legacy Drop", 90);
        drop_empty_rewards.rewards.clear();

        let mut campaigns = vec![Campaign {
            id: "camp-fallback".to_string(),
            platform: PlatformId::Twitch,
            game: crate::platforms::Game {
                id: "515025".to_string(),
                name: "Overwatch".to_string(),
                slug: None,
                box_art_url: None,
            },
            name: "Fallback Test".to_string(),
            starts_at: now - ChronoDuration::days(2),
            ends_at: now + ChronoDuration::days(2),
            status: CampaignStatus::Active,
            requires_link: false,
            linked: true,
            link_verified: true,
            link_url: None,
            allowed_channels: None,
            drops: vec![
                make_twitch_drop("d-named", None, "River Spray", 60),
                drop_empty_rewards,
            ],
        }];

        let progress = vec![
            make_claimed_progress(None, "  river spray  ", Some("515025"), Some("Overwatch 2"), now),
            make_claimed_progress(None, "legacy drop", None, Some("Overwatch"), now),
        ];

        apply_progress(&mut campaigns, &progress);
        assert!(campaigns[0].drops[0].claimed);
        assert_eq!(campaigns[0].drops[0].current_minutes, 60);
        assert!(campaigns[0].drops[1].claimed);
        assert_eq!(campaigns[0].drops[1].current_minutes, 90);
    }

    #[test]
    fn does_not_mark_new_campaign_claimed_from_old_game_event_drops_before_window() {
        let now = Utc::now();
        let starts_at = now - ChronoDuration::hours(2);
        let ends_at = now + ChronoDuration::days(5);
        let old_awarded_at = starts_at - ChronoDuration::hours(2);

        let mut campaigns = vec![Campaign {
            id: "new-camp".to_string(),
            platform: PlatformId::Twitch,
            game: crate::platforms::Game {
                id: "515025".to_string(),
                name: "Overwatch".to_string(),
                slug: None,
                box_art_url: None,
            },
            name: "New Season".to_string(),
            starts_at,
            ends_at,
            status: CampaignStatus::Active,
            requires_link: false,
            linked: true,
            link_verified: true,
            link_url: None,
            allowed_channels: None,
            drops: vec![make_twitch_drop("d-1", Some("b-loot"), "RoT Lootbox", 120)],
        }];

        let old_progress = vec![make_claimed_progress(
            Some("b-loot"),
            "RoT Lootbox",
            Some("515025"),
            Some("Overwatch"),
            old_awarded_at,
        )];

        apply_progress(&mut campaigns, &old_progress);
        assert!(!campaigns[0].drops[0].claimed);
        assert_eq!(campaigns[0].drops[0].current_minutes, 0);
    }

    #[test]
    fn campaign_refresh_preserves_claimed_benefits_from_runtime_context() {
        let now = Utc::now();
        let mut ctx = RuntimeContext::new();
        let make_fresh_campaign = || Campaign {
            id: "ow-talon-s5".to_string(),
            platform: PlatformId::Twitch,
            game: crate::platforms::Game {
                id: "515025".to_string(),
                name: "Overwatch".to_string(),
                slug: None,
                box_art_url: None,
            },
            name: "Reign of Talon S5 Mid".to_string(),
            starts_at: now - ChronoDuration::days(10),
            ends_at: now + ChronoDuration::days(5),
            status: CampaignStatus::Active,
            requires_link: true,
            linked: false,
            link_verified: true,
            link_url: None,
            allowed_channels: None,
            drops: vec![make_twitch_drop("ow-d-1", Some("b-river"), "River Spray", 60)],
        };

        ctx.campaigns = vec![make_fresh_campaign()];
        let progress = vec![make_claimed_progress(
            Some("b-river"),
            "River Spray",
            Some("515025"),
            Some("Overwatch"),
            now - ChronoDuration::days(5),
        )];
        merge_claimed_benefits(&mut ctx.claimed_benefits, &progress);
        apply_progress(&mut ctx.campaigns, &progress);
        assert!(ctx.campaigns[0].drops[0].claimed);

        // Simulate periodic fetch_campaigns returning fresh 0/6 drops.
        ctx.campaigns = vec![make_fresh_campaign()];
        reconcile_claimed_benefits(&mut ctx.campaigns, &ctx.claimed_benefits);
        assert!(ctx.campaigns[0].drops[0].claimed);
        assert_eq!(ctx.campaigns[0].drops[0].current_minutes, 60);
        assert!(ctx.campaigns[0].linked);
    }
}
