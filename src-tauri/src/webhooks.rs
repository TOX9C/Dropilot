use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, warn};
use url::Url;

use crate::net::allowlist::{validate_url, AllowlistKind, DISCORD_WEBHOOK_HOSTS};
use crate::net::client::AllowlistedClient;
use crate::platforms::PlatformId;
use crate::secrets::SecretStore;
use crate::settings::WebhookSettings;

pub const WEBHOOK_DISCORD_SECRET_KEY: &str = "webhook_discord_url";
const LEGACY_WEBHOOK_TELEGRAM_SECRET_KEY: &str = "webhook_telegram_token";

const ACTION_REQUIRED_COOLDOWN: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebhookChannel {
    Discord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookSecretsStatus {
    pub has_discord_url: bool,
}

#[derive(Debug, Clone)]
pub enum WebhookEvent {
    DropClaimed {
        platform: PlatformId,
        game_name: String,
        campaign_name: String,
        drop_name: String,
        reward_image_url: Option<String>,
    },
    ActionRequired {
        platform: PlatformId,
        category: String,
        title: String,
        detail: String,
    },
    CampaignCompleted {
        platform: PlatformId,
        game_name: String,
        campaign_name: String,
        drops_count: usize,
    },
    EpicFreeGame {
        title: String,
        original_price: String,
        ends_at: DateTime<Utc>,
        store_url: String,
        image_url: Option<String>,
    },
    TestPing,
}

pub struct WebhookDispatcher {
    cooldowns: StdMutex<HashMap<String, Instant>>,
}

impl Default for WebhookDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl WebhookDispatcher {
    pub fn new() -> Self {
        Self {
            cooldowns: StdMutex::new(HashMap::new()),
        }
    }

    pub fn secrets_status() -> WebhookSecretsStatus {
        let has_discord_url = SecretStore::load_named_secret(WEBHOOK_DISCORD_SECRET_KEY)
            .ok()
            .flatten()
            .is_some();
        WebhookSecretsStatus { has_discord_url }
    }

    fn normalize_discord_url(raw_url: &str) -> Result<String, String> {
        let trimmed = raw_url.trim();
        let mut parsed =
            Url::parse(trimmed).map_err(|_| "Invalid Discord webhook URL".to_string())?;
        let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
        if !DISCORD_WEBHOOK_HOSTS.iter().any(|h| *h == host)
            || validate_url(&parsed, AllowlistKind::Webhook).is_err()
        {
            return Err(
                "Discord webhook must be an https://discord.com/api/webhooks/<id>/<token> URL"
                    .to_string(),
            );
        }
        // Normalize legacy/alt Discord domains to discord.com to avoid 301/308 redirects.
        if host != "discord.com" {
            let _ = parsed.set_host(Some("discord.com"));
        }
        // Strip accidental /slack or /github compatibility suffixes so Discord executes native
        // JSON payloads instead of silently ignoring embeds with 204.
        let path_owned = parsed.path().to_string();
        let rest = path_owned
            .strip_prefix("/api/webhooks/")
            .or_else(|| path_owned.strip_prefix("/api/v10/webhooks/"))
            .or_else(|| path_owned.strip_prefix("/api/v9/webhooks/"));
        if let Some(rest) = rest {
            let parts: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
            if parts.len() >= 2 {
                parsed.set_path(&format!("/api/webhooks/{}/{}", parts[0], parts[1]));
            }
        }
        Ok(parsed.to_string())
    }

    pub fn save_discord_url(raw_url: &str) -> Result<(), String> {
        let trimmed = raw_url.trim();
        if trimmed.is_empty() {
            SecretStore::delete_named_secret(WEBHOOK_DISCORD_SECRET_KEY)
                .map_err(|e| e.user_message())?;
            return Ok(());
        }
        let normalized = Self::normalize_discord_url(trimmed)?;
        SecretStore::save_named_secret(WEBHOOK_DISCORD_SECRET_KEY, &normalized)
            .map_err(|e| e.user_message())
    }

    fn clear_secrets_with(
        mut delete: impl FnMut(&str) -> Result<(), crate::platforms::AppError>,
    ) -> Result<(), String> {
        let mut errors = Vec::new();
        for key in [
            WEBHOOK_DISCORD_SECRET_KEY,
            LEGACY_WEBHOOK_TELEGRAM_SECRET_KEY,
        ] {
            if let Err(error) = delete(key) {
                errors.push(format!("{key}: {}", error.user_message()));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "Failed deleting webhook credentials: {}",
                errors.join("; ")
            ))
        }
    }

    pub fn clear_all_secrets() -> Result<(), String> {
        Self::clear_secrets_with(SecretStore::delete_named_secret)
    }

    fn should_send_event(&self, settings: &WebhookSettings, event: &WebhookEvent) -> bool {
        if !settings.discord_enabled {
            return false;
        }
        match event {
            WebhookEvent::DropClaimed { .. } => settings.on_drop_claimed,
            WebhookEvent::CampaignCompleted { .. } => settings.on_campaign_completed,
            WebhookEvent::EpicFreeGame { .. } => settings.on_epic_free_games,
            WebhookEvent::ActionRequired {
                platform, category, ..
            } => {
                if !settings.on_action_required {
                    return false;
                }
                let key = format!("{}:{category}", platform.as_str());
                let now = Instant::now();
                if let Ok(mut map) = self.cooldowns.lock() {
                    if let Some(&until) = map.get(&key) {
                        if now < until {
                            return false;
                        }
                    }
                    map.insert(key, now + ACTION_REQUIRED_COOLDOWN);
                }
                true
            }
            WebhookEvent::TestPing => true,
        }
    }

    /// Fires enabled webhooks asynchronously in a background task without blocking the miner loop.
    pub fn dispatch_async(
        &self,
        http: Arc<AllowlistedClient>,
        settings: WebhookSettings,
        event: WebhookEvent,
    ) {
        if !self.should_send_event(&settings, &event) {
            return;
        }

        tauri::async_runtime::spawn(async move {
            if settings.discord_enabled {
                if let Ok(Some(url_secret)) =
                    SecretStore::load_named_secret(WEBHOOK_DISCORD_SECRET_KEY)
                {
                    let target_url = Self::normalize_discord_url(url_secret.expose())
                        .unwrap_or_else(|_| url_secret.expose().to_string());
                    let payload = build_discord_payload(&event);
                    match http
                        .post_webhook_json("discord_webhook", &target_url, &payload)
                        .await
                    {
                        Ok(_) => info!("Delivered Discord webhook notification"),
                        Err(err) => warn!(error = %err, "Discord webhook delivery failed"),
                    }
                }
            }
        });
    }

    /// Sends a synchronous test ping to a specific channel so the Settings UI gets immediate feedback.
    pub async fn send_test(
        http: &AllowlistedClient,
        channel: WebhookChannel,
    ) -> Result<String, String> {
        let event = WebhookEvent::TestPing;
        match channel {
            WebhookChannel::Discord => {
                let url = SecretStore::load_named_secret(WEBHOOK_DISCORD_SECRET_KEY)
                    .map_err(|e| e.user_message())?
                    .ok_or_else(|| "Save a Discord webhook URL first".to_string())?;
                let normalized = Self::normalize_discord_url(url.expose())?;
                // Best-effort metadata lookup so we can report the webhook name and guild link.
                let (webhook_name, guild_id) = match http
                    .get_webhook_json("discord_webhook_meta", &normalized)
                    .await
                {
                    Ok((st, meta_body)) if st.is_success() => {
                        let meta: Option<Value> = serde_json::from_str(&meta_body).ok();
                        let name = meta
                            .as_ref()
                            .and_then(|v| v.get("name"))
                            .and_then(|v| v.as_str())
                            .map(str::to_string);
                        let gid = meta
                            .as_ref()
                            .and_then(|v| v.get("guild_id"))
                            .and_then(|v| v.as_str())
                            .map(str::to_string);
                        (name, gid)
                    }
                    _ => (None, None),
                };

                let mut wait_url = Url::parse(&normalized)
                    .map_err(|_| "Invalid Discord webhook URL".to_string())?;
                if !wait_url.query_pairs().any(|(k, _)| k == "wait") {
                    wait_url.query_pairs_mut().append_pair("wait", "true");
                }
                let payload = build_discord_payload(&event);
                match http
                    .post_webhook_json("discord_webhook_test", wait_url.as_str(), &payload)
                    .await
                {
                    Ok((status, body)) => {
                        let parsed: Option<Value> = serde_json::from_str(&body).ok();
                        let msg_id = parsed
                            .as_ref()
                            .and_then(|v| v.get("id"))
                            .and_then(|v| v.as_str());
                        let channel_id = parsed
                            .as_ref()
                            .and_then(|v| v.get("channel_id"))
                            .and_then(|v| v.as_str());
                        if let Some(mid) = msg_id {
                            let cid = channel_id.unwrap_or("unknown");
                            info!(
                                status = status.as_u16(),
                                webhook_name = webhook_name.as_deref().unwrap_or("unknown"),
                                guild_id = guild_id.as_deref().unwrap_or("unknown"),
                                channel_id = cid,
                                message_id = mid,
                                "Delivered Discord test webhook notification"
                            );
                            let via = webhook_name
                                .as_deref()
                                .map(|n| format!(" via \"{n}\""))
                                .unwrap_or_default();
                            if let Some(gid) = guild_id.as_deref() {
                                Ok(format!(
                                    "Posted{via} to channel {cid} · https://discord.com/channels/{gid}/{cid}/{mid}"
                                ))
                            } else {
                                Ok(format!(
                                    "Posted{via} to Discord channel {cid} (msg {mid})"
                                ))
                            }
                        } else {
                            warn!(
                                status = status.as_u16(),
                                "Discord webhook returned 2xx without creating a message (check webhook URL / channel type)"
                            );
                            Err(format!(
                                "Discord returned HTTP {} without a created message ID — verify the webhook URL belongs to a text channel (not /slack or /github).",
                                status.as_u16()
                            ))
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Discord test webhook failed");
                        Err(e.user_message())
                    }
                }
            }
        }
    }
}

fn build_discord_payload(event: &WebhookEvent) -> Value {
    let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let (content, embed) = match event {
        WebhookEvent::DropClaimed {
            platform,
            game_name,
            campaign_name,
            drop_name,
            reward_image_url,
        } => {
            let mut obj = json!({
                "title": format!("Drop Claimed · {}", platform.display_name()),
                "description": format!("**{}**", drop_name),
                "color": 0x22c55e,
                "fields": [
                    { "name": "Game", "value": game_name, "inline": true },
                    { "name": "Campaign", "value": campaign_name, "inline": true }
                ],
                "footer": { "text": "Dropilot Auto Drops" },
                "timestamp": ts
            });
            if let Some(img) = reward_image_url.as_deref().filter(|s| s.starts_with("https://")) {
                obj["thumbnail"] = json!({ "url": img });
            }
            (
                format!(
                    "Drop claimed on {}: **{}** ({})",
                    platform.display_name(),
                    drop_name,
                    game_name
                ),
                obj,
            )
        }
        WebhookEvent::ActionRequired {
            platform,
            title,
            detail,
            ..
        } => (
            format!("Action required on {}: {}", platform.display_name(), title),
            json!({
                "title": format!("Action Required · {}", platform.display_name()),
                "description": format!("**{}**\n{}", title, detail),
                "color": 0xf59e0b,
                "footer": { "text": "Dropilot Auto Drops" },
                "timestamp": ts
            }),
        ),
        WebhookEvent::CampaignCompleted {
            platform,
            game_name,
            campaign_name,
            drops_count,
        } => (
            format!(
                "Campaign completed on {}: **{}** ({} rewards)",
                platform.display_name(),
                game_name,
                drops_count
            ),
            json!({
                "title": format!("Campaign Completed · {}", platform.display_name()),
                "description": format!("All **{}** rewards claimed for **{}**!", drops_count, game_name),
                "color": 0xe5a93b,
                "fields": [
                    { "name": "Campaign", "value": campaign_name, "inline": false }
                ],
                "footer": { "text": "Dropilot Auto Drops" },
                "timestamp": ts
            }),
        ),
        WebhookEvent::EpicFreeGame {
            title,
            original_price,
            ends_at,
            store_url,
            image_url,
        } => {
            let value_label = if original_price.is_empty() || original_price == "Free" {
                "FREE".to_string()
            } else {
                format!("~~{original_price}~~ → **FREE**")
            };
            let mut obj = json!({
                "title": format!("Epic Free Game · {title}"),
                "url": store_url,
                "description": format!("{value_label}\nClaim before <t:{}:R>.", ends_at.timestamp()),
                "color": 0xe5a93b,
                "footer": { "text": "Dropilot · Epic Free Weekly" },
                "timestamp": ts
            });
            if let Some(img) = image_url.as_deref().filter(|s| s.starts_with("https://")) {
                obj["image"] = json!({ "url": img });
            }
            (
                format!("Free on Epic Games Store: **{title}** — {store_url}"),
                obj,
            )
        }
        WebhookEvent::TestPing => (
            format!(
                "Dropilot v{} webhook connected — test notification succeeded.",
                env!("CARGO_PKG_VERSION")
            ),
            json!({
                "title": "Dropilot Webhook Connected",
                "description": "Test notification succeeded! You will receive drop claims and security alerts here.",
                "color": 0xe5a93b,
                "footer": { "text": format!("Dropilot v{}", env!("CARGO_PKG_VERSION")) },
                "timestamp": ts
            }),
        ),
    };

    json!({
        "username": "Dropilot",
        "content": content,
        "embeds": [embed]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discord_payload_formats_drop_claimed_embed() {
        let payload = build_discord_payload(&WebhookEvent::DropClaimed {
            platform: PlatformId::Twitch,
            game_name: "Rocket League".to_string(),
            campaign_name: "RLCS Opens".to_string(),
            drop_name: "Very Rare Drop".to_string(),
            reward_image_url: Some("https://static-cdn.jtvnw.net/drop.png".to_string()),
        });
        assert_eq!(payload["username"], "Dropilot");
        assert_eq!(payload["embeds"][0]["title"], "Drop Claimed · Twitch");
        assert_eq!(
            payload["embeds"][0]["thumbnail"]["url"],
            "https://static-cdn.jtvnw.net/drop.png"
        );
    }

    #[test]
    fn action_required_respects_cooldown_per_category() {
        let dispatcher = WebhookDispatcher::new();
        let settings = WebhookSettings {
            discord_enabled: true,
            ..Default::default()
        };
        let ev = WebhookEvent::ActionRequired {
            platform: PlatformId::Twitch,
            category: "blocked".to_string(),
            title: "Check required".to_string(),
            detail: "Verify".to_string(),
        };
        assert!(dispatcher.should_send_event(&settings, &ev));
        assert!(!dispatcher.should_send_event(&settings, &ev));
    }

    #[test]
    fn credential_deletion_failures_are_aggregated() {
        let mut attempted = Vec::new();
        let result = WebhookDispatcher::clear_secrets_with(|key| {
            attempted.push(key.to_string());
            if key == WEBHOOK_DISCORD_SECRET_KEY {
                Err(crate::platforms::AppError::Internal(
                    "injected deletion failure".to_string(),
                ))
            } else {
                Ok(())
            }
        });

        let error = result.expect_err("deletion failure must be returned");
        assert!(error.contains(WEBHOOK_DISCORD_SECRET_KEY));
        assert!(error.contains("injected deletion failure"));
        assert_eq!(
            attempted,
            vec![
                WEBHOOK_DISCORD_SECRET_KEY,
                LEGACY_WEBHOOK_TELEGRAM_SECRET_KEY
            ]
        );
    }

    #[test]
    fn normalizes_discord_webhook_domains_and_suffixes() {
        let u1 = WebhookDispatcher::normalize_discord_url(
            "https://discordapp.com/api/webhooks/123456789/abc_TOKEN/slack?thread_id=999",
        )
        .expect("normalized");
        assert_eq!(
            u1,
            "https://discord.com/api/webhooks/123456789/abc_TOKEN?thread_id=999"
        );
        let u2 = WebhookDispatcher::normalize_discord_url(
            "https://ptb.discord.com/api/v10/webhooks/123456789/abc_TOKEN/github",
        )
        .expect("normalized");
        assert_eq!(u2, "https://discord.com/api/webhooks/123456789/abc_TOKEN");
    }
}
