pub mod claim;
pub mod gql;
pub mod models;
pub mod watch;

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use tauri::AppHandle;
use tokio::sync::Mutex;

use crate::auth::login_window::{
    close_compat_watch_window, close_twitch_bridge_window, twitch_bridge_capture,
};
use crate::miner::selector::game_names_match;
use crate::net::client::AllowlistedClient;
use crate::platforms::twitch::gql::{
    execute_persisted_gql, execute_persisted_gql_batch, execute_persisted_gql_opt,
    fetch_game_directory_streams, ops, validate_oauth_token,
};
use crate::platforms::twitch::models::{
    parse_campaign_details, parse_campaigns_dashboard, parse_directory_channels,
    parse_inventory_progress,
};
use crate::platforms::twitch::watch::TwitchWatchSession;
use crate::platforms::{
    Account, AppError, Campaign, Channel, Drop, DropProgress, Platform, PlatformId,
    WatchTickResult,
};

const TWITCH_DROPS_TAG_ID: &str = "c2542d6d-cd10-4532-919b-3dcc9f54d5bf";

pub struct TwitchPlatform {
    http: Arc<AllowlistedClient>,
    app: Option<AppHandle>,
    bridge_lock: Arc<Mutex<()>>,
    watch: TwitchWatchSession,
}

impl TwitchPlatform {
    pub fn new(
        http: Arc<AllowlistedClient>,
        app: Option<AppHandle>,
        bridge_lock: Arc<Mutex<()>>,
    ) -> Self {
        Self {
            http,
            app,
            bridge_lock,
            watch: TwitchWatchSession::new(),
        }
    }
}

pub fn twitch_game_slug(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut prev_dash = true;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if matches!(ch, ' ' | '-' | '_' | '/') && !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    slug
}

pub fn apply_campaign_details(camp: &mut Campaign, detailed: Campaign) {
    if detailed.allowed_channels.is_some() {
        camp.allowed_channels = detailed.allowed_channels;
    }
    if detailed.link_url.is_some() {
        camp.link_url = detailed.link_url;
        camp.requires_link = detailed.requires_link;
    }
    camp.linked = detailed.linked;
    camp.link_verified = detailed.link_verified;
    if !detailed.drops.is_empty() {
        let prev_drops = std::mem::take(&mut camp.drops);
        camp.drops = detailed
            .drops
            .into_iter()
            .map(|mut next_d| {
                if let Some(old_d) = prev_drops.iter().find(|d| d.id == next_d.id) {
                    next_d.current_minutes = next_d
                        .current_minutes
                        .max(old_d.current_minutes)
                        .min(next_d.required_minutes);
                    next_d.claimed |= old_d.claimed;
                    next_d.claimable |= old_d.claimable && !next_d.claimed;
                    if next_d.instance_id.is_none() {
                        next_d.instance_id = old_d.instance_id.clone();
                    }
                }
                next_d
            })
            .collect();
    }
}

#[async_trait]
impl Platform for TwitchPlatform {
    fn id(&self) -> PlatformId {
        PlatformId::Twitch
    }

    async fn validate_session(&self, token: &str) -> Result<Account, AppError> {
        let info = validate_oauth_token(&self.http, token).await?;
        let id = info
            .get("user_id")
            .and_then(|s| s.as_str())
            .ok_or_else(|| AppError::Auth("Missing user_id in Twitch validation".to_string()))?
            .to_string();
        let login = info
            .get("login")
            .and_then(|s| s.as_str())
            .ok_or_else(|| AppError::Auth("Missing login in Twitch validation".to_string()))?
            .to_string();

        // Try fetching display name and avatar via GQL; fall back gracefully if unavailable.
        let mut display_name = login.clone();
        let mut avatar_url = None;
        if let Ok(profile) =
            execute_persisted_gql(&self.http, token, ops::USER_PROFILE, json!({})).await
        {
            if let Some(dn) = profile
                .pointer("/data/currentUser/displayName")
                .and_then(|s| s.as_str())
            {
                display_name = dn.to_string();
            }
            avatar_url = profile
                .pointer("/data/currentUser/profileImageURL")
                .and_then(|s| s.as_str())
                .map(str::to_string);
        }

        Ok(Account {
            id,
            login,
            display_name,
            avatar_url,
        })
    }

