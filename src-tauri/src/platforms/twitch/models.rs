use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::platforms::{
    AppError, Campaign, CampaignStatus, Channel, ChannelRef, ClaimedBenefit, Drop, DropProgress,
    Game, PlatformId, Reward,
};

pub const GAME_LINKED_ID_PREFIX: &str = "__game_linked_id__:";
pub const GAME_LINKED_NAME_PREFIX: &str = "__game_linked_name__:";
pub const CLAIMED_BENEFIT_PREFIX: &str = "__claimed_benefit__:";

const BOX_ART_WIDTH: &str = "450";
const BOX_ART_HEIGHT: &str = "600";

/// Upgrades Twitch `boxArtURL` strings—whether templated with `{width}x{height}` or
/// pre-baked with tiny dimensions like `-52x72.jpg` / `-120x160.jpg`—to crisp `450x600` (3:4).
pub fn normalize_box_art_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let replaced = trimmed
        .replace("{width}", BOX_ART_WIDTH)
        .replace("{height}", BOX_ART_HEIGHT)
        .replace("%7Bwidth%7D", BOX_ART_WIDTH)
        .replace("%7Bheight%7D", BOX_ART_HEIGHT)
        .replace("%7bwidth%7d", BOX_ART_WIDTH)
        .replace("%7bheight%7d", BOX_ART_HEIGHT);

    let split_idx = replaced.find(['?', '#']).unwrap_or(replaced.len());
    let (path, suffix) = replaced.split_at(split_idx);
    let lower_path = path.to_ascii_lowercase();
    if !(lower_path.contains("jtvnw.net") || lower_path.contains("ttv-boxart")) {
        return replaced;
    }

    let Some(dot_idx) = path.rfind('.') else {
        return replaced;
    };
    let ext = &path[dot_idx..];
    let lower_ext = ext.to_ascii_lowercase();
    if !matches!(lower_ext.as_str(), ".jpg" | ".jpeg" | ".png" | ".webp") {
        return replaced;
    }

    let stem = &path[..dot_idx];
    let Some(dash_idx) = stem.rfind('-') else {
        return replaced;
    };
    let dims = &stem[dash_idx + 1..];
    let Some(x_idx) = dims.find(['x', 'X']) else {
        return replaced;
    };
    let w_part = &dims[..x_idx];
    let h_part = &dims[x_idx + 1..];
    if w_part.is_empty()
        || h_part.is_empty()
        || !w_part.bytes().all(|b| b.is_ascii_digit())
        || !h_part.bytes().all(|b| b.is_ascii_digit())
    {
        return replaced;
    }

    format!(
        "{}-{BOX_ART_WIDTH}x{BOX_ART_HEIGHT}{ext}{suffix}",
        &stem[..dash_idx]
    )
}

pub fn parse_campaigns_dashboard(payload: &Value) -> Result<Vec<Campaign>, AppError> {
    let campaigns_val = payload
        .pointer("/data/currentUser/dropCampaigns")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            AppError::PlatformChanged("Missing /data/currentUser/dropCampaigns array".to_string())
        })?;

    let now = Utc::now();
    let mut out = Vec::with_capacity(campaigns_val.len());
    for item in campaigns_val {
        if let Some(parsed) = parse_single_campaign(item, true) {
            if parsed.status != CampaignStatus::Expired && parsed.ends_at > now {
                out.push(parsed);
            }
        }
    }
    Ok(out)
}

pub fn parse_campaign_details(payload: &Value) -> Result<Option<Campaign>, AppError> {
    let node = payload.pointer("/data/user/dropCampaign").or_else(|| payload.pointer("/data/dropCampaign"));
    let Some(val) = node else {
        return Err(AppError::PlatformChanged(
            "Missing dropCampaign in DropCampaignDetails".to_string(),
        ));
    };
    if val.is_null() {
        return Ok(None);
    }
    Ok(parse_single_campaign(val, false))
}

