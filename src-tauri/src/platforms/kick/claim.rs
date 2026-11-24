use reqwest::Method;
use serde_json::{json, Value};
use tauri::AppHandle;

use crate::net::client::AllowlistedClient;
use crate::platforms::kick::api::{
    kick_request_json_for_profile, KickRequest, KICK_CLAIM_URL, KICK_GAMIFICATION_CHALLENGES_URL,
};
use crate::platforms::kick::models::{
    is_kick_fallback_case_id, normalize_kick_image_url, KICK_GAMIFICATION_PREFIX,
};
use crate::platforms::{AppError, Campaign, ClaimResult, Drop};
use crate::settings::DEFAULT_KICK_PROFILE_ID;
use crate::state::BridgeWaiters;

const REWARD_CONTAINER_KEYS: &[&str] = &[
    "won_reward",
    "claimed_reward",
    "granted_reward",
    "user_reward",
    "reward_received",
    "claim_result",
    "outcome",
    "result",
    "reward",
    "prize",
    "item",
];

const REWARD_ARRAY_KEYS: &[&str] = &["rewards", "prizes", "items"];

const TITLE_KEYS: &[&str] = &[
    "name",
    "title",
    "label",
    "reward_name",
    "prize_name",
    "display_name",
    "item_name",
    "badge_name",
    "description",
];

const IMAGE_KEYS: &[&str] = &[
    "image_url",
    "icon_url",
    "thumbnail_url",
    "reward_image_url",
    "asset_url",
    "media_url",
];

const AMOUNT_KEYS: &[(&str, Option<&str>)] = &[
    ("amount", None),
    ("quantity", None),
    ("value", None),
    ("points", Some("Points")),
    ("kicks", Some("Kicks")),
    ("coins", Some("Coins")),
    ("count", None),
];

fn first_non_empty_str<'a>(obj: &'a Value, keys: &[&str]) -> Option<&'a str> {
    for &key in keys {
        if let Some(s) = obj.get(key).and_then(|v| v.as_str()) {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

fn capitalize_if_lowercase(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.chars().all(|c| !c.is_ascii_uppercase()) {
        let mut chars = trimmed.chars();
        if let Some(first) = chars.next() {
            return format!("{}{}", first.to_ascii_uppercase(), chars.as_str());
        }
    }
    trimmed.to_string()
}

fn parse_positive_amount(v: &Value) -> Option<String> {
    let num = if let Some(u) = v.as_u64() {
        if u == 0 {
            return None;
        }
        return Some(u.to_string());
    } else if let Some(i) = v.as_i64() {
        if i <= 0 {
            return None;
        }
        return Some(i.to_string());
    } else if let Some(f) = v.as_f64() {
        f
    } else {
        v.as_str()?.trim().parse::<f64>().ok()?
    };

    if !num.is_finite() || num <= 0.0 {
        return None;
    }
    if (num - num.round()).abs() < 1e-6 {
        Some(format!("{}", num.round() as u64))
    } else {
        Some(format!("{num}"))
    }
}

fn extract_image_url(obj: &Value) -> Option<String> {
    for &key in IMAGE_KEYS {
        if let Some(val) = obj.get(key) {
            if let Some(s) = val.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                return Some(normalize_kick_image_url(s));
            }
            if let Some(s) = val
                .get("url")
                .and_then(|u| u.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                return Some(normalize_kick_image_url(s));
            }
        }
    }
    None
}

pub(crate) fn extract_single_reward(v: &Value) -> ClaimResult {
    if let Some(s) = v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
        return ClaimResult {
            reward_name: Some(s.to_string()),
            reward_image_url: None,
        };
    }

    if !v.is_object() {
        return ClaimResult::default();
    }

    let reward_image_url = extract_image_url(v);
    let raw_title = first_non_empty_str(v, TITLE_KEYS);

    let mut amount_pair: Option<(String, Option<&'static str>)> = None;
    for &(key, implied_unit) in AMOUNT_KEYS {
        if let Some(val) = v.get(key) {
            if let Some(amt) = parse_positive_amount(val) {
                amount_pair = Some((amt, implied_unit));
                break;
            }
        }
    }

    let explicit_unit = first_non_empty_str(v, &["currency", "unit"]).or_else(|| {
        first_non_empty_str(v, &["reward_type", "type"]).filter(|t| {
            !matches!(
                t.to_ascii_lowercase().as_str(),
                "reward" | "item" | "currency" | "digital" | "virtual" | "unknown"
            )
        })
    });

    let mut base_name: Option<String> = match (raw_title, amount_pair, explicit_unit) {
        (Some(title), Some((amt, _)), Some(unit)) => {
            if title.contains(&amt) {
                Some(title.to_string())
            } else if title.eq_ignore_ascii_case(unit) {
                Some(format!("{amt} {}", capitalize_if_lowercase(unit)))
            } else {
                Some(format!("{amt} {title}"))
            }
        }
        (Some(title), Some((amt, implied_unit)), None) => {
            if title.contains(&amt) || (amt == "1" && implied_unit.is_none()) {
                Some(title.to_string())
            } else {
                Some(format!("{amt} {title}"))
            }
        }
        (None, Some((amt, implied_unit)), Some(unit)) => {
            let _ = implied_unit;
            Some(format!("{amt} {}", capitalize_if_lowercase(unit)))
        }
        (None, Some((amt, Some(implied_unit))), None) => Some(format!("{amt} {implied_unit}")),
        (Some(title), None, _) => Some(title.to_string()),
        _ => None,
    };

    if let Some(ref mut name) = base_name {
        if let Some(rarity) = first_non_empty_str(v, &["rarity"]) {
            let formatted_rarity = capitalize_if_lowercase(rarity);
            let tag = format!("({formatted_rarity})");
            if !name
                .to_ascii_lowercase()
                .contains(&tag.to_ascii_lowercase())
            {
                *name = format!("{name} {tag}");
            }
        }
    }

    ClaimResult {
        reward_name: base_name,
        reward_image_url,
    }
}

fn push_nested_candidates<'a>(obj: &'a Value, out: &mut Vec<&'a Value>) {
    for &key in REWARD_CONTAINER_KEYS {
        if let Some(child) = obj.get(key) {
            out.push(child);
        }
    }
    for &key in REWARD_ARRAY_KEYS {
        if let Some(first) = obj.get(key).and_then(|v| v.as_array()).and_then(|a| a.first()) {
            out.push(first);
        }
    }
}