    async fn fetch_campaigns(&self, token: &str) -> Result<Vec<Campaign>, AppError> {
        let payload = match execute_persisted_gql(
            &self.http,
            token,
            ops::VIEWER_DROPS_DASHBOARD,
            json!({ "fetchRewardCampaigns": true }),
        )
        .await
        {
            Ok(val) => val,
            Err(AppError::Blocked(_)) if self.app.is_some() => {
                let _guard = self.bridge_lock.lock().await;
                let app_handle = self
                    .app
                    .as_ref()
                    .ok_or_else(|| AppError::Internal("Missing app handle".to_string()))?;
                twitch_bridge_capture(
                    app_handle,
                    "https://www.twitch.tv/drops/campaigns",
                    "campaigns",
                )
                .await?
            }
            Err(err) => return Err(err),
        };

        let mut campaigns = parse_campaigns_dashboard(&payload)?;

        if !campaigns.is_empty() {
            // Enrich campaigns via DropCampaignDetails (including when ViewerDropsDashboard came from the bridge)
            // so per-game isAccountConnected status, allowed channels, and rewards populate cleanly.
            let account = self.validate_session(token).await.ok();
            let mut candidate_keys: Vec<String> = Vec::new();
            if let Some(ref acct) = account {
                if !acct.id.is_empty() {
                    candidate_keys.push(acct.id.clone());
                }
                if !acct.login.is_empty() && acct.login != acct.id {
                    candidate_keys.push(acct.login.clone());
                }
            }

            let mut working_key: Option<String> = None;
            for key in &candidate_keys {
                match execute_persisted_gql(
                    &self.http,
                    token,
                    ops::DROP_CAMPAIGN_DETAILS,
                    json!({
                        "dropID": campaigns[0].id,
                        "channelLogin": key,
                    }),
                )
                .await
                {
                    Ok(details_val) => {
                        if let Ok(Some(detailed)) = parse_campaign_details(&details_val) {
                            apply_campaign_details(&mut campaigns[0], detailed);
                            working_key = Some(key.clone());
                            break;
                        }
                    }
                    Err(AppError::Blocked(msg)) | Err(AppError::PlatformChanged(msg)) => {
                        tracing::info!(
                            reason = %msg,
                            "Twitch DropCampaignDetails direct enrichment unavailable; using summary list"
                        );
                        break;
                    }
                    Err(_) => {}
                }
            }

            if let Some(key) = working_key {
                for chunk in campaigns[1..].chunks_mut(20) {
                    let vars_chunk: Vec<_> = chunk
                        .iter()
                        .map(|camp| {
                            json!({
                                "dropID": camp.id,
                                "channelLogin": key,
                            })
                        })
                        .collect();

                    match execute_persisted_gql_batch(
                        &self.http,
                        token,
                        ops::DROP_CAMPAIGN_DETAILS,
                        &vars_chunk,
                    )
                    .await
                    {
                        Ok(items) => {
                            for (idx, item) in items.iter().enumerate() {
                                if let Ok(Some(detailed)) = parse_campaign_details(item) {
                                    if let Some(target) =
                                        chunk.iter_mut().find(|c| c.id == detailed.id)
                                    {
                                        apply_campaign_details(target, detailed);
                                    } else if let Some(target) = chunk.get_mut(idx) {
                                        apply_campaign_details(target, detailed);
                                    }
                                }
                            }
                        }
                        Err(AppError::Blocked(msg)) | Err(AppError::PlatformChanged(msg)) => {
                            tracing::info!(
                                reason = %msg,
                                "Stopped Twitch DropCampaignDetails batch enrichment early; keeping fetched campaigns"
                            );
                            break;
                        }
                        Err(_) => break,
                    }
                }
            }
        }

        Ok(campaigns)
    }

