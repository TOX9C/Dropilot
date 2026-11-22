use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, COOKIE};
use reqwest::{Method, StatusCode};
use serde_json::Value;
use tauri::AppHandle;

use crate::auth::login_window::kick_webview_fetch_with_waiters;
use crate::net::client::{parse_header_name_value, AllowlistedClient};
use crate::platforms::{AppError, PlatformId};
use crate::state::BridgeWaiters;

pub const KICK_USER_URL: &str = "https://kick.com/api/v1/user";
pub const KICK_CAMPAIGNS_URL: &str = "https://web.kick.com/api/v1/drops/campaigns";
pub const KICK_PROGRESS_URL: &str = "https://web.kick.com/api/v1/drops/progress";
pub const KICK_CLAIM_URL: &str = "https://web.kick.com/api/v1/drops/claim";
pub const KICK_GAMIFICATION_CHALLENGES_URL: &str =
    "https://web.kick.com/api/v1/gamification/challenges";
pub const KICK_LIVESTREAMS_BASE: &str = "https://kick.com/stream/livestreams/en";
pub const KICK_CHANNEL_V2_BASE: &str = "https://kick.com/api/v2/channels";
pub const KICK_VIEWER_TOKEN_URL: &str = "https://websockets.kick.com/viewer/v1/token";
pub const KICK_VIEWER_WS_URL: &str = "wss://websockets.kick.com/viewer/v1/connect";
/// Kick's official public web client token used by kick.com's own player for viewer WebSocket auth.
pub const KICK_PUBLIC_CLIENT_TOKEN: &str =
    "e1393935a959b4020a4491574f6490129f678acdaa92760471263db43487f823";

pub struct KickRequest<'a> {
    pub op_name: &'a str,
    pub method: Method,
    pub url: &'a str,
    pub json_body: Option<&'a Value>,
    pub not_found_is_resource: bool,
    pub allow_bridge: bool,
}

pub fn normalize_kick_bearer_token(raw: &str) -> String {
    raw.replace("%7C", "|").replace("%7c", "|")
}

pub fn build_kick_headers(token: &str) -> Result<HeaderMap, AppError> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    let (origin_k, origin_v) = parse_header_name_value("Origin", "https://kick.com")?;
    headers.insert(origin_k, origin_v);

    let (ref_k, ref_v) = parse_header_name_value("Referer", "https://kick.com/")?;
    headers.insert(ref_k, ref_v);

    let (plat_k, plat_v) = parse_header_name_value("x-app-platform", "web")?;
    headers.insert(plat_k, plat_v);

    // Kick's web session cookie is accepted as Bearer or session_token cookie depending on endpoint.
    let bearer = normalize_kick_bearer_token(token);
    if let Ok(auth) = HeaderValue::from_str(&format!("Bearer {bearer}")) {
        headers.insert(AUTHORIZATION, auth);
    }
    if let Ok(cookie) = HeaderValue::from_str(&format!("session_token={token}")) {
        headers.insert(COOKIE, cookie);
    }

    Ok(headers)
}

pub(crate) fn should_fallback_to_kick_bridge(
    status: StatusCode,
    headers: &HeaderMap,
    _body: &str,
) -> bool {
    status == StatusCode::FORBIDDEN
        || status == StatusCode::SERVICE_UNAVAILABLE
        || headers.contains_key("cf-mitigated")
}

