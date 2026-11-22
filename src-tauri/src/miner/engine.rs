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
    close_compat_watch_window, close_kick_bridge_window, close_twitch_bridge_window,
};
use crate::miner::scheduler::{
    jittered_duration, ERROR_RETRY_SECS, IDLE_RETRY_SECS, LIVE_CHECK_SECS,
    MIN_CAMPAIGN_REFRESH_SECS, MOCK_WATCH_TICK_SECS, PROGRESS_POLL_SECS, WATCH_TICK_SECS,
};
use crate::miner::selector::{
    evaluate_switch, ordered_target_campaigns, select_best_channel, SwitchContext, SwitchDecision,
};
use crate::net::backoff::Backoff;
use crate::platforms::kick::models::is_kick_fallback_case_id;
use crate::platforms::twitch::apply_campaign_details;
use crate::platforms::twitch::models::{GAME_LINKED_ID_PREFIX, GAME_LINKED_NAME_PREFIX};
use crate::platforms::{
    Account, AppError, Campaign, Channel, Drop, DropProgress, MinerState, Platform, PlatformId,
    PlatformSnapshot, RecentClaim, WatchTickResult,
};
use crate::secrets::SecretStore;
use crate::state::{AppState, MinerControl};
use crate::tray::update_tray_from_state;

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

