use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{Duration, Utc};

use crate::platforms::{
    Account, AppError, Campaign, CampaignStatus, Channel, ChannelRef, ClaimResult, Drop,
    DropProgress, Game, Platform, PlatformId, Reward, WatchTickResult,
};

#[derive(Debug)]
struct MockState {
    campaigns: Vec<Campaign>,
    tick_count: u32,
}

#[derive(Debug)]
pub struct MockPlatform {
    id: PlatformId,
    state: Mutex<MockState>,
}

impl MockPlatform {
    pub fn new(id: PlatformId) -> Self {
        let now = Utc::now();
        let campaigns = match id {
            PlatformId::Twitch => vec![
                Campaign {
                    id: "tw-rust-1".to_string(),
                    platform: PlatformId::Twitch,
                    game: Game {
                        id: "263490".to_string(),
                        name: "Rust".to_string(),
                        slug: None,
                        box_art_url: None,
                    },
                    name: "Rust Twitch Rivals Drops".to_string(),
                    starts_at: now - Duration::hours(12),
                    ends_at: now + Duration::hours(42),
                    status: CampaignStatus::Active,
                    requires_link: true,
                    linked: true,
                    link_verified: true,
                    link_url: Some("https://twitch.facepunch.com".to_string()),
                    allowed_channels: Some(vec![
                        ChannelRef {
                            id: "101".to_string(),
                            login: "hJune".to_string(),
                            display_name: "hJune".to_string(),
                        },
                        ChannelRef {
                            id: "102".to_string(),
                            login: "Posty".to_string(),
                            display_name: "Posty".to_string(),
                        },
                    ]),
                    drops: vec![
                        Drop {
                            id: "tw-rust-d1".to_string(),
                            instance_id: Some("inst-tw-rust-d1".to_string()),
                            name: "Cobalt Garage Door".to_string(),
                            rewards: vec![Reward {
                                name: "Cobalt Garage Door".to_string(),
                                image_url: None,
                            }],
                            required_minutes: 30,
                            current_minutes: 15,
                            claimable: false,
                            claimed: false,
                            needs_manual_claim: false,
                        },
                        Drop {
                            id: "tw-rust-d2".to_string(),
                            instance_id: Some("inst-tw-rust-d2".to_string()),
                            name: "Rivals Hoodie".to_string(),
                            rewards: vec![Reward {
                                name: "Rivals Hoodie".to_string(),
                                image_url: None,
                            }],
                            required_minutes: 60,
                            current_minutes: 0,
                            claimable: false,
                            claimed: false,
                            needs_manual_claim: false,
                        },
                    ],
                },
                Campaign {
                    id: "tw-poe2-1".to_string(),
                    platform: PlatformId::Twitch,
                    game: Game {
                        id: "1702520304".to_string(),
                        name: "Path of Exile 2".to_string(),
                        slug: None,
                        box_art_url: None,
                    },
                    name: "Dawn of the Hunt Week 1".to_string(),
                    starts_at: now - Duration::hours(6),
                    ends_at: now + Duration::days(3),
                    status: CampaignStatus::Active,
                    requires_link: true,
                    linked: false,
                    link_verified: true,
                    link_url: Some("https://www.pathofexile.com/my-account/twitch".to_string()),
                    allowed_channels: None,
                    drops: vec![Drop {
                        id: "tw-poe2-d1".to_string(),
                        instance_id: Some("inst-tw-poe2-d1".to_string()),
                        name: "Executioner's Portal Effect".to_string(),
                        rewards: vec![Reward {
                            name: "Executioner's Portal".to_string(),
                            image_url: None,
                        }],
                        required_minutes: 120,
                        current_minutes: 0,
                        claimable: false,
                        claimed: false,
                        needs_manual_claim: false,
                    }],
                },
                Campaign {
                    id: "tw-cs2-urgent".to_string(),
                    platform: PlatformId::Twitch,
                    game: Game {
                        id: "32399".to_string(),
                        name: "Counter-Strike 2".to_string(),
                        slug: None,
                        box_art_url: None,
                    },
                    name: "Major Austin Viewer Souvenir".to_string(),
                    starts_at: now - Duration::hours(19),
                    ends_at: now + Duration::minutes(270),
                    status: CampaignStatus::Active,
                    requires_link: false,
                    linked: true,
                    link_verified: true,
                    link_url: None,
                    allowed_channels: None,
                    drops: vec![Drop {
                        id: "tw-cs2-d1".to_string(),
                        instance_id: Some("inst-tw-cs2-d1".to_string()),
                        name: "Championship Sticker Capsule".to_string(),
                        rewards: vec![Reward {
                            name: "Sticker Capsule".to_string(),
                            image_url: None,
                        }],
                        required_minutes: 30,
                        current_minutes: 5,
                        claimable: false,
                        claimed: false,
                        needs_manual_claim: false,
                    }],
                },
                Campaign {
                    id: "tw-val-1".to_string(),
                    platform: PlatformId::Twitch,
                    game: Game {
                        id: "516575".to_string(),
                        name: "VALORANT".to_string(),
                        slug: None,
                        box_art_url: None,
                    },
                    name: "Masters Playoffs Capsule".to_string(),
                    starts_at: now + Duration::hours(18),
                    ends_at: now + Duration::days(5),
                    status: CampaignStatus::Upcoming,
                    requires_link: true,
                    linked: true,
                    link_verified: true,
                    link_url: Some("https://www.twitch.tv/drops/campaigns".to_string()),
                    allowed_channels: None,
                    drops: vec![Drop {
                        id: "tw-val-d1".to_string(),
                        instance_id: None,
                        name: "Playoffs Spray".to_string(),
                        rewards: vec![Reward {
                            name: "Playoffs Spray".to_string(),
                            image_url: None,
                        }],
                        required_minutes: 60,
                        current_minutes: 0,
                        claimable: false,
                        claimed: false,
                        needs_manual_claim: false,
                    }],
                },
            ],
            PlatformId::Kick => vec![
                Campaign {
                    id: "kk-rust-1".to_string(),
                    platform: PlatformId::Kick,
                    game: Game {
                        id: "rust".to_string(),
                        name: "Rust".to_string(),
                        slug: None,
                        box_art_url: None,
                    },
                    name: "Kick Rust Creators Round".to_string(),
                    starts_at: now - Duration::hours(4),
                    ends_at: now + Duration::hours(36),
                    status: CampaignStatus::Active,
                    requires_link: true,
                    linked: true,
                    link_verified: true,
                    link_url: Some("https://kick.com/drops/inventory".to_string()),
                    allowed_channels: None,
                    drops: vec![Drop {
                        id: "kk-rust-d1".to_string(),
                        instance_id: Some("inst-kk-rust-d1".to_string()),
                        name: "Emerald Sleeping Bag".to_string(),
                        rewards: vec![Reward {
                            name: "Emerald Sleeping Bag".to_string(),
                            image_url: None,
                        }],
                        required_minutes: 45,
                        current_minutes: 20,
                        claimable: false,
                        claimed: false,
                        needs_manual_claim: false,
                    }],
                },
                Campaign {
                    id: "kk-eft-1".to_string(),
                    platform: PlatformId::Kick,
                    game: Game {
                        id: "eft".to_string(),
                        name: "Escape from Tarkov".to_string(),
                        slug: None,
                        box_art_url: None,
                    },
                    name: "Tarkov Weekend Drop".to_string(),
                    starts_at: now - Duration::hours(2),
                    ends_at: now + Duration::hours(60),
                    status: CampaignStatus::Active,
                    requires_link: false,
                    linked: true,
                    link_verified: true,
                    link_url: None,
                    allowed_channels: None,
                    drops: vec![Drop {
                        id: "kk-eft-d1".to_string(),
                        instance_id: Some("inst-kk-eft-d1".to_string()),
                        name: "Streamer Supply Case".to_string(),
                        rewards: vec![Reward {
                            name: "Streamer Supply Case".to_string(),
                            image_url: None,
                        }],
                        required_minutes: 90,
                        current_minutes: 90,
                        claimable: false,
                        claimed: true,
                        needs_manual_claim: false,
                    }],
                },
                Campaign {
                    id: "kk-poe2-upcoming".to_string(),
                    platform: PlatformId::Kick,
                    game: Game {
                        id: "poe2".to_string(),
                        name: "Path of Exile 2".to_string(),
                        slug: None,
                        box_art_url: None,
                    },
                    name: "Kick Launch Race Drops".to_string(),
                    starts_at: now + Duration::hours(14),
                    ends_at: now + Duration::days(4),
                    status: CampaignStatus::Upcoming,
                    requires_link: true,
                    linked: false,
                    link_verified: true,
                    link_url: Some("https://kick.com/drops/inventory".to_string()),
                    allowed_channels: None,
                    drops: vec![Drop {
                        id: "kk-poe2-d1".to_string(),
                        instance_id: None,
                        name: "Verdant Weapon Effect".to_string(),
                        rewards: vec![Reward {
                            name: "Verdant Weapon Effect".to_string(),
                            image_url: None,
                        }],
                        required_minutes: 60,
                        current_minutes: 0,
                        claimable: false,
                        claimed: false,
                        needs_manual_claim: false,
                    }],
                },
            ],
        };

        Self {
            id,
            state: Mutex::new(MockState {
                campaigns,
                tick_count: 0,
            }),
        }
    }

