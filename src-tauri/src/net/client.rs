use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, RETRY_AFTER, USER_AGENT};
use reqwest::{Method, Response, StatusCode};
use serde::de::DeserializeOwned;
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue as WsHeaderValue;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tracing::{debug, warn};
use url::Url;

use crate::net::allowlist::{validate_url, AllowlistKind};
use crate::net::rate_limit::TokenBucket;
use crate::net::redact::redact;
use crate::platforms::{AppError, PlatformId};

pub const DEFAULT_BROWSER_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0";

pub type WsConnection = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: String,
}

#[derive(Debug)]
pub struct AllowlistedClient {
    http_platform: reqwest::Client,
    http_update: reqwest::Client,
    twitch_limiter: Arc<TokenBucket>,
    twitch_cdn_limiter: Arc<TokenBucket>,
    kick_limiter: Arc<TokenBucket>,
    user_agent: RwLock<String>,
}

impl AllowlistedClient {
    pub fn new() -> Result<Self, AppError> {
        let redirect_policy = reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.error("too many redirects");
            }
            if let Err(err) = validate_url(attempt.url(), AllowlistKind::Platform) {
                return attempt.error(err);
            }
            attempt.follow()
        });

        let http_platform = reqwest::Client::builder()
            .use_rustls_tls()
            .redirect(redirect_policy)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| AppError::Internal(format!("Failed to build HTTP client: {e}")))?;

        let http_update = reqwest::Client::builder()
            .use_rustls_tls()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| AppError::Internal(format!("Failed to build update HTTP client: {e}")))?;

        Ok(Self {
            http_platform,
            http_update,
            twitch_limiter: Arc::new(TokenBucket::standard_platform()),
            twitch_cdn_limiter: Arc::new(TokenBucket::new(20.0, 5.0)),
            kick_limiter: Arc::new(TokenBucket::standard_platform()),
            user_agent: RwLock::new(DEFAULT_BROWSER_USER_AGENT.to_string()),
        })
    }

    pub async fn set_user_agent(&self, ua: String) {
        if !ua.trim().is_empty() {
            let mut guard = self.user_agent.write().await;
            *guard = ua;
        }
    }

    pub async fn user_agent(&self) -> String {
        self.user_agent.read().await.clone()
    }

    fn limiter(&self, platform: PlatformId) -> &TokenBucket {
        match platform {
            PlatformId::Twitch => &self.twitch_limiter,
            PlatformId::Kick => &self.kick_limiter,
        }
    }

    pub async fn request_platform(
        &self,
        platform: PlatformId,
        op_name: &str,
        method: Method,
        raw_url: &str,
        headers: HeaderMap,
        json_body: Option<&serde_json::Value>,
    ) -> Result<HttpResponse, AppError> {
        let parsed = Url::parse(raw_url)
            .map_err(|e| AppError::Internal(format!("Invalid URL for {op_name}: {e}")))?;
        validate_url(&parsed, AllowlistKind::Platform)?;

        self.limiter(platform).acquire().await;

        let ua = self.user_agent().await;
        let mut req = self.http_platform.request(method, parsed);
        req = req.header(USER_AGENT, ua);
        for (name, val) in &headers {
            req = req.header(name, val);
        }
        if let Some(body) = json_body {
            req = req.json(body);
        }

        let started = Instant::now();
        let resp = req
            .send()
            .await
            .map_err(|e| map_reqwest_error(op_name, e))?;

        let status = resp.status();
        let resp_headers = resp.headers().clone();
        let body = resp
            .text()
            .await
            .map_err(|e| AppError::Network(format!("{op_name} body read error: {}", redact(&e.to_string()))))?;

        let elapsed_ms = started.elapsed().as_millis();
        debug!(
            platform = platform.as_str(),
            op = op_name,
            status = status.as_u16(),
            elapsed_ms = elapsed_ms as u64,
            "HTTP request completed"
        );

        if status == StatusCode::TOO_MANY_REQUESTS {
            let retry_after = parse_retry_after(&resp_headers);
            return Err(AppError::RateLimited {
                retry_after_secs: retry_after,
                message: format!("{op_name} returned 429"),
            });
        }

        if status.is_server_error() {
            return Err(AppError::Network(format!(
                "{op_name} returned HTTP {}",
                status.as_u16()
            )));
        }

        Ok(HttpResponse {
            status,
            headers: resp_headers,
            body,
        })
    }

    pub async fn post_form_platform(
        &self,
        platform: PlatformId,
        op_name: &str,
        raw_url: &str,
        headers: HeaderMap,
        form_body: String,
    ) -> Result<HttpResponse, AppError> {
        let parsed = Url::parse(raw_url)
            .map_err(|e| AppError::Internal(format!("Invalid URL for {op_name}: {e}")))?;
        validate_url(&parsed, AllowlistKind::Platform)?;

        self.limiter(platform).acquire().await;

        let ua = self.user_agent().await;
        let mut req = self
            .http_platform
            .post(parsed)
            .header(USER_AGENT, ua)
            .header("Content-Type", "application/x-www-form-urlencoded");
        for (name, val) in &headers {
            req = req.header(name, val);
        }
        req = req.body(form_body);

        let started = Instant::now();
        let resp = req
            .send()
            .await
            .map_err(|e| map_reqwest_error(op_name, e))?;

        let status = resp.status();
        let resp_headers = resp.headers().clone();
        let body = resp
            .text()
            .await
            .map_err(|e| AppError::Network(format!("{op_name} body read error: {}", redact(&e.to_string()))))?;

        debug!(
            platform = platform.as_str(),
            op = op_name,
            status = status.as_u16(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "HTTP form POST completed"
        );

        if status == StatusCode::TOO_MANY_REQUESTS {
            let retry_after = parse_retry_after(&resp_headers);
            return Err(AppError::RateLimited {
                retry_after_secs: retry_after,
                message: format!("{op_name} returned 429"),
            });
        }

        if status.is_server_error() {
            return Err(AppError::Network(format!(
                "{op_name} returned HTTP {}",
                status.as_u16()
            )));
        }

        Ok(HttpResponse {
            status,
            headers: resp_headers,
            body,
        })
    }

    pub async fn head_platform(
        &self,
        platform: PlatformId,
        op_name: &str,
        raw_url: &str,
        headers: HeaderMap,
    ) -> Result<StatusCode, AppError> {
        let parsed = Url::parse(raw_url)
            .map_err(|e| AppError::Internal(format!("Invalid URL for {op_name}: {e}")))?;
        validate_url(&parsed, AllowlistKind::Platform)?;

        self.limiter(platform).acquire().await;

        let ua = self.user_agent().await;
        let mut req = self.http_platform.head(parsed).header(USER_AGENT, ua);
        for (name, val) in &headers {
            req = req.header(name, val);
        }

        let started = Instant::now();
        let resp = req
            .send()
            .await
            .map_err(|e| map_reqwest_error(op_name, e))?;
        let status = resp.status();
        let elapsed_ms = started.elapsed().as_millis();

        debug!(
            platform = platform.as_str(),
            op = op_name,
            status = status.as_u16(),
            elapsed_ms = elapsed_ms as u64,
            "HTTP HEAD completed"
        );

        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(AppError::RateLimited {
                retry_after_secs: parse_retry_after(resp.headers()),
                message: format!("{op_name} returned 429"),
            });
        }

        if status.is_server_error() {
            return Err(AppError::Network(format!(
                "{op_name} returned HTTP {}",
                status.as_u16()
            )));
        }

        Ok(status)
    }

    /// Lightweight allowlisted GET for Twitch HLS playlists (`*.ttvnw.net`) using the
    /// dedicated CDN token bucket so periodic playlist refreshes never starve GQL calls.
    pub async fn get_twitch_cdn_text(
        &self,
        op_name: &str,
        raw_url: &str,
        close_connection: bool,
    ) -> Result<HttpResponse, AppError> {
        let parsed = Url::parse(raw_url)
            .map_err(|e| AppError::Internal(format!("Invalid URL for {op_name}: {e}")))?;
        validate_url(&parsed, AllowlistKind::Platform)?;

        self.twitch_cdn_limiter.acquire().await;

        let ua = self.user_agent().await;
        let mut req = self.http_platform.get(parsed).header(USER_AGENT, ua);
        if close_connection {
            req = req.header("Connection", "close");
        }

        let resp = req
            .send()
            .await
            .map_err(|e| map_reqwest_error(op_name, e))?;
        let status = resp.status();
        let headers = resp.headers().clone();
        let body = resp
            .text()
            .await
            .map_err(|e| AppError::Network(format!("{op_name} body read error: {}", redact(&e.to_string()))))?;

        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }

    /// Lightweight allowlisted HEAD request for Twitch HLS edge chunks (`*.hls.ttvnw.net`).
    /// Downloads 0 video bytes while signaling active stream playback to Twitch's drop tracker.
    pub async fn head_twitch_cdn(
        &self,
        op_name: &str,
        raw_url: &str,
    ) -> Result<StatusCode, AppError> {
        let parsed = Url::parse(raw_url)
            .map_err(|e| AppError::Internal(format!("Invalid URL for {op_name}: {e}")))?;
        validate_url(&parsed, AllowlistKind::Platform)?;

        self.twitch_cdn_limiter.acquire().await;

        let ua = self.user_agent().await;
        let resp = self
            .http_platform
            .head(parsed)
            .header(USER_AGENT, ua)
            .send()
            .await
            .map_err(|e| map_reqwest_error(op_name, e))?;

        Ok(resp.status())
    }

    pub async fn fetch_update_json<T: DeserializeOwned>(
        &self,
        raw_url: &str,
    ) -> Result<T, AppError> {
        let parsed = Url::parse(raw_url)
            .map_err(|e| AppError::Internal(format!("Invalid update URL: {e}")))?;
        validate_url(&parsed, AllowlistKind::UpdateCheck)?;

        let started = Instant::now();
        let resp: Response = self
            .http_update
            .get(parsed)
            .header(USER_AGENT, format!("Dropilot/{}", env!("CARGO_PKG_VERSION")))
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| map_reqwest_error("update_check", e))?;

        let status = resp.status();
        let elapsed_ms = started.elapsed().as_millis();
        debug!(
            op = "update_check",
            status = status.as_u16(),
            elapsed_ms = elapsed_ms as u64,
            "GitHub release check completed"
        );

        if !status.is_success() {
            return Err(AppError::Network(format!(
                "Update check returned HTTP {}",
                status.as_u16()
            )));
        }

        resp.json::<T>()
            .await
            .map_err(|e| AppError::PlatformChanged(format!("Unexpected GitHub release JSON: {e}")))
    }

    pub async fn fetch_remote_config_json<T: DeserializeOwned>(
        &self,
        raw_url: &str,
    ) -> Result<T, AppError> {
        let parsed = Url::parse(raw_url)
            .map_err(|e| AppError::Internal(format!("Invalid remote config URL: {e}")))?;
        validate_url(&parsed, AllowlistKind::RemoteConfig)?;

        let started = Instant::now();
        let resp: Response = self
            .http_update
            .get(parsed)
            .header(USER_AGENT, format!("Dropilot/{}", env!("CARGO_PKG_VERSION")))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| map_reqwest_error("remote_config", e))?;

        let status = resp.status();
        debug!(
            op = "remote_config",
            status = status.as_u16(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "Remote config fetch completed"
        );

        if !status.is_success() {
            return Err(AppError::Network(format!(
                "Remote config returned HTTP {}",
                status.as_u16()
            )));
        }

        resp.json::<T>()
            .await
            .map_err(|e| AppError::PlatformChanged(format!("Invalid remote config JSON: {e}")))
    }

    pub async fn fetch_epic_free_games_json(
        &self,
        raw_url: &str,
    ) -> Result<serde_json::Value, AppError> {
        let parsed = Url::parse(raw_url)
            .map_err(|e| AppError::Internal(format!("Invalid Epic feed URL: {e}")))?;
        validate_url(&parsed, AllowlistKind::EpicFeed)?;

        let started = Instant::now();
        let resp: Response = self
            .http_update
            .get(parsed)
            .header(USER_AGENT, format!("Dropilot/{}", env!("CARGO_PKG_VERSION")))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| map_reqwest_error("epic_free_games", e))?;

        let status = resp.status();
        debug!(
            op = "epic_free_games",
            status = status.as_u16(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "Epic free games fetch completed"
        );

        if !status.is_success() {
            return Err(AppError::Network(format!(
                "Epic Games catalog returned HTTP {}",
                status.as_u16()
            )));
        }

        resp.json::<serde_json::Value>()
            .await
            .map_err(|e| AppError::PlatformChanged(format!("Invalid Epic Games catalog JSON: {e}")))
    }

    pub async fn get_webhook_json(
        &self,
        op_name: &str,
        raw_url: &str,
    ) -> Result<(StatusCode, String), AppError> {
        let parsed = Url::parse(raw_url)
            .map_err(|e| AppError::Internal(format!("Invalid webhook URL: {}", redact(&e.to_string()))))?;
        validate_url(&parsed, AllowlistKind::Webhook)?;

        let resp: Response = self
            .http_update
            .get(parsed)
            .header(
                USER_AGENT,
                format!(
                    "DiscordBot (https://github.com/theflash1211/dropilot, {})",
                    env!("CARGO_PKG_VERSION")
                ),
            )
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| map_reqwest_error(op_name, e))?;

        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Ok((status, body))
    }

    pub async fn post_webhook_json(
        &self,
        op_name: &str,
        raw_url: &str,
        payload: &serde_json::Value,
    ) -> Result<(StatusCode, String), AppError> {
        let parsed = Url::parse(raw_url)
            .map_err(|e| AppError::Internal(format!("Invalid webhook URL: {}", redact(&e.to_string()))))?;
        validate_url(&parsed, AllowlistKind::Webhook)?;

        let started = Instant::now();
        let resp: Response = self
            .http_update
            .post(parsed)
            .header(
                USER_AGENT,
                format!(
                    "DiscordBot (https://github.com/theflash1211/dropilot, {})",
                    env!("CARGO_PKG_VERSION")
                ),
            )
            .header("Accept", "application/json")
            .json(payload)
            .send()
            .await
            .map_err(|e| map_reqwest_error(op_name, e))?;

        let status = resp.status();
        let headers = resp.headers().clone();
        let body = resp.text().await.unwrap_or_default();
        debug!(
            op = op_name,
            status = status.as_u16(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "Webhook POST completed"
        );

        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(AppError::RateLimited {
                retry_after_secs: parse_retry_after(&headers),
                message: format!("{op_name} returned 429"),
            });
        }

        if !status.is_success() {
            let trimmed = redact(body.trim());
            let detail = if trimmed.is_empty() {
                format!("{op_name} returned HTTP {}", status.as_u16())
            } else {
                let snippet: String = trimmed.chars().take(200).collect();
                format!("{op_name} returned HTTP {}: {snippet}", status.as_u16())
            };
            warn!(op = op_name, status = status.as_u16(), detail = %detail, "Webhook request failed");
            return Err(AppError::Network(detail));
        }

        Ok((status, body))
    }

    pub async fn connect_ws(
        &self,
        platform: PlatformId,
        op_name: &str,
        raw_url: &str,
        origin: &str,
        extra_headers: HashMap<String, String>,
    ) -> Result<WsConnection, AppError> {
        let parsed = Url::parse(raw_url)
            .map_err(|e| AppError::Internal(format!("Invalid WS URL for {op_name}: {e}")))?;
        validate_url(&parsed, AllowlistKind::Platform)?;

        self.limiter(platform).acquire().await;

        let mut request = parsed
            .as_str()
            .into_client_request()
            .map_err(|e| AppError::Internal(format!("WS request build error: {e}")))?;

        let ua = self.user_agent().await;
        if let Ok(origin_val) = WsHeaderValue::from_str(origin) {
            request.headers_mut().insert("Origin", origin_val);
        }
        if let Ok(ua_val) = WsHeaderValue::from_str(&ua) {
            request.headers_mut().insert("User-Agent", ua_val);
        }
        for (k, v) in extra_headers {
            if let (Ok(name), Ok(val)) = (
                k.parse::<tokio_tungstenite::tungstenite::http::HeaderName>(),
                WsHeaderValue::from_str(&v),
            ) {
                request.headers_mut().insert(name, val);
            }
        }

        let started = Instant::now();
        let (stream, resp) = connect_async(request)
            .await
            .map_err(|e| AppError::Network(format!("{op_name} WS connect failed: {}", redact(&e.to_string()))))?;

        debug!(
            platform = platform.as_str(),
            op = op_name,
            status = resp.status().as_u16(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "WebSocket connected"
        );

        Ok(stream)
    }
}

