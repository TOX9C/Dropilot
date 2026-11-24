pub mod kick;
#[cfg(any(feature = "mock", test))]
pub mod mock;
pub mod twitch;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlatformId {
    Twitch,
    Kick,
}

impl PlatformId {
    pub fn as_str(self) -> &'static str {
        match self {
            PlatformId::Twitch => "twitch",
            PlatformId::Kick => "kick",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            PlatformId::Twitch => "Twitch",
            PlatformId::Kick => "Kick",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    pub box_art_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelRef {
    pub id: String,
    pub login: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: String,
    pub login: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub game_name: String,
    pub viewer_count: u64,
    pub is_live: bool,
    pub drops_enabled: bool,
    pub is_followed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reward {
    pub name: String,
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Drop {
    pub id: String,
    pub instance_id: Option<String>,
    pub name: String,
    pub rewards: Vec<Reward>,
    pub required_minutes: u32,
    pub current_minutes: u32,
    pub claimable: bool,
    pub claimed: bool,
    pub needs_manual_claim: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CampaignStatus {
    Active,
    Upcoming,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Campaign {
    pub id: String,
    pub platform: PlatformId,
    pub game: Game,
    pub name: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub status: CampaignStatus,
    pub requires_link: bool,
    pub linked: bool,
    #[serde(default)]
    pub link_verified: bool,
    pub link_url: Option<String>,
    pub allowed_channels: Option<Vec<ChannelRef>>,
    pub drops: Vec<Drop>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DropProgress {
    pub campaign_id: String,
    pub drop_id: String,
    pub instance_id: Option<String>,
    pub current_minutes: u32,
    /// Optional completion ratio in basis points (0..=10_000) when a platform progress API
    /// reports fractional progress without repeating `required_minutes`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_bps: Option<u32>,
    pub claimable: bool,
    pub claimed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drop: Option<Drop>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub campaign_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub login: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MinerState {
    LoggedOut,
    Idle,
    Selecting,
    Mining,
    Claiming,
    Paused,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformSnapshot {
    pub platform: PlatformId,
    pub state: MinerState,
    pub state_reason: Option<String>,
    pub account: Option<Account>,
    pub watching: Option<Channel>,
    pub current_campaign_id: Option<String>,
    pub current_drop_id: Option<String>,
    #[serde(default)]
    pub watching_since: Option<DateTime<Utc>>,
    #[serde(default)]
    pub progress_synced_at: Option<DateTime<Utc>>,
    pub next_action_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub campaigns: Vec<Campaign>,
    pub candidate_channels: Vec<Channel>,
    #[serde(default)]
    pub pinned_channel: Option<String>,
}

impl PlatformSnapshot {
    pub fn logged_out(platform: PlatformId) -> Self {
        Self {
            platform,
            state: MinerState::LoggedOut,
            state_reason: Some("Not logged in".to_string()),
            account: None,
            watching: None,
            current_campaign_id: None,
            current_drop_id: None,
            watching_since: None,
            progress_synced_at: None,
            next_action_at: None,
            last_error: None,
            campaigns: Vec::new(),
            candidate_channels: Vec::new(),
            pinned_channel: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentClaim {
    pub id: String,
    pub platform: PlatformId,
    pub game_name: String,
    pub campaign_name: String,
    pub drop_name: String,
    pub reward_image_url: Option<String>,
    pub claimed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub twitch: PlatformSnapshot,
    pub kick: PlatformSnapshot,
    pub recent_claims: Vec<RecentClaim>,
    pub mock_active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchTickResult {
    Watched,
    StreamOffline,
}

#[derive(Debug, Clone, Error)]
pub enum AppError {
    #[error("Authentication required: {0}")]
    Auth(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Rate limited by platform: {message}")]
    RateLimited {
        retry_after_secs: Option<u64>,
        message: String,
    },

    #[error("Blocked by platform protection: {0}")]
    Blocked(String),

    #[error("Platform API may have changed; an update may be needed ({0})")]
    PlatformChanged(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("No live eligible channels available: {0}")]
    NoCandidates(String),

    #[error("Outbound host blocked by allowlist: {0}")]
    HostNotAllowed(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn user_message(&self) -> String {
        match self {
            AppError::Auth(msg) => format!("Please log in again ({msg})"),
            AppError::Network(msg) => format!("Network issue: {msg}"),
            AppError::RateLimited { retry_after_secs, .. } => match retry_after_secs {
                Some(s) => format!("Rate limited; retrying after {s}s"),
                None => "Rate limited; backing off".to_string(),
            },
            AppError::Blocked(msg) => {
                format!("Platform security check requires your action in the official window: {msg}")
            }
            AppError::PlatformChanged(detail) => {
                format!("Platform changed something; an update may be needed ({detail})")
            }
            AppError::NotFound(msg) => msg.clone(),
            AppError::NoCandidates(msg) => msg.clone(),
            AppError::HostNotAllowed(host) => format!("Blocked non-allowlisted host: {host}"),
            AppError::Internal(msg) => format!("Internal error: {msg}"),
        }
    }
}

#[async_trait]
pub trait Platform: Send + Sync {
    fn id(&self) -> PlatformId;

    async fn fetch_public_campaigns(&self) -> Result<Vec<Campaign>, AppError> {
        Ok(Vec::new())
    }

    async fn validate_session(&self, token: &str) -> Result<Account, AppError>;

    async fn fetch_campaigns(&self, token: &str) -> Result<Vec<Campaign>, AppError>;

    async fn fetch_progress(&self, token: &str) -> Result<Vec<DropProgress>, AppError>;

    async fn find_candidate_channels(
        &self,
        token: &str,
        campaign: &Campaign,
    ) -> Result<Vec<Channel>, AppError>;

    async fn check_live(&self, token: &str, channel: &Channel) -> Result<Channel, AppError>;

    /// Verifies that a live channel is currently participating in the target campaign.
    /// Defaults to `true` for platforms without per-channel drop campaign verification.
    async fn is_channel_participating(
        &self,
        _token: &str,
        _channel: &Channel,
        _campaign: &Campaign,
    ) -> Result<bool, AppError> {
        Ok(true)
    }

    async fn watch_tick(
        &self,
        token: &str,
        channel: &Channel,
        compat_mode: bool,
    ) -> Result<WatchTickResult, AppError>;

    async fn stop_watching(&self) {}

    /// Clears any background progress-bridge cooldown so an explicit user refresh
    /// or an imminent drop claim syncs immediately.
    fn reset_progress_sync_cooldown(&self) {}

    /// Returns cached reward drops hydrated from channel-level drop discovery (e.g. Twitch
    /// `DropsHighlightService_AvailableDrops`) when summary campaign lists omit `timeBasedDrops`.
    fn cached_campaign_drops(&self, _campaign_id: &str) -> Option<Vec<Drop>> {
        None
    }

    async fn claim(&self, token: &str, campaign: &Campaign, drop: &Drop) -> Result<(), AppError>;

    async fn logout(&self, token: Option<&str>) -> Result<(), AppError>;
}
