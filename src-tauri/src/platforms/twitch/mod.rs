pub mod claim;
pub mod gql;
pub mod models;
pub mod watch;

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::json;
use tauri::AppHandle;
use tokio::sync::Mutex;

use crate::auth::login_window::{
    close_compat_watch_window, close_twitch_bridge_window, twitch_bridge_capture,
    twitch_bridge_claim_drop,
};
use crate::miner::selector::game_names_match;
use crate::net::client::AllowlistedClient;
use crate::platforms::twitch::gql::{
    execute_persisted_gql, execute_persisted_gql_batch, execute_persisted_gql_opt,
    fetch_channel_live_status, fetch_channels_available_drop_campaign_ids,
    fetch_channels_available_drops_data, fetch_game_directory_streams, ops, validate_oauth_token,
};
use crate::platforms::twitch::models::{
    parse_campaign_details, parse_campaigns_dashboard, parse_directory_channels,
    parse_drop_current_session_progress, parse_inventory_progress,
};
use crate::platforms::twitch::watch::TwitchWatchSession;
use crate::platforms::{
    Account, AppError, Campaign, Channel, ClaimResult, Drop, DropProgress, Platform, PlatformId,
    WatchTickResult,
};

const TWITCH_DROPS_TAG_ID: &str = "c2542d6d-cd10-4532-919b-3dcc9f54d5bf";
const TWITCH_INVENTORY_BRIDGE_COOLDOWN: Duration = Duration::from_secs(10 * 60);

