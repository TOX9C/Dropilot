use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};
use serde_json::Value;

use crate::miner::selector::game_names_match;
use crate::platforms::{
    Account, AppError, Campaign, CampaignStatus, Channel, ChannelRef, Drop, DropProgress, Game,
    PlatformId, Reward,
};

pub const KICK_GAMIFICATION_PREFIX: &str = "gamification:";
pub const KICK_SUNSET_CASE_CAMPAIGN_ID: &str = "kick-sunset-reward-case-gta6";
pub const KICK_DAILY_CASE_CAMPAIGN_ID: &str = "kick-case-fallback:daily-reward-case";
pub const KICK_WEEKLY_CASE_CAMPAIGN_ID: &str = "kick-case-fallback:weekly-reward-case";
pub const KICK_ANY_STREAM_GAME_ID: &str = "kick-any-stream";
const KICK_GTA_BANNER_URL: &str =
    "https://files.kick.com/images/subcategories/13155/banner/52ddce2b-7aee-4097-9cf4-7e8de52fa6b7";

pub fn is_kick_fallback_case_id(id: &str) -> bool {
    id == KICK_SUNSET_CASE_CAMPAIGN_ID || id.starts_with("kick-case-fallback:")
}

pub fn is_kick_any_stream_campaign(campaign: &Campaign) -> bool {
    is_kick_fallback_case_id(&campaign.id)
        || (campaign.id.starts_with(KICK_GAMIFICATION_PREFIX)
            && campaign.game.id == KICK_ANY_STREAM_GAME_ID)
        || campaign.game.id == KICK_ANY_STREAM_GAME_ID
}

pub fn parse_account(v: &Value) -> Result<Account, AppError> {
    let root = v.get("data").unwrap_or(v);
    let id = root
        .get("id")
        .and_then(|idv| {
            idv.as_u64()
                .map(|n| n.to_string())
                .or_else(|| idv.as_str().map(str::to_string))
        })
        .ok_or_else(|| AppError::PlatformChanged("Missing id in Kick user response".to_string()))?;

    let login = root
        .get("username")
        .or_else(|| root.get("slug"))
        .and_then(|s| s.as_str())
        .ok_or_else(|| AppError::PlatformChanged("Missing username in Kick user response".to_string()))?
        .to_string();

    let display_name = root
        .get("username")
        .and_then(|s| s.as_str())
        .unwrap_or(&login)
        .to_string();

    let avatar_url = root
        .get("profilepic")
        .or_else(|| root.get("profile_pic"))
        .and_then(|s| s.as_str())
        .map(str::to_string);

    Ok(Account {
        id,
        login,
        display_name,
        avatar_url,
    })
}

pub fn parse_campaigns(v: &Value) -> Result<Vec<Campaign>, AppError> {
    let arr = v
        .get("data")
        .and_then(|d| d.as_array())
        .or_else(|| v.as_array())
        .ok_or_else(|| {
            AppError::PlatformChanged("Expected campaigns array from Kick drops/campaigns".to_string())
        })?;

    let now = Utc::now();
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        if let Some(camp) = parse_single_campaign(item) {
            if camp.status != CampaignStatus::Expired && camp.ends_at > now {
                out.push(camp);
            }
        }
    }
    Ok(out)
}