    async fn fetch_progress(&self, token: &str) -> Result<Vec<DropProgress>, AppError> {
        let payload = match execute_persisted_gql(
            &self.http,
            token,
            ops::INVENTORY,
            json!({ "fetchRewardCampaigns": false }),
        )
        .await
        {
            Ok(val) => val,
            Err(AppError::Blocked(_)) if self.app.is_some() => {
                let _guard = self.bridge_lock.lock().await;
                let app_handle = self
                    .app
                    .as_ref()
                    .ok_or_else(|| AppError::Internal("Missing app handle".to_string()))?;
                twitch_bridge_capture(
                    app_handle,
                    "https://www.twitch.tv/drops/inventory",
                    "inventory",
                )
                .await?
            }
            Err(err) => return Err(err),
        };

        parse_inventory_progress(&payload)
    }

    async fn find_candidate_channels(
        &self,
        token: &str,
        campaign: &Campaign,
    ) -> Result<Vec<Channel>, AppError> {
        // If campaign restricts to specific allowed channels, verify live status on those first.
        if let Some(ref allowed) = campaign.allowed_channels {
            if !allowed.is_empty() {
                let mut live_allowed = Vec::new();
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
                        if checked.is_live
                            && game_names_match(&checked.game_name, &campaign.game.name)
                        {
                            live_allowed.push(checked);
                        }
                    }
                }
                if !live_allowed.is_empty() {
                    return Ok(live_allowed);
                }
            }
        }

        // 1. Fast direct GQL query by Game ID (or Game Name) — resolves real slug (e.g. "overwatch-2")
        // and returns live drops-enabled streams in ~40ms without needing the hidden browser bridge.
        let by_id_res = match fetch_game_directory_streams(
            &self.http,
            Some(token),
            &campaign.game.id,
            &campaign.game.name,
            TWITCH_DROPS_TAG_ID,
        )
        .await
        {
            Ok(val) => Ok(val),
            Err(AppError::Blocked(_)) => {
                fetch_game_directory_streams(
                    &self.http,
                    None,
                    &campaign.game.id,
                    &campaign.game.name,
                    TWITCH_DROPS_TAG_ID,
                )
                .await
            }
            Err(err) => Err(err),
        };

        let mut resolved_slug: Option<String> = campaign
            .game
            .slug
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        if let Ok(ref payload) = by_id_res {
            if let Some(s) = payload
                .pointer("/data/game/slug")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                resolved_slug = Some(s.to_string());
            }
            if let Ok(channels) = parse_directory_channels(payload, &campaign.game.name) {
                if !channels.is_empty() {
                    return Ok(channels);
                }
            }
        }

        let slug = resolved_slug.unwrap_or_else(|| twitch_game_slug(&campaign.game.name));
        let vars = json!({
            "name": campaign.game.name,
            "slug": slug,
            "options": {
                "sort": "VIEWER_COUNT",
                "recommendationsContext": { "platform": "web" },
                "tags": [TWITCH_DROPS_TAG_ID]
            },
            "sortTypeIsRecency": false,
            "limit": 30,
            "includeCostreaming": false,
            "includeIsDJ": false
        });

        let direct_result = match execute_persisted_gql(
            &self.http,
            token,
            ops::DIRECTORY_PAGE_GAME,
            vars.clone(),
        )
        .await
        {
            Ok(val) => Ok(val),
            Err(AppError::Blocked(_)) => {
                execute_persisted_gql_opt(&self.http, None, ops::DIRECTORY_PAGE_GAME, vars).await
            }
            Err(err) => Err(err),
        };

        let parsed_direct = match direct_result {
            Ok(payload) => parse_directory_channels(&payload, &campaign.game.name),
            Err(err) => Err(err),
        };

        if let Ok(ref channels) = parsed_direct {
            if !channels.is_empty() {
                return Ok(channels.clone());
            }
        }

        if let Some(ref app_handle) = self.app {
            let need_bridge = match parsed_direct {
                Ok(ref channels) => channels.is_empty(),
                Err(AppError::Blocked(_) | AppError::PlatformChanged(_)) => true,
                Err(_) => false,
            };
            if need_bridge {
                let _guard = self.bridge_lock.lock().await;
                let url = format!("https://www.twitch.tv/directory/category/{slug}?filter=drops");
                let payload = twitch_bridge_capture(app_handle, &url, "directory").await?;
                let channels = parse_directory_channels(&payload, &campaign.game.name)?;
                tracing::info!(
                    game = %campaign.game.name,
                    slug = %slug,
                    candidates = channels.len(),
                    "Fetched Twitch directory candidates via official page"
                );
                return Ok(channels);
            }
        }

        parsed_direct
    }

    async fn check_live(&self, token: &str, channel: &Channel) -> Result<Channel, AppError> {
        let vars = json!({ "channelLogin": channel.login });
        let payload = match execute_persisted_gql(&self.http, token, ops::USE_LIVE, vars.clone()).await {
            Ok(val) => val,
            Err(AppError::Blocked(_)) => {
                execute_persisted_gql_opt(&self.http, None, ops::USE_LIVE, vars).await?
            }
            Err(err) => return Err(err),
        };

        let user_node = payload.pointer("/data/user").ok_or_else(|| {
            AppError::PlatformChanged("Missing /data/user in UseLive response".to_string())
        })?;

        if user_node.is_null() {
            return Err(AppError::NotFound(format!(
                "Channel '{}' not found",
                channel.login
            )));
        }

        let stream = user_node.get("stream");
        let is_live = stream.map(|s| !s.is_null()).unwrap_or(false);
        let viewers = stream
            .and_then(|s| s.get("viewersCount"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let game_name = stream
            .and_then(|s| s.pointer("/game/displayName").or_else(|| s.pointer("/game/name")))
            .and_then(|v| v.as_str())
            .unwrap_or(&channel.game_name)
            .to_string();

        let mut updated = channel.clone();
        updated.is_live = is_live;
        updated.viewer_count = viewers;
        updated.game_name = game_name;
        Ok(updated)
    }

    async fn watch_tick(
        &self,
        token: &str,
        channel: &Channel,
        compat_mode: bool,
    ) -> Result<WatchTickResult, AppError> {
        self.watch
            .tick(&self.http, self.app.as_ref(), token, channel, compat_mode)
            .await
    }

    async fn claim(&self, token: &str, _campaign: &Campaign, drop: &Drop) -> Result<(), AppError> {
        claim::claim_drop(&self.http, token, drop).await
    }

    async fn logout(&self, _token: Option<&str>) -> Result<(), AppError> {
        self.watch.invalidate_token();
        if let Some(ref app_handle) = self.app {
            close_compat_watch_window(app_handle, PlatformId::Twitch);
            close_twitch_bridge_window(app_handle);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::twitch_game_slug;

    #[test]
    fn builds_clean_twitch_game_slugs() {
        assert_eq!(twitch_game_slug("Rust"), "rust");
        assert_eq!(twitch_game_slug("Counter-Strike 2"), "counter-strike-2");
        assert_eq!(twitch_game_slug("Path of Exile 2"), "path-of-exile-2");
        assert_eq!(
            twitch_game_slug("Tom Clancy's Rainbow Six: Siege"),
            "tom-clancys-rainbow-six-siege"
        );
        assert_eq!(twitch_game_slug("Dungeons & Dragons!"), "dungeons-dragons");
    }
}
