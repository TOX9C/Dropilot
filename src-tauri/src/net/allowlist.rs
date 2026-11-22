use std::net::IpAddr;

use tracing::warn;
use url::Url;

use crate::platforms::AppError;

pub const TWITCH_EXACT_HOSTS: &[&str] = &[
    "twitch.tv",
    "www.twitch.tv",
    "gql.twitch.tv",
    "id.twitch.tv",
    "passport.twitch.tv",
    "spade.twitch.tv",
    "beacon.twitch.tv",
    "assets.twitch.tv",
    "static.twitchcdn.net",
    "usher.ttvnw.net",
    "static-cdn.jtvnw.net",
];

pub const TWITCH_SUFFIX_HOSTS: &[&str] = &[".ttvnw.net", ".jtvnw.net"];

pub const KICK_EXACT_HOSTS: &[&str] = &["kick.com"];

pub const KICK_SUFFIX_HOSTS: &[&str] = &[".kick.com", ".live-video.net"];

pub const UPDATE_CHECK_HOST: &str = "api.github.com";
pub const UPDATE_CHECK_PATH: &str = "/repos/droppilot/droppilot/releases/latest";

pub const LOGIN_IDP_EXACT_HOSTS: &[&str] = &[
    "accounts.google.com",
    "appleid.apple.com",
    "challenges.cloudflare.com",
    "id.twitch.tv",
    "passport.twitch.tv",
];

pub const LOGIN_IDP_SUFFIX_HOSTS: &[&str] = &[
    ".twitch.tv",
    ".kick.com",
    ".google.com",
    ".apple.com",
    ".amazon.com",
    ".gstatic.com",
    ".googleapis.com",
];

pub const EXTERNAL_LINK_EXACT_HOSTS: &[&str] = &[
    "twitch.tv",
    "www.twitch.tv",
    "kick.com",
    "github.com",
    "developer.microsoft.com",
];

pub const EXTERNAL_LINK_SUFFIX_HOSTS: &[&str] = &[".twitch.tv", ".kick.com"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowlistKind {
    Platform,
    UpdateCheck,
    LoginNavigation,
    ExternalOpen,
}

fn matches_host(host: &str, exact: &[&str], suffixes: &[&str]) -> bool {
    let normalized = host.trim_end_matches('.').to_ascii_lowercase();
    if exact.iter().any(|allowed| normalized == *allowed) {
        return true;
    }
    suffixes.iter().any(|suffix| {
        normalized.ends_with(suffix) && normalized.len() > suffix.len()
    })
}

fn is_valid_host_syntax(url: &Url) -> Result<String, AppError> {
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AppError::HostNotAllowed(
            "URLs with embedded credentials are not allowed".to_string(),
        ));
    }

    let Some(host) = url.host_str() else {
        return Err(AppError::HostNotAllowed("Missing URL host".to_string()));
    };

    let trimmed = host.trim_start_matches('[').trim_end_matches(']');
    if trimmed.parse::<IpAddr>().is_ok() {
        return Err(AppError::HostNotAllowed(format!(
            "Direct IP address connections are blocked ({host})"
        )));
    }

    if let Some(port) = url.port() {
        if port != 443 {
            return Err(AppError::HostNotAllowed(format!(
                "Non-standard port {port} blocked for {host}"
            )));
        }
    }

    Ok(host.to_ascii_lowercase())
}

