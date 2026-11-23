use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use chrono::{SecondsFormat, Utc};
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
pub const WEBHOOK_TELEGRAM_SECRET_KEY: &str = "webhook_telegram_token";

const ACTION_REQUIRED_COOLDOWN: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebhookChannel {
    Discord,
    Telegram,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookSecretsStatus {
    pub has_discord_url: bool,
    pub has_telegram_token: bool,
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
        let has_telegram_token = SecretStore::load_named_secret(WEBHOOK_TELEGRAM_SECRET_KEY)
            .ok()
            .flatten()
            .is_some();
        WebhookSecretsStatus {
            has_discord_url,
            has_telegram_token,
        }
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

    pub fn save_telegram_token(raw_token: &str) -> Result<(), String> {
        let trimmed = raw_token.trim();
        if trimmed.is_empty() {
            SecretStore::delete_named_secret(WEBHOOK_TELEGRAM_SECRET_KEY)
                .map_err(|e| e.user_message())?;
            return Ok(());
        }
        if trimmed.contains(char::is_whitespace) || !trimmed.contains(':') {
            return Err(
                "Telegram bot token must look like 123456789:ABCdef... (from @BotFather)"
                    .to_string(),
            );
        }
        let probe = format!("https://api.telegram.org/bot{trimmed}/sendMessage");
        let parsed =
            Url::parse(&probe).map_err(|_| "Invalid Telegram bot token characters".to_string())?;
        validate_url(&parsed, AllowlistKind::Webhook)
            .map_err(|_| "Invalid Telegram bot token format".to_string())?;

        SecretStore::save_named_secret(WEBHOOK_TELEGRAM_SECRET_KEY, trimmed)
            .map_err(|e| e.user_message())
    }

    pub fn clear_all_secrets() {
        let _ = SecretStore::delete_named_secret(WEBHOOK_DISCORD_SECRET_KEY);
        let _ = SecretStore::delete_named_secret(WEBHOOK_TELEGRAM_SECRET_KEY);
    }

    fn should_send_event(&self, settings: &WebhookSettings, event: &WebhookEvent) -> bool {
        if !settings.discord_enabled && !settings.telegram_enabled {
            return false;
        }
        match event {
            WebhookEvent::DropClaimed { .. } => settings.on_drop_claimed,
            WebhookEvent::CampaignCompleted { .. } => settings.on_campaign_completed,
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

            if settings.telegram_enabled && !settings.telegram_chat_id.trim().is_empty() {
                if let Ok(Some(tok_secret)) =
                    SecretStore::load_named_secret(WEBHOOK_TELEGRAM_SECRET_KEY)
                {
                    let url = format!(
                        "https://api.telegram.org/bot{}/sendMessage",
                        tok_secret.expose()
                    );
                    let payload =
                        build_telegram_payload(settings.telegram_chat_id.trim(), &event);
                    match http.post_webhook_json("telegram_webhook", &url, &payload).await {
                        Ok(_) => info!("Delivered Telegram webhook notification"),
                        Err(err) => warn!(error = %err, "Telegram webhook delivery failed"),
                    }
                }
            }
        });
    }

    /// Sends a synchronous test ping to a specific channel so the Settings UI gets immediate feedback.
    pub async fn send_test(
        http: &AllowlistedClient,
        channel: WebhookChannel,
        telegram_chat_id: &str,
    ) -> Result<(), String> {
        let event = WebhookEvent::TestPing;
        match channel {
            WebhookChannel::Discord => {
                let url = SecretStore::load_named_secret(WEBHOOK_DISCORD_SECRET_KEY)
                    .map_err(|e| e.user_message())?
                    .ok_or_else(|| "Save a Discord webhook URL first".to_string())?;
                let target_url = Self::normalize_discord_url(url.expose())?;
                let payload = build_discord_payload(&event);
                match http
                    .post_webhook_json("discord_webhook_test", &target_url, &payload)
                    .await
                {
                    Ok(_) => {
                        info!("Delivered Discord test webhook notification");
                        Ok(())
                    }
                    Err(e) => {
                        warn!(error = %e, "Discord test webhook failed");
                        Err(e.user_message())
                    }
                }
            }
            WebhookChannel::Telegram => {
                let chat_id = telegram_chat_id.trim();
                if chat_id.is_empty() {
                    return Err("Enter your Telegram Chat ID first".to_string());
                }
                let tok = SecretStore::load_named_secret(WEBHOOK_TELEGRAM_SECRET_KEY)
                    .map_err(|e| e.user_message())?
                    .ok_or_else(|| "Save a Telegram bot token first".to_string())?;
                let url = format!("https://api.telegram.org/bot{}/sendMessage", tok.expose());
                let payload = build_telegram_payload(chat_id, &event);
                match http
                    .post_webhook_json("telegram_webhook_test", &url, &payload)
                    .await
                {
                    Ok(_) => {
                        info!("Delivered Telegram test webhook notification");
                        Ok(())
                    }
                    Err(e) => {
                        warn!(error = %e, "Telegram test webhook failed");
                        Err(e.user_message())
                    }
                }
            }
        }
    }
}