pub struct TwitchPlatform {
    http: Arc<AllowlistedClient>,
    app: Option<AppHandle>,
    bridge_lock: Arc<Mutex<()>>,
    watch: TwitchWatchSession,
    last_watched_channel: StdMutex<Option<(String, String)>>,
    last_inventory_bridge_at: StdMutex<Option<Instant>>,
    hydrated_drops: StdMutex<HashMap<String, Vec<Drop>>>,
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
            last_watched_channel: StdMutex::new(None),
            last_inventory_bridge_at: StdMutex::new(None),
            hydrated_drops: StdMutex::new(HashMap::new()),
        }
    }

    async fn filter_participating_channels(
        &self,
        token: &str,
        campaign: &Campaign,
        channels: Vec<Channel>,
    ) -> Vec<Channel> {
        let channels: Vec<Channel> = match campaign.allowed_channels.as_ref().filter(|a| !a.is_empty()) {
            Some(allowed) => channels
                .into_iter()
                .filter(|ch| {
                    allowed
                        .iter()
                        .any(|a| a.login.eq_ignore_ascii_case(&ch.login) || a.id == ch.id)
                })
                .collect(),
            None => channels,
        };

        if channels.is_empty() {
            return channels;
        }
        let ids: Vec<String> = channels.iter().map(|c| c.id.clone()).collect();
        match fetch_channels_available_drops_data(&self.http, Some(token), &ids).await {
            Ok(data) if !data.campaign_ids_by_channel.is_empty() => {
                if !data.drops_by_campaign.is_empty() {
                    if let Ok(mut cache) = self.hydrated_drops.lock() {
                        for (camp_id, drops) in data.drops_by_campaign {
                            if !drops.is_empty() {
                                cache.insert(camp_id, drops);
                            }
                        }
                    }
                }
                let avail_map = data.campaign_ids_by_channel;
                let total = channels.len();
                let mut participating: Vec<Channel> = channels
                    .into_iter()
                    .filter(|ch| {
                        avail_map
                            .get(&ch.id)
                            .map(|camp_ids| camp_ids.iter().any(|id| id == &campaign.id))
                            .unwrap_or(false)
                    })
                    .collect();
                participating.sort_by_key(|a| std::cmp::Reverse(a.viewer_count));
                if participating.is_empty() {
                    let channels_with_drops = avail_map.values().filter(|v| !v.is_empty()).count();
                    let sample_ids: Vec<&str> = avail_map
                        .values()
                        .flatten()
                        .map(String::as_str)
                        .take(5)
                        .collect();
                    tracing::info!(
                        game = %campaign.game.name,
                        campaign = %campaign.name,
                        campaign_id = %campaign.id,
                        candidates = total,
                        channels_with_drops,
                        sample_campaign_ids = ?sample_ids,
                        participating = 0,
                        "Verified Twitch participating channels for drop campaign"
                    );
                } else {
                    tracing::info!(
                        game = %campaign.game.name,
                        campaign = %campaign.name,
                        candidates = total,
                        participating = participating.len(),
                        "Verified Twitch participating channels for drop campaign"
                    );
                }
                participating
            }
            Ok(_) => Vec::new(),
            Err(err) => {
                tracing::warn!(
                    campaign = %campaign.name,
                    error = %err,
                    "Could not verify Twitch DropsHighlightService_AvailableDrops; rejecting unverified directory candidates"
                );
                Vec::new()
            }
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
    let already_linked = camp.linked && camp.link_verified;
    camp.linked = already_linked || detailed.linked;
    camp.link_verified = already_linked || detailed.link_verified;
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
        // 1. Try pure-Rust direct Inventory GQL first (0 MB WebView2 RAM).
        match execute_persisted_gql(
            &self.http,
            token,
            ops::INVENTORY,
            json!({ "fetchRewardCampaigns": false }),
        )
        .await
        {
            Ok(val) => return parse_inventory_progress(&val),
            Err(AppError::Blocked(_)) => {}
            Err(err) => return Err(err),
        }

        // 2. Try pure-Rust DropCurrentSessionContext for the active watched channel
        //    so live minutes sync over lightweight HTTP without spawning any hidden WebView2 tab.
        let watched = self
            .last_watched_channel
            .lock()
            .ok()
            .and_then(|g| g.clone());
        let last_bridge = self
            .last_inventory_bridge_at
            .lock()
            .ok()
            .and_then(|g| *g);
        let bridge_due = match last_bridge {
            None => true,
            Some(at) => watched.is_some() && at.elapsed() >= TWITCH_INVENTORY_BRIDGE_COOLDOWN,
        };

        let mut completed_needs_claim = false;
        if let Some((ref ch_id, ref ch_login)) = watched {
            if let Ok(sess_val) = execute_persisted_gql(
                &self.http,
                token,
                ops::DROP_CURRENT_SESSION_CONTEXT,
                json!({
                    "channelID": ch_id,
                    "channelLogin": ch_login,
                }),
            )
            .await
            {
                if let Some((prog, completed)) = parse_drop_current_session_progress(&sess_val) {
                    if !completed && !bridge_due {
                        return Ok(vec![prog]);
                    }
                    completed_needs_claim = completed;
                }
            }
        }

        // 3. Only open the temporary official Inventory page bridge when truly needed:
        //    - First sync after startup/login or explicit manual Refresh, OR
        //    - A drop reached 100% and needs dropInstanceID for auto-claim, OR
        //    - Actively watching and >= 10 minutes elapsed since last bridge sync.
        let Some(ref app_handle) = self.app else {
            return Ok(Vec::new());
        };

        let should_run_bridge = bridge_due || completed_needs_claim;

        if !should_run_bridge {
            return Ok(Vec::new());
        }

        let _guard = self.bridge_lock.lock().await;
        let payload = twitch_bridge_capture(
            app_handle,
            "https://www.twitch.tv/drops/inventory",
            "inventory",
        )
        .await?;

        if let Ok(mut g) = self.last_inventory_bridge_at.lock() {
            *g = Some(Instant::now());
        }

        parse_inventory_progress(&payload)
    }

    async fn find_candidate_channels(
        &self,
        token: &str,
        campaign: &Campaign,
    ) -> Result<Vec<Channel>, AppError> {
        // If campaign restricts to specific allowed channels, verify live status on those ONLY.
        // Never fall through to general directory streams for a channel-restricted campaign.
        if let Some(ref allowed) = campaign.allowed_channels {
            if !allowed.is_empty() {
                let mut live_allowed = Vec::new();
                for ch_ref in allowed.iter().take(25) {
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
                            live_allowed.push(checked);
                        }
                    }
                }
                let verified = self
                    .filter_participating_channels(token, campaign, live_allowed)
                    .await;
                return Ok(verified);
            }
        }

        // 1. Fast direct GQL query by Game ID (or Game Name) — resolves real slug (e.g. "overwatch-2")
        // and returns live drops-enabled streams in ~40ms without needing the hidden browser bridge.
        let by_id_res = match fetch_game_directory_streams(
            &self.http,
            Some(token),
            &campaign.game.id,
            &campaign.game.name,
            Some(TWITCH_DROPS_TAG_ID),
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
                    Some(TWITCH_DROPS_TAG_ID),
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

        let mut raw_candidates: Vec<Channel> = Vec::new();
        if let Ok(ref payload) = by_id_res {
            if let Some(s) = payload
                .pointer("/data/game/slug")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                resolved_slug = Some(s.to_string());
            }
            if let Ok(channels) = parse_directory_channels(payload, &campaign.game.name) {
                raw_candidates = channels;
            }
        }

        // Supplement with top game directory streams when Twitch's Drops tag index returns
        // fewer than 8 streams — EVERY channel is strictly verified against
        // DropsHighlightService_AvailableDrops so non-participating streamers are always excluded.
        if raw_candidates.len() < 8 {
            let untagged = match fetch_game_directory_streams(
                &self.http,
                Some(token),
                &campaign.game.id,
                &campaign.game.name,
                None,
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
                        None,
                    )
                    .await
                }
                Err(err) => Err(err),
            };
            if let Ok(ref payload) = untagged {
                if let Ok(extra) = parse_directory_channels(payload, &campaign.game.name) {
                    for ch in extra {
                        if !raw_candidates
                            .iter()
                            .any(|existing| existing.id == ch.id || existing.login.eq_ignore_ascii_case(&ch.login))
                        {
                            raw_candidates.push(ch);
                        }
                    }
                }
            }
        }

        if !raw_candidates.is_empty() {
            let verified = self
                .filter_participating_channels(token, campaign, raw_candidates)
                .await;
            if !verified.is_empty() {
                return Ok(verified);
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
                return Ok(self
                    .filter_participating_channels(token, campaign, channels.clone())
                    .await);
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
                return Ok(self
                    .filter_participating_channels(token, campaign, channels)
                    .await);
            }
        }

        match parsed_direct {
            Ok(channels) => Ok(self
                .filter_participating_channels(token, campaign, channels)
                .await),
            Err(err) => Err(err),
        }
    }

    async fn check_live(&self, token: &str, channel: &Channel) -> Result<Channel, AppError> {
        let rich_payload = match fetch_channel_live_status(&self.http, Some(token), &channel.login).await {
            Ok(val) => Ok(val),
            Err(AppError::Blocked(_)) => {
                fetch_channel_live_status(&self.http, None, &channel.login).await
            }
            Err(err) => Err(err),
        };

        let payload = match rich_payload {
            Ok(val) => val,
            Err(_) => {
                let vars = json!({ "channelLogin": channel.login });
                match execute_persisted_gql(&self.http, token, ops::USE_LIVE, vars.clone()).await {
                    Ok(val) => val,
                    Err(AppError::Blocked(_)) => {
                        execute_persisted_gql_opt(&self.http, None, ops::USE_LIVE, vars).await?
                    }
                    Err(err) => return Err(err),
                }
            }
        };

        let user_node = payload.pointer("/data/user").ok_or_else(|| {
            AppError::PlatformChanged("Missing /data/user in channel live response".to_string())
        })?;

        if user_node.is_null() {
            return Err(AppError::NotFound(format!(
                "Channel '{}' not found",
                channel.login
            )));
        }

        let stream = user_node.get("stream").filter(|s| !s.is_null());
        let is_live = stream.is_some();
        let viewers = stream
            .and_then(|s| s.get("viewersCount"))
            .and_then(|v| v.as_u64())
            .unwrap_or(if is_live { channel.viewer_count } else { 0 });
        let game_name = stream
            .and_then(|s| s.pointer("/game/displayName").or_else(|| s.pointer("/game/name")))
            .and_then(|v| v.as_str())
            .unwrap_or(&channel.game_name)
            .to_string();

        let mut updated = channel.clone();
        if let Some(uid) = user_node
            .get("id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            updated.id = uid.to_string();
        }
        if let Some(dn) = user_node
            .get("displayName")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            updated.display_name = dn.to_string();
        }
        if let Some(av) = user_node
            .get("profileImageURL")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            updated.avatar_url = Some(av.to_string());
        }
        updated.is_live = is_live;
        updated.viewer_count = viewers;
        updated.game_name = game_name;
        updated.stream_id = stream
            .and_then(|s| s.get("id"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        Ok(updated)
    }

    async fn is_channel_participating(
        &self,
        token: &str,
        channel: &Channel,
        campaign: &Campaign,
    ) -> Result<bool, AppError> {
        if let Some(ref allowed) = campaign.allowed_channels {
            if !allowed.is_empty()
                && !allowed
                    .iter()
                    .any(|a| a.login.eq_ignore_ascii_case(&channel.login) || a.id == channel.id)
            {
                return Ok(false);
            }
        }
        let ids = vec![channel.id.clone()];
        match fetch_channels_available_drop_campaign_ids(&self.http, Some(token), &ids).await {
            Ok(avail_map) if !avail_map.is_empty() => Ok(avail_map
                .get(&channel.id)
                .map(|camp_ids| camp_ids.iter().any(|id| id == &campaign.id))
                .unwrap_or(false)),
            _ => Ok(true),
        }
    }

    async fn watch_tick(
        &self,
        token: &str,
        channel: &Channel,
        compat_mode: bool,
    ) -> Result<WatchTickResult, AppError> {
        if let Ok(mut g) = self.last_watched_channel.lock() {
            *g = Some((channel.id.clone(), channel.login.clone()));
        }
        self.watch
            .tick(&self.http, self.app.as_ref(), token, channel, compat_mode)
            .await
    }

    async fn stop_watching(&self) {
        self.watch.close().await;
        if let Ok(mut g) = self.last_watched_channel.lock() {
            *g = None;
        }
    }

    fn reset_progress_sync_cooldown(&self) {
        if let Ok(mut g) = self.last_inventory_bridge_at.lock() {
            *g = None;
        }
    }

    fn cached_campaign_drops(&self, campaign_id: &str) -> Option<Vec<Drop>> {
        self.hydrated_drops
            .lock()
            .ok()
            .and_then(|g| g.get(campaign_id).cloned())
    }

    async fn claim(
        &self,
        token: &str,
        campaign: &Campaign,
        drop: &Drop,
    ) -> Result<ClaimResult, AppError> {
        match claim::claim_drop(&self.http, token, drop).await {
            Ok(()) => Ok(ClaimResult::default()),
            Err(AppError::Blocked(_)) | Err(AppError::NotFound(_)) if self.app.is_some() => {
                let _guard = self.bridge_lock.lock().await;
                let app_handle = self
                    .app
                    .as_ref()
                    .ok_or_else(|| AppError::Internal("Missing app handle".to_string()))?;
                let reward_names: Vec<String> =
                    drop.rewards.iter().map(|r| r.name.clone()).collect();
                twitch_bridge_claim_drop(
                    app_handle,
                    &campaign.name,
                    &campaign.game.name,
                    &drop.id,
                    &drop.name,
                    &reward_names,
                    drop.instance_id.as_deref(),
                )
                .await?;
                Ok(ClaimResult::default())
            }
            Err(err) => Err(err),
        }
    }

    async fn logout(&self, _token: Option<&str>) -> Result<(), AppError> {
        self.watch.invalidate_token().await;
        if let Ok(mut g) = self.last_watched_channel.lock() {
            *g = None;
        }
        if let Ok(mut g) = self.last_inventory_bridge_at.lock() {
            *g = None;
        }
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
