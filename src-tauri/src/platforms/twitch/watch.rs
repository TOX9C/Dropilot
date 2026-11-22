use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use reqwest::header::HeaderMap;
use reqwest::Method;
use serde_json::json;
use tauri::AppHandle;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::interval;
use tracing::{debug, info, warn};

use crate::auth::login_window::{close_compat_watch_window, ensure_compat_watch_window};
use crate::net::client::AllowlistedClient;
use crate::platforms::twitch::gql::{
    execute_persisted_gql, fetch_channel_live_status, ops, validate_oauth_token,
    TWITCH_USHER_HLS_BASE,
};
use crate::platforms::{AppError, Channel, PlatformId, WatchTickResult};

const DEFAULT_SPADE_URL: &str = "https://spade.twitch.tv/track";
const SPADE_CACHE_TTL: Duration = Duration::from_secs(6 * 3600);
const HLS_WATCH_INTERVAL_SECS: u64 = 20;
const HLS_TOKEN_REFRESH_SECS: u64 = 15 * 60;
const MAX_SEEN_CHUNKS: usize = 64;

#[derive(Debug, Clone)]
struct CachedSpadeUrl {
    url: String,
    fetched_at: Instant,
}

struct ActiveTwitchWatchTask {
    channel_login: String,
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
    }

    pub async fn tick(
        &self,
        http: &Arc<AllowlistedClient>,
        app: Option<&AppHandle>,
        auth_token: &str,
        channel: &Channel,
        compat_mode: bool,
    ) -> Result<WatchTickResult, AppError> {
        if compat_mode {
            self.close().await;
            if let Some(app_handle) = app {
                ensure_compat_watch_window(app_handle, PlatformId::Twitch, &channel.login)?;
            }
            return Ok(WatchTickResult::Watched);
        }

        // Default ultra-light stream-less mode: never keep a hidden Chromium video player open.
        if let Some(app_handle) = app {
            close_compat_watch_window(app_handle, PlatformId::Twitch);
        }

        let mut guard = self.task.lock().await;
        if let Some(ref active) = *guard {
            if active.channel_login.eq_ignore_ascii_case(&channel.login)
                && active.alive.load(Ordering::Relaxed)
                && !active.handle.is_finished()
            {
                if active.offline.load(Ordering::Relaxed) {
                    if let Some(prev) = guard.take() {
                        prev.stop();
                    }
                    return Ok(WatchTickResult::StreamOffline);
                }
                return Ok(WatchTickResult::Watched);
            }
            if active.channel_login.eq_ignore_ascii_case(&channel.login)
                && active.offline.load(Ordering::Relaxed)
            {
                if let Some(prev) = guard.take() {
                    prev.stop();
                }
                return Ok(WatchTickResult::StreamOffline);
            }
        }

        if let Some(prev) = guard.take() {
            prev.stop();
        }

        // 1. Obtain an authenticated HLS lowest-quality variant playlist URL using the user's
        //    real Twitch OAuth token (PlaybackAccessToken encodes user_id for drop credit).
        let Some((initial_variant_url, pat_has_user_id)) =
            fetch_hls_variant_url(http, auth_token, &channel.login).await?
        else {
            return Ok(WatchTickResult::StreamOffline);
        };

        // 2. Immediately perform the first zero-video-byte HLS HEAD watch cycle synchronously
        //    so we confirm the stream is live and start earning drop credit right away.
        let mut seen_chunks = VecDeque::with_capacity(MAX_SEEN_CHUNKS);
        let initial_chunks = match send_hls_head_cycle(http, &initial_variant_url, &mut seen_chunks).await {
            Ok(CycleOutcome::Watched(n)) => n,
            Ok(CycleOutcome::RefreshNeeded) => {
                warn!(
                    channel = %channel.login,
                    "Initial Twitch HLS media playlist returned error; treating stream offline"
                );
                return Ok(WatchTickResult::StreamOffline);
            }
            Err(err) => return Err(err),
        };

        info!(
            channel = %channel.login,
            chunks = initial_chunks,
            authenticated = pat_has_user_id,
            "Connected lightweight Twitch stream-less HLS HEAD watcher (0 Mbps video)"
        );

        // Pre-resolve user_id + spade_url (best-effort companion analytics ping every 60s).
        let user_id_opt = self.get_or_fetch_user_id(http, auth_token).await.ok();
        let spade_url = self.get_or_fetch_spade_url(http).await;
        let broadcast_info = resolve_channel_and_broadcast_ids(http, auth_token, channel)
            .await
            .ok()
            .flatten();

        if let (Some(uid), Some((ref cid, ref bid))) = (user_id_opt, &broadcast_info) {
            let _ = send_spade_minute_watched(
                http,
                &spade_url,
                uid,
                cid,
                bid,
                &channel.login,
                &channel.game_name,
            )
            .await;
        }

        let alive = Arc::new(AtomicBool::new(true));
        let offline = Arc::new(AtomicBool::new(false));
        let alive_task = alive.clone();
        let offline_task = offline.clone();
        let http_task = http.clone();
        let token_owned = auth_token.to_string();
        let login_owned = channel.login.clone();
        let game_owned = channel.game_name.clone();

        let handle = tokio::spawn(async move {
            let mut variant_url = initial_variant_url;
            let mut variant_fetched_at = Instant::now();
            let mut cycle_count: u64 = 1;
            let mut ticker = interval(Duration::from_secs(HLS_WATCH_INTERVAL_SECS));
            // Skip immediate first tick since we just ran the initial cycle above.
            ticker.tick().await;

            while alive_task.load(Ordering::Relaxed) {
                ticker.tick().await;
                if !alive_task.load(Ordering::Relaxed) {
                    break;
                }

                // Periodically refresh the signed PlaybackAccessToken before its ~20m TTL expires.
                if variant_fetched_at.elapsed() >= Duration::from_secs(HLS_TOKEN_REFRESH_SECS) {
                    match fetch_hls_variant_url(&http_task, &token_owned, &login_owned).await {
                        Ok(Some((next_url, _))) => {
                            variant_url = next_url;
                            variant_fetched_at = Instant::now();
                            seen_chunks.clear();
                        }
                        Ok(None) => {
                            info!(channel = %login_owned, "Twitch stream went offline during HLS token refresh");
                            offline_task.store(true, Ordering::Relaxed);
                            break;
                        }
                        Err(err) => {
                            debug!(channel = %login_owned, error = %err, "Transient Twitch HLS token refresh error");
                        }
                    }
                }

                match send_hls_head_cycle(&http_task, &variant_url, &mut seen_chunks).await {
                    Ok(CycleOutcome::Watched(n)) => {
                        cycle_count += 1;
                        debug!(
                            channel = %login_owned,
                            chunks = n,
                            cycle = cycle_count,
                            "Sent lightweight Twitch HLS HEAD watch heartbeat"
                        );
                    }
                    Ok(CycleOutcome::RefreshNeeded) => {
                        match fetch_hls_variant_url(&http_task, &token_owned, &login_owned).await {
                            Ok(Some((next_url, _))) => {
                                variant_url = next_url;
                                variant_fetched_at = Instant::now();
                                seen_chunks.clear();
                                if let Ok(CycleOutcome::Watched(n)) =
                                    send_hls_head_cycle(&http_task, &variant_url, &mut seen_chunks)
                                        .await
                                {
                                    cycle_count += 1;
                                    debug!(
                                        channel = %login_owned,
                                        chunks = n,
                                        "Refreshed Twitch HLS playlist and resumed HEAD watch"
                                    );
                                }
                            }
                            Ok(None) => {
                                info!(channel = %login_owned, "Twitch stream ended (HLS playlist offline)");
                                offline_task.store(true, Ordering::Relaxed);
                                break;
                            }
                            Err(err) => {
                                debug!(channel = %login_owned, error = %err, "Twitch HLS refresh failed; retrying next tick");
                            }
                        }
                    }
                    Err(err) => {
                        debug!(channel = %login_owned, error = %err, "Transient Twitch HLS HEAD cycle error");
                    }
                }

                // Every 3 cycles (~60s), also send a lightweight 300-byte Spade minute-watched event.
                if cycle_count % 3 == 0 {
                    if let (Some(uid), Some((ref cid, ref bid))) = (user_id_opt, &broadcast_info) {
                        let _ = send_spade_minute_watched(
                            &http_task,
                            &spade_url,
                            uid,
                            cid,
                            bid,
                            &login_owned,
                            &game_owned,
                        )
                        .await;
                    }
                }
            }

            alive_task.store(false, Ordering::Relaxed);
        });

        *guard = Some(ActiveTwitchWatchTask {
            channel_login: channel.login.clone(),
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

        let info = validate_oauth_token(http, auth_token).await?;
        let uid = info
            .get("user_id")
            .and_then(|v| {
                v.as_str()
                    .and_then(|s| s.parse::<u64>().ok())
                    .or_else(|| v.as_u64())
            })
            .ok_or_else(|| AppError::Auth("Missing numeric user_id in Twitch session".to_string()))?;

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
        let resolved = extracted.unwrap_or_else(|| DEFAULT_SPADE_URL.to_string());

        if let Ok(mut guard) = self.spade_url.lock() {
            *guard = Some(CachedSpadeUrl {
                url: resolved.clone(),
                fetched_at: Instant::now(),
            });
        }
        resolved
    }
}

enum CycleOutcome {
    Watched(usize),
    RefreshNeeded,
}

async fn fetch_hls_variant_url(
    http: &AllowlistedClient,
    auth_token: &str,
    channel_login: &str,
) -> Result<Option<(String, bool)>, AppError> {
    let login_lower = channel_login.to_ascii_lowercase();
    let vars = json!({
        "isLive": true,
        "isVod": false,
        "login": login_lower,
        "platform": "web",
        "playerType": "site",
        "vodID": ""
    });

    let pat_resp = match execute_persisted_gql(http, auth_token, ops::PLAYBACK_ACCESS_TOKEN, vars.clone()).await {
        Ok(v) => v,
        Err(AppError::PlatformChanged(_)) => {
            execute_persisted_gql(http, auth_token, ops::PLAYBACK_ACCESS_TOKEN_TEMPLATE, vars).await?
        }
        Err(err) => return Err(err),
    };

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

    let has_user_id = serde_json::from_str::<serde_json::Value>(value)
        .ok()
        .and_then(|v| v.get("user_id").cloned())
        .map(|u| !u.is_null())
        .unwrap_or(false);

    let encoded_sig: String =
        url::form_urlencoded::byte_serialize(signature.as_bytes()).collect();
    let encoded_token: String =
        url::form_urlencoded::byte_serialize(value.as_bytes()).collect();
    let master_url = format!(
        "{TWITCH_USHER_HLS_BASE}/{login_lower}.m3u8?sig={encoded_sig}&token={encoded_token}"
    );

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
        return Ok(None);
    }

    Ok(extract_lowest_variant_url(&resp.body).map(|u| (u, has_user_id)))
}