fn parse_single_campaign(v: &Value) -> Option<Campaign> {
    let id = v
        .get("id")
        .and_then(|x| x.as_str().map(str::to_string).or_else(|| x.as_u64().map(|n| n.to_string())))?;
    let name = v
        .get("name")
        .and_then(|s| s.as_str())
        .unwrap_or("Kick Drop Campaign")
        .to_string();

    let starts_at = v
        .get("starts_at")
        .and_then(|s| s.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    let ends_at = v
        .get("ends_at")
        .and_then(|s| s.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    let status_raw = v.get("status").and_then(|s| s.as_str()).unwrap_or("active");
    let status = match status_raw.to_ascii_lowercase().as_str() {
        "active" => CampaignStatus::Active,
        "upcoming" => CampaignStatus::Upcoming,
        _ => CampaignStatus::Expired,
    };

    let cat = v.get("category");
    let game = Game {
        id: cat
            .and_then(|c| c.get("id"))
            .and_then(|x| x.as_u64().map(|n| n.to_string()).or_else(|| x.as_str().map(str::to_string)))
            .unwrap_or_default(),
        name: cat
            .and_then(|c| c.get("name"))
            .and_then(|s| s.as_str())
            .unwrap_or("Unknown Category")
            .to_string(),
        slug: cat
            .and_then(|c| c.get("slug"))
            .and_then(|s| s.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        box_art_url: cat
            .and_then(|c| {
                c.get("image_url")
                    .or_else(|| c.get("banner_url"))
                    .or_else(|| c.pointer("/banner/url"))
            })
            .and_then(|s| s.as_str())
            .filter(|s| !s.trim().is_empty())
            .map(normalize_kick_image_url),
    };

    let explicit_linked = v
        .get("is_connected")
        .or_else(|| v.get("connected"))
        .and_then(|b| b.as_bool());
    let connect_url = v
        .get("connect_url")
        .and_then(|s| s.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let fallback_url = v
        .get("url")
        .and_then(|s| s.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let requires_link = v
        .get("requires_connection")
        .and_then(|b| b.as_bool())
        .unwrap_or_else(|| connect_url.is_some());
    let link_url = connect_url.or(fallback_url);
    let linked = if requires_link {
        explicit_linked.unwrap_or(false)
    } else {
        explicit_linked.unwrap_or(true)
    };
    let link_verified = true;

    let allowed_channels = v.get("channels").and_then(|c| c.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|ch| {
                let cid = ch
                    .get("id")
                    .and_then(|x| x.as_u64().map(|n| n.to_string()).or_else(|| x.as_str().map(str::to_string)))?;
                let slug = ch.get("slug").or_else(|| ch.get("username"))?.as_str()?.to_string();
                let display = ch
                    .get("user")
                    .and_then(|u| u.get("username"))
                    .and_then(|s| s.as_str())
                    .unwrap_or(&slug)
                    .to_string();
                Some(ChannelRef {
                    id: cid,
                    login: slug,
                    display_name: display,
                })
            })
            .collect::<Vec<_>>()
    });

    let drops = v
        .get("rewards")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().filter_map(parse_reward_as_drop).collect::<Vec<_>>())
        .unwrap_or_default();

    Some(Campaign {
        id,
        platform: PlatformId::Kick,
        game,
        name,
        starts_at,
        ends_at,
        status,
        requires_link,
        linked,
        link_verified,
        link_url,
        allowed_channels,
        drops,
    })
}

fn parse_reward_as_drop(r: &Value) -> Option<Drop> {
    let id = r
        .get("id")
        .and_then(|x| x.as_str().map(str::to_string).or_else(|| x.as_u64().map(|n| n.to_string())))?;
    let name = r
        .get("name")
        .and_then(|s| s.as_str())
        .unwrap_or("Reward")
        .to_string();
    let image_url = r
        .get("image_url")
        .and_then(|s| s.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            if s.starts_with("https://") || s.starts_with("http://") {
                s.to_string()
            } else {
                format!("https://ext.cdn.kick.com/{}", s.trim_start_matches('/'))
            }
        });
    let required_minutes = r
        .get("required_units")
        .or_else(|| r.get("required_minutes"))
        .and_then(|n| n.as_u64())
        .unwrap_or(60) as u32;

    Some(Drop {
        id,
        instance_id: None,
        name: name.clone(),
        rewards: vec![Reward { name, image_url }],
        required_minutes,
        current_minutes: 0,
        claimable: false,
        claimed: false,
        needs_manual_claim: false,
    })
}

pub fn parse_progress(v: &Value) -> Result<Vec<DropProgress>, AppError> {
    let arr = v
        .get("data")
        .and_then(|d| d.as_array())
        .or_else(|| v.as_array())
        .ok_or_else(|| {
            AppError::PlatformChanged("Expected array from Kick drops/progress".to_string())
        })?;

    let mut out = Vec::new();
    for camp in arr {
        let Some(camp_id) = camp
            .get("id")
            .and_then(|x| x.as_str().map(str::to_string).or_else(|| x.as_u64().map(|n| n.to_string())))
        else {
            continue;
        };
        let Some(rewards) = camp.get("rewards").and_then(|r| r.as_array()) else {
            continue;
        };
        for r in rewards {
            let Some(drop_id) = r
                .get("id")
                .and_then(|x| x.as_str().map(str::to_string).or_else(|| x.as_u64().map(|n| n.to_string())))
            else {
                continue;
            };
            let required = r
                .get("required_units")
                .or_else(|| r.get("required_minutes"))
                .and_then(|n| n.as_u64())
                .unwrap_or(0) as u32;
            let progress_val = r.get("progress").and_then(|p| p.as_f64());
            let direct_units = r
                .get("current_units")
                .or_else(|| r.get("progress_units"))
                .or_else(|| r.get("watched_minutes"))
                .and_then(|n| n.as_f64());
            let (current_minutes, progress_bps) = if let Some(pv) = progress_val {
                if (0.0..=1.0).contains(&pv) && required > 1 {
                    (((pv * required as f64).round() as u32).min(required), None)
                } else if pv > 0.0 && pv <= 1.0 && required == 0 {
                    (0, Some((pv * 10_000.0).round().clamp(0.0, 10_000.0) as u32))
                } else if required > 0 {
                    ((pv.round() as u32).min(required), None)
                } else {
                    (pv.round().max(0.0) as u32, None)
                }
            } else if let Some(units) = direct_units {
                let mins = units.round().max(0.0) as u32;
                (if required > 0 { mins.min(required) } else { mins }, None)
            } else {
                (0, None)
            };
            let claimed = r.get("claimed").and_then(|b| b.as_bool()).unwrap_or(false);
            let claimable = !claimed && required > 0 && current_minutes >= required;
            let linked = if current_minutes > 0 || progress_bps.unwrap_or(0) > 0 || claimed {
                Some(true)
            } else {
                None
            };

            out.push(DropProgress {
                campaign_id: camp_id.clone(),
                drop_id,
                instance_id: None,
                current_minutes,
                progress_bps,
                claimable,
                claimed,
                linked,
                drop: None,
                campaign_name: None,
                game_name: None,
            });
        }
    }
    Ok(out)
}

pub fn parse_channel_v2(v: &Value, expected_game: &str) -> Result<Channel, AppError> {
    let id = v
        .get("id")
        .and_then(|x| x.as_u64().map(|n| n.to_string()).or_else(|| x.as_str().map(str::to_string)))
        .ok_or_else(|| AppError::PlatformChanged("Missing id in Kick channel v2".to_string()))?;

    let login = v
        .get("slug")
        .and_then(|s| s.as_str())
        .ok_or_else(|| AppError::PlatformChanged("Missing slug in Kick channel v2".to_string()))?
        .to_string();

    let display_name = v
        .pointer("/user/username")
        .and_then(|s| s.as_str())
        .unwrap_or(&login)
        .to_string();
    let avatar_url = v
        .pointer("/user/profile_pic")
        .and_then(|s| s.as_str())
        .map(str::to_string);

    let livestream = v.get("livestream").filter(|ls| !ls.is_null());
    let is_live = livestream.is_some();
    let viewer_count = livestream
        .and_then(|ls| ls.get("viewer_count"))
        .and_then(|n| n.as_u64())
        .unwrap_or(0);
    let game_name = livestream
        .and_then(|ls| ls.pointer("/categories/0/name"))
        .and_then(|s| s.as_str())
        .unwrap_or(expected_game)
        .to_string();
    let is_followed = v
        .get("following")
        .and_then(|b| b.as_bool())
        .unwrap_or(false);

    let stream_id = livestream.and_then(|ls| {
        ls.get("id")
            .and_then(|x| x.as_u64().map(|n| n.to_string()).or_else(|| x.as_str().map(str::to_string)))
    });

    Ok(Channel {
        id,
        login,
        display_name,
        avatar_url,
        game_name,
        viewer_count,
        is_live,
        drops_enabled: true,
        is_followed,
        stream_id,
    })
}

fn normalize_kick_image_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        trimmed.to_string()
    } else {
        format!("https://ext.cdn.kick.com/{}", trimmed.trim_start_matches('/'))
    }
}

