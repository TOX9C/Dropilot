use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock as StdRwLock};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::net::allowlist::REMOTE_CONFIG_URL;
use crate::net::client::AllowlistedClient;
use crate::settings::appdata_dir;

const CACHE_TTL_SECS: i64 = 3600;
const MAX_BANNER_LEN: usize = 280;

fn default_true() -> bool {
    true
}

fn default_schema_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub twitch_web_client_id: Option<String>,
    #[serde(default)]
    pub twitch_gql_hashes: HashMap<String, String>,
    #[serde(default)]
    pub twitch_claim_selectors: Vec<String>,
    #[serde(default)]
    pub maintenance_banner: Option<String>,
    #[serde(default = "default_true")]
    pub twitch_enabled: bool,
    #[serde(default = "default_true")]
    pub kick_enabled: bool,
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            updated_at: None,
            twitch_web_client_id: None,
            twitch_gql_hashes: HashMap::new(),
            twitch_claim_selectors: Vec::new(),
            maintenance_banner: None,
            twitch_enabled: true,
            kick_enabled: true,
        }
    }
}

impl RemoteConfig {
    pub fn sanitize_and_validate(mut self) -> Result<Self, String> {
        if self.schema_version == 0 || self.schema_version > 2 {
            return Err(format!(
                "Unsupported remote config schema version {}",
                self.schema_version
            ));
        }

        if let Some(ref cid) = self.twitch_web_client_id {
            let trimmed = cid.trim();
            if trimmed.is_empty() {
                self.twitch_web_client_id = None;
            } else if (10..=64).contains(&trimmed.len())
                && trimmed.chars().all(|c| c.is_ascii_alphanumeric())
            {
                self.twitch_web_client_id = Some(trimmed.to_string());
            } else {
                warn!("Ignoring invalid remote twitchWebClientId");
                self.twitch_web_client_id = None;
            }
        }

        self.twitch_gql_hashes.retain(|op, hash| {
            let valid_op = !op.trim().is_empty()
                && op.len() <= 80
                && op
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_');
            let valid_hash = hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit());
            valid_op && valid_hash
        });
        for val in self.twitch_gql_hashes.values_mut() {
            *val = val.to_ascii_lowercase();
        }

        self.twitch_claim_selectors.retain(|s| {
            let t = s.trim();
            !t.is_empty() && t.len() <= 200
        });
        self.twitch_claim_selectors.truncate(16);

