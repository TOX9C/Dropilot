use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::{Mutex as StdMutex, OnceLock};

use chrono::Utc;
use rand::Rng;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, ORIGIN, REFERER};
use reqwest::{Method, StatusCode};
use serde_json::{json, Value};

use crate::net::client::{parse_header_name_value, AllowlistedClient};
use crate::platforms::twitch::models::{parse_directory_channels, parse_drop};
use crate::platforms::{AppError, Drop, PlatformId};
use crate::remote_config::{resolve_gql_hash, resolve_twitch_client_id};
use crate::settings::DEFAULT_TWITCH_PROFILE_ID;

/// Official public Twitch web client ID observed on https://www.twitch.tv.
/// Dropilot never impersonates mobile, TV, or console clients.
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
/// If Twitch rotates a hash and returns `PersistedQueryNotFound`, Dropilot surfaces
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

static TWITCH_DEVICE_ID: OnceLock<String> = OnceLock::new();
static TWITCH_SESSION_ID: OnceLock<String> = OnceLock::new();
static TWITCH_INTEGRITY_CACHE: OnceLock<StdMutex<HashMap<String, TwitchIntegrityContext>>> =
    OnceLock::new();

const INTEGRITY_RENEW_MARGIN_MS: i64 = 5 * 60 * 1000;
const INTEGRITY_DEFAULT_TTL_MS: i64 = 2 * 60 * 60 * 1000;

/// Real browser integrity context captured from WebView2's official `www.twitch.tv` handshake.
/// Never stores OAuth credentials — only an FNV-1a fingerprint to invalidate when the token changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TwitchIntegrityContext {
    pub client_integrity: String,
    pub client_version: Option<String>,
    pub client_session_id: Option<String>,
    pub device_id: Option<String>,
    pub user_agent: Option<String>,
    pub expires_at_ms: i64,
    pub token_fingerprint: u64,
}

fn integrity_cache() -> &'static StdMutex<HashMap<String, TwitchIntegrityContext>> {
    TWITCH_INTEGRITY_CACHE.get_or_init(|| StdMutex::new(HashMap::new()))
}

pub fn token_fingerprint(token: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in token.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Returns a consistent 32-char hex device identifier fallback for Twitch web GQL and Spade calls.
pub fn twitch_device_id() -> &'static str {
    TWITCH_DEVICE_ID.get_or_init(|| {
        let mut rng = rand::thread_rng();
        let bytes: [u8; 16] = rng.gen();
        let mut out = String::with_capacity(32);
        for b in bytes {
            let _ = write!(&mut out, "{b:02x}");
        }
        out
    })
}

/// Returns a consistent 16-char hex session identifier for Twitch web GQL calls.
pub fn twitch_session_id() -> &'static str {
    TWITCH_SESSION_ID.get_or_init(|| {
        let mut rng = rand::thread_rng();
        let bytes: [u8; 8] = rng.gen();
        let mut out = String::with_capacity(16);
        for b in bytes {
            let _ = write!(&mut out, "{b:02x}");
        }
        out
    })
}

fn is_integrity_context_fresh(ctx: &TwitchIntegrityContext, auth_token: &str, now_ms: i64) -> bool {
    !ctx.client_integrity.trim().is_empty()
        && ctx.token_fingerprint == token_fingerprint(auth_token)
        && ctx.expires_at_ms - INTEGRITY_RENEW_MARGIN_MS > now_ms
}

pub fn cache_twitch_integrity_context(
    profile_id: &str,
    auth_token: &str,
    mut ctx: TwitchIntegrityContext,
) {
    if ctx.client_integrity.trim().is_empty() || auth_token.trim().is_empty() {
        return;
    }
    let now_ms = Utc::now().timestamp_millis();
    if ctx.expires_at_ms <= now_ms {
        ctx.expires_at_ms = now_ms + INTEGRITY_DEFAULT_TTL_MS;
    }
    ctx.token_fingerprint = token_fingerprint(auth_token);
    if let Ok(mut guard) = integrity_cache().lock() {
        guard.insert(profile_id.to_string(), ctx);
    }
}

pub fn get_twitch_integrity_context(
    profile_id: &str,
    auth_token: &str,
) -> Option<TwitchIntegrityContext> {
    if auth_token.trim().is_empty() {
        return None;
    }
    let now_ms = Utc::now().timestamp_millis();
    let guard = integrity_cache().lock().ok()?;
    guard
        .get(profile_id)
        .filter(|ctx| is_integrity_context_fresh(ctx, auth_token, now_ms))
        .cloned()
}

