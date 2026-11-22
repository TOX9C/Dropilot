use std::collections::HashMap;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Method, StatusCode};
use serde_json::{json, Value};

use crate::net::client::{parse_header_name_value, AllowlistedClient};
use crate::platforms::{AppError, PlatformId};

/// Official public Twitch web client ID observed on https://www.twitch.tv.
/// DropPilot never impersonates mobile, TV, or console clients.
pub const TWITCH_WEB_CLIENT_ID: &str = "kimne78kx3ncx6brgo4mv6wki5h1ko";

pub const TWITCH_GQL_URL: &str = "https://gql.twitch.tv/gql";
pub const TWITCH_VALIDATE_URL: &str = "https://id.twitch.tv/oauth2/validate";
pub const TWITCH_USHER_HLS_BASE: &str = "https://usher.ttvnw.net/api/channel/hls";

#[derive(Debug, Clone, Copy)]
pub struct PersistedOp {
    pub operation_name: &'static str,
    pub sha256_hash: &'static str,
}

/// Persisted GQL operations used by Twitch's web client.
/// If Twitch rotates a hash and returns `PersistedQueryNotFound`, DropPilot surfaces
/// `AppError::PlatformChanged` rather than retrying aggressively. See `docs/ENDPOINTS.md`
/// for instructions on verifying hashes in browser DevTools.
pub mod ops {
    use super::PersistedOp;

    pub const VIEWER_DROPS_DASHBOARD: PersistedOp = PersistedOp {
        operation_name: "ViewerDropsDashboard",
        sha256_hash: "5a4da2ab3d5b47c9f9ce864e727b2cb346af1e3ea8b897fe8f704a97ff017619",
    };

    pub const INVENTORY: PersistedOp = PersistedOp {
        operation_name: "Inventory",
        sha256_hash: "d86775d0ef16a63a33ad52e80eaff963b2d5b72fada7c991504a57496e1d8e4b",
    };

    pub const DROP_CAMPAIGN_DETAILS: PersistedOp = PersistedOp {
        operation_name: "DropCampaignDetails",
        sha256_hash: "039277bf98f3130929262cc7c6efd9c141ca3749cb6dca442fc8ead9a53f77c1",
    };

    pub const DIRECTORY_PAGE_GAME: PersistedOp = PersistedOp {
        operation_name: "DirectoryPage_Game",
        sha256_hash: "c7c9d5aad09155c4161d2382092dc44610367f3536aac39019ec2582ae5065f9",
    };

    pub const USE_LIVE: PersistedOp = PersistedOp {
        operation_name: "UseLive",
        sha256_hash: "639d5f11bfb8bf3053b424d9ef650d04c4ebb7d94711d644afb08fe9a0fad5d9",
    };

    pub const PLAYBACK_ACCESS_TOKEN: PersistedOp = PersistedOp {
        operation_name: "PlaybackAccessToken",
        sha256_hash: "ed230aa1e33e07eebb8928504583da78a5173989fadfb1ac94be06a04f3cdbe9",
    };

    pub const PLAYBACK_ACCESS_TOKEN_TEMPLATE: PersistedOp = PersistedOp {
        operation_name: "PlaybackAccessToken_Template",
        sha256_hash: "0828119ded1c13477966434e15800ff57ddacf13ba1911c129dc2200705b0712",
    };

    pub const CLAIM_DROP_REWARDS: PersistedOp = PersistedOp {
        operation_name: "DropsPage_ClaimDropRewards",
        sha256_hash: "a455deea71bdc9015b78eb49f4acfbce8baa7ccbedd28e549bb025bd0f751930",
    };

    pub const USER_PROFILE: PersistedOp = PersistedOp {
        operation_name: "CoreActionsCurrentUser",
        sha256_hash: "6b5b63a013cf66a995d61f71a508ab5c8e4473350c5d4136f846ba65e8101e95",
    };

    pub const DROPS_HIGHLIGHT_AVAILABLE_DROPS: PersistedOp = PersistedOp {
        operation_name: "DropsHighlightService_AvailableDrops",
        sha256_hash: "782dad0f032942260171d2d80a654f88bdd0c5a9dddc392e9bc92218a0f42d20",
    };

