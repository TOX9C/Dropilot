use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{DateTime, Duration as ChronoDuration, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::platforms::{PlatformId, RecentClaim};
use crate::settings::appdata_dir;

const HISTORY_SCHEMA_VERSION: u32 = 1;
const MAX_HISTORY_CLAIMS: usize = 5_000;
const MAX_DAILY_BUCKETS: usize = 365;
// The 90-second progress poll reaches this threshold after 4.5 minutes.
const WATCH_HISTORY_SAVE_INTERVAL: Duration = Duration::from_secs(4 * 60 + 30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClaimPlatform {
    Twitch,
    Kick,
    Epic,
}

impl ClaimPlatform {
    pub fn as_str(self) -> &'static str {
        match self {
            ClaimPlatform::Twitch => "twitch",
            ClaimPlatform::Kick => "kick",
            ClaimPlatform::Epic => "epic",
        }
    }
}

impl From<PlatformId> for ClaimPlatform {
    fn from(p: PlatformId) -> Self {
        match p {
            PlatformId::Twitch => ClaimPlatform::Twitch,
            PlatformId::Kick => ClaimPlatform::Kick,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimRecordStatus {
    Claimed,
    ManualRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimHistoryEntry {
    pub id: String,
    pub platform: ClaimPlatform,
    pub game_name: String,
    pub campaign_name: String,
    pub drop_name: String,
    pub reward_image_url: Option<String>,
    pub claimed_at: DateTime<Utc>,
    pub status: ClaimRecordStatus,
}

impl From<&RecentClaim> for ClaimHistoryEntry {
    fn from(c: &RecentClaim) -> Self {
        Self {
            id: c.id.clone(),
            platform: c.platform.into(),
            game_name: c.game_name.clone(),
            campaign_name: c.campaign_name.clone(),
            drop_name: c.drop_name.clone(),
            reward_image_url: c.reward_image_url.clone(),
            claimed_at: c.claimed_at,
            status: ClaimRecordStatus::Claimed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryStore {
    #[serde(default = "default_schema")]
    pub schema_version: u32,
    #[serde(default)]
    pub total_watch_minutes: u64,
    #[serde(default)]
    pub daily_minutes: BTreeMap<String, u32>,
    #[serde(default)]
    pub claims: Vec<ClaimHistoryEntry>,
    #[serde(skip)]
    last_watch_save: Option<Instant>,
}

fn default_schema() -> u32 {
    HISTORY_SCHEMA_VERSION
}

impl Default for HistoryStore {
    fn default() -> Self {
        Self {
            schema_version: HISTORY_SCHEMA_VERSION,
            total_watch_minutes: 0,
            daily_minutes: BTreeMap::new(),
            claims: Vec::new(),
            last_watch_save: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyActivityPoint {
    pub date: String,
    pub minutes: u32,
    pub claims: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryStats {
    pub total_claims: usize,
    pub twitch_claims: usize,
    pub kick_claims: usize,
    pub epic_claims: usize,
    pub total_watch_minutes: u64,
    pub current_streak_days: u32,
    pub best_streak_days: u32,
    pub top_game: Option<String>,
    pub top_game_claims: usize,
    pub daily_activity: Vec<DailyActivityPoint>,
    pub claims: Vec<ClaimHistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryExportResult {
    pub filename: String,
    pub saved_path: String,
    pub content: String,
}

pub fn history_path() -> PathBuf {
    appdata_dir().join("history.json")
}

pub fn exports_dir() -> PathBuf {
    appdata_dir().join("exports")
}

impl HistoryStore {
    pub fn load() -> Self {
        Self::load_from_path(&history_path())
    }

    pub fn load_from_path(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }
        let raw = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                warn!(error = %err, "Failed reading history.json; starting empty");
                return Self::default();
            }
        };
        match serde_json::from_str::<Self>(&raw) {
            Ok(mut parsed) => {
                parsed.schema_version = HISTORY_SCHEMA_VERSION;
                if parsed.claims.len() > MAX_HISTORY_CLAIMS {
                    parsed.claims.truncate(MAX_HISTORY_CLAIMS);
                }
                parsed
            }
            Err(err) => {
                warn!(error = %err, "Invalid history.json; starting empty");
                Self::default()
            }
        }
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to_path(&history_path())
    }

    pub fn save_to_path(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed creating history directory: {e}"))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed serializing history: {e}"))?;
        let tmp = path.with_extension("json.tmp");
        {
            use std::io::Write;
            let mut file = fs::File::create(&tmp)
                .map_err(|e| format!("Failed creating temp history file: {e}"))?;
            file.write_all(json.as_bytes())
                .map_err(|e| format!("Failed writing temp history file: {e}"))?;
            file.sync_all()
                .map_err(|e| format!("Failed syncing temp history file: {e}"))?;
        }

        #[cfg(windows)]
        if path.exists() {
            let prev = path.with_extension("json.prev");
            let _ = fs::remove_file(&prev);
            fs::rename(path, &prev)
                .map_err(|e| format!("Failed rotating previous history file: {e}"))?;
            match fs::rename(&tmp, path) {
                Ok(()) => {
                    let _ = fs::remove_file(&prev);
                }
                Err(e) => {
                    let _ = fs::rename(&prev, path);
                    return Err(format!("Failed committing history file: {e}"));
                }
            }
        } else {
            fs::rename(&tmp, path)
                .map_err(|e| format!("Failed committing history file: {e}"))?;
        }

        #[cfg(not(windows))]
        fs::rename(&tmp, path).map_err(|e| format!("Failed committing history file: {e}"))?;

        Ok(())
    }

    pub fn record_claim(&mut self, entry: ClaimHistoryEntry) {
        if let Some(existing) = self.claims.iter_mut().find(|c| c.id == entry.id) {
            // Upgrade ManualRequired -> Claimed if the user later claimed it.
            if existing.status != ClaimRecordStatus::Claimed
                && entry.status == ClaimRecordStatus::Claimed
            {
                *existing = entry;
            }
            return;
        }
        self.claims.insert(0, entry);
        if self.claims.len() > MAX_HISTORY_CLAIMS {
            self.claims.truncate(MAX_HISTORY_CLAIMS);
        }
    }

    pub fn remove_claim(&mut self, id: &str) {
        self.claims.retain(|c| c.id != id);
    }

    pub fn begin_watch_save(&mut self, now: Instant) -> bool {
        let due = self.last_watch_save.map_or(true, |last| {
            now.duration_since(last) >= WATCH_HISTORY_SAVE_INTERVAL
        });
        if due {
            self.last_watch_save = Some(now);
        }
        due
    }

    pub fn record_watch_minutes(&mut self, delta_minutes: u32) {
        if delta_minutes == 0 {
            return;
        }
        self.total_watch_minutes = self.total_watch_minutes.saturating_add(delta_minutes as u64);
        let key = Local::now().format("%Y-%m-%d").to_string();
        let bucket = self.daily_minutes.entry(key).or_insert(0);
        *bucket = bucket.saturating_add(delta_minutes);

        while self.daily_minutes.len() > MAX_DAILY_BUCKETS {
            if let Some(oldest) = self.daily_minutes.keys().next().cloned() {
                self.daily_minutes.remove(&oldest);
            } else {
                break;
            }
        }
    }

    pub fn recent_claimed(&self, max: usize) -> Vec<RecentClaim> {
        self.claims
            .iter()
            .filter(|c| c.status == ClaimRecordStatus::Claimed)
            .filter_map(|c| {
                let platform = match c.platform {
                    ClaimPlatform::Twitch => PlatformId::Twitch,
                    ClaimPlatform::Kick => PlatformId::Kick,
                    ClaimPlatform::Epic => return None,
                };
                Some(RecentClaim {
                    id: c.id.clone(),
                    platform,
                    game_name: c.game_name.clone(),
                    campaign_name: c.campaign_name.clone(),
                    drop_name: c.drop_name.clone(),
                    reward_image_url: c.reward_image_url.clone(),
                    claimed_at: c.claimed_at,
                })
            })
            .take(max)
            .collect()
    }

    pub fn build_stats(&self) -> HistoryStats {
        let today = Local::now().date_naive();
        self.build_stats_for_date(today)
    }

    pub fn build_stats_for_date(&self, today: NaiveDate) -> HistoryStats {
        let claimed_only: Vec<&ClaimHistoryEntry> = self
            .claims
            .iter()
            .filter(|c| c.status == ClaimRecordStatus::Claimed)
            .collect();

        let total_claims = claimed_only.len();
        let twitch_claims = claimed_only
            .iter()
            .filter(|c| c.platform == ClaimPlatform::Twitch)
            .count();
        let kick_claims = claimed_only
            .iter()
            .filter(|c| c.platform == ClaimPlatform::Kick)
            .count();
        let epic_claims = claimed_only
            .iter()
            .filter(|c| c.platform == ClaimPlatform::Epic)
            .count();

        let mut game_counts: HashMap<&str, usize> = HashMap::new();
        let mut claims_by_day: HashMap<String, usize> = HashMap::new();

        for c in &claimed_only {
            *game_counts.entry(c.game_name.as_str()).or_insert(0) += 1;
            let day_key = c
                .claimed_at
                .with_timezone(&Local)
                .format("%Y-%m-%d")
                .to_string();
            *claims_by_day.entry(day_key).or_insert(0) += 1;
        }

        let (top_game, top_game_claims) = game_counts
            .into_iter()
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
            .map(|(g, count)| (Some(g.to_string()), count))
            .unwrap_or((None, 0));

        // Collect all active dates (either watch minutes > 0 or >= 1 claim).
        let mut active_dates: Vec<NaiveDate> = self
            .daily_minutes
            .iter()
            .filter(|(_, &mins)| mins > 0)
            .filter_map(|(k, _)| NaiveDate::parse_from_str(k, "%Y-%m-%d").ok())
            .collect();
        for k in claims_by_day.keys() {
            if let Ok(d) = NaiveDate::parse_from_str(k, "%Y-%m-%d") {
                active_dates.push(d);
            }
        }
        active_dates.sort_unstable();
        active_dates.dedup();

        let (current_streak_days, best_streak_days) = compute_streaks(&active_dates, today);

        let mut daily_activity = Vec::with_capacity(14);
        for offset in (0..14).rev() {
            let day = today - ChronoDuration::days(offset);
            let key = day.format("%Y-%m-%d").to_string();
            let minutes = self.daily_minutes.get(&key).copied().unwrap_or(0);
            let claims = claims_by_day.get(&key).copied().unwrap_or(0);
            daily_activity.push(DailyActivityPoint {
                date: key,
                minutes,
                claims,
            });
        }

        HistoryStats {
            total_claims,
            twitch_claims,
            kick_claims,
            epic_claims,
            total_watch_minutes: self.total_watch_minutes,
            current_streak_days,
            best_streak_days,
            top_game,
            top_game_claims,
            daily_activity,
            claims: self.claims.clone(),
        }
    }

    pub fn to_csv(&self) -> String {
        let mut out = String::from("claimed_at,platform,game,campaign,drop,status\n");
        for c in &self.claims {
            let status = match c.status {
                ClaimRecordStatus::Claimed => "claimed",
                ClaimRecordStatus::ManualRequired => "manual_required",
            };
            out.push_str(&format!(
                "{},{},{},{},{},{}\n",
                csv_escape(&c.claimed_at.to_rfc3339()),
                c.platform.as_str(),
                csv_escape(&c.game_name),
                csv_escape(&c.campaign_name),
                csv_escape(&c.drop_name),
                status
            ));
        }
        out
    }

    pub fn export_to_disk(&self, format: &str) -> Result<HistoryExportResult, String> {
        let ext = match format.to_ascii_lowercase().as_str() {
            "csv" => "csv",
            "json" => "json",
            other => return Err(format!("Unsupported export format: {other}")),
        };

        let content = if ext == "csv" {
            self.to_csv()
        } else {
            serde_json::to_string_pretty(&self.build_stats())
                .map_err(|e| format!("Failed serializing JSON export: {e}"))?
        };

        let dir = exports_dir();
        fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed creating exports folder: {e}"))?;
        let stamp = Local::now().format("%Y%m%d-%H%M%S");
        let filename = format!("dropilot-history-{stamp}.{ext}");
        let full_path = dir.join(&filename);
        fs::write(&full_path, content.as_bytes())
            .map_err(|e| format!("Failed writing export file: {e}"))?;

        Ok(HistoryExportResult {
            filename,
            saved_path: full_path.to_string_lossy().to_string(),
            content,
        })
    }
}

fn csv_escape(field: &str) -> String {
    if field.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

fn compute_streaks(sorted_dates: &[NaiveDate], today: NaiveDate) -> (u32, u32) {
    if sorted_dates.is_empty() {
        return (0, 0);
    }

    let mut best: u32 = 1;
    let mut run: u32 = 1;
    for pair in sorted_dates.windows(2) {
        if pair[1] - pair[0] == ChronoDuration::days(1) {
            run += 1;
            if run > best {
                best = run;
            }
        } else {
            run = 1;
        }
    }

    let yesterday = today - ChronoDuration::days(1);
    let last = *sorted_dates.last().unwrap();
    let current = if last == today || last == yesterday {
        let mut cur: u32 = 1;
        for idx in (1..sorted_dates.len()).rev() {
            if sorted_dates[idx] - sorted_dates[idx - 1] == ChronoDuration::days(1) {
                cur += 1;
            } else {
                break;
            }
        }
        cur
    } else {
        0
    };

    (current, best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_current_and_best_streaks() {
        let d = |day: u32| NaiveDate::from_ymd_opt(2026, 11, day).unwrap();
        let dates = vec![d(1), d(2), d(3), d(4), d(10), d(11), d(12)];
        assert_eq!(compute_streaks(&dates, d(12)), (3, 4));
        assert_eq!(compute_streaks(&dates, d(13)), (3, 4));
        assert_eq!(compute_streaks(&dates, d(15)), (0, 4));
    }

    #[test]
    fn debounces_watch_minute_writes_without_losing_minutes() {
        let path = std::env::temp_dir().join(format!(
            "dropilot-history-debounce-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut store = HistoryStore::default();
        let started = std::time::Instant::now();

        store.record_watch_minutes(1);
        assert!(store.begin_watch_save(started));
        store.save_to_path(&path).unwrap();
        store.record_watch_minutes(2);
        assert!(!store.begin_watch_save(started + std::time::Duration::from_secs(90)));
        assert_eq!(HistoryStore::load_from_path(&path).total_watch_minutes, 1);

        assert!(store.begin_watch_save(started + std::time::Duration::from_secs(300)));
        store.save_to_path(&path).unwrap();
        assert_eq!(HistoryStore::load_from_path(&path).total_watch_minutes, 3);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn dedups_claims_and_upgrades_manual_to_claimed() {
        let mut store = HistoryStore::default();
        let entry = ClaimHistoryEntry {
            id: "c1-d1".to_string(),
            platform: ClaimPlatform::Twitch,
            game_name: "Rust".to_string(),
            campaign_name: "Rivals".to_string(),
            drop_name: "Hoodie, \"Pro\"".to_string(),
            reward_image_url: None,
            claimed_at: Utc::now(),
            status: ClaimRecordStatus::ManualRequired,
        };
        store.record_claim(entry.clone());
        assert_eq!(store.claims.len(), 1);
        assert_eq!(store.claims[0].status, ClaimRecordStatus::ManualRequired);

        let mut claimed = entry;
        claimed.status = ClaimRecordStatus::Claimed;
        store.record_claim(claimed);
        assert_eq!(store.claims.len(), 1);
        assert_eq!(store.claims[0].status, ClaimRecordStatus::Claimed);

        let csv = store.to_csv();
        assert!(csv.contains("\"Hoodie, \"\"Pro\"\"\""));
    }
}