pub fn cached_twitch_user_agent(profile_id: &str) -> Option<String> {
    let guard = integrity_cache().lock().ok()?;
    guard
        .get(profile_id)
        .and_then(|ctx| ctx.user_agent.as_ref())
        .map(|ua| ua.trim().to_string())
        .filter(|ua| !ua.is_empty())
}

pub fn clear_twitch_integrity_context(profile_id: &str) {
    if let Ok(mut guard) = integrity_cache().lock() {
        guard.remove(profile_id);
    }
}

pub fn build_twitch_headers(token: Option<&str>) -> Result<HeaderMap, AppError> {
    build_twitch_headers_for_profile(Some(DEFAULT_TWITCH_PROFILE_ID), token)
}

pub fn build_twitch_headers_for_profile(
    profile_id: Option<&str>,
    token: Option<&str>,
) -> Result<HeaderMap, AppError> {
    let mut headers = HeaderMap::new();
    let client_id = resolve_twitch_client_id(TWITCH_WEB_CLIENT_ID);
    let (cid_name, cid_val) = parse_header_name_value("Client-ID", &client_id)?;
    headers.insert(cid_name, cid_val);
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(ORIGIN, HeaderValue::from_static("https://www.twitch.tv"));
    headers.insert(REFERER, HeaderValue::from_static("https://www.twitch.tv/"));

    let integrity_ctx = match (profile_id, token) {
        (Some(pid), Some(tok)) => get_twitch_integrity_context(pid, tok),
        _ => None,
    };

    let device_id = integrity_ctx
        .as_ref()
        .and_then(|ctx| ctx.device_id.as_deref())
        .filter(|d| !d.trim().is_empty())
        .unwrap_or(twitch_device_id());
    let (dev_name, dev_val) = parse_header_name_value("X-Device-Id", device_id)?;
    headers.insert(dev_name, dev_val);

    let session_id = integrity_ctx
        .as_ref()
        .and_then(|ctx| ctx.client_session_id.as_deref())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(twitch_session_id());
    let (sess_name, sess_val) = parse_header_name_value("Client-Session-Id", session_id)?;
    headers.insert(sess_name, sess_val);

    if let Some(ref ctx) = integrity_ctx {
        if let Some(ref ver) = ctx.client_version {
            if !ver.trim().is_empty() {
                if let Ok((ver_name, ver_val)) =
                    parse_header_name_value("Client-Version", ver.trim())
                {
                    headers.insert(ver_name, ver_val);
                }
            }
        }
        if let Ok((int_name, int_val)) =
            parse_header_name_value("Client-Integrity", ctx.client_integrity.trim())
        {
            headers.insert(int_name, int_val);
        }
    }

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
    let hash = resolve_gql_hash(op.operation_name, op.sha256_hash);
    json!({
        "operationName": op.operation_name,
        "variables": variables,
        "extensions": {
            "persistedQuery": {
                "version": 1,
                "sha256Hash": hash
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
    execute_persisted_gql_opt_for_profile(
        http,
        Some(DEFAULT_TWITCH_PROFILE_ID),
        Some(token),
        op,
        variables,
    )
    .await
}

pub async fn execute_persisted_gql_for_profile(
    http: &AllowlistedClient,
    profile_id: &str,
    token: &str,
    op: PersistedOp,
    variables: Value,
) -> Result<Value, AppError> {
    execute_persisted_gql_opt_for_profile(http, Some(profile_id), Some(token), op, variables).await
}

pub async fn execute_persisted_gql_opt(
    http: &AllowlistedClient,
    token: Option<&str>,
    op: PersistedOp,
    variables: Value,
) -> Result<Value, AppError> {
    execute_persisted_gql_opt_for_profile(
        http,
        Some(DEFAULT_TWITCH_PROFILE_ID),
        token,
        op,
        variables,
    )
    .await
}

pub async fn execute_persisted_gql_opt_for_profile(
    http: &AllowlistedClient,
    profile_id: Option<&str>,
    token: Option<&str>,
    op: PersistedOp,
    variables: Value,
) -> Result<Value, AppError> {
    let headers = build_twitch_headers_for_profile(profile_id, token)?;
    let used_integrity = headers.contains_key("client-integrity");
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
        if used_integrity && matches!(err, AppError::Blocked(_)) {
            if let Some(pid) = profile_id {
                clear_twitch_integrity_context(pid);
            }
        }
        return Err(err);
    }

    let parsed: Value = serde_json::from_str(&resp.body).map_err(|e| {
        AppError::PlatformChanged(format!("Invalid JSON from {}: {e}", op.operation_name))
    })?;

    if let Err(err) = check_gql_errors(op.operation_name, &parsed) {
        if used_integrity && matches!(err, AppError::Blocked(_)) {
            if let Some(pid) = profile_id {
                clear_twitch_integrity_context(pid);
            }
        }
        return Err(err);
    }

    Ok(parsed)
}

pub async fn execute_persisted_gql_batch(
    http: &AllowlistedClient,
    token: &str,
    op: PersistedOp,
    variables_list: &[Value],
) -> Result<Vec<Value>, AppError> {
    execute_persisted_gql_batch_opt_for_profile(
        http,
        Some(DEFAULT_TWITCH_PROFILE_ID),
        Some(token),
        op,
        variables_list,
    )
    .await
}

pub async fn execute_persisted_gql_batch_for_profile(
    http: &AllowlistedClient,
    profile_id: &str,
    token: &str,
    op: PersistedOp,
    variables_list: &[Value],
) -> Result<Vec<Value>, AppError> {
    execute_persisted_gql_batch_opt_for_profile(
        http,
        Some(profile_id),
        Some(token),
        op,
        variables_list,
    )
    .await
}

pub async fn execute_persisted_gql_batch_opt(
    http: &AllowlistedClient,
    token: Option<&str>,
    op: PersistedOp,
    variables_list: &[Value],
) -> Result<Vec<Value>, AppError> {
    execute_persisted_gql_batch_opt_for_profile(
        http,
        Some(DEFAULT_TWITCH_PROFILE_ID),
        token,
        op,
        variables_list,
    )
    .await
}

pub async fn execute_persisted_gql_batch_opt_for_profile(
    http: &AllowlistedClient,
    profile_id: Option<&str>,
    token: Option<&str>,
    op: PersistedOp,
    variables_list: &[Value],
) -> Result<Vec<Value>, AppError> {
    if variables_list.is_empty() {
        return Ok(Vec::new());
    }

    let headers = build_twitch_headers_for_profile(profile_id, token)?;
    let used_integrity = headers.contains_key("client-integrity");
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
        if used_integrity && matches!(err, AppError::Blocked(_)) {
            if let Some(pid) = profile_id {
                clear_twitch_integrity_context(pid);
            }
        }
        return Err(err);
    }

    let items: Vec<Value> = serde_json::from_str(&resp.body).map_err(|e| {
        AppError::PlatformChanged(format!(
            "Invalid JSON batch from {}: {e}",
            op.operation_name
        ))
    })?;

    for item in &items {
        if let Err(err) = check_gql_errors(op.operation_name, item) {
            if used_integrity && matches!(err, AppError::Blocked(_)) {
                if let Some(pid) = profile_id {
                    clear_twitch_integrity_context(pid);
                }
            }
            return Err(err);
        }
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

#[derive(Debug, Default)]
pub struct ChannelAvailableDrops {
    pub campaign_ids_by_channel: HashMap<String, Vec<String>>,
    pub drops_by_campaign: HashMap<String, Vec<Drop>>,
}

/// Queries `DropsHighlightService_AvailableDrops` in a single batch for the given Twitch
/// broadcaster IDs and returns both `channel_id -> active viewerDropCampaign IDs` and
/// `campaign_id -> hydrated timeBasedDrops` (rewards, images, required minutes).
///
/// Always queries unauthenticated first because Twitch's `viewerDropCampaigns` field is
/// viewer-scoped when an OAuth token is attached and can return empty arrays (`[]`) with
/// HTTP 200 when viewer-specific highlight filters or integrity checks suppress banners.
/// Falls back to a lightweight plain GraphQL batch query if the persisted hash returns empty
/// or fails, and optionally checks the authenticated view if public lookup finds no campaigns.
pub async fn fetch_channels_available_drops_data(
    http: &AllowlistedClient,
    token: Option<&str>,
    channel_ids: &[String],
) -> Result<ChannelAvailableDrops, AppError> {
    let valid_ids: Vec<String> = channel_ids
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
        .collect();

    if valid_ids.is_empty() {
        return Ok(ChannelAvailableDrops::default());
    }

    fn merge_items(
        out: &mut ChannelAvailableDrops,
        chunk: &[String],
        items: &[Value],
    ) {
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
            let entry = out.campaign_ids_by_channel.entry(cid).or_default();
            if let Some(arr) = chan
                .and_then(|c| c.get("viewerDropCampaigns"))
                .and_then(|v| v.as_array())
            {
                for camp in arr {
                    if let Some(id) = camp.get("id").and_then(|v| v.as_str()) {
                        if !entry.iter().any(|existing| existing == id) {
                            entry.push(id.to_string());
                        }
                        if !out.drops_by_campaign.contains_key(id) {
                            if let Some(drops_arr) =
                                camp.get("timeBasedDrops").and_then(|v| v.as_array())
                            {
                                let parsed_drops: Vec<Drop> = drops_arr
                                    .iter()
                                    .filter_map(parse_drop)
                                    .filter(|d| d.required_minutes > 0)
                                    .collect();
                                if !parsed_drops.is_empty() {
                                    out.drops_by_campaign.insert(id.to_string(), parsed_drops);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    async fn query_plain_batch(
        http: &AllowlistedClient,
        token: Option<&str>,
        chunk: &[String],
    ) -> Result<Vec<Value>, AppError> {
        const PLAIN_QUERY: &str =
            "query($id: ID!) { channel(id: $id) { id viewerDropCampaigns { id timeBasedDrops { id name requiredMinutesWatched benefitEdges { benefit { name imageAssetURL } } } } } }";
        let headers = build_twitch_headers(token)?;
        let batch_payload = Value::Array(
            chunk
                .iter()
                .map(|cid| {
                    json!({
                        "query": PLAIN_QUERY,
                        "variables": { "id": cid }
                    })
                })
                .collect(),
        );
        let resp = http
            .request_platform(
                PlatformId::Twitch,
                "ChannelAvailableDropsPlain",
                Method::POST,
                TWITCH_GQL_URL,
                headers,
                Some(&batch_payload),
            )
            .await?;
        if let Some(err) = map_gql_http_status("ChannelAvailableDropsPlain", resp.status, &resp.body)
        {
            return Err(err);
        }
        let items: Vec<Value> = serde_json::from_str(&resp.body).map_err(|e| {
            AppError::PlatformChanged(format!(
                "Invalid JSON batch from ChannelAvailableDropsPlain: {e}"
            ))
        })?;
        for item in &items {
            check_gql_errors("ChannelAvailableDropsPlain", item)?;
        }
        Ok(items)
    }

    let mut out = ChannelAvailableDrops {
        campaign_ids_by_channel: HashMap::with_capacity(valid_ids.len()),
        drops_by_campaign: HashMap::new(),
    };

    for chunk in valid_ids.chunks(25) {
        let vars: Vec<Value> = chunk
            .iter()
            .map(|cid| json!({ "channelID": cid }))
            .collect();

        let mut chunk_has_any = false;
        if let Ok(items) = execute_persisted_gql_batch_opt(
            http,
            None,
            ops::DROPS_HIGHLIGHT_AVAILABLE_DROPS,
            &vars,
        )
        .await
        {
            merge_items(&mut out, chunk, &items);
            chunk_has_any = chunk.iter().any(|cid| {
                out.campaign_ids_by_channel
                    .get(cid)
                    .map(|v| !v.is_empty())
                    .unwrap_or(false)
            });
        }

        if !chunk_has_any {
            if let Ok(items) = query_plain_batch(http, None, chunk).await {
                merge_items(&mut out, chunk, &items);
                chunk_has_any = chunk.iter().any(|cid| {
                    out.campaign_ids_by_channel
                        .get(cid)
                        .map(|v| !v.is_empty())
                        .unwrap_or(false)
                });
            }
        }

        if !chunk_has_any && token.is_some() {
            if let Ok(items) = execute_persisted_gql_batch_opt(
                http,
                token,
                ops::DROPS_HIGHLIGHT_AVAILABLE_DROPS,
                &vars,
            )
            .await
            {
                merge_items(&mut out, chunk, &items);
            }
        }
    }

    Ok(out)
}

pub async fn fetch_channels_available_drop_campaign_ids(
    http: &AllowlistedClient,
    token: Option<&str>,
    channel_ids: &[String],
) -> Result<HashMap<String, Vec<String>>, AppError> {
    let data = fetch_channels_available_drops_data(http, token, channel_ids).await?;
    Ok(data.campaign_ids_by_channel)
}

/// Fast pure-Rust reward preview hydration for any Twitch campaign by probing live
/// Drops-enabled channels in the campaign's game category via public `AvailableDrops`.
pub async fn fetch_campaign_drops_via_public_directory(
    http: &AllowlistedClient,
    game_id: &str,
    game_name: &str,
    campaign_id: &str,
) -> Option<Vec<Drop>> {
    const TWITCH_DROPS_TAG_ID: &str = "c2542d6d-cd10-4532-919b-3dcc9f54d5bf";

    let payload = fetch_game_directory_streams(
        http,
        None,
        game_id,
        game_name,
        Some(TWITCH_DROPS_TAG_ID),
    )
    .await
    .ok()
    .or(
        fetch_game_directory_streams(http, None, game_id, game_name, None)
            .await
            .ok(),
    )?;

    let channels = parse_directory_channels(&payload, game_name).ok()?;
    let ids: Vec<String> = channels.into_iter().take(12).map(|c| c.id).collect();
    if ids.is_empty() {
        return None;
    }

    let data = fetch_channels_available_drops_data(http, None, &ids)
        .await
        .ok()?;
    data.drops_by_campaign.get(campaign_id).cloned()
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

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{AUTHORIZATION, ORIGIN, REFERER};

    #[test]
    fn build_twitch_headers_includes_web_context_and_device_id() {
        let anon = build_twitch_headers(None).expect("anon headers");
        assert!(anon.contains_key("client-id"));
        assert_eq!(anon.get(ORIGIN).and_then(|v| v.to_str().ok()), Some("https://www.twitch.tv"));
        assert_eq!(anon.get(REFERER).and_then(|v| v.to_str().ok()), Some("https://www.twitch.tv/"));
        let dev = anon
            .get("x-device-id")
            .and_then(|v| v.to_str().ok())
            .expect("x-device-id present");
        assert_eq!(dev.len(), 32);
        assert!(dev.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(!anon.contains_key(AUTHORIZATION));

        let authed = build_twitch_headers(Some("tok123")).expect("authed headers");
        assert_eq!(
            authed.get(AUTHORIZATION).and_then(|v| v.to_str().ok()),
            Some("OAuth tok123")
        );
        assert_eq!(
            authed.get("x-device-id").and_then(|v| v.to_str().ok()),
            Some(dev)
        );
    }

    #[test]
    fn integrity_context_caches_by_profile_and_token_fingerprint() {
        let profile = "test-integrity-profile";
        let tok_a = "oauth-alpha-token";
        let tok_b = "oauth-beta-token";
        clear_twitch_integrity_context(profile);

        let now_ms = Utc::now().timestamp_millis();
        cache_twitch_integrity_context(
            profile,
            tok_a,
            TwitchIntegrityContext {
                client_integrity: "v4.public.fake-token".to_string(),
                client_version: Some("ver-123".to_string()),
                client_session_id: Some("sess-456".to_string()),
                device_id: Some("real-device-789".to_string()),
                user_agent: Some("Mozilla/5.0 TestEdge".to_string()),
                expires_at_ms: now_ms + 3_600_000,
                token_fingerprint: 0,
            },
        );

        let headers_a =
            build_twitch_headers_for_profile(Some(profile), Some(tok_a)).expect("headers a");
        assert_eq!(
            headers_a.get("client-integrity").and_then(|v| v.to_str().ok()),
            Some("v4.public.fake-token")
        );
        assert_eq!(
            headers_a.get("client-version").and_then(|v| v.to_str().ok()),
            Some("ver-123")
        );
        assert_eq!(
            headers_a.get("client-session-id").and_then(|v| v.to_str().ok()),
            Some("sess-456")
        );
        assert_eq!(
            headers_a.get("x-device-id").and_then(|v| v.to_str().ok()),
            Some("real-device-789")
        );
        assert_eq!(
            cached_twitch_user_agent(profile).as_deref(),
            Some("Mozilla/5.0 TestEdge")
        );

        // A different OAuth token on the same profile must not reuse the old token's integrity proof.
        let headers_b =
            build_twitch_headers_for_profile(Some(profile), Some(tok_b)).expect("headers b");
        assert!(!headers_b.contains_key("client-integrity"));

        clear_twitch_integrity_context(profile);
        assert!(get_twitch_integrity_context(profile, tok_a).is_none());
    }
}
