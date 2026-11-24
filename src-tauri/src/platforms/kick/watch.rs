use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use reqwest::Method;
use serde_json::json;
use tauri::AppHandle;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::interval;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

use crate::auth::login_window::{
    close_compat_watch_window_for_profile, ensure_compat_watch_window_for_profile,
};
use crate::net::client::AllowlistedClient;
use crate::platforms::kick::api::{
    kick_request_json_for_profile, normalize_kick_bearer_token, KickRequest, KICK_VIEWER_TOKEN_URL,
    KICK_VIEWER_WS_URL,
};
use crate::platforms::{AppError, Channel, PlatformId, WatchTickResult};
use crate::settings::DEFAULT_KICK_PROFILE_ID;
use crate::state::BridgeWaiters;

struct ActiveKickWsTask {
    channel_id: String,
    alive: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl ActiveKickWsTask {
    fn stop(self) {
        self.alive.store(false, Ordering::Relaxed);
        self.handle.abort();
    }
}

#[derive(Default)]
pub struct KickWatchSession {
    ws_task: Mutex<Option<ActiveKickWsTask>>,
}

impl KickWatchSession {
    pub fn new() -> Self {
        Self::default()
    }

    async fn close_ws_only(&self) {
        let mut guard = self.ws_task.lock().await;
        if let Some(active) = guard.take() {
            active.stop();
        }
    }

    pub async fn close(&self) {
        self.close_ws_only().await;
    }

