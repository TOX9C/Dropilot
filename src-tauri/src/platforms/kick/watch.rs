use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::SinkExt;
use reqwest::Method;
use serde_json::json;
use tauri::AppHandle;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;
use tracing::info;

use crate::auth::login_window::{close_compat_watch_window, ensure_compat_watch_window};
use crate::net::client::{AllowlistedClient, WsConnection};
use crate::platforms::kick::api::{
    kick_request_json, KickRequest, KICK_VIEWER_TOKEN_URL, KICK_VIEWER_WS_URL,
};
use crate::platforms::{AppError, Channel, PlatformId, WatchTickResult};
use crate::state::BridgeWaiters;

struct ActiveKickWs {
    channel_id: String,
    stream: WsConnection,
}

#[derive(Default)]
pub struct KickWatchSession {
    ws: Mutex<Option<ActiveKickWs>>,
    compat_fallback_active: AtomicBool,
}

impl KickWatchSession {
    pub fn new() -> Self {
        Self::default()
    }

    async fn close_ws_only(&self) {
        let mut guard = self.ws.lock().await;
        if let Some(mut active) = guard.take() {
            let _ = active.stream.close(None).await;
        }
    }

    pub async fn close(&self) {
        self.compat_fallback_active.store(false, Ordering::Relaxed);
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
        if compat_mode || self.compat_fallback_active.load(Ordering::Relaxed) {
            self.close_ws_only().await;
            if let Some(app_handle) = app {
                ensure_compat_watch_window(app_handle, PlatformId::Kick, &channel.login)?;
                return Ok(WatchTickResult::Watched);
            }
        } else if let Some(app_handle) = app {
            close_compat_watch_window(app_handle, PlatformId::Kick);
        }

        let mut guard = self.ws.lock().await;

        let needs_reconnect = match *guard {
            Some(ref active) => active.channel_id != channel.id,
            None => true,
        };

        if needs_reconnect {
            if let Some(mut prev) = guard.take() {
                let _ = prev.stream.close(None).await;
            }

            let viewer_token = match fetch_viewer_token(http, app, waiters, token).await {
                Ok(tok) => tok,
                Err(AppError::Blocked(reason)) | Err(AppError::PlatformChanged(reason)) => {
                    if let Some(app_handle) = app {
                        info!(
                            channel = %channel.login,
                            reason = %reason,
                            "Kick viewer presence requires official page session; using hidden muted official Kick player"
                        );
                        self.compat_fallback_active.store(true, Ordering::Relaxed);
                        ensure_compat_watch_window(app_handle, PlatformId::Kick, &channel.login)?;
                        return Ok(WatchTickResult::Watched);
                    }
                    return Err(AppError::Blocked(reason));
                }
                Err(err) => return Err(err),
            };

            let ws_url = format!("{KICK_VIEWER_WS_URL}?token={viewer_token}");
            let mut stream = match http
                .connect_ws(
                    PlatformId::Kick,
                    "kick_viewer_ws",
                    &ws_url,
                    "https://kick.com",
                    HashMap::new(),
                )
                .await
            {
                Ok(ws) => ws,
                Err(AppError::Blocked(reason)) | Err(AppError::PlatformChanged(reason)) => {
                    if let Some(app_handle) = app {
                        info!(
                            channel = %channel.login,
                            reason = %reason,
                            "Kick viewer presence requires official page session; using hidden muted official Kick player"
                        );
                        self.compat_fallback_active.store(true, Ordering::Relaxed);
                        ensure_compat_watch_window(app_handle, PlatformId::Kick, &channel.login)?;
                        return Ok(WatchTickResult::Watched);
                    }
                    return Err(AppError::Blocked(reason));
                }
                Err(err) => return Err(err),
            };

            let channel_id_val: serde_json::Value = match channel.id.parse::<u64>() {
                Ok(num) => json!(num),
                Err(_) => json!(channel.id),
            };
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

            *guard = Some(ActiveKickWs {
                channel_id: channel.id.clone(),
                stream,
            });
            return Ok(WatchTickResult::Watched);
        }

        if let Some(ref mut active) = *guard {
            let ping_frame = json!({ "type": "ping" });
            if let Err(err) = active.stream.send(Message::Text(ping_frame.to_string().into())).await {
                *guard = None;
                return Err(AppError::Network(format!("Kick WS ping failed: {err}")));
            }
        }

        Ok(WatchTickResult::Watched)
    }
}

async fn fetch_viewer_token(
    http: &AllowlistedClient,
    app: Option<&AppHandle>,
    waiters: &BridgeWaiters,
    token: &str,
) -> Result<String, AppError> {
    let payload = kick_request_json(
        http,
        app,
        waiters,
        token,
        KickRequest {
            op_name: "kick_viewer_token",
            method: Method::GET,
            url: KICK_VIEWER_TOKEN_URL,
            json_body: None,
            not_found_is_resource: false,
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