pub fn parse_header_name_value(name: &str, value: &str) -> Result<(HeaderName, HeaderValue), AppError> {
    let h_name = HeaderName::from_bytes(name.as_bytes())
        .map_err(|e| AppError::Internal(format!("Invalid header name: {e}")))?;
    let h_val = HeaderValue::from_str(value)
        .map_err(|e| AppError::Internal(format!("Invalid header value: {e}")))?;
    Ok((h_name, h_val))
}

fn parse_retry_after(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
}

fn map_reqwest_error(op_name: &str, err: reqwest::Error) -> AppError {
    let msg = redact(&err.to_string());
    if err.is_redirect() && msg.contains("HostNotAllowed") {
        warn!(op = op_name, "Redirect blocked by host allowlist");
        return AppError::HostNotAllowed(format!("Redirect target blocked during {op_name}"));
    }
    AppError::Network(format!("{op_name} failed: {msg}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_non_allowlisted_host_before_network() {
        let client = AllowlistedClient::new().expect("client initializes");
        let err = client
            .request_platform(
                PlatformId::Twitch,
                "blocked_test",
                Method::GET,
                "https://example.com/secret",
                HeaderMap::new(),
                None,
            )
            .await
            .expect_err("non-allowlisted host must fail");

        assert!(matches!(err, AppError::HostNotAllowed(_)));
    }
}