    fn mock_channels(&self, game_name: &str) -> Vec<Channel> {
        match (self.id, game_name) {
            (PlatformId::Twitch, "Rust") => vec![
                Channel {
                    id: "101".to_string(),
                    login: "hJune".to_string(),
                    display_name: "hJune".to_string(),
                    avatar_url: None,
                    game_name: "Rust".to_string(),
                    viewer_count: 14_820,
                    is_live: true,
                    drops_enabled: true,
                    is_followed: true,
                    stream_id: Some("mock-stream-101".to_string()),
                },
                Channel {
                    id: "102".to_string(),
                    login: "Posty".to_string(),
                    display_name: "Posty".to_string(),
                    avatar_url: None,
                    game_name: "Rust".to_string(),
                    viewer_count: 6_410,
                    is_live: true,
                    drops_enabled: true,
                    is_followed: false,
                    stream_id: Some("mock-stream-102".to_string()),
                },
            ],
            (PlatformId::Twitch, _) => vec![Channel {
                id: "103".to_string(),
                login: "Zizaran".to_string(),
                display_name: "Zizaran".to_string(),
                avatar_url: None,
                game_name: game_name.to_string(),
                viewer_count: 18_300,
                is_live: true,
                drops_enabled: true,
                is_followed: true,
                stream_id: Some("mock-stream-103".to_string()),
            }],
            (PlatformId::Kick, _) => vec![
                Channel {
                    id: "201".to_string(),
                    login: "xQc".to_string(),
                    display_name: "xQc".to_string(),
                    avatar_url: None,
                    game_name: game_name.to_string(),
                    viewer_count: 24_900,
                    is_live: true,
                    drops_enabled: true,
                    is_followed: false,
                    stream_id: Some("900201".to_string()),
                },
                Channel {
                    id: "202".to_string(),
                    login: "Buddha".to_string(),
                    display_name: "Buddha".to_string(),
                    avatar_url: None,
                    game_name: game_name.to_string(),
                    viewer_count: 8_140,
                    is_live: true,
                    drops_enabled: true,
                    is_followed: true,
                    stream_id: Some("900202".to_string()),
                },
            ],
        }
    }
}