    pub const DROP_CURRENT_SESSION_CONTEXT: PersistedOp = PersistedOp {
        operation_name: "DropCurrentSessionContext",
        sha256_hash: "4d06b702d25d652afb9ef835d2a550031f1cf762b193523a92166f40ea3d142b",
    };
}

pub fn build_twitch_headers(token: Option<&str>) -> Result<HeaderMap, AppError> {
    let mut headers = HeaderMap::new();
    let (cid_name, cid_val) = parse_header_name_value("Client-ID", TWITCH_WEB_CLIENT_ID)?;
    headers.insert(cid_name, cid_val);
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    if let Some(tok) = token {
        let auth_val = HeaderValue::from_str(&format!("OAuth {tok}"))
            .map_err(|e| AppError::Internal(format!("Invalid auth token header: {e}")))?;
        headers.insert(AUTHORIZATION, auth_val);
    }

    Ok(headers)
}

pub async fn validate_oauth_token(
    http: &AllowlistedClient,
    token: &str,
) -> Result<Value, AppError> {
    let mut headers = HeaderMap::new();
    let auth_val = HeaderValue::from_str(&format!("OAuth {token}"))
        .map_err(|e| AppError::Internal(format!("Invalid auth token header: {e}")))?;
    headers.insert(AUTHORIZATION, auth_val);

    let resp = http
        .request_platform(
            PlatformId::Twitch,
            "twitch_validate_token",
            Method::GET,
            TWITCH_VALIDATE_URL,
            headers,
            None,
        )
        .await?;

    if resp.status == StatusCode::UNAUTHORIZED {
        return Err(AppError::Auth("Twitch session token rejected (401)".to_string()));
    }
    if resp.status == StatusCode::NOT_FOUND {
        return Err(AppError::PlatformChanged(
            "Twitch OAuth validate endpoint returned 404".to_string(),
        ));
    }
    if !resp.status.is_success() {
        return Err(AppError::Network(format!(
            "Twitch validate returned HTTP {}",
            resp.status.as_u16()
        )));
    }

    serde_json::from_str(&resp.body)
        .map_err(|e| AppError::PlatformChanged(format!("Unexpected Twitch validate JSON: {e}")))
}

fn build_persisted_query_body(op: PersistedOp, variables: &Value) -> Value {
    json!({
        "operationName": op.operation_name,
        "variables": variables,
        "extensions": {
            "persistedQuery": {
                "version": 1,
                "sha256Hash": op.sha256_hash
            }
        }
    })
}

fn map_gql_http_status(op_name: &str, status: StatusCode, body: &str) -> Option<AppError> {
    if status == StatusCode::UNAUTHORIZED {
        return Some(AppError::Auth("Twitch GQL returned 401 Unauthorized".to_string()));
    }
    if status == StatusCode::FORBIDDEN {
        let lower = body.to_ascii_lowercase();
        if lower.contains("integrity") || lower.contains("challenge") || lower.contains("blocked") {
            return Some(AppError::Blocked("Twitch integrity check required".to_string()));
        }
        return Some(AppError::Auth("Twitch GQL returned 403 Forbidden".to_string()));
    }
    if status == StatusCode::NOT_FOUND {
        return Some(AppError::PlatformChanged(format!(
            "Twitch GQL returned 404 for {op_name}"
        )));
    }
    if !status.is_success() {
        return Some(AppError::Network(format!(
            "{op_name} returned HTTP {}",
            status.as_u16()
        )));
    }
    None
}

fn check_gql_errors(op_name: &str, item: &Value) -> Result<(), AppError> {
    if let Some(errors) = item.get("errors").and_then(|e| e.as_array()) {
        for err_obj in errors {
            let msg = err_obj
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown GQL error");
            let lower = msg.to_ascii_lowercase();

            if lower.contains("persistedquerynotfound") {
                return Err(AppError::PlatformChanged(format!(
                    "PersistedQueryNotFound for {op_name}"
                )));
            }
            if lower.contains("integrity")
                || lower.contains("challenge")
                || lower.contains("failed integrity")
            {
                return Err(AppError::Blocked(format!(
                    "Twitch integrity check triggered on {op_name}: {msg}"
                )));
            }
            if lower.contains("unauthenticated") || lower.contains("unauthorized") {
                return Err(AppError::Auth(msg.to_string()));
            }
        }
    }
    Ok(())
}