pub fn spawn_miner_supervisor(
    platform_id: PlatformId,
    app: AppHandle,
    state: Arc<AppState>,
    backend: PlatformBackend,
) -> mpsc::Sender<MinerControl> {
    let (tx, mut rx) = mpsc::channel::<MinerControl>(32);

    tauri::async_runtime::spawn(async move {
        loop {
            let result = run_miner_loop(
                platform_id,
                app.clone(),
                state.clone(),
                backend.clone(),
                &mut rx,
            )
            .await;

            match result {
                Ok(ExitReason::Shutdown) => {
                    info!(platform = platform_id.as_str(), "Miner stopped cleanly");
                    break;
                }
                Ok(ExitReason::Restart) => {
                    warn!(platform = platform_id.as_str(), "Restarting miner loop");
                    sleep(Duration::from_secs(3)).await;
                }
                Err(err) => {
                    error!(
                        platform = platform_id.as_str(),
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
    last_live_check: Option<Instant>,
    last_watch_tick: Option<Instant>,
    last_progress_advance: DateTime<Utc>,
    last_voluntary_switch: Option<DateTime<Utc>>,
    last_seen_total_minutes: u32,
    backoff: Backoff,
    retry_not_before: Option<Instant>,
    paused: bool,
    hydrated_campaign_ids: HashSet<String>,
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
            last_live_check: None,
            last_watch_tick: None,
            last_progress_advance: Utc::now(),
            last_voluntary_switch: None,
            last_seen_total_minutes: 0,
            backoff: Backoff::new(),
            retry_not_before: None,
            paused: false,
            hydrated_campaign_ids: HashSet::new(),
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
        self.no_live_channels_until.clear();
        self.avoid_channels_until.clear();
        self.last_campaign_fetch = None;
        self.last_progress_fetch = None;
        self.last_live_check = None;
        self.last_watch_tick = None;
        self.retry_not_before = None;
        self.next_action_at = None;
        self.backoff.reset();
    }

    fn prune_cooldowns(&mut self) {
        let now_inst = Instant::now();
        self.no_live_channels_until.retain(|_, until| *until > now_inst);
        self.avoid_channels_until.retain(|_, until| *until > now_inst);
    }

    fn to_snapshot(&self, platform: PlatformId, pinned_channel: Option<String>) -> PlatformSnapshot {
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
            campaigns: self.campaigns.clone(),
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

async fn clear_pinned_channel(state: &AppState, platform: PlatformId) {
    let changed = {
        let mut guard = state.settings.write().await;
        let plat = match platform {
            PlatformId::Twitch => &mut guard.twitch,
            PlatformId::Kick => &mut guard.kick,
        };
        plat.pinned_channel.take().is_some()
    };
    if changed {
        let _ = state.persist_settings().await;
    }
}

async fn run_miner_loop(
    platform_id: PlatformId,
    app: AppHandle,
    state: Arc<AppState>,
    backend: PlatformBackend,
    rx: &mut mpsc::Receiver<MinerControl>,
) -> Result<ExitReason, String> {
    let mut ctx = RuntimeContext::new();

    // Load initial token from Credential Manager (or mock session when mock mode is active).
    load_initial_token(platform_id, &state, &backend, &mut ctx).await;
    sync_snapshot(&app, &state, platform_id, &ctx, true).await;

    loop {
        // Check if we need to reload token when switching between real and mock mode.
        let is_mock = state.mock_active.load(Ordering::Relaxed);
        if is_mock && ctx.token.is_none() {
            load_initial_token(platform_id, &state, &backend, &mut ctx).await;
            sync_snapshot(&app, &state, platform_id, &ctx, true).await;
        }

        // Wait either for a MinerControl command or the next step tick.
        let wait_dur = compute_next_wake_delay(&state, &ctx);
        tokio::select! {
            maybe_cmd = rx.recv() => {
                let Some(cmd) = maybe_cmd else {
                    return Ok(ExitReason::Restart);
                };
                match handle_control(platform_id, &app, &state, &backend, &mut ctx, cmd).await {
                    ControlOutcome::Continue => {}
                    ControlOutcome::Shutdown => return Ok(ExitReason::Shutdown),
                }
                sync_snapshot(&app, &state, platform_id, &ctx, true).await;
            }
            _ = sleep(wait_dur) => {
                step_miner(platform_id, &app, &state, &backend, &mut ctx).await;
                sync_snapshot(&app, &state, platform_id, &ctx, false).await;
            }
        }
    }
}

enum ControlOutcome {
    Continue,
    Shutdown,
}

async fn load_initial_token(
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

    match SecretStore::load_token(platform_id) {
        Ok(Some(secret)) => {
            let tok = secret.expose().to_string();
            match backend.active(state).validate_session(&tok).await {
                Ok(acct) => {
                    ctx.token = Some(tok);
                    ctx.account = Some(acct);
                    ctx.state = MinerState::Idle;
                    ctx.state_reason = Some("Loading campaigns...".to_string());
                    info!(platform = platform_id.as_str(), "Session restored from Credential Manager");
                }
                Err(AppError::Auth(_)) => {
                    let _ = SecretStore::delete_token(platform_id);
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
            warn!(platform = platform_id.as_str(), error = %err, "Failed reading Credential Manager");
            ctx.reset_session("Not logged in");
        }
    }
}

async fn handle_control(
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
            close_compat_watch_window(app, platform_id);
            match platform_id {
                PlatformId::Twitch => close_twitch_bridge_window(app),
                PlatformId::Kick => close_kick_bridge_window(app),
            }
            return ControlOutcome::Shutdown;
        }
        MinerControl::LoginComplete => {
            ctx.paused = false;
            ctx.last_error = None;
            ctx.backoff.reset();
            load_initial_token(platform_id, state, backend, ctx).await;
            ctx.last_campaign_fetch = None;
            ctx.retry_not_before = None;
            step_miner(platform_id, app, state, backend, ctx).await;
        }
        MinerControl::Logout => {
            let plat = backend.active(state);
            let _ = plat.logout(ctx.token.as_deref()).await;
            if !state.mock_active.load(Ordering::Relaxed) {
                let _ = SecretStore::delete_token(platform_id);
            }
            ctx.paused = false;
            ctx.last_error = None;
            ctx.reset_session("Logged out");
            info!(platform = platform_id.as_str(), "Logged out");
        }
        MinerControl::Pause => {
            backend.active(state).stop_watching().await;
            close_compat_watch_window(app, platform_id);
            if platform_id == PlatformId::Kick {
                close_kick_bridge_window(app);
            }
            ctx.paused = true;
            ctx.watching = None;
            ctx.watching_since = None;
            ctx.state = MinerState::Paused;
            ctx.state_reason = Some("Paused by user".to_string());
            ctx.next_action_at = None;
            info!(platform = platform_id.as_str(), "Mining paused");
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
                step_miner(platform_id, app, state, backend, ctx).await;
            }
            info!(platform = platform_id.as_str(), "Mining resumed");
        }
        MinerControl::SwitchChannel => {
            clear_pinned_channel(state, platform_id).await;
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
                step_miner(platform_id, app, state, backend, ctx).await;
            }
        }
        MinerControl::PinChannel(pin) => {
            if let Some(ref pin_login) = pin {
                info!(platform = platform_id.as_str(), channel = %pin_login, "Pinned channel updated");
                ctx.avoid_channels_until
                    .remove(&pin_login.to_ascii_lowercase());
            }
            ctx.last_voluntary_switch = None;
            if ctx.token.is_some() && !ctx.paused {
                step_miner(platform_id, app, state, backend, ctx).await;
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
                step_miner(platform_id, app, state, backend, ctx).await;
            }
        }
        MinerControl::SettingsUpdated => {
            ctx.no_live_channels_until.clear();
            ctx.last_voluntary_switch = None;
            let is_mock = state.mock_active.load(Ordering::Relaxed);
            if is_mock && ctx.token.as_deref() != Some("mock-session") {
                ctx.reset_session("Switching to mock mode");
                load_initial_token(platform_id, state, backend, ctx).await;
            } else if !is_mock && ctx.token.as_deref() == Some("mock-session") {
                ctx.reset_session("Switching to live mode");
                load_initial_token(platform_id, state, backend, ctx).await;
            }
            if ctx.token.is_some() && !ctx.paused {
                step_miner(platform_id, app, state, backend, ctx).await;
            }
        }
        MinerControl::RefreshNow => {
            ctx.last_campaign_fetch = None;
            ctx.last_progress_fetch = None;
            ctx.no_live_channels_until.clear();
            ctx.avoid_channels_until.clear();
            backend.active(state).reset_progress_sync_cooldown();
            if ctx.token.is_some() && !ctx.paused {
                step_miner(platform_id, app, state, backend, ctx).await;
            }
        }
        MinerControl::CampaignDetailsLoaded(detailed) => {
            ctx.hydrated_campaign_ids.insert(detailed.id.clone());
            if let Some(camp) = ctx.campaigns.iter_mut().find(|c| c.id == detailed.id) {
                apply_campaign_details(camp, *detailed);
            }
            propagate_linked_games(&mut ctx.campaigns);
        }
    }
    ControlOutcome::Continue
}

fn compute_next_wake_delay(state: &AppState, ctx: &RuntimeContext) -> Duration {
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
        return jittered_duration(tick).min(Duration::from_secs(60));
    }

    Duration::from_secs(10)
}

async fn step_miner(
    platform_id: PlatformId,
    app: &AppHandle,
    state: &AppState,
    backend: &PlatformBackend,
    ctx: &mut RuntimeContext,
) {
    let settings = state.settings.read().await.clone();
    let plat_settings = match platform_id {
        PlatformId::Twitch => settings.twitch.clone(),
        PlatformId::Kick => settings.kick.clone(),
    };

    if !plat_settings.enabled {
        ctx.watching = None;
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
                handle_platform_error(platform_id, app, state, ctx, err).await;
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
                }
                propagate_linked_games(&mut fetched);
                ctx.campaigns = fetched;
                ctx.last_campaign_fetch = Some(Instant::now());
                ctx.backoff.reset();
            }
            Err(err) => {
                handle_platform_error(platform_id, app, state, ctx, err).await;
                return;
            }
        }
    }

    // 3. Refresh progress periodically (or frequently in mock mode).
    let progress_secs = if is_mock { 6 } else { PROGRESS_POLL_SECS };
    let need_progress = ctx
        .last_progress_fetch
        .map(|t| t.elapsed() >= Duration::from_secs(progress_secs))
        .unwrap_or(true);

    if need_progress {
        match plat.fetch_progress(&token).await {
            Ok(progress_list) => {
                if !progress_list.is_empty() {
                    apply_progress(&mut ctx.campaigns, &progress_list);
                    ctx.progress_synced_at = Some(Utc::now());
                } else if ctx.progress_synced_at.is_none() {
                    ctx.progress_synced_at = Some(Utc::now());
                }
                ctx.last_progress_fetch = Some(Instant::now());
                ctx.backoff.reset();

                let total_mins = total_watched_minutes(&ctx.campaigns);
                if total_mins > ctx.last_seen_total_minutes {
                    ctx.last_seen_total_minutes = total_mins;
                    ctx.last_progress_advance = Utc::now();
                }
            }
            Err(err) => {
                handle_platform_error(platform_id, app, state, ctx, err).await;
                return;
            }
        }
    }

    // 4. Auto-claim any claimable drops.
    if settings.auto_claim {
        claim_ready_drops(platform_id, app, state, &plat, &token, ctx, settings.notifications).await;
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
        close_compat_watch_window(app, platform_id);
        ctx.watching = None;
        ctx.watching_since = None;
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
                    handle_platform_error(platform_id, app, state, ctx, err).await;
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
                                    clear_pinned_channel(state, platform_id).await;
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
            sync_snapshot(app, state, platform_id, ctx, false).await;

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
                            clear_pinned_channel(state, platform_id).await;
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
                        ctx.current_drop_id = active_drop_for(cand_camp).map(|d| d.id.clone());
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
                    handle_platform_error(platform_id, app, state, ctx, err).await;
                    return;
                }
            }
        }

        if ctx.watching.is_none() {
            plat.stop_watching().await;
            close_compat_watch_window(app, platform_id);
            ctx.watching_since = None;
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

        ctx.state = MinerState::Mining;
        ctx.current_campaign_id = Some(active_camp.id.clone());
        ctx.current_drop_id = active_drop_for(&active_camp).map(|d| d.id.clone());
        ctx.state_reason = Some(format!(
            "Watching {} ({})",
            channel.display_name, active_camp.game.name
        ));
        ctx.last_error = None;
        ctx.next_action_at =
            Some(Utc::now() + ChronoDuration::from_std(tick_interval).unwrap_or_default());

        if should_tick {
            match plat
                .watch_tick(&token, channel, settings.compat_watch_mode)
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

                    // If a drop is right at completion threshold, fetch progress right away on next cycle.
                    if let Some(drop) = active_drop_for(&active_camp) {
                        if drop.current_minutes + 1 >= drop.required_minutes {
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
                    handle_platform_error(platform_id, app, state, ctx, err).await;
                }
            }
        }
    }
}

