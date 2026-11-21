use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use reqwest::header::HeaderMap;
use reqwest::{Method, StatusCode};
use serde_json::json;
use tauri::AppHandle;
use tracing::warn;

use crate::auth::login_window::{close_compat_watch_window, ensure_compat_watch_window};
use crate::net::client::AllowlistedClient;
use crate::platforms::twitch::gql::{
    execute_persisted_gql, ops, TWITCH_USHER_HLS_BASE,
};
use crate::platforms::{AppError, Channel, PlatformId, WatchTickResult};

#[derive(Debug, Clone)]
struct CachedPlaybackToken {
    channel_login: String,
    value: String,
    signature: String,
    fetched_at: Instant,
}

#[derive(Debug, Default)]
pub struct TwitchWatchSession {
    cached_token: Mutex<Option<CachedPlaybackToken>>,
    compat_fallback_active: AtomicBool,
}

impl TwitchWatchSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn tick(
        &self,
        http: &AllowlistedClient,
        app: Option<&AppHandle>,
        auth_token: &str,
        channel: &Channel,
        compat_mode: bool,
    ) -> Result<WatchTickResult, AppError> {
        if compat_mode || self.compat_fallback_active.load(Ordering::Relaxed) {
            if let Some(app_handle) = app {
                ensure_compat_watch_window(app_handle, PlatformId::Twitch, &channel.login)?;
                return Ok(WatchTickResult::Watched);
            }
        } else if let Some(app_handle) = app {
            close_compat_watch_window(app_handle, PlatformId::Twitch);
        }

        // Lightweight official player watch-time signal:
        // Fetch lowest-bandwidth HLS playlist and send a HEAD request on the latest segment.
        // Zero video bytes downloaded, zero decoding or rendering.
        let (value, signature) = match self
            .get_or_fetch_playback_token(http, auth_token, &channel.login)
            .await
        {
            Ok(pair) => pair,
            Err(AppError::Blocked(reason)) | Err(AppError::PlatformChanged(reason)) => {
                if let Some(app_handle) = app {
                    warn!(
                        channel = %channel.login,
                        reason = %reason,
                        "PlaybackAccessToken hit integrity check; using hidden muted official player fallback"
                    );
                    self.compat_fallback_active.store(true, Ordering::Relaxed);
                    ensure_compat_watch_window(app_handle, PlatformId::Twitch, &channel.login)?;
                    return Ok(WatchTickResult::Watched);
                }
                return Err(AppError::Blocked(reason));
            }
            Err(err) => return Err(err),
        };

        let master_url = format!(
            "{TWITCH_USHER_HLS_BASE}/{}.m3u8?allow_source=true&allow_audio_only=true&player_backend=mediaplayer&sig={}&token={}",
            channel.login,
            url::form_urlencoded::byte_serialize(signature.as_bytes()).collect::<String>(),
            url::form_urlencoded::byte_serialize(value.as_bytes()).collect::<String>()
        );

        let master_resp = http
            .request_platform(
                PlatformId::Twitch,
                "twitch_hls_master",
                Method::GET,
                &master_url,
                HeaderMap::new(),
                None,
            )
            .await?;

        if master_resp.status == StatusCode::NOT_FOUND || master_resp.status == StatusCode::GONE {
            self.invalidate_token();
            return Ok(WatchTickResult::StreamOffline);
        }
        if !master_resp.status.is_success() {
            return Err(AppError::Network(format!(
                "Usher HLS master returned HTTP {}",
                master_resp.status.as_u16()
            )));
        }

        let Some(media_playlist_url) = pick_lowest_variant_url(&master_resp.body) else {
            return Ok(WatchTickResult::StreamOffline);
        };

        let media_resp = http
            .request_platform(
                PlatformId::Twitch,
                "twitch_hls_media",
                Method::GET,
                &media_playlist_url,
                HeaderMap::new(),
                None,
            )
            .await?;

        if media_resp.status == StatusCode::NOT_FOUND {
            self.invalidate_token();
            return Ok(WatchTickResult::StreamOffline);
        }
        if !media_resp.status.is_success() {
            return Err(AppError::Network(format!(
                "HLS media playlist returned HTTP {}",
                media_resp.status.as_u16()
            )));
        }

        if let Some(segment_url) = pick_latest_segment_url(&media_resp.body) {
            let head_status = http
                .head_platform(
                    PlatformId::Twitch,
                    "twitch_hls_segment_head",
                    &segment_url,
                    HeaderMap::new(),
                )
                .await?;
            if head_status == StatusCode::NOT_FOUND {
                return Ok(WatchTickResult::StreamOffline);
            }
        }

        Ok(WatchTickResult::Watched)
    }

    pub fn invalidate_token(&self) {
        self.compat_fallback_active.store(false, Ordering::Relaxed);
        if let Ok(mut guard) = self.cached_token.lock() {
            *guard = None;
        }
    }

    async fn get_or_fetch_playback_token(
        &self,
        http: &AllowlistedClient,
        auth_token: &str,
        channel_login: &str,
    ) -> Result<(String, String), AppError> {
        if let Ok(guard) = self.cached_token.lock() {
            if let Some(ref cached) = *guard {
                if cached.channel_login.eq_ignore_ascii_case(channel_login)
                    && cached.fetched_at.elapsed() < Duration::from_secs(12 * 60)
                {
                    return Ok((cached.value.clone(), cached.signature.clone()));
                }
            }
        }

        let vars = json!({
            "isLive": true,
            "login": channel_login,
            "isVod": false,
            "vodID": "",
            "playerType": "site",
            "platform": "web"
        });

        let payload =
            execute_persisted_gql(http, auth_token, ops::PLAYBACK_ACCESS_TOKEN, vars).await?;

        let token_node = payload
            .pointer("/data/streamPlaybackAccessToken")
            .ok_or_else(|| {
                AppError::PlatformChanged(
                    "Missing /data/streamPlaybackAccessToken in PlaybackAccessToken".to_string(),
                )
            })?;

        if token_node.is_null() {
            return Err(AppError::NotFound("Channel stream playback token unavailable".to_string()));
        }

        let value = token_node
            .get("value")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::PlatformChanged("Missing playback token value".to_string()))?
            .to_string();
        let signature = token_node
            .get("signature")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::PlatformChanged("Missing playback signature".to_string()))?
            .to_string();

        if let Ok(mut guard) = self.cached_token.lock() {
            *guard = Some(CachedPlaybackToken {
                channel_login: channel_login.to_string(),
                value: value.clone(),
                signature: signature.clone(),
                fetched_at: Instant::now(),
            });
        }

        Ok((value, signature))
    }
}

fn pick_lowest_variant_url(master_m3u8: &str) -> Option<String> {
    let mut best_bw = u64::MAX;
    let mut best_url: Option<String> = None;
    let mut next_bw: Option<u64> = None;

    for raw_line in master_m3u8.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("#EXT-X-STREAM-INF:") {
            let is_audio_only = line.contains("audio_only");
            let bw = line
                .split(',')
                .find_map(|part| {
                    part.trim()
                        .strip_prefix("BANDWIDTH=")
                        .and_then(|n| n.parse::<u64>().ok())
                })
                .unwrap_or(u64::MAX - 1);
            next_bw = Some(if is_audio_only { 0 } else { bw });
        } else if !line.starts_with('#') && line.starts_with("https://") {
            let bw = next_bw.take().unwrap_or(u64::MAX - 1);
            if bw < best_bw {
                best_bw = bw;
                best_url = Some(line.to_string());
            }
        }
    }

    best_url
}

fn pick_latest_segment_url(media_m3u8: &str) -> Option<String> {
    media_m3u8
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty() && !line.starts_with('#') && line.starts_with("https://"))
        .map(str::to_string)
}