pub fn extract_claim_reward(payload: &Value) -> ClaimResult {
    let mut candidates: Vec<&Value> = Vec::new();

    if let Some(data) = payload.get("data") {
        if let Some(first_data) = data.as_array().and_then(|a| a.first()) {
            push_nested_candidates(first_data, &mut candidates);
            candidates.push(first_data);
        } else if data.is_object() {
            push_nested_candidates(data, &mut candidates);
            if let Some(challenge) = data.get("challenge").filter(|c| c.is_object()) {
                push_nested_candidates(challenge, &mut candidates);
            }
        }
    }

    push_nested_candidates(payload, &mut candidates);

    if let Some(data) = payload.get("data").filter(|d| d.is_object()) {
        candidates.push(data);
    }
    candidates.push(payload);

    let mut result = ClaimResult::default();
    for cand in candidates {
        let extracted = extract_single_reward(cand);
        if result.reward_name.is_none() {
            result.reward_name = extracted.reward_name;
        }
        if result.reward_image_url.is_none() {
            result.reward_image_url = extracted.reward_image_url;
        }
        if result.reward_name.is_some() && result.reward_image_url.is_some() {
            break;
        }
    }

    result
}

fn log_claim_reward_summary(campaign_name: &str, reward: &ClaimResult, payload: &Value) {
    if reward.reward_name.is_some() || reward.reward_image_url.is_some() {
        tracing::info!(
            campaign = %campaign_name,
            reward = ?reward.reward_name,
            has_image = reward.reward_image_url.is_some(),
            "Extracted Kick claim reward"
        );
    } else {
        let keys: Vec<&str> = payload
            .as_object()
            .map(|m| m.keys().map(String::as_str).collect())
            .unwrap_or_default();
        tracing::info!(
            campaign = %campaign_name,
            response_keys = ?keys,
            "Kick claim response did not include reward details"
        );
    }
}

pub async fn claim_drop(
    http: &AllowlistedClient,
    app: Option<&AppHandle>,
    waiters: &BridgeWaiters,
    token: &str,
    campaign: &Campaign,
    drop: &Drop,
) -> Result<ClaimResult, AppError> {
    claim_drop_for_profile(
        http,
        app,
        waiters,
        DEFAULT_KICK_PROFILE_ID,
        token,
        campaign,
        drop,
    )
    .await
}