#[async_trait]
impl Platform for MockPlatform {
    fn id(&self) -> PlatformId {
        self.id
    }

    async fn fetch_public_campaigns(&self) -> Result<Vec<Campaign>, AppError> {
        self.fetch_campaigns("mock-session").await
    }

    async fn validate_session(&self, _token: &str) -> Result<Account, AppError> {
        Ok(match self.id {
            PlatformId::Twitch => Account {
                id: "mock-tw-1".to_string(),
                login: "demo_pilot".to_string(),
                display_name: "DemoPilot".to_string(),
                avatar_url: None,
            },
            PlatformId::Kick => Account {
                id: "mock-kk-1".to_string(),
                login: "kick_pilot".to_string(),
                display_name: "KickPilot".to_string(),
                avatar_url: None,
            },
        })
    }

    async fn fetch_campaigns(&self, _token: &str) -> Result<Vec<Campaign>, AppError> {
        let guard = self
            .state
            .lock()
            .map_err(|_| AppError::Internal("Mock state lock poisoned".to_string()))?;
        Ok(guard.campaigns.clone())
    }

    async fn fetch_progress(&self, _token: &str) -> Result<Vec<DropProgress>, AppError> {
        let guard = self
            .state
            .lock()
            .map_err(|_| AppError::Internal("Mock state lock poisoned".to_string()))?;

        let mut list = Vec::new();
        for c in &guard.campaigns {
            for d in &c.drops {
                list.push(DropProgress {
                    campaign_id: c.id.clone(),
                    drop_id: d.id.clone(),
                    instance_id: d.instance_id.clone(),
                    current_minutes: d.current_minutes,
                    progress_bps: None,
                    claimable: d.claimable,
                    claimed: d.claimed,
                    linked: Some(c.linked),
                    drop: None,
                    campaign_name: Some(c.name.clone()),
                    game_name: Some(c.game.name.clone()),
                });
            }
        }
        Ok(list)
    }