    pub async fn tick(
        &self,
        http: &AllowlistedClient,
        app: Option<&AppHandle>,
        waiters: &BridgeWaiters,
        token: &str,
        channel: &Channel,
        compat_mode: bool,
    ) -> Result<WatchTickResult, AppError> {
        self.tick_for_profile(
            http,
            app,
            waiters,
            DEFAULT_KICK_PROFILE_ID,
            token,
            channel,
            compat_mode,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn tick_for_profile(
        &self,
        http: &AllowlistedClient,
        app: Option<&AppHandle>,
        waiters: &BridgeWaiters,
        profile_id: &str,
        token: &str,
        channel: &Channel,
        compat_mode: bool,
    ) -> Result<WatchTickResult, AppError> {
        if compat_mode {
            self.close_ws_only().await;
            if let Some(app_handle) = app {
                ensure_compat_watch_window_for_profile(
                    app_handle,
                    PlatformId::Kick,
                    profile_id,
                    &channel.login,
                )?;
            }
            return Ok(WatchTickResult::Watched);
        }

        // Default ultra-light stream-less mode: never keep a hidden Chromium video player open.
        if let Some(app_handle) = app {
            close_compat_watch_window_for_profile(app_handle, PlatformId::Kick, profile_id);
        }

        let mut guard = self.ws_task.lock().await;
        let needs_reconnect = match *guard {
            Some(ref active) => {
                active.channel_id != channel.id
                    || !active.alive.load(Ordering::Relaxed)
                    || active.handle.is_finished()
            }
            None => true,
        };

        if !needs_reconnect {
            return Ok(WatchTickResult::Watched);
        }

        if let Some(prev) = guard.take() {
            prev.stop();
        }

        let viewer_token = fetch_viewer_token(http, app, waiters, profile_id, token).await?;
        let ws_url = format!("{KICK_VIEWER_WS_URL}?token={viewer_token}");
        let mut extra_headers = HashMap::new();
        let bearer = normalize_kick_bearer_token(token);
        extra_headers.insert("Authorization".to_string(), format!("Bearer {bearer}"));
        extra_headers.insert("Cookie".to_string(), format!("session_token={token}"));

        let mut stream = http
            .connect_ws(
                PlatformId::Kick,
                "kick_viewer_ws",
                &ws_url,
                "https://kick.com",
                extra_headers,
            )
            .await?;

        let channel_id_val: serde_json::Value = match channel.id.parse::<u64>() {
            Ok(num) => json!(num),
            Err(_) => json!(channel.id),
        };
        let livestream_id_val: Option<serde_json::Value> = channel.stream_id.as_deref().map(|s| {
            match s.parse::<u64>() {
                Ok(num) => json!(num),
                Err(_) => json!(s),
            }
        });

        let handshake = json!({
            "type": "channel_handshake",
            "data": {
                "message": {
                    "channelId": channel_id_val
                }
            }
        });

        stream
            .send(Message::Text(handshake.to_string().into()))
            .await
            .map_err(|e| AppError::Network(format!("Kick WS handshake failed: {e}")))?;

        if let Some(ref ls_id) = livestream_id_val {
            let watch_evt = json!({
                "type": "user_event",
                "data": {
                    "message": {
                        "name": "tracking.user.watch.livestream",
                        "channel_id": channel_id_val,
                        "livestream_id": ls_id
                    }
                }
            });
            let _ = stream.send(Message::Text(watch_evt.to_string().into())).await;
        }

        info!(
            channel = %channel.login,
            "Connected lightweight Kick stream-less viewer WebSocket (0 Mbps video)"
        );

        let alive = Arc::new(AtomicBool::new(true));
        let alive_task = alive.clone();
        let login_log = channel.login.clone();

        let handle = tokio::spawn(async move {
            let (mut sink, mut reader) = stream.split();
            let mut ping_ticker = interval(Duration::from_secs(15));
            let mut watch_ticker = interval(Duration::from_secs(120));
            // Skip immediate first tick since we just sent handshake + watch event.
            ping_ticker.tick().await;
            watch_ticker.tick().await;

            while alive_task.load(Ordering::Relaxed) {
                tokio::select! {
                    _ = ping_ticker.tick() => {
                        let ping = json!({ "type": "ping" });
                        if let Err(err) = sink.send(Message::Text(ping.to_string().into())).await {
                            debug!(channel = %login_log, error = %err, "Kick viewer WS ping ended");
                            break;
                        }
                    }
                    _ = watch_ticker.tick() => {
                        if let Some(ref ls_id) = livestream_id_val {
                            let watch_evt = json!({
                                "type": "user_event",
                                "data": {
                                    "message": {
                                        "name": "tracking.user.watch.livestream",
                                        "channel_id": channel_id_val,
                                        "livestream_id": ls_id
                                    }
                                }
                            });
                            if let Err(err) = sink.send(Message::Text(watch_evt.to_string().into())).await {
                                debug!(channel = %login_log, error = %err, "Kick viewer WS watch event ended");
                                break;
                            }
                        }
                    }
                    incoming = reader.next() => {
                        match incoming {
                            Some(Ok(Message::Ping(data))) => {
                                let _ = sink.send(Message::Pong(data)).await;
                            }
                            Some(Ok(Message::Close(_))) | None => {
                                warn!(channel = %login_log, "Kick viewer WS closed by remote; will reconnect");
                                break;
                            }
                            Some(Err(err)) => {
                                debug!(channel = %login_log, error = %err, "Kick viewer WS read error");
                                break;
                            }
                            Some(Ok(_)) => {}
                        }
                    }
                }
            }
            alive_task.store(false, Ordering::Relaxed);
            let _ = sink.close().await;
        });

        *guard = Some(ActiveKickWsTask {
            channel_id: channel.id.clone(),
            alive,
            handle,
        });

        Ok(WatchTickResult::Watched)
    }
}

async fn fetch_viewer_token(
    http: &AllowlistedClient,
    app: Option<&AppHandle>,
    waiters: &BridgeWaiters,
    profile_id: &str,
    token: &str,
) -> Result<String, AppError> {
    let payload = kick_request_json_for_profile(
        http,
        app,
        waiters,
        profile_id,
        token,
        KickRequest {
            op_name: "kick_viewer_token",
            method: Method::GET,
            url: KICK_VIEWER_TOKEN_URL,
            json_body: None,
            not_found_is_resource: false,
            allow_bridge: false,
        },
    )
    .await?;

    payload
        .pointer("/data/token")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            AppError::PlatformChanged("Missing /data/token in Kick viewer token response".to_string())
        })
}
