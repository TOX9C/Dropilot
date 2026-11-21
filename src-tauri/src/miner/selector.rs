use chrono::{DateTime, Utc};

use crate::platforms::{Campaign, CampaignStatus, Channel};
use crate::settings::{IdleBehavior, PlatformSettings, PriorityItem, Settings};

pub const STALL_SWITCH_SECS: i64 = 15 * 60;
pub const VOLUNTARY_SWITCH_HYSTERESIS_SECS: i64 = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchDecision {
    Keep,
    SwitchImmediate(String),
    SwitchVoluntary(String),
}

pub fn is_campaign_eligible(
    campaign: &Campaign,
    settings: &Settings,
    platform_settings: &PlatformSettings,
    now: DateTime<Utc>,
) -> bool {
    if !settings.disclaimer_accepted() {
        return false;
    }
    if campaign.status != CampaignStatus::Active || campaign.ends_at <= now || campaign.starts_at > now {
        return false;
    }
    if platform_settings.excluded_campaigns.contains(&campaign.id) {
        return false;
    }
    if campaign.requires_link && !campaign.linked && !settings.farm_unlinked {
        return false;
    }
    campaign.drops.is_empty()
        || campaign
            .drops
            .iter()
            .any(|d| !d.claimed && !d.needs_manual_claim)
}

pub fn select_target_campaign<'a>(
    campaigns: &'a [Campaign],
    settings: &Settings,
    platform_settings: &PlatformSettings,
    now: DateTime<Utc>,
) -> Option<&'a Campaign> {
    let eligible: Vec<&Campaign> = campaigns
        .iter()
        .filter(|c| is_campaign_eligible(c, settings, platform_settings, now))
        .collect();

    if eligible.is_empty() {
        return None;
    }

    // 1. Check user-ordered priority list first.
    for item in &platform_settings.priority {
        match item {
            PriorityItem::Campaign { campaign_id, .. } => {
                if let Some(found) = eligible.iter().copied().find(|c| &c.id == campaign_id) {
                    return Some(found);
                }
            }
            PriorityItem::Game { game_id, .. } => {
                if let Some(found) = eligible
                    .iter()
                    .copied()
                    .filter(|c| &c.game.id == game_id)
                    .min_by_key(|c| c.ends_at)
                {
                    return Some(found);
                }
            }
        }
    }

    // 2. Check explicitly selected campaigns not yet placed in priority list.
    if let Some(found) = eligible
        .iter()
        .copied()
        .filter(|c| platform_settings.farm_campaigns.contains(&c.id))
        .min_by_key(|c| c.ends_at)
    {
        return Some(found);
    }

    // 3. Fallback according to idle behavior.
    if settings.idle_behavior == IdleBehavior::FarmAny {
        return eligible.into_iter().min_by_key(|c| c.ends_at);
    }

    None
}

pub fn select_best_channel<'a>(
    campaign: &Campaign,
    candidates: &'a [Channel],
    pinned_login: Option<&str>,
) -> Option<&'a Channel> {
    let allowed_list = campaign
        .allowed_channels
        .as_ref()
        .filter(|list| !list.is_empty());

    let eligible: Vec<&Channel> = candidates
        .iter()
        .filter(|ch| ch.is_live && ch.drops_enabled)
        .filter(|ch| {
            ch.game_name.is_empty()
                || ch.game_name.eq_ignore_ascii_case(&campaign.game.name)
        })
        .filter(|ch| match allowed_list {
            Some(list) => list
                .iter()
                .any(|a| a.login.eq_ignore_ascii_case(&ch.login) || a.id == ch.id),
            None => true,
        })
        .collect();

    if eligible.is_empty() {
        return None;
    }

    if let Some(pin) = pinned_login {
        if let Some(pinned) = eligible
            .iter()
            .copied()
            .find(|ch| ch.login.eq_ignore_ascii_case(pin))
        {
            return Some(pinned);
        }
    }

    eligible.into_iter().max_by_key(|ch| {
        let in_allowed = allowed_list
            .map(|list| {
                list.iter()
                    .any(|a| a.login.eq_ignore_ascii_case(&ch.login) || a.id == ch.id)
            })
            .unwrap_or(false);
        (in_allowed, ch.is_followed, ch.viewer_count)
    })
}

pub struct SwitchContext<'a> {
    pub current_channel: &'a Channel,
    pub current_campaign: &'a Campaign,
    pub top_campaign: Option<&'a Campaign>,
    pub pinned_login: Option<&'a str>,
    pub now: DateTime<Utc>,
    pub last_progress_advance: DateTime<Utc>,
    pub last_voluntary_switch: Option<DateTime<Utc>>,
}

