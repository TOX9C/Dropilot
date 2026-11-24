use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use chrono::{SecondsFormat, Utc};
use reqwest::header::{HeaderMap, HeaderValue, ORIGIN, REFERER};
use reqwest::Method;
use serde_json::{json, Value};
use tauri::AppHandle;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::interval;
use tracing::{debug, info, warn};

use crate::auth::login_window::{
    close_compat_watch_window_for_profile, ensure_compat_watch_window_for_profile,
    twitch_bridge_capture_for_profile,
};
use crate::net::allowlist::{validate_url, AllowlistKind};
use crate::net::client::AllowlistedClient;
use crate::platforms::twitch::gql::{
    cached_twitch_user_agent, clear_twitch_integrity_context, execute_persisted_gql_for_profile,
    fetch_channel_live_status, get_twitch_integrity_context, ops, validate_oauth_token,
    TWITCH_USHER_HLS_BASE,
};
use crate::platforms::{AppError, Channel, PlatformId, WatchTickResult};
use crate::settings::DEFAULT_TWITCH_PROFILE_ID;

const DEFAULT_SPADE_URL: &str = "https://spade.twitch.tv/track";
const SPADE_CACHE_TTL: Duration = Duration::from_secs(6 * 3600);
const HLS_WATCH_INTERVAL_SECS: u64 = 12;
const SPADE_WATCH_INTERVAL_SECS: u64 = 60;
const INTEGRITY_BRIDGE_COOLDOWN_SECS: u64 = 5 * 60;
const DEFAULT_HLS_TOKEN_REFRESH_SECS: u64 = 12 * 60;
const MIN_HLS_TOKEN_REFRESH_SECS: u64 = 60;
const MAX_HLS_TOKEN_REFRESH_SECS: u64 = 15 * 60;
const MAX_SEEN_CHUNKS: usize = 96;
const MAX_CONSECUTIVE_HLS_FAILURES: u32 = 3;

#[derive(Debug, Clone)]
struct CachedSpadeUrl {
    url: String,
    fetched_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BroadcastInfo {
    channel_id: String,
    broadcast_id: String,
    game_id: String,
    game_name: String,
}

#[derive(Debug, Clone)]
struct HlsVariantTarget {
    variant_url: String,
    refresh_after: Duration,
}

#[derive(Debug, Clone)]
struct ParsedPlaybackAccessToken {
    value: String,
    signature: String,
    has_user_id: bool,
    refresh_after: Duration,
}

struct ActiveTwitchWatchTask {
    channel_login: String,
    auth_token: String,
    alive: Arc<AtomicBool>,
    offline: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl ActiveTwitchWatchTask {
    fn stop(self) {
        self.alive.store(false, Ordering::Relaxed);
        self.handle.abort();
    }
}

#[derive(Default)]
pub struct TwitchWatchSession {
    task: Mutex<Option<ActiveTwitchWatchTask>>,
    spade_url: StdMutex<Option<CachedSpadeUrl>>,
    cached_user_id: StdMutex<Option<u64>>,
    last_integrity_attempt_at: StdMutex<Option<Instant>>,
}

struct WatchGqlContext<'a> {
    app: Option<&'a AppHandle>,
    profile_id: &'a str,
    bridge_lock: Option<&'a Arc<Mutex<()>>>,
    last_integrity_attempt_at: Option<&'a StdMutex<Option<Instant>>>,
}

impl TwitchWatchSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn close(&self) {
        let mut guard = self.task.lock().await;
        if let Some(active) = guard.take() {
            active.stop();
        }
    }

    pub async fn invalidate_token(&self) {
        self.close().await;
        if let Ok(mut g) = self.cached_user_id.lock() {
            *g = None;
        }
        if let Ok(mut g) = self.last_integrity_attempt_at.lock() {
            *g = None;
        }
    }