async fn send_hls_head_cycle(
    http: &AllowlistedClient,
    variant_url: &str,
    seen_chunks: &mut VecDeque<String>,
) -> Result<CycleOutcome, AppError> {
    let media_resp = http
        .get_twitch_cdn_text("twitch_hls_media", variant_url, true)
        .await?;

    if media_resp.status.as_u16() >= 400 {
        return Ok(CycleOutcome::RefreshNeeded);
    }

    let trimmed = media_resp.body.trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Ok(CycleOutcome::RefreshNeeded);
    }

    let all_chunks = extract_chunk_urls(&media_resp.body);
    if all_chunks.is_empty() {
        return Ok(CycleOutcome::RefreshNeeded);
    }

    // Send HEAD requests on unseen chunks (typically 8–10 new 2s segments per 20s cycle),
    // falling back to the latest 2 chunks if the playlist hasn't rolled yet.
    let mut targets: Vec<String> = all_chunks
        .iter()
        .filter(|u| !seen_chunks.iter().any(|s| s == *u))
        .cloned()
        .collect();
    if targets.is_empty() {
        targets = all_chunks
            .iter()
            .rev()
            .take(2)
            .cloned()
            .collect();
    }

    let mut ok_count = 0usize;
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
                return Ok(CycleOutcome::RefreshNeeded);
            }
            Ok(_) | Err(_) => {}
        }
    }

    if ok_count == 0 {
        return Ok(CycleOutcome::RefreshNeeded);
    }

    Ok(CycleOutcome::Watched(ok_count))
}

