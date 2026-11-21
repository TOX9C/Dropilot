use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::platforms::{
    AppError, Campaign, CampaignStatus, Channel, ChannelRef, Drop, DropProgress, Game, PlatformId,
    Reward,
};

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
        box_art_url: game_obj
            .and_then(|g| g.get("boxArtURL"))
            .and_then(|s| s.as_str())
            .map(|url| url.replace("{width}", "285").replace("{height}", "380")),
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

    let has_drops = !drops.is_empty();
    let (linked, link_verified) = if is_dashboard_summary && !has_drops {
        // Twitch's bulk ViewerDropsDashboard returns placeholder self.isAccountConnected=false
        // on summary rows without resolving per-game links; real link state is resolved by
        // DropCampaignDetails and Inventory (dropCampaignsInProgress).
        (true, !requires_link)
    } else {
        let explicit = v
            .pointer("/self/isAccountConnected")
            .and_then(|b| b.as_bool());
        (explicit.unwrap_or(true), !requires_link || explicit.is_some())
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

fn parse_drop(d: &Value) -> Option<Drop> {
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

    let Some(in_progress) = inventory
        .get("dropCampaignsInProgress")
        .and_then(|v| v.as_array())
    else {
        return Ok(Vec::new());
    };

    let mut progress = Vec::new();
    for camp in in_progress {
        let Some(camp_id) = camp.get("id").and_then(|s| s.as_str()) else {
            continue;
        };
        let camp_linked = camp
            .pointer("/self/isAccountConnected")
            .and_then(|b| b.as_bool());
        let Some(drops) = camp.get("timeBasedDrops").and_then(|a| a.as_array()) else {
            continue;
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
            let claimable = !claimed && required > 0 && current >= required && instance_id.is_some();

            progress.push(DropProgress {
                campaign_id: camp_id.to_string(),
                drop_id: drop_id.to_string(),
                instance_id,
                current_minutes: current,
                claimable,
                claimed,
                linked: camp_linked,
                drop: parse_drop(drop),
            });
        }
    }
    Ok(progress)
}

pub fn parse_directory_channels(payload: &Value, expected_game: &str) -> Result<Vec<Channel>, AppError> {
    let edges = payload
        .pointer("/data/game/streams/edges")
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
        let Some(id) = broadcaster.get("id").and_then(|s| s.as_str()) else {
            continue;
        };
        let Some(login) = broadcaster.get("login").and_then(|s| s.as_str()) else {
            continue;
        };
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
        });
    }

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
    fn dashboard_summary_does_not_falsely_mark_unlinked() {
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
                        }
                    ]
                }
            }
        });

        let parsed = parse_campaigns_dashboard(&payload).expect("campaigns");
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].requires_link);
        assert!(parsed[0].linked);
        assert!(!parsed[0].link_verified);
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
                                "id": "camp-unlinked",
                                "self": { "isAccountConnected": false },
                                "timeBasedDrops": [
                                    {
                                        "id": "drop-1",
                                        "name": "Box",
                                        "requiredMinutesWatched": 30,
                                        "self": { "currentMinutesWatched": 5, "isClaimed": false }
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
        assert_eq!(parsed[0].linked, Some(false));
    }

    #[test]
    fn parse_directory_channels_returns_platform_changed_when_game_is_null() {
        let payload = json!({
            "data": {
                "game": null
            }
        });
        assert!(matches!(
            parse_directory_channels(&payload, "Escape from Tarkov"),
            Err(AppError::PlatformChanged(_))
        ));
    }
}