        self.maintenance_banner = self.maintenance_banner.and_then(|b| {
            let trimmed = b.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.chars().take(MAX_BANNER_LEN).collect())
            }
        });

        Ok(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedRemoteConfig {
    fetched_at: DateTime<Utc>,
    config: RemoteConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RemoteConfigSource {
    Remote,
    Cache,
    Bundled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteConfigStatus {
    pub source: RemoteConfigSource,
    pub fetched_at: Option<DateTime<Utc>>,
    pub updated_at: Option<String>,
    pub overridden_hashes: usize,
    pub maintenance_banner: Option<String>,
    pub twitch_enabled: bool,
    pub kick_enabled: bool,
    pub last_error: Option<String>,
}

impl Default for RemoteConfigStatus {
    fn default() -> Self {
        Self {
            source: RemoteConfigSource::Bundled,
            fetched_at: None,
            updated_at: None,
            overridden_hashes: 0,
            maintenance_banner: None,
            twitch_enabled: true,
            kick_enabled: true,
            last_error: None,
        }
    }
}

pub fn offline_refresh_status(mut status: RemoteConfigStatus) -> RemoteConfigStatus {
    status.last_error = Some("Remote config sync is disabled in offline mock mode.".to_string());
    status
}

static ACTIVE_CONFIG: OnceLock<StdRwLock<RemoteConfig>> = OnceLock::new();

fn active_lock() -> &'static StdRwLock<RemoteConfig> {
    ACTIVE_CONFIG.get_or_init(|| StdRwLock::new(RemoteConfig::default()))
}

fn publish_active_config(cfg: &RemoteConfig) {
    if let Ok(mut guard) = active_lock().write() {
        *guard = cfg.clone();
    }
}

/// Resolves a Twitch GQL persisted query hash from the active remote config,
/// falling back to the bundled constant hash when not overridden.
pub fn resolve_gql_hash(operation_name: &str, bundled_default: &str) -> String {
    if let Ok(guard) = active_lock().read() {
        if let Some(override_hash) = guard.twitch_gql_hashes.get(operation_name) {
            return override_hash.clone();
        }
    }
    bundled_default.to_string()
}

/// Resolves the Twitch public web Client-ID from remote config or bundled default.
pub fn resolve_twitch_client_id(bundled_default: &str) -> String {
    if let Ok(guard) = active_lock().read() {
        if let Some(ref cid) = guard.twitch_web_client_id {
            return cid.clone();
        }
    }
    bundled_default.to_string()
}

/// Resolves extra CSS selectors for Twitch's Claim Now button from remote config.
pub fn resolve_claim_selectors() -> Vec<String> {
    if let Ok(guard) = active_lock().read() {
        return guard.twitch_claim_selectors.clone();
    }
    Vec::new()
}

pub fn cache_path() -> PathBuf {
    appdata_dir().join("remote_config.json")
}

pub struct RemoteConfigManager {
    config: RwLock<RemoteConfig>,
    status: RwLock<RemoteConfigStatus>,
    cache_path: PathBuf,
}

impl RemoteConfigManager {
    pub fn load_initial() -> Self {
        let path = cache_path();
        if let Some(cached) = load_cache_from_path(&path) {
            publish_active_config(&cached.config);
            let status = status_from_config(
                RemoteConfigSource::Cache,
                Some(cached.fetched_at),
                &cached.config,
                None,
            );
            return Self {
                config: RwLock::new(cached.config),
                status: RwLock::new(status),
                cache_path: path,
            };
        }

        let bundled = RemoteConfig::default();
        publish_active_config(&bundled);
        Self {
            config: RwLock::new(bundled),
            status: RwLock::new(RemoteConfigStatus::default()),
            cache_path: path,
        }
    }

    pub async fn status(&self) -> RemoteConfigStatus {
        self.status.read().await.clone()
    }

    pub async fn current(&self) -> RemoteConfig {
        self.config.read().await.clone()
    }

    pub async fn reset_to_bundled(&self) {
        let bundled = RemoteConfig::default();
        publish_active_config(&bundled);
        *self.config.write().await = bundled;
        *self.status.write().await = RemoteConfigStatus::default();
        let _ = fs::remove_file(&self.cache_path);
    }

    pub async fn refresh_if_stale(
        &self,
        http: &AllowlistedClient,
        should_refresh: impl FnOnce() -> bool,
    ) -> RemoteConfigStatus {
        let is_fresh = {
            let st = self.status.read().await;
            st.fetched_at
                .map(|t| Utc::now() - t < ChronoDuration::seconds(CACHE_TTL_SECS))
                .unwrap_or(false)
        };
        if is_fresh || !should_refresh() {
            return self.status().await;
        }
        self.refresh_now(http).await
    }

    pub async fn refresh_now(&self, http: &AllowlistedClient) -> RemoteConfigStatus {
        match http
            .fetch_remote_config_json::<RemoteConfig>(REMOTE_CONFIG_URL)
            .await
        {
            Ok(raw) => match raw.sanitize_and_validate() {
                Ok(valid) => {
                    let now = Utc::now();
                    let cached = CachedRemoteConfig {
                        fetched_at: now,
                        config: valid.clone(),
                    };
                    if let Err(err) = save_cache_to_path(&cache_path(), &cached) {
                        warn!(error = %err, "Failed caching remote config");
                    }
                    publish_active_config(&valid);
                    let next_status = status_from_config(
                        RemoteConfigSource::Remote,
                        Some(now),
                        &valid,
                        None,
                    );
                    info!(
                        hashes = valid.twitch_gql_hashes.len(),
                        updated_at = ?valid.updated_at,
                        "Synced remote hotfix config"
                    );
                    *self.config.write().await = valid;
                    *self.status.write().await = next_status.clone();
                    next_status
                }
                Err(reason) => {
                    warn!(reason = %reason, "Rejected invalid remote config payload");
                    let mut st = self.status.write().await;
                    st.last_error = Some(reason);
                    st.clone()
                }
            },
            Err(err) => {
                let msg = err.user_message();
                warn!(error = %msg, "Could not fetch remote config; continuing with cached/bundled defaults");
                let mut st = self.status.write().await;
                st.last_error = Some(msg);
                st.clone()
            }
        }
    }
}

fn status_from_config(
    source: RemoteConfigSource,
    fetched_at: Option<DateTime<Utc>>,
    cfg: &RemoteConfig,
    last_error: Option<String>,
) -> RemoteConfigStatus {
    RemoteConfigStatus {
        source,
        fetched_at,
        updated_at: cfg.updated_at.clone(),
        overridden_hashes: cfg.twitch_gql_hashes.len(),
        maintenance_banner: cfg.maintenance_banner.clone(),
        twitch_enabled: cfg.twitch_enabled,
        kick_enabled: cfg.kick_enabled,
        last_error,
    }
}

#[cfg(test)]
pub(crate) fn cached_manager_for_test(cache_path: PathBuf) -> RemoteConfigManager {
    let fetched_at = DateTime::parse_from_rfc3339("2026-01-02T03:04:05Z")
        .expect("valid test timestamp")
        .with_timezone(&Utc);
    let config = RemoteConfig {
        updated_at: Some("cached-update".to_string()),
        maintenance_banner: Some("cached banner".to_string()),
        twitch_enabled: false,
        ..Default::default()
    };
    let status = status_from_config(RemoteConfigSource::Cache, Some(fetched_at), &config, None);
    RemoteConfigManager {
        config: RwLock::new(config),
        status: RwLock::new(status),
        cache_path,
    }
}

fn load_cache_from_path(path: &Path) -> Option<CachedRemoteConfig> {
    let raw = fs::read_to_string(path).ok()?;
    let parsed: CachedRemoteConfig = serde_json::from_str(&raw).ok()?;
    let valid = parsed.config.sanitize_and_validate().ok()?;
    Some(CachedRemoteConfig {
        fetched_at: parsed.fetched_at,
        config: valid,
    })
}

fn save_cache_to_path(path: &Path, cached: &CachedRemoteConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed creating cache directory: {e}"))?;
    }
    let json = serde_json::to_string_pretty(cached)
        .map_err(|e| format!("Failed serializing remote config cache: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json.as_bytes())
        .map_err(|e| format!("Failed writing temp remote config cache: {e}"))?;
    #[cfg(windows)]
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(&tmp, path)
        .map_err(|e| format!("Failed committing remote config cache: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_hashes_and_drops_malformed_entries() {
        let mut cfg = RemoteConfig::default();
        cfg.twitch_gql_hashes.insert(
            "ViewerDropsDashboard".to_string(),
            "5A4DA2AB3D5B47C9F9CE864E727B2CB346AF1E3EA8B897FE8F704A97FF017619".to_string(),
        );
        cfg.twitch_gql_hashes
            .insert("BadHash".to_string(), "not-a-sha256".to_string());
        cfg.twitch_gql_hashes
            .insert("bad op!".to_string(), "a".repeat(64));
        cfg.twitch_web_client_id = Some("kimne78kx3ncx6brgo4mv6wki5h1ko".to_string());

        let sanitized = cfg.sanitize_and_validate().expect("valid config");
        assert_eq!(sanitized.twitch_gql_hashes.len(), 1);
        assert_eq!(
            sanitized.twitch_gql_hashes.get("ViewerDropsDashboard").map(String::as_str),
            Some("5a4da2ab3d5b47c9f9ce864e727b2cb346af1e3ea8b897fe8f704a97ff017619")
        );
        assert_eq!(
            sanitized.twitch_web_client_id.as_deref(),
            Some("kimne78kx3ncx6brgo4mv6wki5h1ko")
        );
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let cfg = RemoteConfig {
            schema_version: 99,
            ..Default::default()
        };
        assert!(cfg.sanitize_and_validate().is_err());
    }

    #[tokio::test]
    async fn stale_refresh_honors_the_final_offline_guard() {
        let manager = RemoteConfigManager {
            config: RwLock::new(RemoteConfig::default()),
            status: RwLock::new(RemoteConfigStatus::default()),
            cache_path: PathBuf::new(),
        };
        let http = AllowlistedClient::new().expect("HTTP client");
        let checked = std::cell::Cell::new(false);
        let status = manager
            .refresh_if_stale(&http, || {
                checked.set(true);
                false
            })
            .await;
        assert!(checked.get());
        assert_eq!(status.source, RemoteConfigSource::Bundled);
        assert!(status.last_error.is_none());
    }

    #[test]
    fn offline_refresh_preserves_cached_status_and_reports_the_skip() {
        let fetched_at = Utc::now();
        let cached = RemoteConfigStatus {
            source: RemoteConfigSource::Cache,
            fetched_at: Some(fetched_at),
            overridden_hashes: 7,
            ..Default::default()
        };
        let skipped = offline_refresh_status(cached);
        assert_eq!(skipped.source, RemoteConfigSource::Cache);
        assert_eq!(skipped.fetched_at, Some(fetched_at));
        assert_eq!(skipped.overridden_hashes, 7);
        assert_eq!(
            skipped.last_error.as_deref(),
            Some("Remote config sync is disabled in offline mock mode.")
        );
    }
}
