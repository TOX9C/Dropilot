use reqwest::Method;
use serde_json::json;
use tauri::AppHandle;

use crate::net::client::AllowlistedClient;
use crate::platforms::kick::api::{
    kick_request_json, KickRequest, KICK_CLAIM_URL, KICK_GAMIFICATION_CHALLENGES_URL,
};
use crate::platforms::kick::models::{is_kick_fallback_case_id, KICK_GAMIFICATION_PREFIX};
use crate::platforms::{AppError, Campaign, Drop};
use crate::state::BridgeWaiters;

pub async fn claim_drop(
    http: &AllowlistedClient,
    app: Option<&AppHandle>,
    waiters: &BridgeWaiters,
    token: &str,
    campaign: &Campaign,
    drop: &Drop,
) -> Result<(), AppError> {
    if is_kick_fallback_case_id(&campaign.id) {
        return Err(AppError::Blocked(format!(
            "Open Kick Rewards to open your {}",
            campaign.name
        )));
    }

    if let Some(challenge_id) = campaign.id.strip_prefix(KICK_GAMIFICATION_PREFIX) {
        let claim_url = format!("{KICK_GAMIFICATION_CHALLENGES_URL}/{challenge_id}/claim");
        let empty = json!({});
        let res = kick_request_json(
            http,
            app,
            waiters,
            token,
            KickRequest {
                op_name: "kick_claim_challenge",
                method: Method::POST,
                url: &claim_url,
                json_body: Some(&empty),
                not_found_is_resource: true,
                allow_bridge: false,
            },
        )
        .await;
        return match res {
            Ok(payload) => {
                if let Some(err_msg) = payload.get("error").and_then(|e| e.as_str()) {
                    Err(AppError::Blocked(err_msg.to_string()))
                } else {
                    Ok(())
                }
            }
            Err(_) => Err(AppError::Blocked(
                "Open Kick Rewards to open your Sunset Reward Case".to_string(),
            )),
        };
    }

    let body = json!({
        "reward_id": drop.id,
        "campaign_id": campaign.id
    });

    let payload = kick_request_json(
        http,
        app,
        waiters,
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

    Ok(())
}
