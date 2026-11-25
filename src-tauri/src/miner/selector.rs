use std::collections::HashSet;

use chrono::{DateTime, Utc};

use crate::platforms::kick::models::is_kick_any_stream_campaign;
use crate::platforms::{Campaign, CampaignStatus, Channel};
use crate::settings::{IdleBehavior, PlatformSettings, PriorityItem, Settings};

pub const STALL_SWITCH_SECS: i64 = 15 * 60;
pub const VOLUNTARY_SWITCH_HYSTERESIS_SECS: i64 = 120;
pub const ENDING_SOON_WINDOW_MINS: i64 = 24 * 60;
pub const ENDING_SOON_SLACK_MINS: i64 = 12 * 60;

fn time_left_mins(campaign: &Campaign, now: DateTime<Utc>) -> i64 {
    (campaign.ends_at - now).num_minutes().max(0)
}

fn campaign_remaining_mins(campaign: &Campaign) -> i64 {
    if campaign.drops.is_empty() {
        return 60;
    }
    campaign
        .drops
        .iter()
        .filter(|d| !d.claimed && !d.needs_manual_claim)
        .map(|d| d.required_minutes.saturating_sub(d.current_minutes) as i64)
        .max()
        .unwrap_or(0)
}

fn next_drop_remaining_mins(campaign: &Campaign) -> i64 {
    if campaign.drops.is_empty() {
        return 15;
    }
    campaign
        .drops
        .iter()
        .filter(|d| !d.claimed && !d.needs_manual_claim)
        .map(|d| d.required_minutes.saturating_sub(d.current_minutes) as i64)
        .filter(|&m| m > 0)
        .min()
        .unwrap_or(0)
}

fn is_ending_soon_boostable(campaign: &Campaign, now: DateTime<Utc>) -> bool {
    let tl = time_left_mins(campaign, now);
    if tl <= 0 {
        return false;
    }
    let next_m = next_drop_remaining_mins(campaign);
    if next_m > tl {
        return false;
    }
    let rem = campaign_remaining_mins(campaign);
    tl <= ENDING_SOON_WINDOW_MINS || (tl - rem) <= ENDING_SOON_SLACK_MINS
}

/// Optional deadline-aware priority boost: promotes lower-ranked priority campaigns
/// that are ending soon ahead of higher-priority campaigns when the higher-priority
/// campaigns still have enough time to finish afterward.
fn apply_ending_soon_boost(
    mut remaining: Vec<&Campaign>,
    now: DateTime<Utc>,
) -> Vec<&Campaign> {
    if remaining.len() <= 1 {
        return remaining;
    }

    let mut result = Vec::with_capacity(remaining.len());
    let mut elapsed_planned_mins: i64 = 0;

    while !remaining.is_empty() {
        let mut best_jump: Option<(usize, DateTime<Utc>)> = None;

        for (idx, u) in remaining.iter().enumerate() {
            if idx == 0 || !is_ending_soon_boostable(u, now) {
                continue;
            }
            let u_tl = time_left_mins(u, now);
            let u_dur = campaign_remaining_mins(u).min(u_tl);

            let can_jump_all = remaining[..idx].iter().all(|h| {
                let h_tl = time_left_mins(h, now);
                let h_rem = campaign_remaining_mins(h);
                u.ends_at < h.ends_at && h_tl >= elapsed_planned_mins + u_dur + h_rem
            });

            if can_jump_all {
                let replace = match best_jump {
                    None => true,
                    Some((_, best_end)) => u.ends_at < best_end,
                };
                if replace {
                    best_jump = Some((idx, u.ends_at));
                }
            }
        }

        let pick_idx = best_jump.map(|(i, _)| i).unwrap_or(0);
        let picked = remaining.remove(pick_idx);
        let picked_dur = campaign_remaining_mins(picked).min(time_left_mins(picked, now));
        elapsed_planned_mins += picked_dur;
        result.push(picked);
    }

    result
}