    async fn find_candidate_channels(
        &self,
        _token: &str,
        campaign: &Campaign,
    ) -> Result<Vec<Channel>, AppError> {
        Ok(self.mock_channels(&campaign.game.name))
    }

    async fn check_live(&self, _token: &str, channel: &Channel) -> Result<Channel, AppError> {
        let mut updated = channel.clone();
        updated.is_live = true;
        Ok(updated)
    }

    async fn watch_tick(
        &self,
        _token: &str,
        channel: &Channel,
        _compat_mode: bool,
    ) -> Result<WatchTickResult, AppError> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| AppError::Internal("Mock state lock poisoned".to_string()))?;

        guard.tick_count = guard.tick_count.saturating_add(1);

        // Advance the first uncompleted active drop matching the watched game by +5 mock minutes.
        let mut game_matches: HashMap<String, bool> = HashMap::new();
        for camp in &mut guard.campaigns {
            let matches_game = camp.game.name.eq_ignore_ascii_case(&channel.game_name);
            game_matches.insert(camp.id.clone(), matches_game);
            if camp.status == CampaignStatus::Active && matches_game {
                for drop in &mut camp.drops {
                    if !drop.claimed && !drop.claimable {
                        drop.current_minutes = (drop.current_minutes + 5).min(drop.required_minutes);
                        if drop.current_minutes >= drop.required_minutes {
                            drop.claimable = true;
                        }
                        return Ok(WatchTickResult::Watched);
                    }
                }
            }
        }

        Ok(WatchTickResult::Watched)
    }

    async fn claim(
        &self,
        _token: &str,
        campaign: &Campaign,
        drop: &Drop,
    ) -> Result<ClaimResult, AppError> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| AppError::Internal("Mock state lock poisoned".to_string()))?;

        for c in &mut guard.campaigns {
            if c.id == campaign.id {
                if let Some(d) = c.drops.iter_mut().find(|d| d.id == drop.id) {
                    d.claimed = true;
                    d.claimable = false;
                    d.needs_manual_claim = false;
                }
            }
        }
        Ok(ClaimResult::default())
    }

    async fn logout(&self, _token: Option<&str>) -> Result<(), AppError> {
        Ok(())
    }
}