async fn claim_ready_drops(
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
    sync_snapshot(app, state, platform_id, ctx, false).await;

    for (camp_id, game_name, camp_name, drop) in to_claim {
        let Some(camp_clone) = ctx.campaigns.iter().find(|c| c.id == camp_id).cloned() else {
            continue;
        };
        match plat.claim(token, &camp_clone, &drop).await {
            Ok(()) => {
                info!(
                    platform = platform_id.as_str(),
                    drop = %drop.name,
                    game = %game_name,
                    "Claimed drop"
                );
                if let Some(camp) = ctx.campaigns.iter_mut().find(|c| c.id == camp_id) {
                    if let Some(d) = camp.drops.iter_mut().find(|d| d.id == drop.id) {
                        d.claimed = true;
                        d.claimable = false;
                        d.needs_manual_claim = false;
                    }
                }
                let reward_img = drop.rewards.first().and_then(|r| r.image_url.clone());
                state
                    .record_claim(RecentClaim {
                        id: format!("{}-{}", camp_id, drop.id),
                        platform: platform_id,
                        game_name: game_name.clone(),
                        campaign_name: camp_name,
                        drop_name: drop.name.clone(),
                        reward_image_url: reward_img,
                        claimed_at: Utc::now(),
                    })
                    .await;

                if notify_enabled {
                    send_toast(
                        app,
                        &format!("{} Drop Claimed", platform_id.display_name()),
                        &format!("{} ({})", drop.name, game_name),
                    );
                }
            }
            Err(AppError::Blocked(msg)) => {
                warn!(
                    platform = platform_id.as_str(),
                    drop = %drop.name,
                    reason = %msg,
                    "Claim needs manual action in official window"
                );
                if let Some(camp) = ctx.campaigns.iter_mut().find(|c| c.id == camp_id) {
                    if let Some(d) = camp.drops.iter_mut().find(|d| d.id == drop.id) {
                        d.needs_manual_claim = true;
                    }
                }
                send_toast(
                    app,
                    "Claim needs you",
                    &format!(
                        "{} requires manual claim on {}'s drops page",
                        drop.name,
                        platform_id.display_name()
                    ),
                );
            }
            Err(err) => {
                warn!(
                    platform = platform_id.as_str(),
                    drop = %drop.name,
                    error = %err,
                    "Claim attempt failed"
                );
            }
        }
    }
}