fn build_discord_payload(event: &WebhookEvent) -> Value {
    let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let embed = match event {
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
            obj
        }
        WebhookEvent::ActionRequired {
            platform,
            title,
            detail,
            ..
        } => json!({
            "title": format!("Action Required · {}", platform.display_name()),
            "description": format!("**{}**\n{}", title, detail),
            "color": 0xf59e0b,
            "footer": { "text": "Dropilot Auto Drops" },
            "timestamp": ts
        }),
        WebhookEvent::CampaignCompleted {
            platform,
            game_name,
            campaign_name,
            drops_count,
        } => json!({
            "title": format!("Campaign Completed · {}", platform.display_name()),
            "description": format!("All **{}** rewards claimed for **{}**!", drops_count, game_name),
            "color": 0x06b6d4,
            "fields": [
                { "name": "Campaign", "value": campaign_name, "inline": false }
            ],
            "footer": { "text": "Dropilot Auto Drops" },
            "timestamp": ts
        }),
        WebhookEvent::TestPing => json!({
            "title": "Dropilot Webhook Connected",
            "description": "Test notification succeeded! You will receive drop claims and security alerts here.",
            "color": 0x06b6d4,
            "footer": { "text": format!("Dropilot v{}", env!("CARGO_PKG_VERSION")) },
            "timestamp": ts
        }),
    };

    json!({
        "username": "Dropilot",
        "embeds": [embed]
    })
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn build_telegram_payload(chat_id: &str, event: &WebhookEvent) -> Value {
    let text = match event {
        WebhookEvent::DropClaimed {
            platform,
            game_name,
            campaign_name,
            drop_name,
            ..
        } => format!(
            "<b>Drop Claimed ({})</b>\nReward: <b>{}</b>\nGame: {}\nCampaign: {}",
            escape_html(platform.display_name()),
            escape_html(drop_name),
            escape_html(game_name),
            escape_html(campaign_name),
        ),
        WebhookEvent::ActionRequired {
            platform,
            title,
            detail,
            ..
        } => format!(
            "<b>Action Required ({})</b>\n{}\n{}",
            escape_html(platform.display_name()),
            escape_html(title),
            escape_html(detail),
        ),
        WebhookEvent::CampaignCompleted {
            platform,
            game_name,
            campaign_name,
            drops_count,
        } => format!(
            "<b>Campaign Completed ({})</b>\nGame: <b>{}</b>\nCampaign: {}\nRewards claimed: {}",
            escape_html(platform.display_name()),
            escape_html(game_name),
            escape_html(campaign_name),
            drops_count,
        ),
        WebhookEvent::TestPing => format!(
            "<b>Dropilot Connected</b>\nTest notification succeeded (v{}). Drop claims and action alerts will arrive here.",
            env!("CARGO_PKG_VERSION")
        ),
    };

    json!({
        "chat_id": chat_id,
        "text": text,
        "parse_mode": "HTML",
        "disable_web_page_preview": true
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
    fn telegram_payload_escapes_html_entities() {
        let payload = build_telegram_payload(
            "12345",
            &WebhookEvent::ActionRequired {
                platform: PlatformId::Twitch,
                category: "manual_claim".to_string(),
                title: "Claim <needs> you & verify".to_string(),
                detail: "Open inventory".to_string(),
            },
        );
        let text = payload["text"].as_str().unwrap_or("");
        assert!(text.contains("Claim &lt;needs&gt; you &amp; verify"));
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
}