pub async fn kick_request_json(
    http: &AllowlistedClient,
    app: Option<&AppHandle>,
    waiters: &BridgeWaiters,
    token: &str,
    req: KickRequest<'_>,
) -> Result<Value, AppError> {
    let KickRequest {
        op_name,
        method,
        url,
        json_body,
        not_found_is_resource,
        allow_bridge,
    } = req;
    let mut headers = build_kick_headers(token)?;
    if url.starts_with("https://websockets.kick.com/") {
        let (ct_k, ct_v) = parse_header_name_value("x-client-token", KICK_PUBLIC_CLIENT_TOKEN)?;
        headers.insert(ct_k, ct_v);
    }
    let resp = http
        .request_platform(PlatformId::Kick, op_name, method.clone(), url, headers, json_body)
        .await?;

    if resp.status == StatusCode::UNAUTHORIZED {
        return Err(AppError::Auth("Kick session expired or unauthorized (401)".to_string()));
    }

    if allow_bridge && should_fallback_to_kick_bridge(resp.status, &resp.headers, &resp.body) {
        // Fallback: execute the same request from inside Dropilot's hidden kick.com WebView2 page.
        if let Some(app_handle) = app {
            let method_str = if method == Method::POST { "POST" } else { "GET" };
            let body_str = json_body.and_then(|v| serde_json::to_string(v).ok());
            let bearer = normalize_kick_bearer_token(token);
            let bridged = kick_webview_fetch_with_waiters(
                app_handle,
                waiters,
                method_str,
                url,
                body_str.as_deref(),
                Some(&bearer),
            )
            .await?;

            if bridged.status == 401 {
                return Err(AppError::Auth("Kick session expired (401)".to_string()));
            }
            if bridged.status == 404 {
                return Err(if not_found_is_resource {
                    AppError::NotFound(format!("{op_name} returned 404"))
                } else {
                    AppError::PlatformChanged(format!("Kick endpoint returned 404 for {op_name}"))
                });
            }
            if bridged.status < 200 || bridged.status >= 300 {
                return Err(AppError::Blocked(format!(
                    "Kick returned HTTP {} via official page",
                    bridged.status
                )));
            }

            if bridged.body.trim().is_empty() {
                return Ok(serde_json::json!({}));
            }

            return serde_json::from_str(&bridged.body).map_err(|e| {
                AppError::PlatformChanged(format!("Unexpected JSON from Kick bridge ({op_name}): {e}"))
            });
        }

        return Err(AppError::Blocked(
            "Kick bot protection blocked direct request".to_string(),
        ));
    }

    if resp.status == StatusCode::FORBIDDEN {
        return Err(AppError::Blocked(format!(
            "Kick denied {op_name} (403); verify in official window"
        )));
    }

    if resp.status == StatusCode::NOT_FOUND {
        return Err(if not_found_is_resource {
            AppError::NotFound(format!("{op_name} resource not found"))
        } else {
            AppError::PlatformChanged(format!("Kick endpoint returned 404 for {op_name}"))
        });
    }

    if !resp.status.is_success() {
        return Err(AppError::Network(format!(
            "Kick {op_name} returned HTTP {}",
            resp.status.as_u16()
        )));
    }

    if resp.body.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }

    serde_json::from_str(&resp.body).map_err(|e| {
        AppError::PlatformChanged(format!("Unexpected JSON schema from Kick {op_name}: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::{normalize_kick_bearer_token, should_fallback_to_kick_bridge};
    use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
    use reqwest::StatusCode;

    #[test]
    fn normalizes_encoded_pipes_and_preserves_plus_in_kick_bearer_token() {
        assert_eq!(
            normalize_kick_bearer_token("12345%7Cabc%7cdef+xyz"),
            "12345|abc|def+xyz"
        );
        assert_eq!(
            normalize_kick_bearer_token("987|plain-token"),
            "987|plain-token"
        );
    }

    #[test]
    fn falls_back_to_kick_bridge_on_json_403_503_or_cf_header() {
        let empty = HeaderMap::new();
        assert!(should_fallback_to_kick_bridge(
            StatusCode::FORBIDDEN,
            &empty,
            r#"{"error":"Request blocked by security policy"}"#
        ));
        assert!(should_fallback_to_kick_bridge(
            StatusCode::SERVICE_UNAVAILABLE,
            &empty,
            "service unavailable"
        ));

        let mut cf_headers = HeaderMap::new();
        cf_headers.insert(
            HeaderName::from_static("cf-mitigated"),
            HeaderValue::from_static("challenge"),
        );
        assert!(should_fallback_to_kick_bridge(
            StatusCode::TOO_MANY_REQUESTS,
            &cf_headers,
            ""
        ));

        assert!(should_fallback_to_kick_bridge(
            StatusCode::FORBIDDEN,
            &empty,
            r#"{"data":{"type":"Forbidden"}}"#
        ));
        assert!(!should_fallback_to_kick_bridge(
            StatusCode::OK,
            &empty,
            r#"{"id":1}"#
        ));
        assert!(!should_fallback_to_kick_bridge(
            StatusCode::UNAUTHORIZED,
            &empty,
            r#"{"message":"Unauthenticated"}"#
        ));
    }
}
