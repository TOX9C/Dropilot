const SECRET_KEYS: &[&str] = &[
    "auth-token=",
    "session_token=",
    "xsrf-token=",
    "cf_clearance=",
    "client_token=",
    "access_token=",
    "refresh_token=",
    "token=",
    "authorization:",
    "authorization=",
    "cookie:",
    "set-cookie:",
    "x-xsrf-token:",
];

const AUTH_PREFIXES: &[&str] = &["oauth ", "bearer "];

/// Scrubs auth tokens, session cookies, and sensitive header values before logging or emitting UI events.
pub fn redact(input: &str) -> String {
    let mut out = input.to_string();

    for prefix in AUTH_PREFIXES {
        out = scrub_after_prefix(&out, prefix);
    }

    for key in SECRET_KEYS {
        out = scrub_after_key(&out, key);
    }

    out = scrub_discord_webhooks(&out);

    out
}

fn scrub_discord_webhooks(input: &str) -> String {
    const API: &str = "/api/";
    const WEBHOOKS: &str = "webhooks/";

    let lower = input.to_ascii_lowercase();
    let mut result = String::with_capacity(input.len());
    let mut cursor = 0;

    while let Some(rel_idx) = lower[cursor..].find(API) {
        let api_start = cursor + rel_idx;
        let after_api = api_start + API.len();
        let webhook_start = if lower[after_api..].starts_with(WEBHOOKS) {
            after_api
        } else {
            let bytes = lower.as_bytes();
            let mut idx = after_api;
            if bytes.get(idx) != Some(&b'v') {
                cursor = after_api;
                continue;
            }
            idx += 1;
            let digits_start = idx;
            while bytes.get(idx).is_some_and(|b| b.is_ascii_digit()) {
                idx += 1;
            }
            if idx == digits_start || bytes.get(idx) != Some(&b'/') {
                cursor = after_api;
                continue;
            }
            idx + 1
        };

        let val_start = webhook_start + WEBHOOKS.len();
        if !lower[webhook_start..].starts_with(WEBHOOKS) {
            cursor = after_api;
            continue;
        }
        result.push_str(&input[cursor..val_start]);
        result.push_str("[REDACTED]");

        let rest = &input[val_start..];
        let val_len = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ';' | ',' | ')' | '}'))
            .unwrap_or(rest.len());
        cursor = val_start + val_len;
    }

    result.push_str(&input[cursor..]);
    result
}

fn scrub_after_prefix(input: &str, prefix_lower: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut result = String::with_capacity(input.len());
    let mut cursor = 0;

    while let Some(rel_idx) = lower[cursor..].find(prefix_lower) {
        let match_start = cursor + rel_idx;
        let val_start = match_start + prefix_lower.len();
        result.push_str(&input[cursor..val_start]);
        result.push_str("[REDACTED]");

        let rest = &input[val_start..];
        let val_len = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ';' | ',' | '&' | ')' | '}'))
            .unwrap_or(rest.len());
        cursor = val_start + val_len;
    }

    result.push_str(&input[cursor..]);
    result
}

fn scrub_after_key(input: &str, key_lower: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut result = String::with_capacity(input.len());
    let mut cursor = 0;

    while let Some(rel_idx) = lower[cursor..].find(key_lower) {
        let match_start = cursor + rel_idx;

        // Avoid matching inside a longer identifier like "max_token=" when looking for "token=".
        if key_lower == "token=" && match_start > 0 {
            let prev = input.as_bytes()[match_start - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'-' {
                let advance = match_start + key_lower.len();
                result.push_str(&input[cursor..advance]);
                cursor = advance;
                continue;
            }
        }

        let after_key = match_start + key_lower.len();
        let after_spaces = skip_spaces(input, after_key);
        result.push_str(&input[cursor..after_spaces]);
        result.push_str("[REDACTED]");

        let rest = &input[after_spaces..];
        let val_len = if key_lower.ends_with(':') {
            rest.find(['\r', '\n'])
                .unwrap_or(rest.len())
        } else {
            rest.find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ';' | ',' | '&' | '}'))
                .unwrap_or(rest.len())
        };
        cursor = after_spaces + val_len;
    }

    result.push_str(&input[cursor..]);
    result
}

fn skip_spaces(s: &str, mut idx: usize) -> usize {
    let bytes = s.as_bytes();
    while idx < bytes.len() && bytes[idx] == b' ' {
        idx += 1;
    }
    idx
}

#[cfg(test)]
mod tests {
    use super::redact;

    #[test]
    fn redacts_tokens_cookies_and_headers() {
        let line = "auth failed Authorization: OAuth supersecrettoken123 Cookie: auth-token=abc123def; cf_clearance=xyz999";
        let scrubbed = redact(line);
        assert!(!scrubbed.contains("supersecrettoken123"));
        assert!(!scrubbed.contains("abc123def"));
        assert!(!scrubbed.contains("xyz999"));
        assert!(scrubbed.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_ws_query_token_and_bearer() {
        let line = "connecting wss://websockets.kick.com/viewer/v1/connect?token=secret_ws_tok_42 with Bearer jwt_abc_xyz";
        let scrubbed = redact(line);
        assert!(!scrubbed.contains("secret_ws_tok_42"));
        assert!(!scrubbed.contains("jwt_abc_xyz"));
        assert!(scrubbed.contains("token=[REDACTED]"));
        assert!(scrubbed.contains("Bearer [REDACTED]"));
    }

    #[test]
    fn preserves_normal_operational_logs() {
        let line = "GQL ViewerDropsDashboard status=200 elapsed_ms=142";
        assert_eq!(redact(line), line);
    }

    #[test]
    fn redacts_discord_webhooks() {
        for path in [
            "/api/webhooks/123456/super_secret_hook",
            "/api/v8/webhooks/123456/super_secret_hook",
            "/api/v10/webhooks/123456/super_secret_hook",
        ] {
            let scrubbed = redact(&format!("POST https://discord.com{path} failed"));
            assert!(!scrubbed.contains("super_secret_hook"), "{scrubbed}");
            assert!(scrubbed.contains("/webhooks/[REDACTED]"), "{scrubbed}");
        }
    }
}
