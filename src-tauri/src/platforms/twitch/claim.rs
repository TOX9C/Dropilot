use serde_json::json;

use crate::net::client::AllowlistedClient;
use crate::platforms::twitch::gql::{execute_persisted_gql, ops};
use crate::platforms::{AppError, Drop};

pub async fn claim_drop(
    http: &AllowlistedClient,
    token: &str,
    drop: &Drop,
) -> Result<(), AppError> {
    let Some(ref instance_id) = drop.instance_id else {
        return Err(AppError::NotFound(format!(
            "Drop '{}' is missing dropInstanceID",
            drop.name
        )));
    };

    let vars = json!({
        "input": {
            "dropInstanceID": instance_id
        }
    });

    let payload = execute_persisted_gql(http, token, ops::CLAIM_DROP_REWARDS, vars).await?;

    let status = payload
        .pointer("/data/claimDropRewards/status")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match status {
        "ELIGIBLE_FOR_ALL" | "DROP_INSTANCE_ALREADY_CLAIMED" | "SUCCESS" => Ok(()),
        "" => Err(AppError::PlatformChanged(
            "Missing /data/claimDropRewards/status in claim response".to_string(),
        )),
        other if other.to_ascii_lowercase().contains("integrity") => Err(AppError::Blocked(
            "Twitch requires completing claim on the official inventory page".to_string(),
        )),
        other => Err(AppError::Network(format!("Unexpected Twitch claim status: {other}"))),
    }
}
