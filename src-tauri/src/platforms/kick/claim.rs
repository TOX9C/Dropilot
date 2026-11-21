use reqwest::Method;
use serde_json::json;
use tauri::AppHandle;

use crate::net::client::AllowlistedClient;
use crate::platforms::kick::api::{kick_request_json, KickRequest, KICK_CLAIM_URL};
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