pub fn evaluate_switch(ctx: SwitchContext<'_>) -> SwitchDecision {
    if !ctx.current_channel.is_live {
        return SwitchDecision::SwitchImmediate("Channel went offline".to_string());
    }

    if !ctx.current_channel.drops_enabled {
        return SwitchDecision::SwitchImmediate("Drops disabled on channel".to_string());
    }

    if !ctx.current_channel.game_name.is_empty()
        && !ctx
            .current_channel
            .game_name
            .eq_ignore_ascii_case(&ctx.current_campaign.game.name)
    {
        return SwitchDecision::SwitchImmediate(format!(
            "Channel switched game to {}",
            ctx.current_channel.game_name
        ));
    }

    let campaign_done = !ctx.current_campaign.drops.is_empty()
        && ctx
            .current_campaign
            .drops
            .iter()
            .all(|d| d.claimed || d.needs_manual_claim);

    if campaign_done
        || ctx.current_campaign.status != CampaignStatus::Active
        || ctx.current_campaign.ends_at <= ctx.now
    {
        return SwitchDecision::SwitchImmediate("Current campaign completed or ended".to_string());
    }

    if (ctx.now - ctx.last_progress_advance).num_seconds() >= STALL_SWITCH_SECS {
        return SwitchDecision::SwitchImmediate(
            "No drop progress for 15 minutes; rotating channel".to_string(),
        );
    }

    let voluntary_allowed = ctx
        .last_voluntary_switch
        .map(|prev| (ctx.now - prev).num_seconds() >= VOLUNTARY_SWITCH_HYSTERESIS_SECS)
        .unwrap_or(true);

    if voluntary_allowed {
        if let Some(top) = ctx.top_campaign {
            if top.id != ctx.current_campaign.id {
                return SwitchDecision::SwitchVoluntary(format!(
                    "Higher-priority campaign available: {}",
                    top.name
                ));
            }
        }

        if let Some(pin) = ctx.pinned_login {
            if !ctx.current_channel.login.eq_ignore_ascii_case(pin) {
                return SwitchDecision::SwitchVoluntary(format!("Switching to pinned channel {pin}"));
            }
        }
    }

    SwitchDecision::Keep
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platforms::{ChannelRef, Drop, Game, PlatformId, Reward};
    use chrono::Duration;

    fn mk_drop(id: &str, claimed: bool) -> Drop {
        Drop {
            id: id.to_string(),
            instance_id: None,
            name: id.to_string(),
            rewards: vec![Reward {
                name: "Skin".to_string(),
                image_url: None,
            }],
            required_minutes: 60,
            current_minutes: if claimed { 60 } else { 15 },
            claimable: false,
            claimed,
            needs_manual_claim: false,
        }
    }

    fn mk_campaign(id: &str, game_id: &str, game_name: &str, linked: bool, ends_hours: i64) -> Campaign {
        let now = Utc::now();
        Campaign {
            id: id.to_string(),
            platform: PlatformId::Twitch,
            game: Game {
                id: game_id.to_string(),
                name: game_name.to_string(),
                box_art_url: None,
            },
            name: format!("{game_name} Campaign"),
            starts_at: now - Duration::hours(1),
            ends_at: now + Duration::hours(ends_hours),
            status: CampaignStatus::Active,
            requires_link: true,
            linked,
            link_verified: true,
            link_url: Some("https://www.twitch.tv/drops".to_string()),
            allowed_channels: None,
            drops: vec![mk_drop("drop-1", false)],
        }
    }

    fn mk_channel(login: &str, game: &str, viewers: u64, followed: bool, live: bool) -> Channel {
        Channel {
            id: login.to_string(),
            login: login.to_string(),
            display_name: login.to_string(),
            avatar_url: None,
            game_name: game.to_string(),
            viewer_count: viewers,
            is_live: live,
            drops_enabled: true,
            is_followed: followed,
        }
    }

    #[test]
    fn respects_disclaimer_and_unlinked_rules() {
        let now = Utc::now();
        let campaigns = vec![mk_campaign("c1", "g1", "Rust", false, 5)];
        let mut settings = Settings::default();
        settings.idle_behavior = IdleBehavior::FarmAny;

        // Blocked before disclaimer
        assert!(select_target_campaign(&campaigns, &settings, &settings.twitch, now).is_none());

        // Accepted disclaimer, still unlinked -> skipped by default
        settings.disclaimer_accepted_version = 1;
        assert!(select_target_campaign(&campaigns, &settings, &settings.twitch, now).is_none());

        // Enabled farm_unlinked -> selected
        settings.farm_unlinked = true;
        assert_eq!(
            select_target_campaign(&campaigns, &settings, &settings.twitch, now).map(|c| c.id.as_str()),
            Some("c1")
        );
    }

    #[test]
    fn honors_priority_list_over_farm_any() {
        let now = Utc::now();
        let c1 = mk_campaign("c1", "g1", "Rust", true, 12);
        let c2 = mk_campaign("c2", "g2", "VALORANT", true, 2);
        let campaigns = vec![c1, c2];

        let mut settings = Settings::default();
        settings.disclaimer_accepted_version = 1;
        settings.idle_behavior = IdleBehavior::StayIdle;
        settings.twitch.priority = vec![PriorityItem::Game {
            game_id: "g1".to_string(),
            game_name: "Rust".to_string(),
        }];

        let picked = select_target_campaign(&campaigns, &settings, &settings.twitch, now);
        assert_eq!(picked.map(|c| c.id.as_str()), Some("c1"));
    }

    #[test]
    fn channel_ranking_prefers_pinned_allowed_followed_then_viewers() {
        let mut camp = mk_campaign("c1", "g1", "Rust", true, 4);
        let ch_high = mk_channel("high", "Rust", 50_000, false, true);
        let ch_followed = mk_channel("followed", "Rust", 500, true, true);
        let ch_offline = mk_channel("offline", "Rust", 99_000, true, false);
        let candidates = vec![ch_high.clone(), ch_followed.clone(), ch_offline];

        // Without allowed_channels, followed beats higher unfollowed viewers
        let picked = select_best_channel(&camp, &candidates, None).expect("channel selected");
        assert_eq!(picked.login, "followed");

        // Pinned wins over all
        let pinned = select_best_channel(&camp, &candidates, Some("high")).expect("pinned");
        assert_eq!(pinned.login, "high");

        // With allowed_channels restriction, only allowed channels qualify
        camp.allowed_channels = Some(vec![ChannelRef {
            id: "high".to_string(),
            login: "high".to_string(),
            display_name: "high".to_string(),
        }]);
        let restricted = select_best_channel(&camp, &candidates, Some("followed")).expect("allowed");
        assert_eq!(restricted.login, "high");
    }

    #[test]
    fn switch_rules_enforce_immediate_offline_and_2min_voluntary_hysteresis() {
        let now = Utc::now();
        let c1 = mk_campaign("c1", "g1", "Rust", true, 5);
        let c2 = mk_campaign("c2", "g2", "Escape from Tarkov", true, 5);
        let mut ch = mk_channel("streamer", "Rust", 1000, false, true);

        // Voluntary switch blocked within 120s hysteresis
        let dec = evaluate_switch(SwitchContext {
            current_channel: &ch,
            current_campaign: &c1,
            top_campaign: Some(&c2),
            pinned_login: None,
            now,
            last_progress_advance: now,
            last_voluntary_switch: Some(now - Duration::seconds(60)),
        });
        assert_eq!(dec, SwitchDecision::Keep);

        // Voluntary switch allowed after 120s
        let dec2 = evaluate_switch(SwitchContext {
            current_channel: &ch,
            current_campaign: &c1,
            top_campaign: Some(&c2),
            pinned_login: None,
            now,
            last_progress_advance: now,
            last_voluntary_switch: Some(now - Duration::seconds(130)),
        });
        assert!(matches!(dec2, SwitchDecision::SwitchVoluntary(_)));

        // Stall >= 15 min triggers immediate switch even inside hysteresis window
        let stalled = evaluate_switch(SwitchContext {
            current_channel: &ch,
            current_campaign: &c1,
            top_campaign: Some(&c1),
            pinned_login: None,
            now,
            last_progress_advance: now - Duration::minutes(16),
            last_voluntary_switch: Some(now - Duration::seconds(10)),
        });
        assert!(matches!(stalled, SwitchDecision::SwitchImmediate(_)));

        // Offline triggers immediate switch even inside hysteresis window
        ch.is_live = false;
        let offline = evaluate_switch(SwitchContext {
            current_channel: &ch,
            current_campaign: &c1,
            top_campaign: Some(&c1),
            pinned_login: None,
            now,
            last_progress_advance: now,
            last_voluntary_switch: Some(now - Duration::seconds(5)),
        });
        assert!(matches!(offline, SwitchDecision::SwitchImmediate(_)));
    }

    #[test]
    fn empty_drops_campaign_stays_eligible_until_inventory_hydrates() {
        let now = Utc::now();
        let mut camp = mk_campaign("c-empty", "g1", "Rust", true, 6);
        camp.drops.clear();

        let mut settings = Settings::default();
        settings.disclaimer_accepted_version = 1;
        settings.idle_behavior = IdleBehavior::FarmAny;

        assert!(is_campaign_eligible(&camp, &settings, &settings.twitch, now));

        let ch = mk_channel("streamer", "Rust", 1000, false, true);
        let dec = evaluate_switch(SwitchContext {
            current_channel: &ch,
            current_campaign: &camp,
            top_campaign: Some(&camp),
            pinned_login: None,
            now,
            last_progress_advance: now,
            last_voluntary_switch: None,
        });
        assert_eq!(dec, SwitchDecision::Keep);
    }
}