pub async fn execute_persisted_gql(
    http: &AllowlistedClient,
    token: &str,
    op: PersistedOp,
    variables: Value,
) -> Result<Value, AppError> {
    execute_persisted_gql_opt(http, Some(token), op, variables).await
}

pub async fn execute_persisted_gql_opt(
    http: &AllowlistedClient,
    token: Option<&str>,
    op: PersistedOp,
    variables: Value,
) -> Result<Value, AppError> {
    let headers = build_twitch_headers(token)?;
    let payload = build_persisted_query_body(op, &variables);

    let resp = http
        .request_platform(
            PlatformId::Twitch,
            op.operation_name,
            Method::POST,
            TWITCH_GQL_URL,
            headers,
            Some(&payload),
        )
        .await?;

    if let Some(err) = map_gql_http_status(op.operation_name, resp.status, &resp.body) {
        return Err(err);
    }

    let parsed: Value = serde_json::from_str(&resp.body).map_err(|e| {
        AppError::PlatformChanged(format!("Invalid JSON from {}: {e}", op.operation_name))
    })?;

    check_gql_errors(op.operation_name, &parsed)?;

    Ok(parsed)
}

pub async fn execute_persisted_gql_batch(
    http: &AllowlistedClient,
    token: &str,
    op: PersistedOp,
    variables_list: &[Value],
) -> Result<Vec<Value>, AppError> {
    execute_persisted_gql_batch_opt(http, Some(token), op, variables_list).await
}

pub async fn execute_persisted_gql_batch_opt(
    http: &AllowlistedClient,
    token: Option<&str>,
    op: PersistedOp,
    variables_list: &[Value],
) -> Result<Vec<Value>, AppError> {
    if variables_list.is_empty() {
        return Ok(Vec::new());
    }

    let headers = build_twitch_headers(token)?;
    let batch_payload = Value::Array(
        variables_list
            .iter()
            .map(|vars| build_persisted_query_body(op, vars))
            .collect(),
    );

    let resp = http
        .request_platform(
            PlatformId::Twitch,
            op.operation_name,
            Method::POST,
            TWITCH_GQL_URL,
            headers,
            Some(&batch_payload),
        )
        .await?;

    if let Some(err) = map_gql_http_status(op.operation_name, resp.status, &resp.body) {
        return Err(err);
    }

    let items: Vec<Value> = serde_json::from_str(&resp.body).map_err(|e| {
        AppError::PlatformChanged(format!(
            "Invalid JSON batch from {}: {e}",
            op.operation_name
        ))
    })?;

    for item in &items {
        check_gql_errors(op.operation_name, item)?;
    }

    Ok(items)
}

pub async fn fetch_channel_live_status(
    http: &AllowlistedClient,
    token: Option<&str>,
    channel_login: &str,
) -> Result<Value, AppError> {
    const LIVE_QUERY: &str = "query($login: String!) { user(login: $login) { id login displayName profileImageURL(width: 70) stream { id viewersCount game { id name displayName } } } }";

    let headers = build_twitch_headers(token)?;
    let payload = json!({
        "query": LIVE_QUERY,
        "variables": {
            "login": channel_login.trim(),
        }
    });

    let resp = http
        .request_platform(
            PlatformId::Twitch,
            "ChannelLiveStatus",
            Method::POST,
            TWITCH_GQL_URL,
            headers,
            Some(&payload),
        )
        .await?;

    if let Some(err) = map_gql_http_status("ChannelLiveStatus", resp.status, &resp.body) {
        return Err(err);
    }

    let parsed: Value = serde_json::from_str(&resp.body).map_err(|e| {
        AppError::PlatformChanged(format!("Invalid JSON from ChannelLiveStatus: {e}"))
    })?;

    check_gql_errors("ChannelLiveStatus", &parsed)?;

    Ok(parsed)
}