    pub async fn tick(
        &self,
        http: &Arc<AllowlistedClient>,
        app: Option<&AppHandle>,
        auth_token: &str,
        channel: &Channel,
        compat_mode: bool,
    ) -> Result<WatchTickResult, AppError> {
        self.tick_for_profile(
            http,
            app,
            DEFAULT_TWITCH_PROFILE_ID,
            None,
            auth_token,
            channel,
            compat_mode,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn tick_for_profile(
        &self,
        http: &Arc<AllowlistedClient>,
        app: Option<&AppHandle>,
        profile_id: &str,
        bridge_lock: Option<&Arc<Mutex<()>>>,
        auth_token: &str,
        channel: &Channel,
        compat_mode: bool,
    ) -> Result<WatchTickResult, AppError> {
        if compat_mode {
            self.close().await;
            if let Some(app_handle) = app {
                ensure_compat_watch_window_for_profile(
                    app_handle,
                    PlatformId::Twitch,
                    profile_id,
                    &channel.login,
                )?;
            }
            return Ok(WatchTickResult::Watched);
        }

        // Default ultra-light stream-less mode: never keep a hidden Chromium video player open.
        if let Some(app_handle) = app {
            close_compat_watch_window_for_profile(app_handle, PlatformId::Twitch, profile_id);
        }

        let mut guard = self.task.lock().await;
        if let Some(ref active) = *guard {
            let same_channel = active.channel_login.eq_ignore_ascii_case(&channel.login);
            let same_token = active.auth_token == auth_token;
            if same_channel && same_token {
                if active.offline.load(Ordering::Relaxed) {
                    if let Some(prev) = guard.take() {
                        prev.stop();
                    }
                    return Ok(WatchTickResult::StreamOffline);
                }
                if active.alive.load(Ordering::Relaxed) && !active.handle.is_finished() {
                    return Ok(WatchTickResult::Watched);
                }
            }
        }

        if let Some(prev) = guard.take() {
            prev.stop();
        }

        let initial_gql_ctx = WatchGqlContext {
            app,
            profile_id,
            bridge_lock,
            last_integrity_attempt_at: Some(&self.last_integrity_attempt_at),
        };

        // 1. Obtain an authenticated HLS lowest-quality variant playlist URL using the user's
        //    real Twitch OAuth token + WebView2 integrity context.
        let Some(initial_target) =
            fetch_hls_variant_url(http, &initial_gql_ctx, auth_token, &channel.login).await?
        else {
            return Ok(WatchTickResult::StreamOffline);
        };

        // 2. Immediately perform the first zero-video-byte HLS HEAD watch cycle synchronously
        //    so we confirm the stream is live and start earning drop credit right away.
        let mut seen_chunks = VecDeque::with_capacity(MAX_SEEN_CHUNKS);
        let initial_chunks =
            match send_hls_head_cycle(http, &channel.login, &initial_target.variant_url, &mut seen_chunks)
                .await
            {
                Ok(CycleOutcome::Watched(n)) => n,
                Ok(CycleOutcome::RefreshNeeded) => {
                    warn!(
                        channel = %channel.login,
                        "Initial Twitch HLS media playlist returned no usable chunks; treating stream offline"
                    );
                    return Ok(WatchTickResult::StreamOffline);
                }
                Err(err) => return Err(err),
            };

        let has_integrity = get_twitch_integrity_context(profile_id, auth_token).is_some();
        info!(
            channel = %channel.login,
            chunks = initial_chunks,
            has_integrity,
            "Connected lightweight Twitch stream-less HLS HEAD watcher (0 Mbps video)"
        );

        // 3. Resolve user_id + spade_url + broadcast/game IDs for companion Spade minute-watched pings.
        let mut user_id_opt = match self.get_or_fetch_user_id(http, auth_token).await {
            Ok(uid) => Some(uid),
            Err(err) => {
                warn!(
                    channel = %channel.login,
                    error = %err,
                    "Could not fetch Twitch user_id for Spade minute-watched; will retry"
                );
                None
            }
        };
        let spade_url = self.get_or_fetch_spade_url(http).await;
        let mut broadcast_info = match resolve_channel_and_broadcast_info(http, auth_token, channel).await {
            Ok(info) => {
                if info.is_none() {
                    warn!(
                        channel = %channel.login,
                        "ChannelLiveStatus returned no active stream IDs for Spade minute-watched; will retry"
                    );
                }
                info
            }
            Err(err) => {
                warn!(
                    channel = %channel.login,
                    error = %err,
                    "Failed resolving Twitch broadcast/game IDs for Spade minute-watched; will retry"
                );
                None
            }
        };

        if let (Some(uid), Some(ref info)) = (user_id_opt, &broadcast_info) {
            if let Err(err) = send_spade_minute_watched(
                http,
                &spade_url,
                uid,
                info,
                &channel.login,
            )
            .await
            {
                warn!(
                    channel = %channel.login,
                    spade_url = %spade_url,
                    error = %err,
                    "Initial Twitch Spade minute-watched beacon failed"
                );
            } else {
                debug!(
                    channel = %channel.login,
                    broadcast_id = %info.broadcast_id,
                    game_id = %info.game_id,
                    "Sent initial Twitch Spade minute-watched beacon"
                );
            }
        }

        let alive = Arc::new(AtomicBool::new(true));
        let offline = Arc::new(AtomicBool::new(false));
        let alive_task = alive.clone();
        let offline_task = offline.clone();
        let http_task = http.clone();
        let token_owned = auth_token.to_string();
        let channel_owned = channel.clone();
        let cached_user_id_arc = Arc::new(StdMutex::new(user_id_opt));
        if let Some(uid) = user_id_opt {
            if let Ok(mut g) = self.cached_user_id.lock() {
                *g = Some(uid);
            }
        }
        let app_task = app.cloned();
        let profile_id_task = profile_id.to_string();
        let bridge_lock_task = bridge_lock.cloned();

        let handle = tokio::spawn(async move {
            let task_gql_ctx = WatchGqlContext {
                app: app_task.as_ref(),
                profile_id: &profile_id_task,
                bridge_lock: bridge_lock_task.as_ref(),
                last_integrity_attempt_at: None,
            };
            let mut variant_url = initial_target.variant_url;
            let mut refresh_after = initial_target.refresh_after;
            let mut variant_fetched_at = Instant::now();
            let mut last_spade_at = Instant::now();
            let mut consecutive_hls_failures: u32 = 0;
            let mut ticker = interval(Duration::from_secs(HLS_WATCH_INTERVAL_SECS));
            // Skip immediate first tick since we just ran the initial cycle above.
            ticker.tick().await;

            while alive_task.load(Ordering::Relaxed) {
                ticker.tick().await;
                if !alive_task.load(Ordering::Relaxed) {
                    break;
                }

                // Periodically refresh the signed PlaybackAccessToken before its TTL expires.
                if variant_fetched_at.elapsed() >= refresh_after {
                    match fetch_hls_variant_url(
                        &http_task,
                        &task_gql_ctx,
                        &token_owned,
                        &channel_owned.login,
                    )
                    .await
                    {
                        Ok(Some(next)) => {
                            variant_url = next.variant_url;
                            refresh_after = next.refresh_after;
                            variant_fetched_at = Instant::now();
                            seen_chunks.clear();
                            if let Ok(Some(fresh_info)) =
                                resolve_channel_and_broadcast_info(&http_task, &token_owned, &channel_owned).await
                            {
                                broadcast_info = Some(fresh_info);
                            }
                        }
                        Ok(None) => {
                            info!(
                                channel = %channel_owned.login,
                                "Twitch stream went offline during HLS token refresh"
                            );
                            offline_task.store(true, Ordering::Relaxed);
                            break;
                        }
                        Err(err) => {
                            warn!(
                                channel = %channel_owned.login,
                                error = %err,
                                "Twitch HLS token refresh failed"
                            );
                        }
                    }
                }

                match send_hls_head_cycle(
                    &http_task,
                    &channel_owned.login,
                    &variant_url,
                    &mut seen_chunks,
                )
                .await
                {
                    Ok(CycleOutcome::Watched(n)) => {
                        consecutive_hls_failures = 0;
                        debug!(
                            channel = %channel_owned.login,
                            chunks = n,
                            "Sent lightweight Twitch HLS HEAD watch heartbeat"
                        );
                    }
                    Ok(CycleOutcome::RefreshNeeded) => {
                        match fetch_hls_variant_url(
                            &http_task,
                            &task_gql_ctx,
                            &token_owned,
                            &channel_owned.login,
                        )
                        .await
                        {
                            Ok(Some(next)) => {
                                variant_url = next.variant_url;
                                refresh_after = next.refresh_after;
                                variant_fetched_at = Instant::now();
                                seen_chunks.clear();
                                if let Ok(Some(fresh_info)) =
                                    resolve_channel_and_broadcast_info(&http_task, &token_owned, &channel_owned).await
                                {
                                    broadcast_info = Some(fresh_info);
                                }
                                match send_hls_head_cycle(
                                    &http_task,
                                    &channel_owned.login,
                                    &variant_url,
                                    &mut seen_chunks,
                                )
                                .await
                                {
                                    Ok(CycleOutcome::Watched(n)) => {
                                        consecutive_hls_failures = 0;
                                        debug!(
                                            channel = %channel_owned.login,
                                            chunks = n,
                                            "Refreshed Twitch HLS playlist and resumed HEAD watch"
                                        );
                                    }
                                    Ok(CycleOutcome::RefreshNeeded) => {
                                        consecutive_hls_failures += 1;
                                        warn!(
                                            channel = %channel_owned.login,
                                            failures = consecutive_hls_failures,
                                            "Refreshed Twitch HLS playlist still returned no valid chunks"
                                        );
                                    }
                                    Err(err) => {
                                        consecutive_hls_failures += 1;
                                        warn!(
                                            channel = %channel_owned.login,
                                            failures = consecutive_hls_failures,
                                            error = %err,
                                            "Twitch HLS HEAD cycle failed after playlist refresh"
                                        );
                                    }
                                }
                            }
                            Ok(None) => {
                                info!(
                                    channel = %channel_owned.login,
                                    "Twitch stream ended (HLS playlist offline)"
                                );
                                offline_task.store(true, Ordering::Relaxed);
                                break;
                            }
                            Err(err) => {
                                consecutive_hls_failures += 1;
                                warn!(
                                    channel = %channel_owned.login,
                                    failures = consecutive_hls_failures,
                                    error = %err,
                                    "Twitch HLS playlist refresh failed"
                                );
                            }
                        }
                    }
                    Err(err) => {
                        consecutive_hls_failures += 1;
                        warn!(
                            channel = %channel_owned.login,
                            failures = consecutive_hls_failures,
                            error = %err,
                            "Twitch HLS HEAD cycle error"
                        );
                    }
                }

                if consecutive_hls_failures >= MAX_CONSECUTIVE_HLS_FAILURES {
                    warn!(
                        channel = %channel_owned.login,
                        failures = consecutive_hls_failures,
                        "Stopping Twitch HLS watcher after repeated failures so miner can reconnect"
                    );
                    break;
                }

                // Every ~60s on an independent timer, send the companion Spade minute-watched beacon.
                if last_spade_at.elapsed() >= Duration::from_secs(SPADE_WATCH_INTERVAL_SECS) {
                    last_spade_at = Instant::now();

                    if user_id_opt.is_none() {
                        match fetch_user_id(&http_task, &token_owned).await {
                            Ok(uid) => {
                                user_id_opt = Some(uid);
                                if let Ok(mut g) = cached_user_id_arc.lock() {
                                    *g = Some(uid);
                                }
                            }
                            Err(err) => {
                                warn!(
                                    channel = %channel_owned.login,
                                    error = %err,
                                    "Retry fetching Twitch user_id for Spade failed"
                                );
                            }
                        }
                    }

                    if broadcast_info.is_none() {
                        match resolve_channel_and_broadcast_info(
                            &http_task,
                            &token_owned,
                            &channel_owned,
                        )
                        .await
                        {
                            Ok(info) => broadcast_info = info,
                            Err(err) => {
                                warn!(
                                    channel = %channel_owned.login,
                                    error = %err,
                                    "Retry resolving Twitch broadcast_id for Spade failed"
                                );
                            }
                        }
                    }

                    if let (Some(uid), Some(ref info)) = (user_id_opt, &broadcast_info) {
                        if let Err(err) = send_spade_minute_watched(
                            &http_task,
                            &spade_url,
                            uid,
                            info,
                            &channel_owned.login,
                        )
                        .await
                        {
                            warn!(
                                channel = %channel_owned.login,
                                spade_url = %spade_url,
                                error = %err,
                                "Twitch Spade minute-watched beacon failed"
                            );
                        } else {
                            debug!(
                                channel = %channel_owned.login,
                                broadcast_id = %info.broadcast_id,
                                "Sent Twitch Spade minute-watched beacon"
                            );
                        }
                    }
                }
            }

            alive_task.store(false, Ordering::Relaxed);
        });

        *guard = Some(ActiveTwitchWatchTask {
            channel_login: channel.login.clone(),
            auth_token: auth_token.to_string(),
            alive,
            offline,
            handle,
        });

        Ok(WatchTickResult::Watched)
    }

    async fn get_or_fetch_user_id(
        &self,
        http: &AllowlistedClient,
        auth_token: &str,
    ) -> Result<u64, AppError> {
        if let Ok(guard) = self.cached_user_id.lock() {
            if let Some(uid) = *guard {
                return Ok(uid);
            }
        }

        let uid = fetch_user_id(http, auth_token).await?;

        if let Ok(mut guard) = self.cached_user_id.lock() {
            *guard = Some(uid);
        }
        Ok(uid)
    }

    async fn get_or_fetch_spade_url(&self, http: &AllowlistedClient) -> String {
        if let Ok(guard) = self.spade_url.lock() {
            if let Some(ref cached) = *guard {
                if cached.fetched_at.elapsed() < SPADE_CACHE_TTL {
                    return cached.url.clone();
                }
            }
        }

        let extracted = discover_spade_url(http).await;
        let resolved = match extracted {
            Some(url) => url,
            None => {
                warn!(
                    fallback = DEFAULT_SPADE_URL,
                    "Could not extract Twitch spade_url dynamically; falling back to default"
                );
                DEFAULT_SPADE_URL.to_string()
            }
        };

        if let Ok(mut guard) = self.spade_url.lock() {
            *guard = Some(CachedSpadeUrl {
                url: resolved.clone(),
                fetched_at: Instant::now(),
            });
        }
        resolved
    }
}

async fn fetch_user_id(http: &AllowlistedClient, auth_token: &str) -> Result<u64, AppError> {
    let info = validate_oauth_token(http, auth_token).await?;
    info.get("user_id")
        .and_then(|v| {
            v.as_str()
                .and_then(|s| s.parse::<u64>().ok())
                .or_else(|| v.as_u64())
        })
        .ok_or_else(|| AppError::Auth("Missing numeric user_id in Twitch session".to_string()))
}

enum CycleOutcome {
    Watched(usize),
    RefreshNeeded,
}

fn percent_encode_query_component(raw: &str) -> String {
    url::form_urlencoded::byte_serialize(raw.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}

fn parse_playback_access_token(
    pat_resp: &Value,
    now_unix_secs: u64,
) -> Result<Option<ParsedPlaybackAccessToken>, AppError> {
    let token_node = pat_resp
        .pointer("/data/streamPlaybackAccessToken")
        .filter(|v| v.is_object());
    let Some(token_node) = token_node else {
        return Ok(None);
    };

    let Some(value) = token_node.get("value").and_then(|v| v.as_str()) else {
        return Ok(None);
    };
    let Some(signature) = token_node.get("signature").and_then(|v| v.as_str()) else {
        return Ok(None);
    };

    let parsed_value = serde_json::from_str::<Value>(value).ok();
    if let Some(ref val_json) = parsed_value {
        if val_json
            .pointer("/authorization/forbidden")
            .and_then(|b| b.as_bool())
            == Some(true)
        {
            let reason = val_json
                .pointer("/authorization/reason")
                .and_then(|r| r.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("Stream playback forbidden by Twitch");
            return Err(AppError::Blocked(format!(
                "Twitch PlaybackAccessToken forbidden: {reason}"
            )));
        }
    }

    let has_user_id = parsed_value
        .as_ref()
        .and_then(|v| v.get("user_id"))
        .map(|u| match u {
            Value::Null => false,
            Value::String(s) => !s.trim().is_empty(),
            Value::Number(n) => n.as_u64().map(|id| id > 0).unwrap_or(true),
            _ => false,
        })
        .unwrap_or(false);

    let refresh_after = parsed_value
        .as_ref()
        .and_then(|v| v.get("expires"))
        .and_then(|e| e.as_u64())
        .and_then(|exp| exp.checked_sub(now_unix_secs))
        .map(|remaining| {
            let target = remaining.saturating_sub(180);
            Duration::from_secs(target.clamp(MIN_HLS_TOKEN_REFRESH_SECS, MAX_HLS_TOKEN_REFRESH_SECS))
        })
        .unwrap_or(Duration::from_secs(DEFAULT_HLS_TOKEN_REFRESH_SECS));

    Ok(Some(ParsedPlaybackAccessToken {
        value: value.to_string(),
        signature: signature.to_string(),
        has_user_id,
        refresh_after,
    }))
}

async fn sync_twitch_user_agent(http: &AllowlistedClient, profile_id: &str) {
    if let Some(ua) = cached_twitch_user_agent(profile_id) {
        http.set_user_agent(ua).await;
    }
}

async fn ensure_watch_integrity_context(
    http: &AllowlistedClient,
    ctx: &WatchGqlContext<'_>,
    auth_token: &str,
    force_refresh: bool,
) {
    if !force_refresh && get_twitch_integrity_context(ctx.profile_id, auth_token).is_some() {
        sync_twitch_user_agent(http, ctx.profile_id).await;
        return;
    }
    let Some(app_handle) = ctx.app else {
        return;
    };

    if !force_refresh {
        if let Some(last_mutex) = ctx.last_integrity_attempt_at {
            if let Ok(mut guard) = last_mutex.lock() {
                if let Some(prev) = *guard {
                    if prev.elapsed() < Duration::from_secs(INTEGRITY_BRIDGE_COOLDOWN_SECS) {
                        return;
                    }
                }
                *guard = Some(Instant::now());
            }
        }
    } else {
        clear_twitch_integrity_context(ctx.profile_id);
        if let Some(last_mutex) = ctx.last_integrity_attempt_at {
            if let Ok(mut guard) = last_mutex.lock() {
                *guard = Some(Instant::now());
            }
        }
    }

    let _bridge_guard = match ctx.bridge_lock {
        Some(lock) => Some(lock.lock().await),
        None => None,
    };

    if !force_refresh && get_twitch_integrity_context(ctx.profile_id, auth_token).is_some() {
        sync_twitch_user_agent(http, ctx.profile_id).await;
        return;
    }

    match twitch_bridge_capture_for_profile(
        app_handle,
        ctx.profile_id,
        "https://www.twitch.tv/drops/campaigns",
        "campaigns",
    )
    .await
    {
        Ok(_) => {
            sync_twitch_user_agent(http, ctx.profile_id).await;
        }
        Err(err) => {
            debug!(
                profile_id = ctx.profile_id,
                error = %err,
                "Twitch integrity pre-sync via official page skipped"
            );
        }
    }
}

fn build_usher_master_url(login_lower: &str, signature: &str, value: &str) -> String {
    let encoded_sig = percent_encode_query_component(signature);
    let encoded_token = percent_encode_query_component(value);
    format!("{TWITCH_USHER_HLS_BASE}/{login_lower}.m3u8?sig={encoded_sig}&token={encoded_token}")
}

async fn fetch_hls_variant_url(
    http: &AllowlistedClient,
    gql_ctx: &WatchGqlContext<'_>,
    auth_token: &str,
    channel_login: &str,
) -> Result<Option<HlsVariantTarget>, AppError> {
    let login_lower = channel_login.to_ascii_lowercase();
    let vars = json!({
        "isLive": true,
        "isVod": false,
        "login": login_lower,
        "platform": "web",
        "playerType": "site",
        "vodID": ""
    });

    ensure_watch_integrity_context(http, gql_ctx, auth_token, false).await;

    let pat_resp = match execute_persisted_gql_for_profile(
        http,
        gql_ctx.profile_id,
        auth_token,
        ops::PLAYBACK_ACCESS_TOKEN,
        vars.clone(),
    )
    .await
    {
        Ok(v) => v,
        Err(AppError::Blocked(_)) if gql_ctx.app.is_some() => {
            ensure_watch_integrity_context(http, gql_ctx, auth_token, true).await;
            execute_persisted_gql_for_profile(
                http,
                gql_ctx.profile_id,
                auth_token,
                ops::PLAYBACK_ACCESS_TOKEN,
                vars.clone(),
            )
            .await?
        }
        Err(AppError::PlatformChanged(_)) => {
            execute_persisted_gql_for_profile(
                http,
                gql_ctx.profile_id,
                auth_token,
                ops::PLAYBACK_ACCESS_TOKEN_TEMPLATE,
                vars.clone(),
            )
            .await?
        }
        Err(err) => return Err(err),
    };

    let now_secs = Utc::now().timestamp().max(0) as u64;
    let Some(mut parsed_pat) = parse_playback_access_token(&pat_resp, now_secs)? else {
        return Ok(None);
    };

    if !parsed_pat.has_user_id {
        warn!(
            channel = %login_lower,
            "Twitch PlaybackAccessToken omitted user_id; verifying OAuth session and trying template fallback"
        );
        // Confirm OAuth token isn't expired first so 401 surfaces cleanly as AppError::Auth.
        let _ = validate_oauth_token(http, auth_token).await?;

        if let Ok(fallback_resp) = execute_persisted_gql_for_profile(
            http,
            gql_ctx.profile_id,
            auth_token,
            ops::PLAYBACK_ACCESS_TOKEN_TEMPLATE,
            vars,
        )
        .await
        {
            if let Some(fallback_pat) = parse_playback_access_token(&fallback_resp, now_secs)? {
                if fallback_pat.has_user_id {
                    parsed_pat = fallback_pat;
                }
            }
        }
    }

    if !parsed_pat.has_user_id {
        return Err(AppError::Blocked(
            "Twitch returned an anonymous playback token without user_id (cannot earn Drops); enable Compatibility Watch Mode in Settings or log in again"
                .to_string(),
        ));
    }

    let master_url =
        build_usher_master_url(&login_lower, &parsed_pat.signature, &parsed_pat.value);

    let resp = http
        .get_twitch_cdn_text("twitch_hls_master", &master_url, false)
        .await?;

    if resp.status.as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status.is_success() {
        return Err(AppError::Network(format!(
            "Twitch Usher HLS master returned HTTP {}",
            resp.status.as_u16()
        )));
    }

    // Usher returns JSON error objects when a stream is offline or unavailable.
    let trimmed = resp.body.trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        warn!(
            channel = %login_lower,
            body = %trimmed.chars().take(200).collect::<String>(),
            "Twitch Usher HLS master returned JSON error payload"
        );
        return Ok(None);
    }

    Ok(extract_lowest_variant_url(&resp.body).map(|variant_url| HlsVariantTarget {
        variant_url,
        refresh_after: parsed_pat.refresh_after,
    }))
}

async fn send_hls_head_cycle(
    http: &AllowlistedClient,
    channel_login: &str,
    variant_url: &str,
    seen_chunks: &mut VecDeque<String>,
) -> Result<CycleOutcome, AppError> {
    let media_resp = http
        .get_twitch_cdn_text("twitch_hls_media", variant_url, true)
        .await?;

    if media_resp.status.as_u16() >= 400 {
        warn!(
            channel = %channel_login,
            status = media_resp.status.as_u16(),
            "Twitch HLS media playlist returned error status; refreshing playlist"
        );
        return Ok(CycleOutcome::RefreshNeeded);
    }

    let trimmed = media_resp.body.trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        warn!(
            channel = %channel_login,
            body = %trimmed.chars().take(200).collect::<String>(),
            "Twitch HLS media playlist returned JSON payload; refreshing playlist"
        );
        return Ok(CycleOutcome::RefreshNeeded);
    }

    let all_chunks = extract_chunk_urls(&media_resp.body);
    if all_chunks.is_empty() {
        warn!(
            channel = %channel_login,
            "Twitch HLS media playlist contained no recognized segment URLs; refreshing playlist"
        );
        return Ok(CycleOutcome::RefreshNeeded);
    }

    // Send HEAD requests on unseen chunks (covering every new 2s segment per 12s cycle),
    // falling back to the latest 2 chunks if the playlist hasn't rolled yet.
    let mut targets: Vec<String> = all_chunks
        .iter()
        .filter(|u| !seen_chunks.iter().any(|s| s == *u))
        .cloned()
        .collect();
    if targets.is_empty() {
        targets = all_chunks.iter().rev().take(2).cloned().collect();
    }

    let mut ok_count = 0usize;
    let mut non_ok_status: Option<u16> = None;
    let mut last_err: Option<String> = None;

    for chunk_url in &targets {
        match http.head_twitch_cdn("twitch_hls_head", chunk_url).await {
            Ok(status) if status.is_success() => {
                ok_count += 1;
                if !seen_chunks.iter().any(|s| s == chunk_url) {
                    if seen_chunks.len() >= MAX_SEEN_CHUNKS {
                        seen_chunks.pop_front();
                    }
                    seen_chunks.push_back(chunk_url.clone());
                }
            }
            Ok(status) if status.as_u16() == 403 => {
                warn!(
                    channel = %channel_login,
                    "Twitch HLS chunk HEAD returned 403; refreshing playback token"
                );
                return Ok(CycleOutcome::RefreshNeeded);
            }
            Ok(status) => {
                non_ok_status = Some(status.as_u16());
            }
            Err(err) => {
                last_err = Some(err.to_string());
            }
        }
    }

    if ok_count == 0 {
        warn!(
            channel = %channel_login,
            targets = targets.len(),
            status = ?non_ok_status,
            error = ?last_err,
            "All Twitch HLS chunk HEAD requests failed; refreshing playlist"
        );
        return Ok(CycleOutcome::RefreshNeeded);
    }

    if non_ok_status.is_some() || last_err.is_some() {
        debug!(
            channel = %channel_login,
            ok_count,
            targets = targets.len(),
            status = ?non_ok_status,
            error = ?last_err,
            "Some Twitch HLS chunk HEAD requests failed in cycle"
        );
    }

    Ok(CycleOutcome::Watched(ok_count))
}

fn extract_lowest_variant_url(master_m3u8: &str) -> Option<String> {
    let mut pending_bw: Option<(bool, u64)> = None;
    let mut best_video: Option<(u64, String)> = None;
    let mut best_any: Option<(u64, String)> = None;
    let mut last_url: Option<String> = None;

    for raw_line in master_m3u8.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(attrs) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let is_audio_only = attrs.contains("audio_only");
            let bw = attrs
                .split(',')
                .find_map(|part| {
                    part.trim()
                        .strip_prefix("BANDWIDTH=")
                        .and_then(|v| v.parse::<u64>().ok())
                })
                .unwrap_or(u64::MAX - 1);
            pending_bw = Some((is_audio_only, bw));
        } else if !line.starts_with('#') && line.starts_with("https://") {
            let url = line.to_string();
            last_url = Some(url.clone());
            if let Some((is_audio_only, bw)) = pending_bw.take() {
                if !is_audio_only && best_video.as_ref().map(|(b, _)| bw < *b).unwrap_or(true) {
                    best_video = Some((bw, url.clone()));
                }
                if best_any.as_ref().map(|(b, _)| bw < *b).unwrap_or(true) {
                    best_any = Some((bw, url));
                }
            }
        }
    }