fn collect_challenge_objects<'a>(v: &'a Value, out: &mut Vec<&'a Value>, depth: usize) {
    if depth > 6 {
        return;
    }
    match v {
        Value::Array(arr) => {
            for item in arr {
                if item.is_object() {
                    let has_identity = item.get("id").is_some()
                        || item.get("challenge_id").is_some()
                        || item.get("title").is_some()
                        || item.get("name").is_some()
                        || item.get("challenge").is_some();
                    if has_identity {
                        out.push(item);
                    } else {
                        collect_challenge_objects(item, out, depth + 1);
                    }
                }
            }
        }
        Value::Object(map) => {
            let mut found_container = false;
            for key in [
                "data",
                "challenges",
                "items",
                "featured",
                "daily",
                "weekly",
                "monthly",
                "event",
                "events",
                "cases",
                "reward_cases",
                "rewards",
                "active",
                "onboarding",
                "streak",
                "special",
                "groups",
                "list",
                "results",
            ] {
                if let Some(child) = map.get(key) {
                    found_container = true;
                    collect_challenge_objects(child, out, depth + 1);
                }
            }
            if !found_container
                && (map.contains_key("id") || map.contains_key("challenge_id"))
                && (map.contains_key("title") || map.contains_key("name"))
            {
                out.push(v);
            }
        }
        _ => {}
    }
}