pub async fn claim_drop_for_profile(
    http: &AllowlistedClient,
    app: Option<&AppHandle>,
    waiters: &BridgeWaiters,
    profile_id: &str,
    token: &str,
    campaign: &Campaign,
    drop: &Drop,
) -> Result<ClaimResult, AppError> {
    if is_kick_fallback_case_id(&campaign.id) {
        return Err(AppError::Blocked(format!(
            "Open Kick Rewards to open your {}",
            campaign.name
        )));
    }

    if let Some(challenge_id) = campaign.id.strip_prefix(KICK_GAMIFICATION_PREFIX) {
        let claim_url = format!("{KICK_GAMIFICATION_CHALLENGES_URL}/{challenge_id}/claim");
        let empty = json!({});
        let res = kick_request_json_for_profile(
            http,
            app,
            waiters,
            profile_id,
            token,
            KickRequest {
                op_name: "kick_claim_challenge",
                method: Method::POST,
                url: &claim_url,
                json_body: Some(&empty),
                not_found_is_resource: true,
                allow_bridge: true,
            },
        )
        .await;
        return match res {
            Ok(payload) => {
                if let Some(err_msg) = payload.get("error").and_then(|e| e.as_str()) {
                    Err(AppError::Blocked(err_msg.to_string()))
                } else {
                    let reward = extract_claim_reward(&payload);
                    log_claim_reward_summary(&campaign.name, &reward, &payload);
                    Ok(reward)
                }
            }
            Err(_) => Err(AppError::Blocked(format!(
                "Open Kick Rewards to open your {}",
                campaign.name
            ))),
        };
    }

    let body = json!({
        "reward_id": drop.id,
        "campaign_id": campaign.id
    });

    let payload = kick_request_json_for_profile(
        http,
        app,
        waiters,
        profile_id,
        token,
        KickRequest {
            op_name: "kick_claim_drop",
            method: Method::POST,
            url: KICK_CLAIM_URL,
            json_body: Some(&body),
            not_found_is_resource: false,
            allow_bridge: true,
        },
    )
    .await?;

    if let Some(err_msg) = payload.get("error").and_then(|e| e.as_str()) {
        let lower = err_msg.to_ascii_lowercase();
        if lower.contains("captcha") || lower.contains("challenge") || lower.contains("verify") {
            return Err(AppError::Blocked(err_msg.to_string()));
        }
        return Err(AppError::Network(err_msg.to_string()));
    }

    let reward = extract_claim_reward(&payload);
    log_claim_reward_summary(&campaign.name, &reward, &payload);
    Ok(reward)
}

#[cfg(test)]
mod tests {
    use super::extract_claim_reward;
    use serde_json::json;

    #[test]
    fn extracts_named_item_and_rarity_from_nested_data_reward() {
        let payload = json!({
            "data": {
                "status": "claimed",
                "reward": {
                    "name": "Neon Badge",
                    "rarity": "epic",
                    "image_url": "rewards/neon-badge.png"
                }
            }
        });

        let res = extract_claim_reward(&payload);
        assert_eq!(res.reward_name.as_deref(), Some("Neon Badge (Epic)"));
        assert_eq!(
            res.reward_image_url.as_deref(),
            Some("https://ext.cdn.kick.com/rewards/neon-badge.png")
        );
    }

    #[test]
    fn formats_currency_and_amount_payloads() {
        let payload = json!({
            "won_reward": {
                "amount": 500,
                "currency": "Kicks",
                "icon_url": "https://files.kick.com/rewards/kicks.png"
            }
        });

        let res = extract_claim_reward(&payload);
        assert_eq!(res.reward_name.as_deref(), Some("500 Kicks"));
        assert_eq!(
            res.reward_image_url.as_deref(),
            Some("https://files.kick.com/rewards/kicks.png")
        );
    }

    #[test]
    fn extracts_from_rewards_array_and_handles_empty_payload() {
        let arr_payload = json!({
            "data": {
                "rewards": [
                    {
                        "title": "GTA VI Digital Key",
                        "thumbnail_url": "https://files.kick.com/gta6-key.png"
                    }
                ]
            }
        });
        let res = extract_claim_reward(&arr_payload);
        assert_eq!(res.reward_name.as_deref(), Some("GTA VI Digital Key"));
        assert_eq!(
            res.reward_image_url.as_deref(),
            Some("https://files.kick.com/gta6-key.png")
        );

        let empty_payload = json!({ "data": { "id": "case-1", "status": "claimed" } });
        let empty = extract_claim_reward(&empty_payload);
        assert!(empty.reward_name.is_none());
        assert!(empty.reward_image_url.is_none());
    }
}
