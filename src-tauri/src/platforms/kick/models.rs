use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::miner::selector::game_names_match;
use crate::platforms::{
    Account, AppError, Campaign, CampaignStatus, Channel, ChannelRef, Drop, DropProgress, Game,
    PlatformId, Reward,
};

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
            .map(str::to_string),
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
                .and_then(|n| n.as_u64())
                .unwrap_or(0) as u32;
            let progress_val = r
                .get("progress")
                .and_then(|p| p.as_f64())
                .unwrap_or(0.0);
            let current_minutes = if progress_val <= 1.0 && required > 1 {
                ((progress_val * required as f64).round() as u32).min(required)
            } else {
                (progress_val.round() as u32).min(required.max(1))
            };
            let claimed = r.get("claimed").and_then(|b| b.as_bool()).unwrap_or(false);
            let claimable = !claimed && required > 0 && current_minutes >= required;
            let linked = if current_minutes > 0 || claimed {
                Some(true)
            } else {
                None
            };

            out.push(DropProgress {
                campaign_id: camp_id.clone(),
                drop_id,
                instance_id: None,
                current_minutes,
                claimable,
                claimed,
                linked,
                drop: None,
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
    })
}

pub fn parse_category_livestreams(
    v: &Value,
    expected_game: &str,
    expected_category_id: &str,
    expected_slug: Option<&str>,
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

        out.push(Channel {
            id,
            login: slug.to_string(),
            display_name,
            avatar_url,
            game_name: expected_game.to_string(),
            viewer_count,
            is_live: true,
            drops_enabled: true,
            is_followed: false,
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
        )
        .expect("filtered kick streams");
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].login, "cod_pro");
    }
}