fn parse_rfc3339_opt(val: Option<&Value>) -> Option<DateTime<Utc>> {
    val.and_then(|s| s.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

fn find_f64_with_key(objs: &[Option<&Value>], keys: &[&'static str]) -> Option<(&'static str, f64)> {
    for obj in objs.iter().flatten() {
        for &k in keys {
            if let Some(v) = obj.get(k) {
                if let Some(n) = v.as_f64() {
                    return Some((k, n));
                }
                if let Some(s) = v.as_str() {
                    if let Ok(n) = s.trim().parse::<f64>() {
                        return Some((k, n));
                    }
                }
            }
        }
    }
    None
}

fn first_of<'a>(parent: &'a Value, key: &str) -> Option<&'a Value> {
    parent.get(key).and_then(|c| {
        c.as_array()
            .and_then(|a| a.first())
            .or_else(|| c.is_object().then_some(c))
    })
}

fn extract_challenge_minutes(item: &Value) -> Option<(u32, u32)> {
    // Support wrapper objects like `{ challenge: { ... }, progress: { ... } }`.
    let inner = item.get("challenge").filter(|c| c.is_object()).unwrap_or(item);
    // Kick's `/api/v1/gamification/challenges` uses singular `condition: { threshold, progress }`
    // (in watch minutes), where remaining minutes = `max(0, condition.threshold - condition.progress)`.
    let cond = first_of(inner, "condition")
        .or_else(|| first_of(item, "condition"))
        .or_else(|| first_of(inner, "conditions"))
        .or_else(|| first_of(item, "conditions"))
        .or_else(|| first_of(inner, "requirements"))
        .or_else(|| first_of(item, "requirements"))
        .or_else(|| first_of(inner, "criteria"))
        .or_else(|| first_of(item, "criteria"))
        .or_else(|| first_of(inner, "objectives"))
        .or_else(|| first_of(item, "objectives"))
        .or_else(|| first_of(inner, "tasks"))
        .or_else(|| first_of(item, "tasks"))
        .or_else(|| first_of(inner, "rules"))
        .or_else(|| first_of(item, "rules"));
    let prog_obj = item
        .get("progress")
        .or_else(|| item.get("user_progress"))
        .or_else(|| inner.get("progress"))
        .or_else(|| inner.get("user_progress"))
        .filter(|p| p.is_object());

    let unit_hint = cond
        .and_then(|c| c.get("unit").or_else(|| c.get("metric")).or_else(|| c.get("type")))
        .or_else(|| prog_obj.and_then(|p| p.get("unit")))
        .or_else(|| inner.get("unit").or_else(|| item.get("unit")))
        .and_then(|u| u.as_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let unit_is_seconds = unit_hint.contains("sec");
    let unit_is_hours = unit_hint.contains("hour") || unit_hint == "hr" || unit_hint == "h";

    let key_to_minutes = |key: &str, raw: f64, default_secs: bool| -> u32 {
        if raw <= 0.0 {
            return 0;
        }
        if key.contains("second") || key.ends_with("_sec") || key.ends_with("_secs") || default_secs {
            (raw / 60.0).round() as u32
        } else if key.contains("hour") || unit_is_hours {
            (raw * 60.0).round() as u32
        } else if unit_is_seconds {
            (raw / 60.0).round() as u32
        } else {
            raw.round() as u32
        }
    };

    let sources = [cond, prog_obj, Some(inner), Some(item)];

    let (req_key, required_raw) = find_f64_with_key(
        &sources,
        &[
            "duration_seconds",
            "required_seconds",
            "target_seconds",
            "threshold_seconds",
            "watch_seconds",
            "duration_minutes",
            "required_minutes",
            "target_minutes",
            "watch_minutes",
            "required_units",
            "duration_hours",
            "required_hours",
            "target_hours",
            "threshold",
            "target_value",
            "target",
            "goal",
            "total",
            "value",
        ],
    )?;

    let key_explicit_secs =
        req_key.contains("second") || req_key.ends_with("_sec") || req_key.ends_with("_secs");
    let mut required = key_to_minutes(req_key, required_raw, false);
    let inferred_seconds = !key_explicit_secs
        && !req_key.contains("hour")
        && !unit_is_seconds
        && !unit_is_hours
        && required >= 3600
        && required % 60 == 0
        && required <= 86400;
    if inferred_seconds {
        required /= 60;
    }
    if required == 0 {
        return None;
    }

    // Check explicit remaining time first (e.g. Kick's "Watch 60 more minutes to claim").
    let rem_sources = [prog_obj, cond, Some(item), Some(inner)];
    if let Some((rem_k, rem_v)) = find_f64_with_key(
        &rem_sources,
        &[
            "remaining_minutes",
            "minutes_remaining",
            "remaining_seconds",
            "seconds_remaining",
            "remaining_units",
            "time_remaining",
        ],
    ) {
        let rem_mins = if rem_k.contains("second") || (inferred_seconds && rem_v > required as f64) {
            (rem_v / 60.0).ceil().max(0.0) as u32
        } else {
            rem_v.ceil().max(0.0) as u32
        };
        let cur = required.saturating_sub(rem_mins.min(required));
        return Some((cur, required));
    }

    // Check explicit percentage keys (0..=100) if present.
    if let Some((_, pct)) = find_f64_with_key(
        &rem_sources,
        &[
            "progress_percentage",
            "progress_percent",
            "completion_percentage",
            "percentage",
            "percent",
        ],
    ) {
        let cur = ((pct.clamp(0.0, 100.0) / 100.0) * required as f64).round() as u32;
        return Some((cur.min(required), required));
    }

    let current_pair = find_f64_with_key(
        &rem_sources,
        &[
            "watched_seconds",
            "watch_time_seconds",
            "seconds_watched",
            "elapsed_seconds",
            "current_seconds",
            "watched_minutes",
            "watch_time_minutes",
            "minutes_watched",
            "elapsed_minutes",
            "current_minutes",
            "current_units",
            "completed_units",
            "current_value",
            "completed_value",
            "current_progress",
            "current",
            "progress",
            "value",
        ],
    );

    let current = match current_pair {
        Some((cur_k, current_raw)) => {
            if current_raw <= 0.0 {
                0
            } else if cur_k == "progress" && current_raw > 0.0 && current_raw < 1.0 && required > 1 {
                // Only treat strictly fractional `0 < progress < 1` as a ratio; in Kick's
                // `condition: { threshold: 180, progress: 1 }`, integer `1` means 1 minute watched!
                ((current_raw * required as f64).round() as u32).min(required)
            } else {
                let default_secs =
                    inferred_seconds || (key_explicit_secs && !cur_k.contains("minute"));
                key_to_minutes(cur_k, current_raw, default_secs).min(required)
            }
        }
        None => 0,
    };

    Some((current, required))
}

pub fn parse_gamification_challenges(v: &Value, now: DateTime<Utc>) -> Vec<Campaign> {
    let mut raw_items = Vec::new();
    collect_challenge_objects(v, &mut raw_items, 0);

    let mut out = Vec::new();
    for item in raw_items {
        let inner = item.get("challenge").filter(|c| c.is_object()).unwrap_or(item);
        let Some(raw_id) = item
            .get("id")
            .or_else(|| item.get("challenge_id"))
            .or_else(|| inner.get("id"))
            .or_else(|| inner.get("challenge_id"))
            .or_else(|| inner.get("slug"))
            .and_then(|x| {
                x.as_str()
                    .map(str::to_string)
                    .or_else(|| x.as_u64().map(|n| n.to_string()))
            })
        else {
            continue;
        };

        let title = inner
            .get("title")
            .or_else(|| inner.get("name"))
            .or_else(|| item.get("title"))
            .or_else(|| item.get("name"))
            .and_then(|s| s.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("Kick Reward Case")
            .to_string();

        let description = inner
            .get("description")
            .or_else(|| item.get("description"))
            .and_then(|s| s.as_str())
            .unwrap_or_default();

        let status_raw = item
            .get("status")
            .or_else(|| inner.get("status"))
            .and_then(|s| s.as_str())
            .unwrap_or("in_progress")
            .to_ascii_lowercase();
        if matches!(status_raw.as_str(), "expired" | "ended" | "disabled" | "archived") {
            continue;
        }

        let claimed = status_raw == "claimed"
            || item
                .get("claimed")
                .or_else(|| inner.get("claimed"))
                .and_then(|b| b.as_bool())
                .unwrap_or(false);
        let explicit_claimable = status_raw == "claimable"
            || status_raw == "completed"
            || status_raw == "ready"
            || item
                .pointer("/actions/can_claim")
                .or_else(|| item.get("can_claim"))
                .or_else(|| inner.get("can_claim"))
                .and_then(|b| b.as_bool())
                .unwrap_or(false);

        let (current_minutes, required_minutes) = match extract_challenge_minutes(item) {
            Some(pair) => pair,
            None if explicit_claimable || claimed => (1, 1),
            None => {
                // Still include named Kick Reward Cases even if duration metadata uses an unseen key.
                let lower_meta = format!("{title} {description}").to_ascii_lowercase();
                if lower_meta.contains("case")
                    || lower_meta.contains("reward")
                    || lower_meta.contains("daily")
                    || lower_meta.contains("weekly")
                    || lower_meta.contains("sunset")
                    || lower_meta.contains("streak")
                {
                    (0, 60)
                } else {
                    continue;
                }
            }
        };

        let claimable = !claimed && (explicit_claimable || current_minutes >= required_minutes);

        let starts_at = parse_rfc3339_opt(
            item.pointer("/window/starts_at")
                .or_else(|| inner.pointer("/window/starts_at"))
                .or_else(|| item.pointer("/active_period/starts_at"))
                .or_else(|| inner.pointer("/active_period/starts_at"))
                .or_else(|| inner.get("starts_at"))
                .or_else(|| item.get("starts_at"))
                .or_else(|| inner.get("window_start"))
                .or_else(|| inner.get("created_at")),
        )
        .unwrap_or_else(|| now - ChronoDuration::hours(1));

        let ends_at = parse_rfc3339_opt(
            item.pointer("/window/ends_at")
                .or_else(|| inner.pointer("/window/ends_at"))
                .or_else(|| item.pointer("/active_period/ends_at"))
                .or_else(|| inner.pointer("/active_period/ends_at"))
                .or_else(|| item.get("ends_at"))
                .or_else(|| item.get("resets_at"))
                .or_else(|| item.get("expires_at"))
                .or_else(|| item.get("window_end"))
                .or_else(|| inner.get("ends_at"))
                .or_else(|| inner.get("resets_at"))
                .or_else(|| inner.get("expires_at"))
                .or_else(|| inner.get("window_end")),
        )
        .unwrap_or_else(|| now + ChronoDuration::hours(24));

        if ends_at <= now {
            continue;
        }

        let image_url = inner
            .get("image_url")
            .or_else(|| inner.get("reward_image_url"))
            .or_else(|| inner.get("thumbnail_url"))
            .or_else(|| inner.get("card_url"))
            .or_else(|| inner.get("icon_url"))
            .or_else(|| item.get("image_url"))
            .or_else(|| item.get("reward_image_url"))
            .or_else(|| item.get("thumbnail_url"))
            .or_else(|| item.get("card_url"))
            .or_else(|| item.get("icon_url"))
            .and_then(|s| s.as_str())
            .filter(|s| !s.trim().is_empty())
            .map(normalize_kick_image_url);

        let lower_meta = format!("{title} {description}").to_ascii_lowercase();
        let is_sunset = lower_meta.contains("sunset") || lower_meta.contains("gta");

        // Respect category-specific challenges when Kick specifies a category; otherwise treat as any-stream case.
        let cat_opt = inner.get("category").or_else(|| item.get("category")).filter(|c| c.is_object());
        let game = if let Some(cat) = cat_opt {
            Game {
                id: cat
                    .get("id")
                    .and_then(|x| x.as_u64().map(|n| n.to_string()).or_else(|| x.as_str().map(str::to_string)))
                    .unwrap_or_else(|| KICK_ANY_STREAM_GAME_ID.to_string()),
                name: cat
                    .get("name")
                    .and_then(|s| s.as_str())
                    .unwrap_or("Kick Reward Cases")
                    .to_string(),
                slug: cat
                    .get("slug")
                    .and_then(|s| s.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .map(str::to_string),
                box_art_url: image_url
                    .clone()
                    .or_else(|| cat.get("image_url").and_then(|s| s.as_str()).map(str::to_string))
                    .or_else(|| Some(KICK_GTA_BANNER_URL.to_string())),
            }
        } else {
            Game {
                id: KICK_ANY_STREAM_GAME_ID.to_string(),
                name: if is_sunset {
                    "Grand Theft Auto VI (GTA) · Sunset Case".to_string()
                } else {
                    "Kick Reward Cases".to_string()
                },
                slug: Some("grand-theft-auto-vi".to_string()),
                box_art_url: image_url
                    .clone()
                    .or_else(|| Some(KICK_GTA_BANNER_URL.to_string())),
            }
        };

        let campaign_id = format!("{KICK_GAMIFICATION_PREFIX}{raw_id}");
        if out.iter().any(|c: &Campaign| c.id == campaign_id) {
            continue;
        }

        // Parse multi-tier milestones/steps if present, otherwise single drop.
        let tier_drops: Vec<Drop> = inner
            .get("tiers")
            .or_else(|| inner.get("milestones"))
            .or_else(|| inner.get("steps"))
            .and_then(|t| t.as_array())
            .map(|tiers| {
                tiers
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, t)| {
                        let (t_cur, t_req) = extract_challenge_minutes(t)?;
                        let t_id = t
                            .get("id")
                            .and_then(|x| x.as_str().map(str::to_string).or_else(|| x.as_u64().map(|n| n.to_string())))
                            .unwrap_or_else(|| format!("{raw_id}-tier-{}", idx + 1));
                        let t_name = t
                            .get("title")
                            .or_else(|| t.get("name"))
                            .and_then(|s| s.as_str())
                            .map(str::to_string)
                            .unwrap_or_else(|| format!("{title} · Tier {}", idx + 1));
                        let t_img = t
                            .get("image_url")
                            .and_then(|s| s.as_str())
                            .filter(|s| !s.trim().is_empty())
                            .map(normalize_kick_image_url)
                            .or_else(|| image_url.clone());
                        let t_claimed = t.get("claimed").and_then(|b| b.as_bool()).unwrap_or(false);
                        let t_claimable = !t_claimed && t_cur >= t_req;
                        Some(Drop {
                            id: t_id,
                            instance_id: None,
                            name: t_name.clone(),
                            rewards: vec![Reward {
                                name: t_name,
                                image_url: t_img,
                            }],
                            required_minutes: t_req,
                            current_minutes: t_cur,
                            claimable: t_claimable,
                            claimed: t_claimed,
                            needs_manual_claim: false,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let drops = if !tier_drops.is_empty() {
            tier_drops
        } else {
            vec![Drop {
                id: raw_id,
                instance_id: None,
                name: title.clone(),
                rewards: vec![Reward {
                    name: if is_sunset && !title.to_ascii_lowercase().contains("gta") {
                        format!("{title} (GTA VI Box)")
                    } else {
                        title.clone()
                    },
                    image_url: image_url.or_else(|| Some(KICK_GTA_BANNER_URL.to_string())),
                }],
                required_minutes,
                current_minutes,
                claimable,
                claimed,
                needs_manual_claim: false,
            }]
        };

        tracing::info!(
            case = %title,
            current_minutes,
            required_minutes,
            remaining_minutes = required_minutes.saturating_sub(current_minutes),
            "Parsed Kick reward case progress"
        );

        out.push(Campaign {
            id: campaign_id,
            platform: PlatformId::Kick,
            game,
            name: title,
            starts_at,
            ends_at,
            status: CampaignStatus::Active,
            requires_link: false,
            linked: true,
            link_verified: true,
            link_url: Some("https://kick.com/drops/campaigns".to_string()),
            allowed_channels: None,
            drops,
        });
    }

    out
}

pub fn parse_gamification_progress(v: &Value, now: DateTime<Utc>) -> Vec<DropProgress> {
    parse_gamification_challenges(v, now)
        .into_iter()
        .flat_map(|camp| {
            let camp_id = camp.id;
            let camp_name = camp.name;
            let game_name = camp.game.name;
            camp.drops.into_iter().map(move |d| DropProgress {
                campaign_id: camp_id.clone(),
                drop_id: d.id.clone(),
                instance_id: None,
                current_minutes: d.current_minutes,
                progress_bps: None,
                claimable: d.claimable,
                claimed: d.claimed,
                linked: Some(true),
                drop: Some(d),
                campaign_name: Some(camp_name.clone()),
                game_name: Some(game_name.clone()),
            })
        })
        .collect()
}

/// Builds fallback Kick Reward Cases (Sunset Reward Case / GTA VI Box, Daily Reward Case,
/// and Weekly Watch Case) for any cases not already returned live by `/api/v1/gamification/challenges`.
pub fn build_kick_reward_cases_fallback(
    now: DateTime<Utc>,
    existing_campaigns: &[Campaign],
) -> Vec<Campaign> {
    let mut out = Vec::new();
    let has_matching = |needle: &str| -> bool {
        existing_campaigns.iter().any(|c| {
            c.id.starts_with(KICK_GAMIFICATION_PREFIX)
                && c.name.to_ascii_lowercase().contains(needle)
        })
    };

    let next_daily_reset = (now + ChronoDuration::days(1))
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|naive| Utc.from_utc_datetime(&naive))
        .unwrap_or_else(|| now + ChronoDuration::hours(24));

    // 1. Sunset Reward Case (GTA VI Box — 3h daily watch any stream, Nov 12 – Dec 7, 2026)
    if !has_matching("sunset") && !has_matching("gta") {
        if let Some(promo_end) = Utc.with_ymd_and_hms(2026, 12, 8, 0, 0, 0).single() {
            if now < promo_end {
                out.push(Campaign {
                    id: KICK_SUNSET_CASE_CAMPAIGN_ID.to_string(),
                    platform: PlatformId::Kick,
                    game: Game {
                        id: KICK_ANY_STREAM_GAME_ID.to_string(),
                        name: "Kick Reward Cases".to_string(),
                        slug: Some("grand-theft-auto-vi".to_string()),
                        box_art_url: Some(KICK_GTA_BANNER_URL.to_string()),
                    },
                    name: "Sunset Reward Case (GTA VI Box)".to_string(),
                    starts_at: now - ChronoDuration::hours(1),
                    ends_at: next_daily_reset.min(promo_end),
                    status: CampaignStatus::Active,
                    requires_link: false,
                    linked: true,
                    link_verified: true,
                    link_url: Some("https://kick.com/drops/campaigns".to_string()),
                    allowed_channels: None,
                    drops: vec![Drop {
                        id: "sunset-reward-case-daily".to_string(),
                        instance_id: None,
                        name: "Sunset Reward Case (GTA VI Box)".to_string(),
                        rewards: vec![Reward {
                            name: "Sunset Reward Case (GTA VI Box)".to_string(),
                            image_url: Some(KICK_GTA_BANNER_URL.to_string()),
                        }],
                        required_minutes: 180,
                        current_minutes: 0,
                        claimable: false,
                        claimed: false,
                        needs_manual_claim: false,
                    }],
                });
            }
        }
    }

    // 2. Daily Reward Case (watch any Kick stream 1h, resets daily)
    if !has_matching("daily") {
        out.push(Campaign {
            id: KICK_DAILY_CASE_CAMPAIGN_ID.to_string(),
            platform: PlatformId::Kick,
            game: Game {
                id: KICK_ANY_STREAM_GAME_ID.to_string(),
                name: "Kick Reward Cases".to_string(),
                slug: Some("grand-theft-auto-vi".to_string()),
                box_art_url: Some("https://ext.kick.com/drops/organization/kickdrops.png".to_string()),
            },
            name: "Daily Reward Case".to_string(),
            starts_at: now - ChronoDuration::hours(1),
            ends_at: next_daily_reset,
            status: CampaignStatus::Active,
            requires_link: false,
            linked: true,
            link_verified: true,
            link_url: Some("https://kick.com/drops/campaigns".to_string()),
            allowed_channels: None,
            drops: vec![Drop {
                id: "kick-daily-reward-case".to_string(),
                instance_id: None,
                name: "Daily Reward Case".to_string(),
                rewards: vec![Reward {
                    name: "Kick Daily Reward Case (1h)".to_string(),
                    image_url: Some("https://ext.kick.com/drops/organization/kickdrops.png".to_string()),
                }],
                required_minutes: 60,
                current_minutes: 0,
                claimable: false,
                claimed: false,
                needs_manual_claim: false,
            }],
        });
    }

    // 3. Weekly Reward Case (watch any Kick stream 5h, resets weekly)
    if !has_matching("weekly") {
        out.push(Campaign {
            id: KICK_WEEKLY_CASE_CAMPAIGN_ID.to_string(),
            platform: PlatformId::Kick,
            game: Game {
                id: KICK_ANY_STREAM_GAME_ID.to_string(),
                name: "Kick Reward Cases".to_string(),
                slug: Some("grand-theft-auto-vi".to_string()),
                box_art_url: Some("https://ext.kick.com/drops/organization/kickdrops.png".to_string()),
            },
            name: "Weekly Reward Case".to_string(),
            starts_at: now - ChronoDuration::hours(1),
            ends_at: now + ChronoDuration::days(7),
            status: CampaignStatus::Active,
            requires_link: false,
            linked: true,
            link_verified: true,
            link_url: Some("https://kick.com/drops/campaigns".to_string()),
            allowed_channels: None,
            drops: vec![Drop {
                id: "kick-weekly-reward-case".to_string(),
                instance_id: None,
                name: "Weekly Reward Case".to_string(),
                rewards: vec![Reward {
                    name: "Kick Weekly Reward Case (5h)".to_string(),
                    image_url: Some("https://ext.kick.com/drops/organization/kickdrops.png".to_string()),
                }],
                required_minutes: 300,
                current_minutes: 0,
                claimable: false,
                claimed: false,
                needs_manual_claim: false,
            }],
        });
    }

    out
}

pub fn parse_category_livestreams(
    v: &Value,
    expected_game: &str,
    expected_category_id: &str,
    expected_slug: Option<&str>,
    allow_any_category: bool,
) -> Result<Vec<Channel>, AppError> {
    let arr = v
        .get("data")
        .and_then(|d| d.as_array())
        .or_else(|| v.as_array())
        .ok_or_else(|| {
            AppError::PlatformChanged("Missing data array in Kick livestreams response".to_string())
        })?;

    let mut out = Vec::new();
    for item in arr {
        let ch_obj = item.get("channel").unwrap_or(item);
        let Some(id) = ch_obj
            .get("id")
            .and_then(|x| x.as_u64().map(|n| n.to_string()).or_else(|| x.as_str().map(str::to_string)))
        else {
            continue;
        };
        let Some(slug) = ch_obj.get("slug").and_then(|s| s.as_str()) else {
            continue;
        };

        if item.get("is_live").and_then(|b| b.as_bool()) == Some(false) {
            continue;
        }

        // Skip streams that explicitly opt out of drops on category-drop campaigns.
        if !allow_any_category {
            let explicit_drops = item
                .get("drops_enabled")
                .or_else(|| item.get("is_drops_enabled"))
                .or_else(|| ch_obj.get("drops_enabled"))
                .and_then(|b| b.as_bool());
            if explicit_drops == Some(false) {
                continue;
            }
        }

        let mut stream_game_name = expected_game.to_string();

        // Verify the stream's actual category matches the target game when category metadata is present.
        if let Some(cat_node) = item
            .pointer("/categories/0")
            .or_else(|| item.get("category"))
        {
            let cat_id = cat_node
                .get("id")
                .and_then(|x| x.as_u64().map(|n| n.to_string()).or_else(|| x.as_str().map(str::to_string)))
                .unwrap_or_default();
            let cat_slug = cat_node
                .get("slug")
                .and_then(|s| s.as_str())
                .unwrap_or_default();
            let cat_name = cat_node
                .get("name")
                .and_then(|s| s.as_str())
                .unwrap_or_default();

            if !cat_name.is_empty() && allow_any_category {
                stream_game_name = cat_name.to_string();
            }

            if !allow_any_category {
                let id_ok = !expected_category_id.is_empty()
                    && !cat_id.is_empty()
                    && cat_id == expected_category_id;
                let slug_ok = expected_slug
                    .filter(|s| !s.is_empty())
                    .map(|s| !cat_slug.is_empty() && cat_slug.eq_ignore_ascii_case(s))
                    .unwrap_or(false);
                let name_ok = !cat_name.is_empty() && game_names_match(cat_name, expected_game);

                if (!cat_id.is_empty() || !cat_slug.is_empty() || !cat_name.is_empty())
                    && !id_ok
                    && !slug_ok
                    && !name_ok
                {
                    continue;
                }
            }
        }

        let display_name = ch_obj
            .pointer("/user/username")
            .and_then(|s| s.as_str())
            .unwrap_or(slug)
            .to_string();
        let avatar_url = ch_obj
            .pointer("/user/profile_pic")
            .or_else(|| ch_obj.pointer("/user/profilepic"))
            .and_then(|s| s.as_str())
            .map(str::to_string);
        let viewer_count = item
            .get("viewer_count")
            .or_else(|| item.get("viewers"))
            .and_then(|n| n.as_u64())
            .unwrap_or(0);

        let stream_id = item.get("id").and_then(|x| {
            x.as_u64()
                .map(|n| n.to_string())
                .or_else(|| x.as_str().map(str::to_string))
        });

        out.push(Channel {
            id,
            login: slug.to_string(),
            display_name,
            avatar_url,
            game_name: stream_game_name,
            viewer_count,
            is_live: true,
            drops_enabled: true,
            is_followed: false,
            stream_id,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use serde_json::json;

    #[test]
    fn handles_empty_connect_url_and_normalizes_kick_reward_images() {
        let now = Utc::now();
        let future = (now + Duration::hours(24)).to_rfc3339();
        let past = (now - Duration::hours(2)).to_rfc3339();
        let payload = json!({
            "data": [
                {
                    "id": "mw4-camp",
                    "name": "Modern Warfare 4",
                    "status": "active",
                    "starts_at": past,
                    "ends_at": future,
                    "connect_url": "",
                    "url": "https://kick.com/category/call-of-duty-modern-warfare-4",
                    "category": {
                        "id": 14138,
                        "name": "Call of Duty: Modern Warfare 4",
                        "slug": "call-of-duty-modern-warfare-4"
                    },
                    "rewards": [
                        {
                            "id": "r1",
                            "name": "MW4 Ghost Badge",
                            "image_url": "drops/reward-image/ghost.png",
                            "required_units": 3000
                        }
                    ]
                },
                {
                    "id": "rotk-camp",
                    "name": "ROTK X KICK",
                    "status": "active",
                    "starts_at": past,
                    "ends_at": future,
                    "connect_url": "https://rotk.app/connections/kick",
                    "category": {
                        "id": 14321,
                        "name": "Return Of The King",
                        "slug": "return-of-the-king"
                    },
                    "rewards": []
                }
            ]
        });

        let camps = parse_campaigns(&payload).expect("kick campaigns");
        assert_eq!(camps.len(), 2);
        assert!(!camps[0].requires_link);
        assert!(camps[0].linked);
        assert_eq!(
            camps[0].game.slug.as_deref(),
            Some("call-of-duty-modern-warfare-4")
        );
        assert_eq!(
            camps[0].drops[0].rewards[0].image_url.as_deref(),
            Some("https://ext.cdn.kick.com/drops/reward-image/ghost.png")
        );

        assert!(camps[1].requires_link);
        assert!(!camps[1].linked);
        assert!(camps[1].link_verified);
    }

    #[test]
    fn filters_out_mismatched_category_streams_on_kick() {
        let payload = json!({
            "data": [
                {
                    "viewer_count": 122000,
                    "channel": { "id": 1, "slug": "coringa", "user": { "username": "Coringa" } },
                    "categories": [
                        { "id": 13155, "name": "Grand Theft Auto VI (GTA)", "slug": "grand-theft-auto-vi" }
                    ]
                },
                {
                    "viewer_count": 3400,
                    "channel": { "id": 2, "slug": "cod_pro", "user": { "username": "CoDPro" } },
                    "categories": [
                        { "id": 14138, "name": "Call of Duty: Modern Warfare 4", "slug": "call-of-duty-modern-warfare-4" }
                    ]
                }
            ]
        });

        let channels = parse_category_livestreams(
            &payload,
            "Call of Duty: Modern Warfare 4",
            "14138",
            Some("call-of-duty-modern-warfare-4"),
            false,
        )
        .expect("filtered kick streams");
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].login, "cod_pro");
    }

    #[test]
    fn parses_sunset_reward_case_gamification_challenge() {
        let now = Utc::now();
        let ends = (now + Duration::hours(13)).to_rfc3339();
        let payload = json!({
            "data": [
                {
                    "id": "sunset-daily-1",
                    "title": "Sunset Reward Case",
                    "description": "Watch any stream for 3 hours to unlock a case packed with free copies of GTA VI.",
                    "status": "in_progress",
                    "ends_at": ends,
                    "conditions": [{ "metric": "watch_time", "threshold": 180, "current_value": 38 }]
                }
            ]
        });

        let camps = parse_gamification_challenges(&payload, now);
        assert_eq!(camps.len(), 1);
        assert_eq!(camps[0].id, "gamification:sunset-daily-1");
        assert!(is_kick_any_stream_campaign(&camps[0]));
        assert_eq!(camps[0].drops[0].required_minutes, 180);
        assert_eq!(camps[0].drops[0].current_minutes, 38);
        assert!(!camps[0].requires_link);
        assert!(camps[0].linked);
    }

    #[test]
    fn parses_live_kick_singular_condition_and_window_schema() {
        let now = Utc::now();
        let win_start = (now - Duration::hours(13)).to_rfc3339();
        let win_end = (now + Duration::minutes(633)).to_rfc3339();
        let payload = json!({
            "data": [
                {
                    "id": "sunset-real-1",
                    "name": "Sunset Reward Case",
                    "description": "Watch any stream for 3 hours to unlock a case packed with free copies of GTA VI.",
                    "status": "in_progress",
                    "recurrence": "daily",
                    "condition": {
                        "threshold": 180,
                        "progress": 134
                    },
                    "window": {
                        "starts_at": win_start,
                        "ends_at": win_end
                    }
                },
                {
                    "id": "sunset-one-min",
                    "name": "Sunset Reward Case 1m",
                    "description": "Watch any stream for 3 hours.",
                    "status": "in_progress",
                    "condition": {
                        "threshold": 180,
                        "progress": 1
                    },
                    "window": {
                        "starts_at": win_start,
                        "ends_at": win_end
                    }
                }
            ]
        });

        let camps = parse_gamification_challenges(&payload, now);
        assert_eq!(camps.len(), 2);
        assert_eq!(camps[0].drops[0].required_minutes, 180);
        assert_eq!(camps[0].drops[0].current_minutes, 134);
        assert!((camps[0].ends_at - now).num_minutes() <= 634);
        assert!((camps[0].ends_at - now).num_minutes() >= 632);
        // Integer progress=1 must stay 1 minute, NOT be multiplied by threshold=180!
        assert_eq!(camps[1].drops[0].required_minutes, 180);
        assert_eq!(camps[1].drops[0].current_minutes, 1);
    }
}