async fn handle_platform_error(
    platform_id: PlatformId,
    app: &AppHandle,
    state: &AppState,
    ctx: &mut RuntimeContext,
    err: AppError,
) {
    match err {
        AppError::Auth(msg) => {
            warn!(platform = platform_id.as_str(), reason = %msg, "Authentication invalid or expired");
            if !state.mock_active.load(Ordering::Relaxed) {
                let _ = SecretStore::delete_token(platform_id);
            }
            ctx.reset_session("Session expired; please log in again");
            ctx.last_error = Some(format!("Session expired ({msg})"));
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
            send_toast(
                app,
                &format!("{} action required", platform_id.display_name()),
                "Please verify in the official window",
            );
        }
        AppError::PlatformChanged(detail) => {
            error!(platform = platform_id.as_str(), detail = %detail, "Platform endpoint/schema changed");
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

    propagate_linked_games(campaigns);
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
    platform_id: PlatformId,
    ctx: &RuntimeContext,
    force_emit: bool,
) {
    let pinned_channel = {
        let guard = state.settings.read().await;
        match platform_id {
            PlatformId::Twitch => guard.twitch.pinned_channel.clone(),
            PlatformId::Kick => guard.kick.pinned_channel.clone(),
        }
    };
    let snap = ctx.to_snapshot(platform_id, pinned_channel);
    {
        let mut guard = state.snapshot_lock(platform_id).write().await;
        *guard = snap;
    }
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
}