    best_video
        .or(best_any)
        .map(|(_, url)| url)
        .or(last_url)
}

fn is_twitch_segment_host(host: &str) -> bool {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    (h.ends_with(".ttvnw.net") && h.len() > ".ttvnw.net".len())
        || (h.ends_with(".live-video.net") && h.len() > ".live-video.net".len())
}

fn extract_chunk_urls(media_m3u8: &str) -> Vec<String> {
    media_m3u8
        .lines()
        .map(str::trim)
        .filter(|line| {
            line.starts_with("https://")
                && url::Url::parse(line)
                    .ok()
                    .and_then(|u| u.host_str().map(is_twitch_segment_host))
                    .unwrap_or(false)
        })
        .map(str::to_string)
        .collect()
}

async fn resolve_channel_and_broadcast_info(
    http: &AllowlistedClient,
    auth_token: &str,
    channel: &Channel,
) -> Result<Option<BroadcastInfo>, AppError> {
    let payload = match fetch_channel_live_status(http, Some(auth_token), &channel.login).await {
        Ok(v) => v,
        Err(AppError::Blocked(_)) => {
            fetch_channel_live_status(http, None, &channel.login).await?
        }
        Err(err) => {
            if let Some(ref sid) = channel.stream_id {
                if !sid.is_empty() && channel.id.chars().all(|c| c.is_ascii_digit()) {
                    return Ok(Some(BroadcastInfo {
                        channel_id: channel.id.clone(),
                        broadcast_id: sid.clone(),
                        game_id: String::new(),
                        game_name: channel.game_name.clone(),
                    }));
                }
            }
            return Err(err);
        }
    };

    let Some(user_node) = payload.pointer("/data/user").filter(|u| !u.is_null()) else {
        return Ok(None);
    };
    let Some(stream_node) = user_node.get("stream").filter(|s| !s.is_null()) else {
        return Ok(None);
    };
    let cid = user_node
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
        .or_else(|| {
            (!channel.id.is_empty() && channel.id.chars().all(|c| c.is_ascii_digit()))
                .then_some(channel.id.as_str())
        });
    let Some(cid) = cid else {
        return Ok(None);
    };
    let Some(sid) = stream_node
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    else {
        return Ok(None);
    };

    let game_id = stream_node
        .pointer("/game/id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let game_name = stream_node
        .pointer("/game/displayName")
        .or_else(|| stream_node.pointer("/game/name"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(&channel.game_name)
        .to_string();

    Ok(Some(BroadcastInfo {
        channel_id: cid.to_string(),
        broadcast_id: sid.to_string(),
        game_id,
        game_name,
    }))
}

async fn discover_spade_url(http: &AllowlistedClient) -> Option<String> {
    let html_resp = http
        .request_platform(
            PlatformId::Twitch,
            "twitch_spade_page",
            Method::GET,
            "https://www.twitch.tv/directory",
            HeaderMap::new(),
            None,
        )
        .await
        .ok()?;

    if let Some(inline) = extract_spade_from_text(&html_resp.body) {
        return Some(inline);
    }

    let settings_url = extract_settings_js_url(&html_resp.body)?;
    let js_resp = http
        .request_platform(
            PlatformId::Twitch,
            "twitch_spade_settings_js",
            Method::GET,
            &settings_url,
            HeaderMap::new(),
            None,
        )
        .await
        .ok()?;

    extract_spade_from_text(&js_resp.body)
}

fn build_spade_minute_watched_payload(
    user_id: u64,
    info: &BroadcastInfo,
    channel_login: &str,
    client_time_iso: &str,
) -> Value {
    let mut props = serde_json::Map::new();
    props.insert("broadcast_id".to_string(), json!(info.broadcast_id));
    props.insert("channel_id".to_string(), json!(info.channel_id));
    props.insert("channel".to_string(), json!(channel_login));
    props.insert("client_time".to_string(), json!(client_time_iso));
    props.insert("game".to_string(), json!(info.game_name));
    if !info.game_id.is_empty() {
        props.insert("game_id".to_string(), json!(info.game_id));
    }
    props.insert("hidden".to_string(), json!(false));
    props.insert("is_live".to_string(), json!(true));
    props.insert("live".to_string(), json!(true));
    props.insert("location".to_string(), json!("channel"));
    props.insert("logged_in".to_string(), json!(true));
    props.insert("minutes_logged".to_string(), json!(1));
    props.insert("muted".to_string(), json!(false));
    props.insert("player".to_string(), json!("site"));
    props.insert("user_id".to_string(), json!(user_id));

    json!([
        {
            "event": "minute-watched",
            "properties": Value::Object(props),
        }
    ])
}

async fn send_spade_minute_watched(
    http: &AllowlistedClient,
    spade_url: &str,
    user_id: u64,
    info: &BroadcastInfo,
    channel_login: &str,
) -> Result<(), AppError> {
    let client_time = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let payload = build_spade_minute_watched_payload(user_id, info, channel_login, &client_time)
        .to_string();
    let encoded = encode_base64(payload.as_bytes());
    let form_body = format!(
        "data={}",
        url::form_urlencoded::byte_serialize(encoded.as_bytes()).collect::<String>()
    );
    let mut headers = HeaderMap::new();
    headers.insert(ORIGIN, HeaderValue::from_static("https://www.twitch.tv"));
    headers.insert(REFERER, HeaderValue::from_static("https://www.twitch.tv/"));

    let mut attempt = 0u8;
    loop {
        attempt += 1;
        let resp = match http
            .post_form_platform(
                PlatformId::Twitch,
                "twitch_spade_watch",
                spade_url,
                headers.clone(),
                form_body.clone(),
            )
            .await
        {
            Ok(r) => r,
            Err(AppError::Network(_)) if attempt < 2 => {
                tokio::time::sleep(Duration::from_millis(400)).await;
                continue;
            }
            Err(err) => return Err(err),
        };

        if !resp.status.is_success() {
            return Err(AppError::Network(format!(
                "Twitch Spade returned HTTP {}",
                resp.status.as_u16()
            )));
        }
        return Ok(());
    }
}

fn extract_settings_js_url(html: &str) -> Option<String> {
    for prefix in [
        "https://assets.twitch.tv/config/settings.",
        "https://static.twitchcdn.net/config/settings.",
    ] {
        if let Some(start) = html.find(prefix) {
            let rest = &html[start..];
            if let Some(end) = rest.find(".js") {
                return Some(rest[..end + 3].to_string());
            }
        }
    }
    None
}

fn extract_spade_from_text(text: &str) -> Option<String> {
    for key in ["\"spade_url\"", "\"beacon_url\""] {
        let mut search_from = 0;
        while let Some(rel) = text[search_from..].find(key) {
            let after_key = &text[search_from + rel + key.len()..];
            let trimmed = after_key.trim_start();
            if let Some(after_colon) = trimmed.strip_prefix(':') {
                let after_colon = after_colon.trim_start();
                if let Some(after_quote) = after_colon.strip_prefix('"') {
                    if let Some(end_quote) = after_quote.find('"') {
                        let candidate = &after_quote[..end_quote];
                        if let Ok(parsed) = url::Url::parse(candidate) {
                            if validate_url(&parsed, AllowlistKind::Platform).is_ok() {
                                return Some(candidate.to_string());
                            }
                        }
                    }
                }
            }
            search_from += rel + key.len();
        }
    }
    None
}

fn encode_base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut i = 0;
    while i < input.len() {
        let b0 = input[i];
        let b1 = if i + 1 < input.len() { input[i + 1] } else { 0 };
        let b2 = if i + 2 < input.len() { input[i + 2] } else { 0 };
        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        if i + 1 < input.len() {
            out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if i + 2 < input.len() {
            out.push(TABLE[(n & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        i += 3;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        build_spade_minute_watched_payload, build_usher_master_url, encode_base64,
        extract_chunk_urls, extract_lowest_variant_url, extract_settings_js_url,
        extract_spade_from_text, parse_playback_access_token, percent_encode_query_component,
        BroadcastInfo,
    };
    use crate::platforms::AppError;
    use serde_json::json;

    #[test]
    fn encodes_standard_base64_and_extracts_spade_url() {
        assert_eq!(encode_base64(b"hello"), "aGVsbG8=");
        assert_eq!(encode_base64(b"a"), "YQ==");
        assert_eq!(encode_base64(b"ab"), "YWI=");
        assert_eq!(encode_base64(b"abc"), "YWJj");

        let html = r#"<script src="https://assets.twitch.tv/config/settings.abc123.js"></script>"#;
        assert_eq!(
            extract_settings_js_url(html).as_deref(),
            Some("https://assets.twitch.tv/config/settings.abc123.js")
        );

        let js = r#"{"other":"https://evil.example/track","beacon_url":"https://beacon.twitch.tv/track","spade_url":"https://spade.twitch.tv/track"}"#;
        assert_eq!(
            extract_spade_from_text(js).as_deref(),
            Some("https://spade.twitch.tv/track")
        );

        // Must not jump past a non-URL spade_url value to grab an unrelated URL later in the JS.
        let decoy = r#"{"spade_url":null,"other":"https://www.twitch.tv/something"}"#;
        assert_eq!(extract_spade_from_text(decoy), None);
    }

    #[test]
    fn parses_master_lowest_variant_and_media_chunk_urls_including_live_video_net() {
        let master = "\
#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=6000000
https://euc11.playlist.ttvnw.net/v1/playlist/high.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=240000,VIDEO=\"160p30\"
https://euc11.playlist.ttvnw.net/v1/playlist/low.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=128000,VIDEO=\"audio_only\"
https://euc11.playlist.ttvnw.net/v1/playlist/audio.m3u8
";
        assert_eq!(
            extract_lowest_variant_url(master).as_deref(),
            Some("https://euc11.playlist.ttvnw.net/v1/playlist/low.m3u8")
        );

        let media = "\
#EXTM3U
#EXTINF:2.000,live
https://abc.cloudfront.hls.ttvnw.net/v1/segment/1.ts
#EXTINF:2.000,live
https://fa6293.rufio.hls.live-video.net/v1/segment/2.ts
#EXTINF:2.000,live
https://evil.example.com/v1/segment/3.ts
";
        let chunks = extract_chunk_urls(media);
        assert_eq!(chunks.len(), 2);
        assert_eq!(
            chunks[0],
            "https://abc.cloudfront.hls.ttvnw.net/v1/segment/1.ts"
        );
        assert_eq!(
            chunks[1],
            "https://fa6293.rufio.hls.live-video.net/v1/segment/2.ts"
        );
    }

    #[test]
    fn validates_playback_access_token_user_id_and_forbidden_state() {
        let now = 1_700_000_000u64;
        let authed = json!({
            "data": {
                "streamPlaybackAccessToken": {
                    "value": "{\"authorization\":{\"forbidden\":false,\"reason\":\"\"},\"user_id\":987654,\"expires\":1700001200}",
                    "signature": "sig123"
                }
            }
        });
        let parsed = parse_playback_access_token(&authed, now)
            .expect("ok")
            .expect("present");
        assert!(parsed.has_user_id);
        assert_eq!(parsed.refresh_after.as_secs(), 900);

        let anon = json!({
            "data": {
                "streamPlaybackAccessToken": {
                    "value": "{\"authorization\":{\"forbidden\":false,\"reason\":\"\"},\"user_id\":null,\"expires\":1700001200}",
                    "signature": "sig123"
                }
            }
        });
        let parsed_anon = parse_playback_access_token(&anon, now)
            .expect("ok")
            .expect("present");
        assert!(!parsed_anon.has_user_id);

        let forbidden = json!({
            "data": {
                "streamPlaybackAccessToken": {
                    "value": "{\"authorization\":{\"forbidden\":true,\"reason\":\"geoblocked\"},\"user_id\":987654}",
                    "signature": "sig123"
                }
            }
        });
        assert!(matches!(
            parse_playback_access_token(&forbidden, now),
            Err(AppError::Blocked(_))
        ));
    }

    #[test]
    fn formats_spade_minute_watched_payload_and_query_encoding() {
        let info = BroadcastInfo {
            channel_id: "50988750".to_string(),
            broadcast_id: "316766162546".to_string(),
            game_id: "515025".to_string(),
            game_name: "Overwatch".to_string(),
        };
        let payload = build_spade_minute_watched_payload(
            123456,
            &info,
            "stylishnoob4",
            "2026-11-24T16:15:00.123Z",
        );
        let props = &payload[0]["properties"];
        assert_eq!(payload[0]["event"], "minute-watched");
        assert_eq!(props["broadcast_id"], "316766162546");
        assert_eq!(props["channel_id"], "50988750");
        assert_eq!(props["channel"], "stylishnoob4");
        assert_eq!(props["client_time"], "2026-11-24T16:15:00.123Z");
        assert_eq!(props["game"], "Overwatch");
        assert_eq!(props["game_id"], "515025");
        assert_eq!(props["location"], "channel");
        assert_eq!(props["player"], "site");
        assert_eq!(props["user_id"], 123456);

        assert_eq!(
            percent_encode_query_component("{\"a\":\"b c\"}"),
            "%7B%22a%22%3A%22b%20c%22%7D"
        );
        assert_eq!(
            build_usher_master_url("kenki521", "sig123", "{\"user_id\":1}"),
            "https://usher.ttvnw.net/api/channel/hls/kenki521.m3u8?sig=sig123&token=%7B%22user_id%22%3A1%7D"
        );
    }
}