pub fn validate_url(url: &Url, kind: AllowlistKind) -> Result<(), AppError> {
    let scheme = url.scheme();
    let allowed_scheme = match kind {
        AllowlistKind::Platform => scheme == "https" || scheme == "wss",
        AllowlistKind::UpdateCheck
        | AllowlistKind::LoginNavigation
        | AllowlistKind::ExternalOpen => scheme == "https",
    };

    if !allowed_scheme {
        warn!(scheme = %scheme, "Blocked outbound URL with disallowed scheme");
        return Err(AppError::HostNotAllowed(format!(
            "Disallowed URL scheme: {scheme}"
        )));
    }

    let host = match is_valid_host_syntax(url) {
        Ok(h) => h,
        Err(err) => {
            warn!(error = %err, "Blocked malformed or IP-literal outbound URL");
            return Err(err);
        }
    };

    let allowed = match kind {
        AllowlistKind::Platform => {
            matches_host(&host, TWITCH_EXACT_HOSTS, TWITCH_SUFFIX_HOSTS)
                || matches_host(&host, KICK_EXACT_HOSTS, KICK_SUFFIX_HOSTS)
        }
        AllowlistKind::UpdateCheck => {
            host == UPDATE_CHECK_HOST && url.path() == UPDATE_CHECK_PATH
        }
        AllowlistKind::LoginNavigation => {
            matches_host(&host, TWITCH_EXACT_HOSTS, TWITCH_SUFFIX_HOSTS)
                || matches_host(&host, KICK_EXACT_HOSTS, KICK_SUFFIX_HOSTS)
                || matches_host(&host, LOGIN_IDP_EXACT_HOSTS, LOGIN_IDP_SUFFIX_HOSTS)
        }
        AllowlistKind::ExternalOpen => {
            matches_host(&host, EXTERNAL_LINK_EXACT_HOSTS, EXTERNAL_LINK_SUFFIX_HOSTS)
        }
    };

    if !allowed {
        warn!(host = %host, path = %url.path(), kind = ?kind, "Blocked non-allowlisted outbound host");
        return Err(AppError::HostNotAllowed(host));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Url {
        Url::parse(s).expect("valid test URL")
    }

    #[test]
    fn allows_official_twitch_and_kick_hosts() {
        assert!(validate_url(&parse("https://gql.twitch.tv/gql"), AllowlistKind::Platform).is_ok());
        assert!(validate_url(&parse("https://id.twitch.tv/oauth2/validate"), AllowlistKind::Platform).is_ok());
        assert!(validate_url(&parse("https://usher.ttvnw.net/api/channel/hls/foo.m3u8"), AllowlistKind::Platform).is_ok());
        assert!(validate_url(&parse("https://video-weaver.jfk04.hls.ttvnw.net/v1/playlist/x"), AllowlistKind::Platform).is_ok());
        assert!(validate_url(&parse("https://static-cdn.jtvnw.net/ttv-boxart/1.jpg"), AllowlistKind::Platform).is_ok());
        assert!(validate_url(&parse("https://web.kick.com/api/v1/drops/campaigns"), AllowlistKind::Platform).is_ok());
        assert!(validate_url(&parse("wss://websockets.kick.com/viewer/v1/connect"), AllowlistKind::Platform).is_ok());
        assert!(validate_url(&parse("https://1234.global-contribute.live-video.net/stream"), AllowlistKind::Platform).is_ok());
    }

    #[test]
    fn rejects_non_allowlisted_and_bypass_attempts() {
        assert!(validate_url(&parse("https://example.com/api"), AllowlistKind::Platform).is_err());
        assert!(validate_url(&parse("https://twitch.tv.evil.com/gql"), AllowlistKind::Platform).is_err());
        assert!(validate_url(&parse("https://faketwitch.tv/gql"), AllowlistKind::Platform).is_err());
        assert!(validate_url(&parse("http://gql.twitch.tv/gql"), AllowlistKind::Platform).is_err());
        assert!(validate_url(&parse("https://127.0.0.1/gql"), AllowlistKind::Platform).is_err());
        assert!(validate_url(&parse("https://user:pass@gql.twitch.tv/gql"), AllowlistKind::Platform).is_err());
        assert!(validate_url(&parse("https://gql.twitch.tv:8443/gql"), AllowlistKind::Platform).is_err());
    }

    #[test]
    fn update_check_requires_exact_repo_path() {
        assert!(validate_url(
            &parse("https://api.github.com/repos/droppilot/droppilot/releases/latest"),
            AllowlistKind::UpdateCheck
        ).is_ok());
        assert!(validate_url(
            &parse("https://api.github.com/repos/other/repo/releases/latest"),
            AllowlistKind::UpdateCheck
        ).is_err());
        assert!(validate_url(
            &parse("https://api.github.com/repos/droppilot/droppilot/releases/latest"),
            AllowlistKind::Platform
        ).is_err());
    }
}
