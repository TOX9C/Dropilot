pub mod api;
pub mod claim;
pub mod models;
pub mod watch;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::Utc;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT};
use reqwest::Method;
use tauri::AppHandle;

use crate::auth::login_window::{close_compat_watch_window, close_kick_bridge_window};
use crate::miner::selector::game_names_match;
use crate::net::client::AllowlistedClient;
use crate::platforms::kick::api::{
    kick_request_json, KickRequest, KICK_CAMPAIGNS_URL, KICK_CHANNEL_V2_BASE,
    KICK_GAMIFICATION_CHALLENGES_URL, KICK_LIVESTREAMS_BASE, KICK_PROGRESS_URL, KICK_USER_URL,
};
use crate::platforms::kick::models::{
    build_kick_reward_cases_fallback, is_kick_any_stream_campaign, parse_account, parse_campaigns,
    parse_category_livestreams, parse_channel_v2, parse_gamification_challenges,
    parse_gamification_progress, parse_progress,
};
use crate::platforms::kick::watch::KickWatchSession;
use crate::platforms::twitch::twitch_game_slug;
use crate::platforms::{
    Account, AppError, Campaign, Channel, Drop, DropProgress, Platform, PlatformId,
    WatchTickResult,
};
use crate::state::BridgeWaiters;

const KICK_GAMIFICATION_COOLDOWN: Duration = Duration::from_secs(30 * 60);

