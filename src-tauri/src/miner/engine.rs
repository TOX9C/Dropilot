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
    evaluate_switch, select_best_channel, select_target_campaign, SwitchContext, SwitchDecision,
};
use crate::net::backoff::Backoff;
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
        self.candidates.clear();
        self.campaigns.clear();
        self.last_campaign_fetch = None;
        self.last_progress_fetch = None;
        self.last_live_check = None;
        self.last_watch_tick = None;
        self.retry_not_before = None;
        self.next_action_at = None;
        self.backoff.reset();
    }

    fn to_snapshot(&self, platform: PlatformId) -> PlatformSnapshot {
        PlatformSnapshot {
            platform,
            state: self.state,
            state_reason: self.state_reason.clone(),
            account: self.account.clone(),
            watching: self.watching.clone(),
            current_campaign_id: self.current_campaign_id.clone(),
            current_drop_id: self.current_drop_id.clone(),
            next_action_at: self.next_action_at,
            last_error: self.last_error.clone(),
            campaigns: self.campaigns.clone(),
            candidate_channels: self.candidates.clone(),
        }
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
            close_compat_watch_window(app, platform_id);
            if platform_id == PlatformId::Kick {
                close_kick_bridge_window(app);
            }
            ctx.paused = true;
            ctx.watching = None;
            ctx.state = MinerState::Paused;
            ctx.state_reason = Some("Paused by user".to_string());
            ctx.next_action_at = None;
            info!(platform = platform_id.as_str(), "Mining paused");
        }
        MinerControl::Resume => {
            ctx.paused = false;
            ctx.retry_not_before = None;
            ctx.backoff.reset();
            if ctx.token.is_some() {
                ctx.state = MinerState::Idle;
                ctx.state_reason = Some("Resuming...".to_string());
                step_miner(platform_id, app, state, backend, ctx).await;
            }
            info!(platform = platform_id.as_str(), "Mining resumed");
        }
        MinerControl::SwitchChannel => {
            if ctx.token.is_some() && !ctx.paused {
                if let Some(ref current) = ctx.watching {
                    let current_login = current.login.clone();
                    ctx.candidates.retain(|c| !c.login.eq_ignore_ascii_case(&current_login));
                }
                ctx.watching = None;
                ctx.last_voluntary_switch = Some(Utc::now());
                step_miner(platform_id, app, state, backend, ctx).await;
            }
        }
        MinerControl::PinChannel(pin) => {
            if let Some(ref pin_login) = pin {
                info!(platform = platform_id.as_str(), channel = %pin_login, "Pinned channel updated");
            }
            ctx.last_voluntary_switch = None;
            if ctx.token.is_some() && !ctx.paused {
                step_miner(platform_id, app, state, backend, ctx).await;
            }
        }
        MinerControl::RetryNow => {
            ctx.retry_not_before = None;
            ctx.last_error = None;
            ctx.backoff.reset();
            if ctx.token.is_some() && !ctx.paused {
                step_miner(platform_id, app, state, backend, ctx).await;
            }
        }
        MinerControl::SettingsUpdated => {
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
            if ctx.token.is_some() && !ctx.paused {
                step_miner(platform_id, app, state, backend, ctx).await;
            }
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
            Ok(fetched) => {
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
                apply_progress(&mut ctx.campaigns, &progress_list);
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

    // 5. Select target campaign according to user priority & settings.
    let now = Utc::now();
    let target_campaign = select_target_campaign(&ctx.campaigns, &settings, &plat_settings, now).cloned();

    let Some(target) = target_campaign else {
        close_compat_watch_window(app, platform_id);
        ctx.watching = None;
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
    };

    // 6. Check current watching state & switch rules.
    if let Some(mut current_ch) = ctx.watching.clone() {
        let need_live_check = ctx
            .last_live_check
            .map(|t| t.elapsed() >= Duration::from_secs(if is_mock { 10 } else { LIVE_CHECK_SECS }))
            .unwrap_or(true);

        if need_live_check {
            match plat.check_live(&token, &current_ch).await {
                Ok(updated) => {
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

        let current_camp = ctx
            .current_campaign_id
            .as_deref()
            .and_then(|id| ctx.campaigns.iter().find(|c| c.id == id))
            .unwrap_or(&target);

        let decision = evaluate_switch(SwitchContext {
            current_channel: &current_ch,
            current_campaign: current_camp,
            top_campaign: Some(&target),
            pinned_login: plat_settings.pinned_channel.as_deref(),
            now,
            last_progress_advance: ctx.last_progress_advance,
            last_voluntary_switch: ctx.last_voluntary_switch,
        });

        match decision {
            SwitchDecision::Keep => {}
            SwitchDecision::SwitchImmediate(reason) => {
                info!(platform = platform_id.as_str(), reason = %reason, "Switching channel immediately");
                ctx.watching = None;
                ctx.candidates.clear();
            }
            SwitchDecision::SwitchVoluntary(reason) => {
                info!(platform = platform_id.as_str(), reason = %reason, "Switching channel");
                ctx.watching = None;
                ctx.candidates.clear();
                ctx.last_voluntary_switch = Some(now);
            }
        }
    }

    // 7. Find a channel if not currently watching one.
    if ctx.watching.is_none() {
        ctx.state = MinerState::Selecting;
        ctx.state_reason = Some(format!("Finding live stream for {}", target.game.name));
        sync_snapshot(app, state, platform_id, ctx, false).await;

        match plat.find_candidate_channels(&token, &target).await {
            Ok(candidates) => {
                ctx.candidates = candidates;
                if let Some(best) = select_best_channel(
                    &target,
                    &ctx.candidates,
                    plat_settings.pinned_channel.as_deref(),
                )
                .cloned()
                {
                    info!(
                        platform = platform_id.as_str(),
                        channel = %best.login,
                        game = %target.game.name,
                        "Selected stream"
                    );
                    ctx.watching = Some(best);
                    ctx.current_campaign_id = Some(target.id.clone());
                    ctx.current_drop_id = active_drop_for(&target).map(|d| d.id.clone());
                    ctx.last_live_check = Some(Instant::now());
                    ctx.last_watch_tick = None;
                    ctx.last_progress_advance = Utc::now();
                } else {
                    ctx.state = MinerState::Idle;
                    ctx.state_reason = Some(format!(
                        "No live channels for {}, retrying in 5 min",
                        target.game.name
                    ));
                    let next_dur = jittered_duration(IDLE_RETRY_SECS);
                    ctx.retry_not_before = Some(Instant::now() + next_dur);
                    ctx.next_action_at =
                        Some(Utc::now() + ChronoDuration::from_std(next_dur).unwrap_or_default());
                    return;
                }
            }
            Err(AppError::NoCandidates(msg)) => {
                ctx.state = MinerState::Idle;
                ctx.state_reason = Some(format!("{msg}; retrying in 5 min"));
                let next_dur = jittered_duration(IDLE_RETRY_SECS);
                ctx.retry_not_before = Some(Instant::now() + next_dur);
                ctx.next_action_at =
                    Some(Utc::now() + ChronoDuration::from_std(next_dur).unwrap_or_default());
                return;
            }
            Err(err) => {
                handle_platform_error(platform_id, app, state, ctx, err).await;
                return;
            }
        }
    }

    // 8. Perform watch tick on the selected stream.
    if let Some(ref channel) = ctx.watching.clone() {
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
        ctx.current_campaign_id = Some(target.id.clone());
        ctx.current_drop_id = active_drop_for(&target).map(|d| d.id.clone());
        ctx.state_reason = Some(format!("Watching {} ({})", channel.display_name, target.game.name));
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
                    // If a drop is right at completion threshold, fetch progress right away on next cycle.
                    if let Some(drop) = active_drop_for(&target) {
                        if drop.current_minutes + 1 >= drop.required_minutes {
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

fn apply_progress(campaigns: &mut [Campaign], progress_list: &[DropProgress]) {
    for p in progress_list {
        if let Some(camp) = campaigns.iter_mut().find(|c| c.id == p.campaign_id) {
            if let Some(linked) = p.linked {
                camp.linked = linked;
                camp.link_verified = true;
            }
            if let Some(drop) = camp.drops.iter_mut().find(|d| d.id == p.drop_id) {
                drop.current_minutes = p.current_minutes.min(drop.required_minutes);
                if p.instance_id.is_some() {
                    drop.instance_id = p.instance_id.clone();
                }
                drop.claimed = p.claimed;
                drop.claimable = p.claimable && !p.claimed;
            } else if let Some(ref hydrated) = p.drop {
                camp.drops.push(hydrated.clone());
            }
        }
    }
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
    let snap = ctx.to_snapshot(platform_id);
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
}