fn normalize_game_alnum(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

pub fn game_names_match(stream_game: &str, campaign_game: &str) -> bool {
    let a = stream_game.trim();
    let b = campaign_game.trim();
    if a.is_empty() || b.is_empty() {
        return true;
    }
    if a.eq_ignore_ascii_case(b) {
        return true;
    }
    let na = normalize_game_alnum(a);
    let nb = normalize_game_alnum(b);
    if na.len() >= 4 && nb.len() >= 4 {
        return na == nb || na.starts_with(&nb) || nb.starts_with(&na);
    }
    false
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchDecision {
    Keep,
    SwitchImmediate(String),
    SwitchVoluntary(String),
}

fn is_campaign_active_window(
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
    campaign.drops.is_empty()
        || campaign
            .drops
            .iter()
            .any(|d| !d.claimed && !d.needs_manual_claim && (settings.auto_claim || !d.claimable))
}

pub fn is_campaign_eligible(
    campaign: &Campaign,
    settings: &Settings,
    platform_settings: &PlatformSettings,
    now: DateTime<Utc>,
) -> bool {
    if !is_campaign_active_window(campaign, settings, platform_settings, now) {
        return false;
    }
    if campaign.requires_link && !campaign.linked && !settings.farm_unlinked {
        return false;
    }
    true
}

/// Returns all eligible active campaigns in strict priority order:
/// 1. User-ordered priority list (each item may match one or more active campaigns, sorted by `ends_at`).
/// 2. Explicitly selected `farm_campaigns` not already in the priority list (sorted by `ends_at`).
/// 3. Fallback `IdleBehavior::FarmAny` eligible campaigns (sorted by `ends_at`).
///
/// Deduplicated by campaign ID while preserving first-seen priority order.
pub fn ordered_target_campaigns<'a>(
    campaigns: &'a [Campaign],
    settings: &Settings,
    platform_settings: &PlatformSettings,
    now: DateTime<Utc>,
) -> Vec<&'a Campaign> {
    let active_pool: Vec<&Campaign> = campaigns
        .iter()
        .filter(|c| is_campaign_active_window(c, settings, platform_settings, now))
        .collect();

    if active_pool.is_empty() {
        return Vec::new();
    }

    let mut ordered: Vec<&'a Campaign> = Vec::new();
    let mut seen: HashSet<&'a str> = HashSet::new();

    let push_unique = |list: &mut Vec<&'a Campaign>, seen: &mut HashSet<&'a str>, c: &'a Campaign| {
        if seen.insert(c.id.as_str()) {
            list.push(c);
        }
    };

    // 1. User-ordered priority list first.
    // On Twitch, if the user explicitly prioritized a campaign or game, allow it even when
    // Twitch GQL reports placeholder isAccountConnected=false for publisher-linked games.
    let mut priority_ordered: Vec<&'a Campaign> = Vec::new();
    for item in &platform_settings.priority {
        match item {
            PriorityItem::Campaign { campaign_id, .. } => {
                if let Some(found) = active_pool.iter().copied().find(|c| {
                    &c.id == campaign_id
                        && (c.platform == crate::platforms::PlatformId::Twitch
                            || is_campaign_eligible(c, settings, platform_settings, now))
                }) {
                    push_unique(&mut priority_ordered, &mut seen, found);
                }
            }
            PriorityItem::Game { game_id, game_name } => {
                let mut matching: Vec<&'a Campaign> = active_pool
                    .iter()
                    .copied()
                    .filter(|c| {
                        let matches_game = (!game_id.is_empty() && c.game.id == *game_id)
                            || (!game_name.trim().is_empty()
                                && game_names_match(&c.game.name, game_name));
                        matches_game
                            && (c.platform == crate::platforms::PlatformId::Twitch
                                || is_campaign_eligible(c, settings, platform_settings, now))
                    })
                    .collect();
                matching.sort_by_key(|c| c.ends_at);
                for c in matching {
                    push_unique(&mut priority_ordered, &mut seen, c);
                }
            }
        }
    }

    if platform_settings.prioritize_ending_soon {
        priority_ordered = apply_ending_soon_boost(priority_ordered, now);
    }
    ordered.extend(priority_ordered);

    // 2. Explicitly selected campaigns not yet placed in priority list.
    let mut farmed: Vec<&'a Campaign> = active_pool
        .iter()
        .copied()
        .filter(|c| {
            platform_settings.farm_campaigns.contains(&c.id)
                && (c.platform == crate::platforms::PlatformId::Twitch
                    || is_campaign_eligible(c, settings, platform_settings, now))
        })
        .collect();
    farmed.sort_by_key(|c| c.ends_at);
    for c in farmed {
        push_unique(&mut ordered, &mut seen, c);
    }

    // 3. Fallback according to idle behavior (strictly requires linked or farm_unlinked).
    if settings.idle_behavior == IdleBehavior::FarmAny {
        let mut rest: Vec<&'a Campaign> = active_pool
            .into_iter()
            .filter(|c| is_campaign_eligible(c, settings, platform_settings, now))
            .collect();
        rest.sort_by_key(|c| c.ends_at);
        for c in rest {
            push_unique(&mut ordered, &mut seen, c);
        }
    }

    ordered
}

