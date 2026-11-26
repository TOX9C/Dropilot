use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::AppHandle;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::net::allowlist::EPIC_FREE_GAMES_URL;
use crate::net::client::AllowlistedClient;
use crate::settings::appdata_dir;
use crate::state::AppState;
use crate::webhooks::WebhookEvent;

const EPIC_CACHE_SCHEMA_VERSION: u32 = 1;
const EPIC_POLL_INTERVAL_SECS: u64 = 30 * 60;
const EPIC_CACHE_TTL_SECS: i64 = 30 * 60;
pub const EPIC_FALLBACK_FREE_GAMES_URL: &str = "https://store.epicgames.com/en-US/free-games";
pub const EPIC_LAUNCHER_FREE_GAMES_URL: &str = "com.epicgames.launcher://store/free-games";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EpicOfferStatus {
    Active,
    Upcoming,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpicFreeGame {
    pub id: String,
    pub namespace: String,
    pub title: String,
    pub description: String,
    pub seller: String,
    pub original_price: String,
    pub status: EpicOfferStatus,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub store_url: String,
    #[serde(default)]
    pub launcher_url: Option<String>,
    pub box_art_url: Option<String>,
    pub banner_url: Option<String>,
    #[serde(default)]
    pub claimed: bool,
    #[serde(default)]
    pub claimed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EpicFeedSnapshot {
    pub games: Vec<EpicFreeGame>,
    pub fetched_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub source: String,
}

impl Default for EpicFeedSnapshot {
    fn default() -> Self {
        Self {
            games: Vec::new(),
            fetched_at: None,
            last_error: None,
            source: "empty".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EpicDiskCache {
    #[serde(default = "default_schema")]
    schema_version: u32,
    #[serde(default)]
    fetched_at: Option<DateTime<Utc>>,
    #[serde(default)]
    games: Vec<EpicFreeGame>,
    #[serde(default)]
    claimed_ids: HashMap<String, DateTime<Utc>>,
    #[serde(default)]
    notified_keys: HashSet<String>,
}

fn default_schema() -> u32 {
    EPIC_CACHE_SCHEMA_VERSION
}

impl Default for EpicDiskCache {
    fn default() -> Self {
        Self {
            schema_version: EPIC_CACHE_SCHEMA_VERSION,
            fetched_at: None,
            games: Vec::new(),
            claimed_ids: HashMap::new(),
            notified_keys: HashSet::new(),
        }
    }
}

pub fn epic_cache_path() -> PathBuf {
    appdata_dir().join("epic_cache.json")
}

struct EpicInner {
    cache: EpicDiskCache,
    last_error: Option<String>,
    source: String,
}

pub struct EpicTracker {
    inner: RwLock<EpicInner>,
}

impl EpicTracker {
    pub fn load_initial() -> Self {
        let cache = load_cache_from_path(&epic_cache_path());
        let source = if cache.games.is_empty() {
            "empty".to_string()
        } else {
            "cache".to_string()
        };
        Self {
            inner: RwLock::new(EpicInner {
                cache,
                last_error: None,
                source,
            }),
        }
    }

    pub async fn snapshot(&self) -> EpicFeedSnapshot {
        let guard = self.inner.read().await;
        let now = Utc::now();
        let mut games: Vec<EpicFreeGame> = guard
            .cache
            .games
            .iter()
            .filter(|g| g.ends_at > now)
            .cloned()
            .map(|mut g| {
                let status = if g.starts_at <= now {
                    EpicOfferStatus::Active
                } else {
                    EpicOfferStatus::Upcoming
                };
                g.status = status;
                if g.launcher_url.is_none() {
                    g.launcher_url = Some(launcher_url_from_store_url(&g.store_url));
                }
                if let Some(&claimed_at) = guard.cache.claimed_ids.get(&g.id) {
                    g.claimed = true;
                    g.claimed_at = Some(claimed_at);
                } else {
                    g.claimed = false;
                    g.claimed_at = None;
                }
                g
            })
            .collect();
        sort_epic_games(&mut games);

        EpicFeedSnapshot {
            games,
            fetched_at: guard.cache.fetched_at,
            last_error: guard.last_error.clone(),
            source: guard.source.clone(),
        }
    }

    pub async fn find_game(&self, game_id: &str) -> Option<EpicFreeGame> {
        let snap = self.snapshot().await;
        snap.games.into_iter().find(|g| g.id == game_id)
    }

    pub async fn is_stale(&self) -> bool {
        let guard = self.inner.read().await;
        match guard.cache.fetched_at {
            None => true,
            Some(ts) => (Utc::now() - ts).num_seconds() >= EPIC_CACHE_TTL_SECS,
        }
    }

    pub async fn refresh_now(
        &self,
        http: &AllowlistedClient,
    ) -> (EpicFeedSnapshot, Vec<EpicFreeGame>) {
        match http.fetch_epic_free_games_json(EPIC_FREE_GAMES_URL).await {
            Ok(payload) => {
                let now = Utc::now();
                let mut parsed = parse_epic_free_games(&payload, now);
                let mut newly_live = Vec::new();

                {
                    let mut guard = self.inner.write().await;
                    let is_first_seed = guard.cache.fetched_at.is_none()
                        && guard.cache.notified_keys.is_empty();

                    for game in &mut parsed {
                        if let Some(&claimed_at) = guard.cache.claimed_ids.get(&game.id) {
                            game.claimed = true;
                            game.claimed_at = Some(claimed_at);
                        }
                        if game.status == EpicOfferStatus::Active {
                            let notif_key =
                                format!("{}:{}", game.id, game.starts_at.timestamp());
                            if guard.cache.notified_keys.insert(notif_key) && !is_first_seed {
                                newly_live.push(game.clone());
                            }
                        }
                    }

                    // Keep notified_keys bounded.
                    if guard.cache.notified_keys.len() > 200 {
                        let active_ids: HashSet<String> = parsed
                            .iter()
                            .map(|g| format!("{}:{}", g.id, g.starts_at.timestamp()))
                            .collect();
                        guard.cache.notified_keys = active_ids;
                    }

                    guard.cache.games = parsed;
                    guard.cache.fetched_at = Some(now);
                    guard.last_error = None;
                    guard.source = "live".to_string();

                    if let Err(err) = save_cache_to_path(&epic_cache_path(), &guard.cache) {
                        warn!(error = %err, "Failed persisting Epic free games cache");
                    }
                }

                let snap = self.snapshot().await;
                info!(
                    count = snap.games.len(),
                    newly_live = newly_live.len(),
                    "Synced Epic Games weekly free catalog"
                );
                (snap, newly_live)
            }
            Err(err) => {
                let msg = err.user_message();
                warn!(error = %msg, "Epic Games free catalog fetch failed");
                {
                    let mut guard = self.inner.write().await;
                    guard.last_error = Some(msg);
                }
                (self.snapshot().await, Vec::new())
            }
        }
    }

    pub async fn set_claimed(&self, game_id: &str, claimed: bool) -> Option<EpicFreeGame> {
        let now = Utc::now();
        {
            let mut guard = self.inner.write().await;
            if claimed {
                guard.cache.claimed_ids.insert(game_id.to_string(), now);
            } else {
                guard.cache.claimed_ids.remove(game_id);
            }
            if let Some(g) = guard.cache.games.iter_mut().find(|g| g.id == game_id) {
                g.claimed = claimed;
                g.claimed_at = if claimed { Some(now) } else { None };
            }
            if let Err(err) = save_cache_to_path(&epic_cache_path(), &guard.cache) {
                warn!(error = %err, "Failed saving Epic claimed status");
            }
        }
        self.find_game(game_id).await
    }

    pub async fn clear_cache(&self) {
        {
            let mut guard = self.inner.write().await;
            guard.cache = EpicDiskCache::default();
            guard.last_error = None;
            guard.source = "empty".to_string();
        }
        let _ = fs::remove_file(epic_cache_path());
    }
}

pub fn spawn_epic_poll_loop(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        // Brief startup delay so Twitch/Kick miners initialize first.
        tokio::time::sleep(Duration::from_secs(4)).await;
        loop {
            if state.epic.is_stale().await {
                let (_snap, newly_live) = state.epic.refresh_now(&state.http).await;
                notify_newly_live_games(&app, &state, &newly_live).await;
            }
            tokio::time::sleep(Duration::from_secs(EPIC_POLL_INTERVAL_SECS)).await;
        }
    });
}

pub async fn notify_newly_live_games(
    app: &AppHandle,
    state: &Arc<AppState>,
    newly_live: &[EpicFreeGame],
) {
    if newly_live.is_empty() {
        return;
    }
    let settings = state.settings.read().await.clone();
    for game in newly_live {
        if settings.notifications {
            let body = if game.original_price.is_empty() || game.original_price == "Free" {
                format!("{} is free to claim on Epic Games Store!", game.title)
            } else {
                format!(
                    "{} ({} value) is free to claim on Epic Games Store!",
                    game.title, game.original_price
                )
            };
            crate::notifications::send_desktop_toast(app, "Epic Free Game Live", &body);
        }
        state
            .dispatch_webhook(WebhookEvent::EpicFreeGame {
                title: game.title.clone(),
                original_price: game.original_price.clone(),
                ends_at: game.ends_at,
                store_url: game.store_url.clone(),
                image_url: game.banner_url.clone().or_else(|| game.box_art_url.clone()),
            })
            .await;
    }
}

pub fn parse_epic_free_games(payload: &Value, now: DateTime<Utc>) -> Vec<EpicFreeGame> {
    let Some(elements) = payload
        .pointer("/data/Catalog/searchStore/elements")
        .and_then(|v| v.as_array())
    else {
        return Vec::new();
    };

    let mut out = Vec::new();
    let mut seen_ids = HashSet::new();

    for el in elements {
        let Some(id) = el.get("id").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) else {
            continue;
        };
        if !seen_ids.insert(id.to_string()) {
            continue;
        }

        let Some(promos) = el.get("promotions").filter(|v| !v.is_null()) else {
            continue;
        };

        let active_window = find_free_promo_window(promos.get("promotionalOffers"), now, true);
        let upcoming_window =
            find_free_promo_window(promos.get("upcomingPromotionalOffers"), now, false);

        let (status, starts_at, ends_at) = if let Some((s, e)) = active_window {
            (EpicOfferStatus::Active, s, e)
        } else if let Some((s, e)) = upcoming_window {
            (EpicOfferStatus::Upcoming, s, e)
        } else {
            continue;
        };

        let title = el
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Epic Free Game")
            .trim()
            .to_string();
        let namespace = el
            .get("namespace")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let description = el
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        let seller = el
            .pointer("/seller/name")
            .and_then(|v| v.as_str())
            .unwrap_or("Epic Games Store")
            .trim()
            .to_string();

        let raw_orig_price = el
            .pointer("/price/totalPrice/fmtPrice/originalPrice")
            .and_then(|v| v.as_str())
            .unwrap_or("0")
            .trim();
        let original_price = if raw_orig_price.is_empty() || raw_orig_price == "0" {
            "Free".to_string()
        } else {
            raw_orig_price.to_string()
        };

        let (store_url, launcher_url) = resolve_epic_urls(el);
        let (box_art_url, banner_url) = extract_epic_images(el);

        out.push(EpicFreeGame {
            id: id.to_string(),
            namespace,
            title,
            description,
            seller,
            original_price,
            status,
            starts_at,
            ends_at,
            store_url,
            launcher_url: Some(launcher_url),
            box_art_url,
            banner_url,
            claimed: false,
            claimed_at: None,
        });
    }

    sort_epic_games(&mut out);
    out
}

fn find_free_promo_window(
    groups_val: Option<&Value>,
    now: DateTime<Utc>,
    require_active: bool,
) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let groups = groups_val?.as_array()?;
    for group in groups {
        let Some(offers) = group.get("promotionalOffers").and_then(|v| v.as_array()) else {
            continue;
        };
        for offer in offers {
            let discount_pct = offer
                .pointer("/discountSetting/discountPercentage")
                .and_then(|v| v.as_u64());
            // Epic represents 100% off (free) as discountPercentage == 0.
            if discount_pct != Some(0) {
                continue;
            }
            let Some(start) = offer
                .get("startDate")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
            else {
                continue;
            };
            let Some(end) = offer
                .get("endDate")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
            else {
                continue;
            };
            if end <= now {
                continue;
            }
            if require_active {
                if start <= now && now < end {
                    return Some((start, end));
                }
            } else if start > now {
                return Some((start, end));
            }
        }
    }
    None
}

pub fn launcher_url_for_game(game: &EpicFreeGame) -> String {
    game.launcher_url
        .clone()
        .filter(|s| s.starts_with("com.epicgames.launcher://"))
        .unwrap_or_else(|| launcher_url_from_store_url(&game.store_url))
}

pub fn launcher_url_from_store_url(store_url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(store_url) {
        let segments: Vec<&str> = parsed
            .path_segments()
            .map(|iter| iter.filter(|s| !s.is_empty()).collect())
            .unwrap_or_default();
        for pair in segments.windows(2) {
            if (pair[0] == "p" || pair[0] == "bundles")
                && !pair[1].is_empty()
                && !pair[1].contains("..")
                && pair[1]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            {
                return format!("com.epicgames.launcher://store/{}/{}", pair[0], pair[1]);
            }
        }
    }
    EPIC_LAUNCHER_FREE_GAMES_URL.to_string()
}

fn resolve_epic_urls(el: &Value) -> (String, String) {
    let is_bundle = el
        .get("offerType")
        .and_then(|v| v.as_str())
        .map(|s| s.eq_ignore_ascii_case("BUNDLE"))
        .unwrap_or(false);
    let section = if is_bundle { "bundles" } else { "p" };

    let mapping_slug = |ptr: &str| -> Option<String> {
        let arr = el.pointer(ptr)?.as_array()?;
        let preferred = arr
            .iter()
            .find(|m| m.get("pageType").and_then(|v| v.as_str()) == Some("productHome"))
            .or_else(|| arr.first());
        preferred
            .and_then(|m| m.get("pageSlug"))
            .and_then(|v| v.as_str())
            .map(clean_slug)
            .filter(|s| !s.is_empty())
    };

    let slug = mapping_slug("/catalogNs/mappings")
        .or_else(|| mapping_slug("/offerMappings"))
        .or_else(|| {
            el.get("productSlug")
                .and_then(|v| v.as_str())
                .map(clean_slug)
                .filter(|s| !s.is_empty())
        })
        .or_else(|| {
            el.get("urlSlug")
                .and_then(|v| v.as_str())
                .map(clean_slug)
                .filter(|s| !s.is_empty() && !looks_like_hex_guid(s))
        });

    match slug {
        Some(s) => (
            format!("https://store.epicgames.com/en-US/{section}/{s}"),
            format!("com.epicgames.launcher://store/{section}/{s}"),
        ),
        None => (
            EPIC_FALLBACK_FREE_GAMES_URL.to_string(),
            EPIC_LAUNCHER_FREE_GAMES_URL.to_string(),
        ),
    }
}

fn clean_slug(raw: &str) -> String {
    let trimmed = raw.trim().trim_matches('/');
    let without_home = trimmed.strip_suffix("/home").unwrap_or(trimmed);
    without_home.trim_matches('/').to_string()
}

fn looks_like_hex_guid(s: &str) -> bool {
    s.len() == 32 && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn normalize_epic_image_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix("https://") {
        if rest.is_empty() {
            return None;
        }
        return Some(trimmed.to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("http://") {
        let host = rest
            .split(['/', '?', '#', ':'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if host == "epicgames.com"
            || host.ends_with(".epicgames.com")
            || host == "unrealengine.com"
            || host.ends_with(".unrealengine.com")
        {
            return Some(format!("https://{rest}"));
        }
    }
    None
}

fn extract_epic_images(el: &Value) -> (Option<String>, Option<String>) {
    let Some(images) = el.get("keyImages").and_then(|v| v.as_array()) else {
        return (None, None);
    };

    let pick = |types: &[&str]| -> Option<String> {
        for wanted in types {
            for img in images {
                let kind = img
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .trim();
                if !kind.eq_ignore_ascii_case(wanted) {
                    continue;
                }
                if let Some(url) = img
                    .get("url")
                    .and_then(|v| v.as_str())
                    .and_then(normalize_epic_image_url)
                {
                    return Some(url);
                }
            }
        }
        None
    };

    let first_https = || -> Option<String> {
        images.iter().find_map(|img| {
            img.get("url")
                .and_then(|v| v.as_str())
                .and_then(normalize_epic_image_url)
        })
    };

    let tall = pick(&[
        "OfferImageTall",
        "DieselStoreFrontTall",
        "CodeRedemption_340x440",
        "Thumbnail",
        "VaultClosed",
        "OfferImageWide",
        "DieselStoreFrontWide",
    ])
    .or_else(first_https);
    let wide = pick(&[
        "OfferImageWide",
        "DieselStoreFrontWide",
        "featuredMedia",
        "GalleryImage",
        "OfferImageTall",
        "Thumbnail",
    ])
    .or_else(first_https);
    (tall, wide)
}

fn sort_epic_games(games: &mut [EpicFreeGame]) {
    games.sort_by(|a, b| {
        let rank = |s: EpicOfferStatus| match s {
            EpicOfferStatus::Active => 0u8,
            EpicOfferStatus::Upcoming => 1u8,
        };
        rank(a.status)
            .cmp(&rank(b.status))
            .then_with(|| match a.status {
                EpicOfferStatus::Active => a.ends_at.cmp(&b.ends_at),
                EpicOfferStatus::Upcoming => a.starts_at.cmp(&b.starts_at),
            })
            .then_with(|| a.title.cmp(&b.title))
    });
}

fn load_cache_from_path(path: &Path) -> EpicDiskCache {
    if !path.exists() {
        return EpicDiskCache::default();
    }
    let Ok(raw) = fs::read_to_string(path) else {
        return EpicDiskCache::default();
    };
    serde_json::from_str::<EpicDiskCache>(&raw).unwrap_or_default()
}

fn save_cache_to_path(path: &Path, cache: &EpicDiskCache) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed creating Epic cache directory: {e}"))?;
    }
    let json = serde_json::to_string_pretty(cache)
        .map_err(|e| format!("Failed serializing Epic cache: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json.as_bytes()).map_err(|e| format!("Failed writing temp Epic cache: {e}"))?;
    #[cfg(windows)]
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(&tmp, path).map_err(|e| format!("Failed committing Epic cache: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_active_and_upcoming_free_games_and_skips_paid_sales() {
        let now = DateTime::parse_from_rfc3339("2026-11-24T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let payload = json!({
            "data": {
                "Catalog": {
                    "searchStore": {
                        "elements": [
                            {
                                "id": "free-now-1",
                                "namespace": "ns1",
                                "title": "Celeste",
                                "description": "Climb Celeste Mountain",
                                "offerType": "BASE_GAME",
                                "seller": { "name": "Maddy Makes Games" },
                                "productSlug": "celeste/home",
                                "catalogNs": {
                                    "mappings": [{ "pageSlug": "celeste", "pageType": "productHome" }]
                                },
                                "price": { "totalPrice": { "fmtPrice": { "originalPrice": "$19.99" } } },
                                "keyImages": [
                                    { "type": "OfferImageTall", "url": "https://cdn1.epicgames.com/celeste-tall.jpg" },
                                    { "type": "OfferImageWide", "url": "https://cdn1.epicgames.com/celeste-wide.jpg" }
                                ],
                                "promotions": {
                                    "promotionalOffers": [{
                                        "promotionalOffers": [{
                                            "startDate": "2026-11-19T16:00:00.000Z",
                                            "endDate": "2026-11-26T16:00:00.000Z",
                                            "discountSetting": { "discountType": "PERCENTAGE", "discountPercentage": 0 }
                                        }]
                                    }],
                                    "upcomingPromotionalOffers": []
                                }
                            },
                            {
                                "id": "upcoming-1",
                                "namespace": "ns2",
                                "title": "Firestone Pack",
                                "description": "Next week freebie",
                                "offerType": "BUNDLE",
                                "seller": { "name": "Holyday Studios" },
                                "productSlug": null,
                                "urlSlug": "1185abfe7c554a1e8c528aba37bf3d0f",
                                "catalogNs": {
                                    "mappings": [{ "pageSlug": "firestone-pack", "pageType": "productHome" }]
                                },
                                "price": { "totalPrice": { "fmtPrice": { "originalPrice": "0" } } },
                                "keyImages": [
                                    { "type": "Thumbnail", "url": "https://cdn1.epicgames.com/firestone.jpg" }
                                ],
                                "promotions": {
                                    "promotionalOffers": [],
                                    "upcomingPromotionalOffers": [{
                                        "promotionalOffers": [{
                                            "startDate": "2026-11-26T16:00:00.000Z",
                                            "endDate": "2026-12-03T16:00:00.000Z",
                                            "discountSetting": { "discountType": "PERCENTAGE", "discountPercentage": 0 }
                                        }]
                                    }]
                                }
                            },
                            {
                                "id": "paid-sale-1",
                                "namespace": "ns3",
                                "title": "TerraScape",
                                "offerType": "BASE_GAME",
                                "promotions": {
                                    "promotionalOffers": [{
                                        "promotionalOffers": [{
                                            "startDate": "2026-11-20T16:00:00.000Z",
                                            "endDate": "2026-12-20T16:00:00.000Z",
                                            "discountSetting": { "discountType": "PERCENTAGE", "discountPercentage": 70 }
                                        }]
                                    }],
                                    "upcomingPromotionalOffers": []
                                }
                            }
                        ]
                    }
                }
            }
        });

        let games = parse_epic_free_games(&payload, now);
        assert_eq!(games.len(), 2);

        assert_eq!(games[0].id, "free-now-1");
        assert_eq!(games[0].status, EpicOfferStatus::Active);
        assert_eq!(games[0].original_price, "$19.99");
        assert_eq!(games[0].store_url, "https://store.epicgames.com/en-US/p/celeste");
        assert_eq!(
            games[0].launcher_url.as_deref(),
            Some("com.epicgames.launcher://store/p/celeste")
        );
        assert_eq!(
            games[0].box_art_url.as_deref(),
            Some("https://cdn1.epicgames.com/celeste-tall.jpg")
        );

        assert_eq!(games[1].id, "upcoming-1");
        assert_eq!(games[1].status, EpicOfferStatus::Upcoming);
        assert_eq!(games[1].original_price, "Free");
        assert_eq!(
            games[1].store_url,
            "https://store.epicgames.com/en-US/bundles/firestone-pack"
        );
        assert_eq!(
            games[1].launcher_url.as_deref(),
            Some("com.epicgames.launcher://store/bundles/firestone-pack")
        );
        assert_eq!(
            launcher_url_from_store_url("https://store.epicgames.com/en-US/free-games"),
            "com.epicgames.launcher://store/free-games"
        );
    }

    #[test]
    fn extracts_unrealengine_cdn_normalizes_http_and_falls_back_in_key_images() {
        let lisa_el = json!({
            "keyImages": [
                { "type": "DieselGameBox", "url": "com.epicgames.video://trailer-123" },
                { "type": "CodeRedemption_340x440", "url": "  http://cdn1.epicgames.com/lisa-340x440.jpg  " },
                { "type": "OfferImageWide", "url": "https://cdn2.unrealengine.com/lisa-wide.jpg" }
            ]
        });
        let (tall, wide) = extract_epic_images(&lisa_el);
        assert_eq!(
            tall.as_deref(),
            Some("https://cdn1.epicgames.com/lisa-340x440.jpg")
        );
        assert_eq!(
            wide.as_deref(),
            Some("https://cdn2.unrealengine.com/lisa-wide.jpg")
        );

        let fallback_el = json!({
            "keyImages": [
                { "type": "VaultTeaser", "url": "" },
                { "type": "Video", "url": "com.epicgames.video://abc" },
                { "type": "CustomPromoArt", "url": "http://cdn2.unrealengine.com/promo-art.png" }
            ]
        });
        let (fb_tall, fb_wide) = extract_epic_images(&fallback_el);
        assert_eq!(
            fb_tall.as_deref(),
            Some("https://cdn2.unrealengine.com/promo-art.png")
        );
        assert_eq!(
            fb_wide.as_deref(),
            Some("https://cdn2.unrealengine.com/promo-art.png")
        );

        let gallery_el = json!({
            "keyImages": [
                { "type": "GalleryImage", "url": "https://cdn2.unrealengine.com/gallery-1.jpg" }
            ]
        });
        let (g_tall, g_wide) = extract_epic_images(&gallery_el);
        assert_eq!(
            g_tall.as_deref(),
            Some("https://cdn2.unrealengine.com/gallery-1.jpg")
        );
        assert_eq!(
            g_wide.as_deref(),
            Some("https://cdn2.unrealengine.com/gallery-1.jpg")
        );
    }
}