pub struct KickPlatform {
    http: Arc<AllowlistedClient>,
    app: Option<AppHandle>,
    waiters: BridgeWaiters,
    watch: KickWatchSession,
    gamification_cooldown_until: StdMutex<Option<Instant>>,
    is_watching: AtomicBool,
    gamification_synced_once: AtomicBool,
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
            gamification_cooldown_until: StdMutex::new(None),
            is_watching: AtomicBool::new(false),
            gamification_synced_once: AtomicBool::new(false),
        }
    }

    fn gamification_on_cooldown(&self) -> bool {
        self.gamification_cooldown_until
            .lock()
            .ok()
            .and_then(|g| *g)
            .map(|until| Instant::now() < until)
            .unwrap_or(false)
    }

    fn record_gamification_unavailable(&self) {
        if let Ok(mut guard) = self.gamification_cooldown_until.lock() {
            *guard = Some(Instant::now() + KICK_GAMIFICATION_COOLDOWN);
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
        let mut campaigns = parse_campaigns(&val).unwrap_or_default();
        let fallbacks = build_kick_reward_cases_fallback(Utc::now(), &campaigns);
        for fb in fallbacks.into_iter().rev() {
            campaigns.insert(0, fb);
        }
        Ok(campaigns)
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
                allow_bridge: true,
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
                allow_bridge: true,
            },
        )
        .await?;

        let mut campaigns = parse_campaigns(&payload)?;
        let now = Utc::now();

        // Fetch real live Kick gamification reward cases (Sunset Reward Case, Daily, Weekly, etc.)
        // with real server-side progress and timers. Uses brief kick-bridge fallback (auto-closes in 2s)
        // when web.kick.com requires full browser session cookies.
        let mut loaded_live_cases = false;
        if !self.gamification_on_cooldown() {
            match kick_request_json(
                &self.http,
                self.app.as_ref(),
                &self.waiters,
                token,
                KickRequest {
                    op_name: "kick_gamification_challenges",
                    method: Method::GET,
                    url: KICK_GAMIFICATION_CHALLENGES_URL,
                    json_body: None,
                    not_found_is_resource: true,
                    allow_bridge: true,
                },
            )
            .await
            {
                Ok(gam_payload) => {
                    let challenges = parse_gamification_challenges(&gam_payload, now);
                    tracing::info!(
                        count = challenges.len(),
                        "Fetched Kick gamification reward cases"
                    );
                    if !challenges.is_empty() {
                        loaded_live_cases = true;
                        self.gamification_synced_once.store(true, Ordering::Relaxed);
                        for ch in challenges.into_iter().rev() {
                            if !campaigns.iter().any(|c| c.id == ch.id) {
                                campaigns.insert(0, ch);
                            }
                        }
                    }
                }
                Err(err) => {
                    self.record_gamification_unavailable();
                    tracing::info!(
                        reason = %err,
                        "Kick gamification challenges unavailable; using built-in Kick reward cases"
                    );
                }
            }
        }

        // Only inject synthetic fallback reward cases if Kick's live gamification API was unavailable.
        if !loaded_live_cases {
            let fallbacks = build_kick_reward_cases_fallback(now, &campaigns);
            for fb in fallbacks.into_iter().rev() {
                if !campaigns.iter().any(|c| c.id == fb.id) {
                    campaigns.insert(0, fb);
                }
            }
        }

        Ok(campaigns)
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
                allow_bridge: true,
            },
        )
        .await?;

        let mut progress = parse_progress(&payload)?;

        // Sync live gamification reward case progress (e.g. Sunset Reward Case remaining minutes)
        // on initial sync or every 5-minute progress poll while actively watching (never while idle).
        let should_sync_gamification = !self.gamification_on_cooldown()
            && (self.is_watching.load(Ordering::Relaxed)
                || !self.gamification_synced_once.load(Ordering::Relaxed));

        if should_sync_gamification {
            match kick_request_json(
                &self.http,
                self.app.as_ref(),
                &self.waiters,
                token,
                KickRequest {
                    op_name: "kick_gamification_progress",
                    method: Method::GET,
                    url: KICK_GAMIFICATION_CHALLENGES_URL,
                    json_body: None,
                    not_found_is_resource: true,
                    allow_bridge: true,
                },
            )
            .await
            {
                Ok(gam_payload) => {
                    self.gamification_synced_once.store(true, Ordering::Relaxed);
                    let gam_progress = parse_gamification_progress(&gam_payload, Utc::now());
                    progress.extend(gam_progress);
                }
                Err(_) => {
                    self.record_gamification_unavailable();
                }
            }
        }

        Ok(progress)
    }

    async fn find_candidate_channels(
        &self,
        token: &str,
        campaign: &Campaign,
    ) -> Result<Vec<Channel>, AppError> {
        if let Some(ref allowed) = campaign.allowed_channels {
            if !allowed.is_empty() {
                let mut live = Vec::new();
                for ch_ref in allowed.iter().take(30) {
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
                        stream_id: None,
                    };
                    if let Ok(checked) = self.check_live(token, &probe).await {
                        if checked.is_live
                            && game_names_match(&checked.game_name, &campaign.game.name)
                        {
                            live.push(checked);
                        }
                    }
                }
                live.sort_by_key(|a| std::cmp::Reverse(a.viewer_count));
                return Ok(live);
            }
        }

        // For Kick "watch any stream" reward cases (such as Sunset Reward Case / GTA VI Box),
        // prefer live GTA VI streams first so watching progresses both the Sunset Case AND
        // Sunset Drop simultaneously, then fall back to any live Kick stream.
        if is_kick_any_stream_campaign(campaign) {
            let gta_url = format!("{KICK_LIVESTREAMS_BASE}?subcategory=grand-theft-auto-vi&sort=desc&limit=25");
            if let Ok(payload) = kick_request_json(
                &self.http,
                self.app.as_ref(),
                &self.waiters,
                token,
                KickRequest {
                    op_name: "kick_find_channels_gta",
                    method: Method::GET,
                    url: &gta_url,
                    json_body: None,
                    not_found_is_resource: false,
                    allow_bridge: true,
                },
            )
            .await
            {
                if let Ok(channels) = parse_category_livestreams(
                    &payload,
                    &campaign.game.name,
                    "",
                    Some("grand-theft-auto-vi"),
                    true,
                ) {
                    if !channels.is_empty() {
                        return Ok(channels);
                    }
                }
            }

            let any_url = format!("{KICK_LIVESTREAMS_BASE}?sort=desc&limit=25");
            let payload = kick_request_json(
                &self.http,
                self.app.as_ref(),
                &self.waiters,
                token,
                KickRequest {
                    op_name: "kick_find_channels_any",
                    method: Method::GET,
                    url: &any_url,
                    json_body: None,
                    not_found_is_resource: false,
                    allow_bridge: true,
                },
            )
            .await?;

            return parse_category_livestreams(&payload, &campaign.game.name, "", None, true);
        }

        let slug = campaign
            .game
            .slug
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| twitch_game_slug(&campaign.game.name));

        let category_query = if !slug.is_empty() {
            format!("?subcategory={slug}&sort=desc&limit=25")
        } else if !campaign.game.id.is_empty() {
            format!("?category_id={}&sort=desc&limit=25", campaign.game.id)
        } else {
            String::new()
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
                allow_bridge: true,
            },
        )
        .await?;

        parse_category_livestreams(
            &payload,
            &campaign.game.name,
            &campaign.game.id,
            Some(slug.as_str()),
            false,
        )
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
                allow_bridge: true,
            },
        )
        .await?;

        parse_channel_v2(&payload, &channel.game_name)
    }

    async fn is_channel_participating(
        &self,
        _token: &str,
        channel: &Channel,
        campaign: &Campaign,
    ) -> Result<bool, AppError> {
        if let Some(ref allowed) = campaign.allowed_channels {
            if !allowed.is_empty() {
                return Ok(allowed
                    .iter()
                    .any(|a| a.login.eq_ignore_ascii_case(&channel.login) || a.id == channel.id));
            }
        }
        Ok(true)
    }

    async fn watch_tick(
        &self,
        token: &str,
        channel: &Channel,
        compat_mode: bool,
    ) -> Result<WatchTickResult, AppError> {
        self.is_watching.store(true, Ordering::Relaxed);
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

    async fn stop_watching(&self) {
        self.is_watching.store(false, Ordering::Relaxed);
        self.watch.close().await;
    }

    fn reset_progress_sync_cooldown(&self) {
        self.gamification_synced_once.store(false, Ordering::Relaxed);
        if let Ok(mut guard) = self.gamification_cooldown_until.lock() {
            *guard = None;
        }
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
        self.is_watching.store(false, Ordering::Relaxed);
        self.gamification_synced_once.store(false, Ordering::Relaxed);
        self.watch.close().await;
        if let Some(ref app_handle) = self.app {
            close_compat_watch_window(app_handle, PlatformId::Kick);
            close_kick_bridge_window(app_handle);
        }
        Ok(())
    }
}