/// Queries `DropsHighlightService_AvailableDrops` in a single batch for the given Twitch
/// broadcaster IDs and returns a map of `channel_id -> active viewerDropCampaign IDs`.
pub async fn fetch_channels_available_drop_campaign_ids(
    http: &AllowlistedClient,
    token: Option<&str>,
    channel_ids: &[String],
) -> Result<HashMap<String, Vec<String>>, AppError> {
    let valid_ids: Vec<String> = channel_ids
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
        .collect();

    if valid_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut result: HashMap<String, Vec<String>> = HashMap::with_capacity(valid_ids.len());

    for chunk in valid_ids.chunks(25) {
        let vars: Vec<Value> = chunk
            .iter()
            .map(|cid| json!({ "channelID": cid }))
            .collect();

        let items = match execute_persisted_gql_batch_opt(
            http,
            token,
            ops::DROPS_HIGHLIGHT_AVAILABLE_DROPS,
            &vars,
        )
        .await
        {
            Ok(v) => v,
            Err(AppError::Blocked(_)) if token.is_some() => {
                execute_persisted_gql_batch_opt(
                    http,
                    None,
                    ops::DROPS_HIGHLIGHT_AVAILABLE_DROPS,
                    &vars,
                )
                .await?
            }
            Err(err) => return Err(err),
        };

        for (idx, item) in items.iter().enumerate() {
            let chan = item.pointer("/data/channel");
            let cid = chan
                .and_then(|c| c.get("id"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .or_else(|| chunk.get(idx).cloned());
            let Some(cid) = cid else {
                continue;
            };
            let camp_ids: Vec<String> = chan
                .and_then(|c| c.get("viewerDropCampaigns"))
                .and_then(|arr| arr.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|c| c.get("id").and_then(|v| v.as_str()).map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            result.insert(cid, camp_ids);
        }
    }

    Ok(result)
}

pub async fn fetch_game_directory_streams(
    http: &AllowlistedClient,
    token: Option<&str>,
    game_id: &str,
    game_name: &str,
    tag_id: Option<&str>,
) -> Result<Value, AppError> {
    const BY_ID_TAG_QUERY: &str = "query($id: ID!, $tags: [String!]) { game(id: $id) { id name displayName slug streams(first: 30, options: { sort: VIEWER_COUNT, tags: $tags }) { edges { node { id viewersCount broadcaster { id login displayName profileImageURL(width: 70) } } } } } }";
    const BY_NAME_TAG_QUERY: &str = "query($name: String!, $tags: [String!]) { game(name: $name) { id name displayName slug streams(first: 30, options: { sort: VIEWER_COUNT, tags: $tags }) { edges { node { id viewersCount broadcaster { id login displayName profileImageURL(width: 70) } } } } } }";
    const BY_ID_ALL_QUERY: &str = "query($id: ID!) { game(id: $id) { id name displayName slug streams(first: 30, options: { sort: VIEWER_COUNT }) { edges { node { id viewersCount broadcaster { id login displayName profileImageURL(width: 70) } } } } } }";
    const BY_NAME_ALL_QUERY: &str = "query($name: String!) { game(name: $name) { id name displayName slug streams(first: 30, options: { sort: VIEWER_COUNT }) { edges { node { id viewersCount broadcaster { id login displayName profileImageURL(width: 70) } } } } } }";

    let headers = build_twitch_headers(token)?;
    let has_id = !game_id.trim().is_empty();
    let payload = match (has_id, tag_id) {
        (true, Some(tag)) => json!({
            "query": BY_ID_TAG_QUERY,
            "variables": { "id": game_id.trim(), "tags": [tag] }
        }),
        (false, Some(tag)) => json!({
            "query": BY_NAME_TAG_QUERY,
            "variables": { "name": game_name.trim(), "tags": [tag] }
        }),
        (true, None) => json!({
            "query": BY_ID_ALL_QUERY,
            "variables": { "id": game_id.trim() }
        }),
        (false, None) => json!({
            "query": BY_NAME_ALL_QUERY,
            "variables": { "name": game_name.trim() }
        }),
    };

    let resp = http
        .request_platform(
            PlatformId::Twitch,
            "GameDirectoryById",
            Method::POST,
            TWITCH_GQL_URL,
            headers,
            Some(&payload),
        )
        .await?;

    if let Some(err) = map_gql_http_status("GameDirectoryById", resp.status, &resp.body) {
        return Err(err);
    }

    let parsed: Value = serde_json::from_str(&resp.body).map_err(|e| {
        AppError::PlatformChanged(format!("Invalid JSON from GameDirectoryById: {e}"))
    })?;

    check_gql_errors("GameDirectoryById", &parsed)?;

    Ok(parsed)
}