pub fn select_target_campaign<'a>(
    campaigns: &'a [Campaign],
    settings: &Settings,
    platform_settings: &PlatformSettings,
    now: DateTime<Utc>,
) -> Option<&'a Campaign> {
    ordered_target_campaigns(campaigns, settings, platform_settings, now)
        .into_iter()
        .next()
}

pub fn select_best_channel<'a>(
    campaign: &Campaign,
    candidates: &'a [Channel],
    pinned_login: Option<&str>,
) -> Option<&'a Channel> {
    let any_stream = is_kick_any_stream_campaign(campaign);
    let allowed_list = campaign
        .allowed_channels
        .as_ref()
        .filter(|list| !list.is_empty());

    let eligible: Vec<&Channel> = candidates
        .iter()
        .filter(|ch| ch.is_live && ch.drops_enabled)
        .filter(|ch| any_stream || game_names_match(&ch.game_name, &campaign.game.name))
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

pub fn evaluate_switch(ctx: SwitchContext<'_>, auto_claim: bool) -> SwitchDecision {
    if !ctx.current_channel.is_live {
        return SwitchDecision::SwitchImmediate("Channel went offline".to_string());
    }

    if !ctx.current_channel.drops_enabled {
        return SwitchDecision::SwitchImmediate("Drops disabled on channel".to_string());
    }

    if !is_kick_any_stream_campaign(ctx.current_campaign)
        && !game_names_match(&ctx.current_channel.game_name, &ctx.current_campaign.game.name)
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
            .all(|d| d.claimed || d.needs_manual_claim || (!auto_claim && d.claimable));

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
                id: None,
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
                slug: None,
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
            stream_id: None,
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
        let dec = evaluate_switch(
            SwitchContext {
                current_channel: &ch,
                current_campaign: &c1,
                top_campaign: Some(&c2),
                pinned_login: None,
                now,
                last_progress_advance: now,
                last_voluntary_switch: Some(now - Duration::seconds(60)),
            },
            true,
        );
        assert_eq!(dec, SwitchDecision::Keep);

        // Voluntary switch allowed after 120s
        let dec2 = evaluate_switch(
            SwitchContext {
                current_channel: &ch,
                current_campaign: &c1,
                top_campaign: Some(&c2),
                pinned_login: None,
                now,
                last_progress_advance: now,
                last_voluntary_switch: Some(now - Duration::seconds(130)),
            },
            true,
        );
        assert!(matches!(dec2, SwitchDecision::SwitchVoluntary(_)));

        // Stall >= 15 min triggers immediate switch even inside hysteresis window
        let stalled = evaluate_switch(
            SwitchContext {
                current_channel: &ch,
                current_campaign: &c1,
                top_campaign: Some(&c1),
                pinned_login: None,
                now,
                last_progress_advance: now - Duration::minutes(16),
                last_voluntary_switch: Some(now - Duration::seconds(10)),
            },
            true,
        );
        assert!(matches!(stalled, SwitchDecision::SwitchImmediate(_)));

        // Offline triggers immediate switch even inside hysteresis window
        ch.is_live = false;
        let offline = evaluate_switch(
            SwitchContext {
                current_channel: &ch,
                current_campaign: &c1,
                top_campaign: Some(&c1),
                pinned_login: None,
                now,
                last_progress_advance: now,
                last_voluntary_switch: Some(now - Duration::seconds(5)),
            },
            true,
        );
        assert!(matches!(offline, SwitchDecision::SwitchImmediate(_)));
    }

    #[test]
    fn claimable_campaign_is_done_when_auto_claim_is_disabled() {
        let now = Utc::now();
        let mut campaign = mk_campaign("claimable", "g1", "Rust", true, 6);
        campaign.drops[0].current_minutes = campaign.drops[0].required_minutes;
        campaign.drops[0].claimable = true;
        let channel = mk_channel("streamer", "Rust", 1000, false, true);

        let decision = evaluate_switch(
            SwitchContext {
                current_channel: &channel,
                current_campaign: &campaign,
                top_campaign: Some(&campaign),
                pinned_login: None,
                now,
                last_progress_advance: now,
                last_voluntary_switch: None,
            },
            false,
        );
        assert!(matches!(decision, SwitchDecision::SwitchImmediate(_)));

        let auto_claim_decision = evaluate_switch(
            SwitchContext {
                current_channel: &channel,
                current_campaign: &campaign,
                top_campaign: Some(&campaign),
                pinned_login: None,
                now,
                last_progress_advance: now,
                last_voluntary_switch: None,
            },
            true,
        );
        assert_eq!(auto_claim_decision, SwitchDecision::Keep);
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
        let dec = evaluate_switch(
            SwitchContext {
                current_channel: &ch,
                current_campaign: &camp,
                top_campaign: Some(&camp),
                pinned_login: None,
                now,
                last_progress_advance: now,
                last_voluntary_switch: None,
            },
            true,
        );
        assert_eq!(dec, SwitchDecision::Keep);
    }

    #[test]
    fn matches_watched_game_by_name_when_drop_appears() {
        let now = Utc::now();
        let eft = mk_campaign("eft-1", "491931", "Escape from Tarkov", true, 6);
        let rust = mk_campaign("rust-1", "263490", "Rust", true, 2);
        let campaigns = vec![rust, eft];

        let mut settings = Settings::default();
        settings.disclaimer_accepted_version = 1;
        settings.idle_behavior = IdleBehavior::StayIdle;
        settings.twitch.priority = vec![PriorityItem::Game {
            game_id: String::new(),
            game_name: "  escape from tarkov ".to_string(),
        }];

        let picked = select_target_campaign(&campaigns, &settings, &settings.twitch, now);
        assert_eq!(picked.map(|c| c.id.as_str()), Some("eft-1"));
        assert!(game_names_match("Overwatch 2", "Overwatch"));
        assert!(!game_names_match("Grand Theft Auto VI (GTA)", "Call of Duty: Modern Warfare 4"));
    }

    #[test]
    fn ordered_target_campaigns_lists_all_priority_items_for_fallback() {
        let now = Utc::now();
        let eft = mk_campaign("eft-1", "491931", "Escape from Tarkov", true, 4);
        let ow_a = mk_campaign("ow-1", "515025", "Overwatch", true, 2);
        let ow_b = mk_campaign("ow-2", "515025", "Overwatch", true, 10);
        let rust = mk_campaign("rust-1", "263490", "Rust", true, 6);
        let campaigns = vec![rust, ow_b, eft, ow_a];

        let mut settings = Settings {
            disclaimer_accepted_version: 1,
            idle_behavior: IdleBehavior::StayIdle,
            ..Default::default()
        };
        settings.twitch.priority = vec![
            PriorityItem::Game {
                game_id: "491931".to_string(),
                game_name: "Escape from Tarkov".to_string(),
            },
            PriorityItem::Game {
                game_id: "515025".to_string(),
                game_name: "Overwatch".to_string(),
            },
        ];
        settings.twitch.farm_campaigns = vec!["rust-1".to_string()];

        let ordered: Vec<&str> = ordered_target_campaigns(&campaigns, &settings, &settings.twitch, now)
            .into_iter()
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(ordered, vec!["eft-1", "ow-1", "ow-2", "rust-1"]);
    }

    #[test]
    fn auto_claim_disabled_excludes_drops_that_need_no_watch_time() {
        let now = Utc::now();
        let mut claimable = mk_campaign("claimable", "g1", "Rust", true, 6);
        claimable.drops[0].current_minutes = claimable.drops[0].required_minutes;
        claimable.drops[0].claimable = true;
        let mut manual = mk_campaign("manual", "g2", "Overwatch", true, 6);
        manual.drops[0].current_minutes = manual.drops[0].required_minutes;
        manual.drops[0].needs_manual_claim = true;
        let incomplete = mk_campaign("incomplete", "g3", "VALORANT", true, 6);
        let campaigns = vec![claimable.clone(), manual.clone(), incomplete.clone()];

        let mut settings = Settings {
            auto_claim: false,
            disclaimer_accepted_version: 1,
            idle_behavior: IdleBehavior::StayIdle,
            ..Default::default()
        };
        settings.twitch.priority = vec![
            PriorityItem::Game {
                game_id: "g1".into(),
                game_name: "Rust".into(),
            },
            PriorityItem::Game {
                game_id: "g2".into(),
                game_name: "Overwatch".into(),
            },
            PriorityItem::Game {
                game_id: "g3".into(),
                game_name: "VALORANT".into(),
            },
        ];

        assert!(!is_campaign_active_window(
            &claimable,
            &settings,
            &settings.twitch,
            now
        ));
        assert!(!is_campaign_active_window(
            &manual,
            &settings,
            &settings.twitch,
            now
        ));
        assert_eq!(
            select_target_campaign(&campaigns, &settings, &settings.twitch, now)
                .map(|c| c.id.as_str()),
            Some("incomplete")
        );

        let mut mixed = claimable;
        mixed.drops.push(mk_drop("incomplete-drop", false));
        assert!(is_campaign_active_window(
            &mixed,
            &settings,
            &settings.twitch,
            now
        ));
    }

    #[test]
    fn auto_claim_enabled_keeps_claimable_campaign_selected() {
        let now = Utc::now();
        let mut claimable = mk_campaign("claimable", "g1", "Rust", true, 6);
        claimable.drops[0].current_minutes = claimable.drops[0].required_minutes;
        claimable.drops[0].claimable = true;
        let campaigns = vec![claimable];
        let mut settings = Settings {
            auto_claim: true,
            disclaimer_accepted_version: 1,
            idle_behavior: IdleBehavior::FarmAny,
            ..Default::default()
        };
        settings.twitch.priority = vec![PriorityItem::Game {
            game_id: "g1".into(),
            game_name: "Rust".into(),
        }];

        assert_eq!(
            select_target_campaign(&campaigns, &settings, &settings.twitch, now)
                .map(|c| c.id.as_str()),
            Some("claimable")
        );
    }

    #[test]
    fn ending_soon_boost_promotes_urgent_fifth_game_when_higher_games_have_time() {
        let now = Utc::now();
        let g1 = mk_campaign("g1", "1", "Overwatch", true, 120);
        let g2 = mk_campaign("g2", "2", "Rust", true, 96);
        let g3 = mk_campaign("g3", "3", "VALORANT", true, 72);
        let g4 = mk_campaign("g4", "4", "Fortnite", true, 48);
        let g5 = mk_campaign("g5", "5", "Escape from Tarkov", true, 4);
        let campaigns = vec![g1, g2, g3, g4, g5];

        let mut settings = Settings {
            disclaimer_accepted_version: 1,
            idle_behavior: IdleBehavior::StayIdle,
            ..Default::default()
        };
        settings.twitch.priority = vec![
            PriorityItem::Game { game_id: "1".into(), game_name: "Overwatch".into() },
            PriorityItem::Game { game_id: "2".into(), game_name: "Rust".into() },
            PriorityItem::Game { game_id: "3".into(), game_name: "VALORANT".into() },
            PriorityItem::Game { game_id: "4".into(), game_name: "Fortnite".into() },
            PriorityItem::Game { game_id: "5".into(), game_name: "Escape from Tarkov".into() },
        ];

        // Default OFF -> strict 1..5 priority order
        settings.twitch.prioritize_ending_soon = false;
        let strict: Vec<&str> = ordered_target_campaigns(&campaigns, &settings, &settings.twitch, now)
            .into_iter()
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(strict, vec!["g1", "g2", "g3", "g4", "g5"]);

        // Enabled -> #5 (ends in 4h) jumps ahead while #1..#4 have 2–5 days
        settings.twitch.prioritize_ending_soon = true;
        let boosted: Vec<&str> = ordered_target_campaigns(&campaigns, &settings, &settings.twitch, now)
            .into_iter()
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(boosted, vec!["g5", "g1", "g2", "g3", "g4"]);
    }

    #[test]
    fn ending_soon_boost_protects_tight_higher_priority_and_skips_unwinnable() {
        let now = Utc::now();
        // #1 ends in 3h, needs 150m (only 30m spare — cannot wait for #2!)
        let mut tight_top = mk_campaign("top", "1", "Overwatch", true, 3);
        tight_top.drops[0].required_minutes = 150;
        tight_top.drops[0].current_minutes = 0;

        // #2 ends in 2h, needs 90m
        let mut urgent_low = mk_campaign("low", "2", "Escape from Tarkov", true, 2);
        urgent_low.drops[0].required_minutes = 90;
        urgent_low.drops[0].current_minutes = 0;

        let mut settings = Settings {
            disclaimer_accepted_version: 1,
            idle_behavior: IdleBehavior::StayIdle,
            ..Default::default()
        };
        settings.twitch.prioritize_ending_soon = true;
        settings.twitch.priority = vec![
            PriorityItem::Game { game_id: "1".into(), game_name: "Overwatch".into() },
            PriorityItem::Game { game_id: "2".into(), game_name: "Escape from Tarkov".into() },
        ];

        // Top priority is tight (180m < 90m + 150m) -> top stays first
        let case1 = [tight_top.clone(), urgent_low.clone()];
        let order = ordered_target_campaigns(&case1, &settings, &settings.twitch, now);
        assert_eq!(order[0].id, "top");

        // If top has 10h left (600m >= 90m + 150m), low runs first so both finish
        let mut roomy_top = tight_top;
        roomy_top.ends_at = now + Duration::hours(10);
        let case2 = [roomy_top.clone(), urgent_low.clone()];
        let order2 = ordered_target_campaigns(&case2, &settings, &settings.twitch, now);
        assert_eq!(order2[0].id, "low");

        // If low ends in 15m but needs 90m (unwinnable), do not boost it
        let mut unwinnable = urgent_low;
        unwinnable.ends_at = now + Duration::minutes(15);
        let case3 = [roomy_top, unwinnable];
        let order3 = ordered_target_campaigns(&case3, &settings, &settings.twitch, now);
        assert_eq!(order3[0].id, "top");
    }
}