fn parse_single_campaign(v: &Value, is_dashboard_summary: bool) -> Option<Campaign> {
    if v.pointer("/allow/isEnabled").and_then(|b| b.as_bool()) == Some(false) {
        return None;
    }

    let id = v.get("id")?.as_str()?.to_string();
    let name = v.get("name")?.as_str().unwrap_or("Drop Campaign").to_string();

    let starts_at = v
        .get("startAt")
        .and_then(|s| s.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    let ends_at = v
        .get("endAt")
        .and_then(|s| s.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    let status_raw = v.get("status").and_then(|s| s.as_str()).unwrap_or("ACTIVE");
    let status = match status_raw {
        "ACTIVE" => CampaignStatus::Active,
        "UPCOMING" => CampaignStatus::Upcoming,
        _ => CampaignStatus::Expired,
    };

    let game_obj = v.get("game");
    let game = Game {
        id: game_obj
            .and_then(|g| g.get("id"))
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        name: game_obj
            .and_then(|g| g.get("displayName").or_else(|| g.get("name")))
            .and_then(|s| s.as_str())
            .unwrap_or("Unknown Game")
            .to_string(),
        slug: game_obj
            .and_then(|g| g.get("slug"))
            .and_then(|s| s.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        box_art_url: game_obj
            .and_then(|g| g.get("boxArtURL"))
            .and_then(|s| s.as_str())
            .map(normalize_box_art_url)
            .filter(|s| !s.is_empty()),
    };

    let drops = v
        .get("timeBasedDrops")
        .and_then(|arr| arr.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(parse_drop)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let link_url = v
        .get("accountLinkURL")
        .and_then(|s| s.as_str())
        .map(str::to_string);
    let requires_link = link_url.is_some();

    let _ = is_dashboard_summary;
    let has_active_progress = drops.iter().any(|d| d.current_minutes > 0 || d.claimed);
    let (linked, link_verified) = if !requires_link || has_active_progress {
        (true, true)
    } else {
        let explicit = v
            .pointer("/self/isAccountConnected")
            .and_then(|b| b.as_bool());
        (explicit.unwrap_or(false), true)
    };

    let allowed_channels = v
        .pointer("/allow/channels")
        .and_then(|arr| arr.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let cid = c.get("id")?.as_str()?.to_string();
                    let login = c.get("name")?.as_str()?.to_string();
                    let display_name = c
                        .get("displayName")
                        .and_then(|d| d.as_str())
                        .unwrap_or(&login)
                        .to_string();
                    Some(ChannelRef {
                        id: cid,
                        login,
                        display_name,
                    })
                })
                .collect::<Vec<_>>()
        });

    Some(Campaign {
        id,
        platform: PlatformId::Twitch,
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

pub(crate) fn parse_drop(d: &Value) -> Option<Drop> {
    let id = d.get("id")?.as_str()?.to_string();
    let name = d.get("name")?.as_str().unwrap_or("Drop").to_string();
    let required_minutes = d
        .get("requiredMinutesWatched")
        .and_then(|n| n.as_u64())
        .unwrap_or(0) as u32;
    let current_minutes = d
        .pointer("/self/currentMinutesWatched")
        .and_then(|n| n.as_u64())
        .unwrap_or(0) as u32;
    let claimed = d
        .pointer("/self/isClaimed")
        .and_then(|b| b.as_bool())
        .unwrap_or(false);
    let instance_id = d
        .pointer("/self/dropInstanceID")
        .and_then(|s| s.as_str())
        .map(str::to_string);

    let rewards = d
        .get("benefitEdges")
        .and_then(|arr| arr.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|edge| {
                    let b = edge.get("benefit")?;
                    Some(Reward {
                        id: b
                            .get("id")
                            .and_then(|s| s.as_str())
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(str::to_string),
                        name: b.get("name")?.as_str()?.to_string(),
                        image_url: b.get("imageAssetURL").and_then(|s| s.as_str()).map(str::to_string),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let claimable = !claimed && required_minutes > 0 && current_minutes >= required_minutes && instance_id.is_some();

    Some(Drop {
        id,
        instance_id,
        name,
        rewards,
        required_minutes,
        current_minutes,
        claimable,
        claimed,
        needs_manual_claim: false,
    })
}

pub fn parse_inventory_progress(payload: &Value) -> Result<Vec<DropProgress>, AppError> {
    let inventory = payload
        .pointer("/data/currentUser/inventory")
        .filter(|v| !v.is_null())
        .ok_or_else(|| {
            AppError::PlatformChanged("Missing /data/currentUser/inventory".to_string())
        })?;

    let now = Utc::now();
    let mut progress = Vec::new();
    if let Some(in_progress) = inventory
        .get("dropCampaignsInProgress")
        .and_then(|v| v.as_array())
    {
        for camp in in_progress {
            let Some(camp_id) = camp.get("id").and_then(|s| s.as_str()) else {
                continue;
            };
            let status_expired = camp
                .get("status")
                .and_then(|s| s.as_str())
                .map(|s| s.eq_ignore_ascii_case("EXPIRED") || s.eq_ignore_ascii_case("ENDED"))
                .unwrap_or(false);
            let ended_by_time = camp
                .get("endAt")
                .and_then(|s| s.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc) <= now)
                .unwrap_or(false);
            if status_expired || ended_by_time {
                continue;
            }
            let camp_name = camp
                .get("name")
                .and_then(|s| s.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let game_name = camp
                .pointer("/game/displayName")
                .or_else(|| camp.pointer("/game/name"))
                .and_then(|s| s.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let raw_linked = camp
                .pointer("/self/isAccountConnected")
                .and_then(|b| b.as_bool());
            let Some(drops) = camp.get("timeBasedDrops").and_then(|a| a.as_array()) else {
                continue;
            };
            let has_progress = drops.iter().any(|d| {
                d.pointer("/self/currentMinutesWatched")
                    .and_then(|n| n.as_u64())
                    .unwrap_or(0)
                    > 0
                    || d.pointer("/self/isClaimed")
                        .and_then(|b| b.as_bool())
                        .unwrap_or(false)
            });
            // Twitch's Inventory persisted query often returns placeholder isAccountConnected=false
            // even for linked publisher accounts (e.g. Battle.net / Tarkov) while actively recording
            // watch minutes. Only propagate positive confirmation from Inventory.
            let camp_linked = if raw_linked == Some(true) || has_progress {
                Some(true)
            } else {
                None
            };
            for drop in drops {
                let Some(drop_id) = drop.get("id").and_then(|s| s.as_str()) else {
                    continue;
                };
                let required = drop
                    .get("requiredMinutesWatched")
                    .and_then(|n| n.as_u64())
                    .unwrap_or(0) as u32;
                let current = drop
                    .pointer("/self/currentMinutesWatched")
                    .and_then(|n| n.as_u64())
                    .unwrap_or(0) as u32;
                let claimed = drop
                    .pointer("/self/isClaimed")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false);
                let instance_id = drop
                    .pointer("/self/dropInstanceID")
                    .and_then(|s| s.as_str())
                    .map(str::to_string);
                let claimable =
                    !claimed && required > 0 && current >= required && instance_id.is_some();

                progress.push(DropProgress {
                    campaign_id: camp_id.to_string(),
                    drop_id: drop_id.to_string(),
                    instance_id,
                    current_minutes: current,
                    progress_bps: None,
                    claimable,
                    claimed,
                    linked: camp_linked,
                    drop: parse_drop(drop),
                    campaign_name: camp_name.clone(),
                    game_name: game_name.clone(),
                    claimed_benefit: None,
                });
            }
        }
    }

    // Also inspect `gameEventDrops` (every drop previously claimed on this Twitch account):
    // 1. Any game with claimed history in Inventory is confirmed linked to the user's account.
    // 2. Claimed benefit entries let the engine detect campaigns whose drops are already 100%
    //    claimed even after Twitch removes them from `dropCampaignsInProgress`.
    if let Some(claimed_history) = inventory.get("gameEventDrops").and_then(|v| v.as_array()) {
        let mut seen_keys = std::collections::HashSet::new();
        for ev in claimed_history {
            let game = ev.get("game").filter(|g| g.is_object());
            let game_id = game
                .and_then(|g| g.get("id"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let game_name = game
                .and_then(|g| g.get("displayName").or_else(|| g.get("name")))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);

            if let Some(ref gid) = game_id {
                let key = format!("{GAME_LINKED_ID_PREFIX}{gid}");
                if seen_keys.insert(key.clone()) {
                    progress.push(DropProgress {
                        campaign_id: key,
                        drop_id: String::new(),
                        instance_id: None,
                        current_minutes: 0,
                        progress_bps: None,
                        claimable: false,
                        claimed: false,
                        linked: Some(true),
                        drop: None,
                        campaign_name: None,
                        game_name: None,
                        claimed_benefit: None,
                    });
                }
            }
            if let Some(ref gname) = game_name {
                let key = format!("{GAME_LINKED_NAME_PREFIX}{}", gname.to_ascii_lowercase());
                if seen_keys.insert(key.clone()) {
                    progress.push(DropProgress {
                        campaign_id: key,
                        drop_id: String::new(),
                        instance_id: None,
                        current_minutes: 0,
                        progress_bps: None,
                        claimable: false,
                        claimed: false,
                        linked: Some(true),
                        drop: None,
                        campaign_name: None,
                        game_name: None,
                        claimed_benefit: None,
                    });
                }
            }

            let total_count = ev.get("totalCount").and_then(|n| n.as_u64()).unwrap_or(1);
            if total_count == 0 {
                continue;
            }
            let benefit_id = ev
                .get("id")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let name = ev
                .get("name")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let last_awarded_at = ev
                .get("lastAwardedAt")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc));

            if let (Some(awarded_at), true) = (last_awarded_at, benefit_id.is_some() || name.is_some()) {
                let marker = benefit_id
                    .as_deref()
                    .or(name.as_deref())
                    .unwrap_or_default();
                progress.push(DropProgress {
                    campaign_id: format!("{CLAIMED_BENEFIT_PREFIX}{marker}"),
                    drop_id: String::new(),
                    instance_id: None,
                    current_minutes: 0,
                    progress_bps: None,
                    claimable: false,
                    claimed: true,
                    linked: Some(true),
                    drop: None,
                    campaign_name: None,
                    game_name: game_name.clone(),
                    claimed_benefit: Some(ClaimedBenefit {
                        benefit_id,
                        name: name.unwrap_or_default(),
                        game_id,
                        game_name,
                        last_awarded_at: awarded_at,
                    }),
                });
            }
        }
    }

    Ok(progress)
}

/// Parses Twitch's lightweight `DropCurrentSessionContext` GQL response.
/// Returns `(progress, completed_needs_claim)` so Dropilot can sync active watch progress
/// over pure-Rust HTTP (0 MB WebView2) and only invoke full Inventory when a drop completes.
pub fn parse_drop_current_session_progress(payload: &Value) -> Option<(DropProgress, bool)> {
    let session = payload
        .pointer("/data/currentUser/dropCurrentSession")
        .filter(|v| v.is_object())?;
    let drop_id = session
        .get("dropID")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?
        .to_string();
    let current = session
        .get("currentMinutesWatched")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let required = session
        .get("requiredMinutesWatched")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let completed = required > 0 && current >= required;
    Some((
        DropProgress {
            campaign_id: String::new(),
            drop_id,
            instance_id: None,
            current_minutes: current,
            progress_bps: None,
            claimable: false,
            claimed: false,
            linked: if current > 0 { Some(true) } else { None },
            drop: None,
            campaign_name: None,
            game_name: None,
            claimed_benefit: None,
        },
        completed,
    ))
}

pub fn parse_directory_channels(payload: &Value, expected_game: &str) -> Result<Vec<Channel>, AppError> {
    let Some(game_node) = payload.pointer("/data/game") else {
        return Err(AppError::PlatformChanged(
            "Missing /data/game in DirectoryPage_Game".to_string(),
        ));
    };

    if game_node.is_null() {
        tracing::info!(game = expected_game, "Twitch directory returned null game");
        return Ok(Vec::new());
    }

    let streams_node = game_node.get("streams").unwrap_or(&Value::Null);
    if streams_node.is_null() {
        tracing::info!(game = expected_game, "Twitch directory returned null streams");
        return Ok(Vec::new());
    }

    let edges = streams_node
        .get("edges")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            AppError::PlatformChanged("Missing /data/game/streams/edges in DirectoryPage_Game".to_string())
        })?;

    let mut channels = Vec::new();
    for edge in edges {
        let Some(node) = edge.get("node") else {
            continue;
        };
        let Some(broadcaster) = node.get("broadcaster") else {
            continue;
        };
        let Some(login) = broadcaster
            .get("login")
            .and_then(|s| s.as_str())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let id = broadcaster
            .get("id")
            .or_else(|| node.get("id"))
            .and_then(|s| s.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(login);
        let display_name = broadcaster
            .get("displayName")
            .and_then(|s| s.as_str())
            .unwrap_or(login)
            .to_string();
        let avatar_url = broadcaster
            .get("profileImageURL")
            .and_then(|s| s.as_str())
            .map(str::to_string);
        let viewer_count = node
            .get("viewersCount")
            .and_then(|n| n.as_u64())
            .unwrap_or(0);
        let is_followed = broadcaster
            .pointer("/self/follower")
            .map(|f| !f.is_null())
            .unwrap_or(false);

        let stream_id = node
            .get("id")
            .and_then(|s| s.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        channels.push(Channel {
            id: id.to_string(),
            login: login.to_string(),
            display_name,
            avatar_url,
            game_name: expected_game.to_string(),
            viewer_count,
            is_live: true,
            drops_enabled: true,
            is_followed,
            stream_id,
        });
    }

    tracing::info!(
        game = expected_game,
        raw_edges = edges.len(),
        parsed = channels.len(),
        "Parsed Twitch directory streams"
    );

    Ok(channels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use serde_json::json;

    #[test]
    fn handles_null_drop_campaigns_in_progress() {
        let payload = json!({
            "data": {
                "currentUser": {
                    "inventory": {
                        "dropCampaignsInProgress": null
                    }
                }
            }
        });
        let parsed = parse_inventory_progress(&payload).expect("should succeed cleanly");
        assert!(parsed.is_empty());

        let missing_inv = json!({ "data": { "currentUser": {} } });
        assert!(matches!(
            parse_inventory_progress(&missing_inv),
            Err(AppError::PlatformChanged(_))
        ));
    }

    #[test]
    fn filters_expired_and_disabled_campaigns() {
        let now = Utc::now();
        let future = (now + Duration::hours(10)).to_rfc3339();
        let past = (now - Duration::hours(2)).to_rfc3339();
        let payload = json!({
            "data": {
                "currentUser": {
                    "dropCampaigns": [
                        {
                            "id": "active-ok",
                            "name": "Active Drops",
                            "status": "ACTIVE",
                            "startAt": past,
                            "endAt": future,
                            "game": { "id": "1", "displayName": "Rust" }
                        },
                        {
                            "id": "expired-ts",
                            "name": "Ended Drops",
                            "status": "ACTIVE",
                            "startAt": past,
                            "endAt": past,
                            "game": { "id": "1", "displayName": "Rust" }
                        },
                        {
                            "id": "disabled-camp",
                            "name": "Disabled Drops",
                            "status": "ACTIVE",
                            "startAt": past,
                            "endAt": future,
                            "allow": { "isEnabled": false },
                            "game": { "id": "1", "displayName": "Rust" }
                        }
                    ]
                }
            }
        });

        let parsed = parse_campaigns_dashboard(&payload).expect("campaigns");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "active-ok");
    }

    #[test]
    fn hydrates_drop_object_inside_inventory_progress() {
        let payload = json!({
            "data": {
                "currentUser": {
                    "inventory": {
                        "dropCampaignsInProgress": [
                            {
                                "id": "camp-1",
                                "timeBasedDrops": [
                                    {
                                        "id": "drop-1",
                                        "name": "Mystery Crate",
                                        "requiredMinutesWatched": 60,
                                        "benefitEdges": [
                                            { "benefit": { "name": "Crate Skin", "imageAssetURL": "https://static-cdn.jtvnw.net/x.png" } }
                                        ],
                                        "self": {
                                            "currentMinutesWatched": 60,
                                            "isClaimed": false,
                                            "dropInstanceID": "inst-99"
                                        }
                                    }
                                ]
                            }
                        ]
                    }
                }
            }
        });

        let parsed = parse_inventory_progress(&payload).expect("progress");
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].claimable);
        let hydrated = parsed[0].drop.as_ref().expect("hydrated drop present");
        assert_eq!(hydrated.name, "Mystery Crate");
        assert_eq!(hydrated.required_minutes, 60);
        assert_eq!(hydrated.rewards.len(), 1);
    }

    #[test]
    fn dashboard_summary_reads_immediate_connection_state() {
        let now = Utc::now();
        let future = (now + Duration::hours(12)).to_rfc3339();
        let past = (now - Duration::hours(2)).to_rfc3339();
        let payload = json!({
            "data": {
                "currentUser": {
                    "dropCampaigns": [
                        {
                            "id": "eft-1",
                            "name": "Tarkov City day 6",
                            "status": "ACTIVE",
                            "startAt": past,
                            "endAt": future,
                            "accountLinkURL": "https://www.escapefromtarkov.com/profile",
                            "self": { "isAccountConnected": false },
                            "game": { "id": "491931", "displayName": "Escape from Tarkov" }
                        },
                        {
                            "id": "ow-1",
                            "name": "Overwatch Drops",
                            "status": "ACTIVE",
                            "startAt": past,
                            "endAt": future,
                            "accountLinkURL": "https://account.battle.net/connections",
                            "self": { "isAccountConnected": true },
                            "game": { "id": "515025", "displayName": "Overwatch" }
                        }
                    ]
                }
            }
        });

        let parsed = parse_campaigns_dashboard(&payload).expect("campaigns");
        assert_eq!(parsed.len(), 2);
        assert!(parsed[0].requires_link);
        assert!(!parsed[0].linked);
        assert!(parsed[0].link_verified);
        assert!(parsed[1].requires_link);
        assert!(parsed[1].linked);
        assert!(parsed[1].link_verified);
        assert_eq!(
            parsed[0].link_url.as_deref(),
            Some("https://www.escapefromtarkov.com/profile")
        );
    }

    #[test]
    fn campaign_details_preserves_real_account_link_status() {
        let now = Utc::now();
        let future = (now + Duration::hours(12)).to_rfc3339();
        let past = (now - Duration::hours(2)).to_rfc3339();
        let unlinked_details = json!({
            "data": {
                "user": {
                    "dropCampaign": {
                        "id": "eft-1",
                        "name": "Tarkov City day 6",
                        "status": "ACTIVE",
                        "startAt": past,
                        "endAt": future,
                        "accountLinkURL": "https://www.escapefromtarkov.com/profile",
                        "self": { "isAccountConnected": false },
                        "game": { "id": "491931", "displayName": "Escape from Tarkov" }
                    }
                }
            }
        });
        let unlinked = parse_campaign_details(&unlinked_details)
            .expect("details ok")
            .expect("campaign present");
        assert!(!unlinked.linked);
        assert!(unlinked.link_verified);

        let linked_details = json!({
            "data": {
                "user": {
                    "dropCampaign": {
                        "id": "eft-1",
                        "name": "Tarkov City day 6",
                        "status": "ACTIVE",
                        "startAt": past,
                        "endAt": future,
                        "accountLinkURL": "https://www.escapefromtarkov.com/profile",
                        "self": { "isAccountConnected": true },
                        "game": { "id": "491931", "displayName": "Escape from Tarkov" }
                    }
                }
            }
        });
        let linked = parse_campaign_details(&linked_details)
            .expect("details ok")
            .expect("campaign present");
        assert!(linked.linked);
        assert!(linked.link_verified);
    }

    #[test]
    fn inventory_progress_carries_campaign_linked_flag() {
        let payload = json!({
            "data": {
                "currentUser": {
                    "inventory": {
                        "dropCampaignsInProgress": [
                            {
                                "id": "camp-earning",
                                "self": { "isAccountConnected": false },
                                "timeBasedDrops": [
                                    {
                                        "id": "drop-1",
                                        "name": "Box",
                                        "requiredMinutesWatched": 30,
                                        "self": { "currentMinutesWatched": 5, "isClaimed": false }
                                    }
                                ]
                            },
                            {
                                "id": "camp-zero",
                                "self": { "isAccountConnected": false },
                                "timeBasedDrops": [
                                    {
                                        "id": "drop-2",
                                        "name": "Box 2",
                                        "requiredMinutesWatched": 30,
                                        "self": { "currentMinutesWatched": 0, "isClaimed": false }
                                    }
                                ]
                            }
                        ]
                    }
                }
            }
        });
        let parsed = parse_inventory_progress(&payload).expect("progress");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].linked, Some(true));
        assert_eq!(parsed[1].linked, None);
    }

    #[test]
    fn parse_directory_channels_handles_null_game_and_missing_broadcaster_id() {
        let null_game = json!({
            "data": {
                "game": null
            }
        });
        assert!(parse_directory_channels(&null_game, "Escape from Tarkov")
            .expect("null game yields empty list")
            .is_empty());

        let missing_game = json!({ "data": {} });
        assert!(matches!(
            parse_directory_channels(&missing_game, "Escape from Tarkov"),
            Err(AppError::PlatformChanged(_))
        ));

        let omitted_broadcaster_id = json!({
            "data": {
                "game": {
                    "streams": {
                        "edges": [
                            {
                                "node": {
                                    "id": "stream-9001",
                                    "viewersCount": 4200,
                                    "broadcaster": {
                                        "login": "pestily",
                                        "displayName": "Pestily"
                                    }
                                }
                            },
                            {
                                "node": {
                                    "broadcaster": {
                                        "login": "lvndmark"
                                    }
                                }
                            }
                        ]
                    }
                }
            }
        });

        let channels = parse_directory_channels(&omitted_broadcaster_id, "Escape from Tarkov")
            .expect("streams parsed without broadcaster.id");
        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0].id, "stream-9001");
        assert_eq!(channels[0].login, "pestily");
        assert_eq!(channels[0].viewer_count, 4200);
        assert_eq!(channels[1].id, "lvndmark");
        assert_eq!(channels[1].login, "lvndmark");
    }

    #[test]
    fn parses_drop_current_session_context_progress() {
        let payload = json!({
            "data": {
                "currentUser": {
                    "dropCurrentSession": {
                        "dropID": "drop-42",
                        "currentMinutesWatched": 18,
                        "requiredMinutesWatched": 60
                    }
                }
            }
        });
        let (prog, completed) =
            parse_drop_current_session_progress(&payload).expect("session progress");
        assert_eq!(prog.drop_id, "drop-42");
        assert_eq!(prog.current_minutes, 18);
        assert_eq!(prog.linked, Some(true));
        assert!(!completed);

        let done_payload = json!({
            "data": {
                "currentUser": {
                    "dropCurrentSession": {
                        "dropID": "drop-42",
                        "currentMinutesWatched": 60,
                        "requiredMinutesWatched": 60
                    }
                }
            }
        });
        let (_, done) =
            parse_drop_current_session_progress(&done_payload).expect("completed session");
        assert!(done);
    }

    #[test]
    fn normalizes_twitch_box_art_urls_to_high_res() {
        assert_eq!(
            normalize_box_art_url("https://static-cdn.jtvnw.net/ttv-boxart/515025-52x72.jpg"),
            "https://static-cdn.jtvnw.net/ttv-boxart/515025-450x600.jpg"
        );
        assert_eq!(
            normalize_box_art_url("https://static-cdn.jtvnw.net/ttv-boxart/515025-120x160.jpg"),
            "https://static-cdn.jtvnw.net/ttv-boxart/515025-450x600.jpg"
        );
        assert_eq!(
            normalize_box_art_url(
                "https://static-cdn.jtvnw.net/ttv-boxart/515025_IGDB-{width}x{height}.jpg"
            ),
            "https://static-cdn.jtvnw.net/ttv-boxart/515025_IGDB-450x600.jpg"
        );
        assert_eq!(
            normalize_box_art_url(
                "https://static-cdn.jtvnw.net/ttv-boxart/515025-%7Bwidth%7Dx%7Bheight%7D.webp?v=2"
            ),
            "https://static-cdn.jtvnw.net/ttv-boxart/515025-450x600.webp?v=2"
        );
        assert_eq!(
            normalize_box_art_url("https://static-cdn.jtvnw.net/ttv-boxart/515025.jpg"),
            "https://static-cdn.jtvnw.net/ttv-boxart/515025.jpg"
        );
        assert_eq!(
            normalize_box_art_url("https://example.com/other-52x72.jpg"),
            "https://example.com/other-52x72.jpg"
        );

        let now = Utc::now();
        let future = (now + Duration::hours(6)).to_rfc3339();
        let past = (now - Duration::hours(1)).to_rfc3339();
        let payload = json!({
            "data": {
                "currentUser": {
                    "dropCampaigns": [
                        {
                            "id": "pogo-1",
                            "name": "Pokémon GO Drops",
                            "status": "ACTIVE",
                            "startAt": past,
                            "endAt": future,
                            "game": {
                                "id": "490655",
                                "displayName": "Pokémon GO",
                                "boxArtURL": "https://static-cdn.jtvnw.net/ttv-boxart/490655-52x72.jpg"
                            }
                        }
                    ]
                }
            }
        });
        let campaigns = parse_campaigns_dashboard(&payload).expect("campaigns");
        assert_eq!(campaigns.len(), 1);
        assert_eq!(
            campaigns[0].game.box_art_url.as_deref(),
            Some("https://static-cdn.jtvnw.net/ttv-boxart/490655-450x600.jpg")
        );
    }

    #[test]
    fn parses_drop_benefit_id_and_inventory_game_event_drops() {
        let drop_val = json!({
            "id": "drop-ow-1",
            "name": "River Spray",
            "requiredMinutesWatched": 120,
            "benefitEdges": [
                {
                    "benefit": {
                        "id": "benefit-river-spray",
                        "name": "River Spray",
                        "imageAssetURL": "https://example.com/river.png"
                    }
                }
            ],
            "self": {
                "currentMinutesWatched": 0,
                "isClaimed": false
            }
        });
        let parsed_drop = parse_drop(&drop_val).expect("drop");
        assert_eq!(parsed_drop.rewards.len(), 1);
        assert_eq!(
            parsed_drop.rewards[0].id.as_deref(),
            Some("benefit-river-spray")
        );
        assert_eq!(parsed_drop.rewards[0].name, "River Spray");

        let awarded_at = "2026-11-04T18:30:00Z";
        let inv_payload = json!({
            "data": {
                "currentUser": {
                    "inventory": {
                        "dropCampaignsInProgress": [],
                        "gameEventDrops": [
                            {
                                "id": "benefit-river-spray",
                                "name": "River Spray",
                                "lastAwardedAt": awarded_at,
                                "totalCount": 1,
                                "game": {
                                    "id": "515025",
                                    "displayName": "Overwatch"
                                }
                            },
                            {
                                "id": "benefit-zero-count",
                                "name": "Unclaimed Zero",
                                "lastAwardedAt": awarded_at,
                                "totalCount": 0,
                                "game": {
                                    "id": "515025",
                                    "displayName": "Overwatch"
                                }
                            }
                        ]
                    }
                }
            }
        });

        let progress = parse_inventory_progress(&inv_payload).expect("inventory progress");
        assert!(progress
            .iter()
            .any(|p| p.campaign_id == format!("{GAME_LINKED_ID_PREFIX}515025")));
        assert!(progress
            .iter()
            .any(|p| p.campaign_id == format!("{GAME_LINKED_NAME_PREFIX}overwatch")));

        let claimed_entries: Vec<_> = progress
            .iter()
            .filter_map(|p| p.claimed_benefit.as_ref())
            .collect();
        assert_eq!(claimed_entries.len(), 1);
        assert_eq!(
            claimed_entries[0].benefit_id.as_deref(),
            Some("benefit-river-spray")
        );
        assert_eq!(claimed_entries[0].name, "River Spray");
        assert_eq!(claimed_entries[0].game_id.as_deref(), Some("515025"));
        assert_eq!(claimed_entries[0].game_name.as_deref(), Some("Overwatch"));
    }
}