fn extract_lowest_variant_url(master_m3u8: &str) -> Option<String> {
    master_m3u8
        .lines()
        .map(str::trim)
        .rfind(|line| line.starts_with("https://"))
        .map(str::to_string)
}

fn extract_chunk_urls(media_m3u8: &str) -> Vec<String> {
    media_m3u8
        .lines()
        .map(str::trim)
        .filter(|line| {
            line.starts_with("https://")
                && url::Url::parse(line)
                    .ok()
                    .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
                    .map(|h| h.ends_with(".ttvnw.net"))
                    .unwrap_or(false)
        })
        .map(str::to_string)
        .collect()
}

async fn resolve_channel_and_broadcast_ids(
    http: &AllowlistedClient,
    auth_token: &str,
    channel: &Channel,
) -> Result<Option<(String, String)>, AppError> {
    if let Some(ref sid) = channel.stream_id {
        if !sid.is_empty() && channel.id.chars().all(|c| c.is_ascii_digit()) {
            return Ok(Some((channel.id.clone(), sid.clone())));
        }
    }

    let payload = match fetch_channel_live_status(http, Some(auth_token), &channel.login).await {
        Ok(v) => v,
        Err(AppError::Blocked(_)) => {
            fetch_channel_live_status(http, None, &channel.login).await?
        }
        Err(err) => return Err(err),
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
        .filter(|s| !s.is_empty())
        .unwrap_or(&channel.id)
        .to_string();
    let Some(sid) = stream_node
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
    else {
        return Ok(None);
    };

    Ok(Some((cid, sid)))
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

async fn send_spade_minute_watched(
    http: &AllowlistedClient,
    spade_url: &str,
    user_id: u64,
    channel_id: &str,
    broadcast_id: &str,
    channel_login: &str,
    game_name: &str,
) -> Result<(), AppError> {
    let payload = json!([
        {
            "event": "minute-watched",
            "properties": {
                "broadcast_id": broadcast_id,
                "channel_id": channel_id,
                "channel": channel_login,
                "client_time": Utc::now().to_rfc3339(),
                "game": game_name,
                "hidden": false,
                "is_live": true,
                "live": true,
                "logged_in": true,
                "minutes_logged": 1,
                "muted": false,
                "player": "site",
                "user_id": user_id
            }
        }
    ])
    .to_string();
    let encoded = encode_base64(payload.as_bytes());
    let form_body = format!(
        "data={}",
        url::form_urlencoded::byte_serialize(encoded.as_bytes()).collect::<String>()
    );
    let _ = http
        .post_form_platform(
            PlatformId::Twitch,
            "twitch_spade_watch",
            spade_url,
            HeaderMap::new(),
            form_body,
        )
        .await?;
    Ok(())
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
        if let Some(pos) = text.find(key) {
            let rest = &text[pos + key.len()..];
            if let Some(https_pos) = rest.find("https://") {
                let url_rest = &rest[https_pos..];
                if let Some(end_quote) = url_rest.find('"') {
                    let candidate = &url_rest[..end_quote];
                    if candidate.starts_with("https://") && candidate.contains("twitch.tv") {
                        return Some(candidate.to_string());
                    }
                }
            }
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
        encode_base64, extract_chunk_urls, extract_lowest_variant_url, extract_settings_js_url,
        extract_spade_from_text,
    };

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

        let js = r#"{"beacon_url":"https://beacon.twitch.tv/track","spade_url":"https://spade.twitch.tv/track"}"#;
        assert_eq!(
            extract_spade_from_text(js).as_deref(),
            Some("https://spade.twitch.tv/track")
        );
    }

    #[test]
    fn parses_master_lowest_variant_and_media_chunk_urls() {
        let master = "\
#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=6000000
https://euc11.playlist.ttvnw.net/v1/playlist/high.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=240000
https://euc11.playlist.ttvnw.net/v1/playlist/low.m3u8
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
https://abc.cloudfront.hls.ttvnw.net/v1/segment/2.ts
";
        let chunks = extract_chunk_urls(media);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[1], "https://abc.cloudfront.hls.ttvnw.net/v1/segment/2.ts");
    }
}
