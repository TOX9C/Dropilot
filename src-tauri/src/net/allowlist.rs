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
pub const UPDATE_CHECK_PATH: &str = "/repos/theflash1211/dropilot/releases/latest";

pub const REMOTE_CONFIG_HOST: &str = "raw.githubusercontent.com";
pub const REMOTE_CONFIG_PATH: &str = "/theflash1211/dropilot/main/remote-config.json";
pub const REMOTE_CONFIG_URL: &str =
    "https://raw.githubusercontent.com/theflash1211/dropilot/main/remote-config.json";

pub const DISCORD_WEBHOOK_HOSTS: &[&str] = &["discord.com", "discordapp.com"];
pub const TELEGRAM_API_HOST: &str = "api.telegram.org";

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
    RemoteConfig,
    Webhook,
    LoginNavigation,
    ExternalOpen,
}

fn is_valid_webhook_url(host: &str, path: &str) -> bool {
    if DISCORD_WEBHOOK_HOSTS.contains(&host) {
        let Some(rest) = path.strip_prefix("/api/webhooks/") else {
            return false;
        };
        let parts: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
        return parts.len() >= 2 && parts[0].chars().all(|c| c.is_ascii_digit());
    }
    if host == TELEGRAM_API_HOST {
        return path.starts_with("/bot")
            && path.ends_with("/sendMessage")
            && path.len() > "/bot/sendMessage".len();
    }
    false
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
        | AllowlistKind::RemoteConfig
        | AllowlistKind::Webhook
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
        AllowlistKind::RemoteConfig => {
            host == REMOTE_CONFIG_HOST && url.path() == REMOTE_CONFIG_PATH
        }
        AllowlistKind::Webhook => is_valid_webhook_url(&host, url.path()),
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
            &parse("https://api.github.com/repos/theflash1211/dropilot/releases/latest"),
            AllowlistKind::UpdateCheck
        ).is_ok());
        assert!(validate_url(
            &parse("https://api.github.com/repos/other/repo/releases/latest"),
            AllowlistKind::UpdateCheck
        ).is_err());
        assert!(validate_url(
            &parse("https://api.github.com/repos/theflash1211/dropilot/releases/latest"),
            AllowlistKind::Platform
        ).is_err());
    }

    #[test]
    fn remote_config_requires_exact_repo_path() {
        assert!(validate_url(
            &parse(REMOTE_CONFIG_URL),
            AllowlistKind::RemoteConfig
        ).is_ok());
        assert!(validate_url(
            &parse("https://raw.githubusercontent.com/evil/repo/main/remote-config.json"),
            AllowlistKind::RemoteConfig
        ).is_err());
        assert!(validate_url(
            &parse("http://raw.githubusercontent.com/theflash1211/dropilot/main/remote-config.json"),
            AllowlistKind::RemoteConfig
        ).is_err());
    }

    #[test]
    fn webhook_allows_only_discord_and_telegram_endpoints() {
        assert!(validate_url(
            &parse("https://discord.com/api/webhooks/123456789012345678/abcDEF_token-123"),
            AllowlistKind::Webhook
        ).is_ok());
        assert!(validate_url(
            &parse("https://discordapp.com/api/webhooks/123456789012345678/abcDEF_token-123"),
            AllowlistKind::Webhook
        ).is_ok());
        assert!(validate_url(
            &parse("https://api.telegram.org/bot123456:ABC-DEF1234/sendMessage"),
            AllowlistKind::Webhook
        ).is_ok());
        assert!(validate_url(
            &parse("https://discord.com/api/v10/users/@me"),
            AllowlistKind::Webhook
        ).is_err());
        assert!(validate_url(
            &parse("https://api.telegram.org/bot123456:ABC/getUpdates"),
            AllowlistKind::Webhook
        ).is_err());
        assert!(validate_url(
            &parse("https://evil.example/api/webhooks/12345/token"),
            AllowlistKind::Webhook
        ).is_err());
    }
}
