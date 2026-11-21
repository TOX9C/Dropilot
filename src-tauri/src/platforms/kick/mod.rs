pub mod api;
pub mod claim;
pub mod models;
pub mod watch;

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT};
use reqwest::Method;
use tauri::AppHandle;

use crate::auth::login_window::{close_compat_watch_window, close_kick_bridge_window};
use crate::net::client::AllowlistedClient;
use crate::platforms::kick::api::{
    kick_request_json, KickRequest, KICK_CAMPAIGNS_URL, KICK_CHANNEL_V2_BASE,
    KICK_LIVESTREAMS_BASE, KICK_PROGRESS_URL, KICK_USER_URL,
};
use crate::platforms::kick::models::{
    parse_account, parse_campaigns, parse_category_livestreams, parse_channel_v2, parse_progress,
};
use crate::platforms::kick::watch::KickWatchSession;
use crate::platforms::{
    Account, AppError, Campaign, Channel, Drop, DropProgress, Platform, PlatformId,
    WatchTickResult,
};
use crate::state::BridgeWaiters;

pub struct KickPlatform {
    http: Arc<AllowlistedClient>,
    app: Option<AppHandle>,
    waiters: BridgeWaiters,
    watch: KickWatchSession,
}

impl KickPlatform {
    pub fn new(
        http: Arc<AllowlistedClient>,
        app: Option<AppHandle>,
        waiters: BridgeWaiters,
    ) -> Self {
        Self {
            http,
            app,
            waiters,
            watch: KickWatchSession::new(),
        }
    }
}

#[async_trait]
impl Platform for KickPlatform {
    fn id(&self) -> PlatformId {
        PlatformId::Kick
    }

    async fn fetch_public_campaigns(&self) -> Result<Vec<Campaign>, AppError> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        let Ok(resp) = self
            .http
            .request_platform(
                PlatformId::Kick,
                "kick_public_campaigns",
                Method::GET,
                KICK_CAMPAIGNS_URL,
                headers,
                None,
            )
            .await
        else {
            return Ok(Vec::new());
        };
        if !resp.status.is_success() {
            return Ok(Vec::new());
        }
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&resp.body) else {
            return Ok(Vec::new());
        };
        Ok(parse_campaigns(&val).unwrap_or_default())
    }

    async fn validate_session(&self, token: &str) -> Result<Account, AppError> {
        let payload = kick_request_json(
            &self.http,
            self.app.as_ref(),
            &self.waiters,
            token,
            KickRequest {
                op_name: "kick_validate_session",
                method: Method::GET,
                url: KICK_USER_URL,
                json_body: None,
                not_found_is_resource: false,
            },
        )
        .await?;

        parse_account(&payload)
    }

    async fn fetch_campaigns(&self, token: &str) -> Result<Vec<Campaign>, AppError> {
        let payload = kick_request_json(
            &self.http,
            self.app.as_ref(),
            &self.waiters,
            token,
            KickRequest {
                op_name: "kick_fetch_campaigns",
                method: Method::GET,
                url: KICK_CAMPAIGNS_URL,
                json_body: None,
                not_found_is_resource: false,
            },
        )
        .await?;

        parse_campaigns(&payload)
    }

    async fn fetch_progress(&self, token: &str) -> Result<Vec<DropProgress>, AppError> {
        let payload = kick_request_json(
            &self.http,
            self.app.as_ref(),
            &self.waiters,
            token,
            KickRequest {
                op_name: "kick_fetch_progress",
                method: Method::GET,
                url: KICK_PROGRESS_URL,
                json_body: None,
                not_found_is_resource: false,
            },
        )
        .await?;

        parse_progress(&payload)
    }

    async fn find_candidate_channels(
        &self,
        token: &str,
        campaign: &Campaign,
    ) -> Result<Vec<Channel>, AppError> {
        if let Some(ref allowed) = campaign.allowed_channels {
            if !allowed.is_empty() {
                let mut live = Vec::new();
                for ch_ref in allowed.iter().take(20) {
                    let probe = Channel {
                        id: ch_ref.id.clone(),
                        login: ch_ref.login.clone(),
                        display_name: ch_ref.display_name.clone(),
                        avatar_url: None,
                        game_name: campaign.game.name.clone(),
                        viewer_count: 0,
                        is_live: false,
                        drops_enabled: true,
                        is_followed: false,
                    };
                    if let Ok(checked) = self.check_live(token, &probe).await {
                        if checked.is_live {
                            live.push(checked);
                        }
                    }
                }
                return Ok(live);
            }
        }

        let category_query = if campaign.game.id.is_empty() {
            String::new()
        } else {
            format!("?category_id={}&sort=desc&limit=25", campaign.game.id)
        };
        let url = format!("{KICK_LIVESTREAMS_BASE}{category_query}");

        let payload = kick_request_json(
            &self.http,
            self.app.as_ref(),
            &self.waiters,
            token,
            KickRequest {
                op_name: "kick_find_channels",
                method: Method::GET,
                url: &url,
                json_body: None,
                not_found_is_resource: false,
            },
        )
        .await?;

        parse_category_livestreams(&payload, &campaign.game.name)
    }

    async fn check_live(&self, token: &str, channel: &Channel) -> Result<Channel, AppError> {
        let url = format!("{KICK_CHANNEL_V2_BASE}/{}", channel.login);
        let payload = kick_request_json(
            &self.http,
            self.app.as_ref(),
            &self.waiters,
            token,
            KickRequest {
                op_name: "kick_check_live",
                method: Method::GET,
                url: &url,
                json_body: None,
                not_found_is_resource: true,
            },
        )
        .await?;

        parse_channel_v2(&payload, &channel.game_name)
    }

    async fn watch_tick(
        &self,
        token: &str,
        channel: &Channel,
        compat_mode: bool,
    ) -> Result<WatchTickResult, AppError> {
        self.watch
            .tick(
                &self.http,
                self.app.as_ref(),
                &self.waiters,
                token,
                channel,
                compat_mode,
            )
            .await
    }

    async fn claim(&self, token: &str, campaign: &Campaign, drop: &Drop) -> Result<(), AppError> {
        claim::claim_drop(
            &self.http,
            self.app.as_ref(),
            &self.waiters,
            token,
            campaign,
            drop,
        )
        .await
    }

    async fn logout(&self, _token: Option<&str>) -> Result<(), AppError> {
        self.watch.close().await;
        if let Some(ref app_handle) = self.app {
            close_compat_watch_window(app_handle, PlatformId::Kick);
            close_kick_bridge_window(app_handle);
        }
        Ok(())
    }
}
