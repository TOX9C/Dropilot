use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::platforms::PlatformId;

pub const CURRENT_SCHEMA_VERSION: u32 = 2;
pub const CURRENT_DISCLAIMER_VERSION: u32 = 1;
pub const MIN_REFRESH_MINUTES: u32 = 10;
pub const MAX_REFRESH_MINUTES: u32 = 240;
pub const DEFAULT_TWITCH_PROFILE_ID: &str = "default-twitch";
pub const DEFAULT_KICK_PROFILE_ID: &str = "default-kick";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountProfile {
    pub id: String,
    pub label: String,
    pub platform: PlatformId,
    #[serde(default)]
    pub settings: PlatformSettings,
}

pub fn default_profile_id(platform: PlatformId) -> &'static str {
    match platform {
        PlatformId::Twitch => DEFAULT_TWITCH_PROFILE_ID,
        PlatformId::Kick => DEFAULT_KICK_PROFILE_ID,
    }
}

pub fn is_default_profile(profile_id: &str) -> bool {
    profile_id == DEFAULT_TWITCH_PROFILE_ID || profile_id == DEFAULT_KICK_PROFILE_ID
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FontFamily {
    #[default]
    Plex,
    Inter,
    System,
    Mono,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FontScale {
    Compact,
    #[default]
    Default,
    Comfortable,
    Large,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdleBehavior {
    #[default]
    StayIdle,
    FarmAny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevelSetting {
    #[default]
    Info,
    Debug,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PriorityItem {
    Game {
        #[serde(rename = "gameId")]
        game_id: String,
        #[serde(rename = "gameName")]
        game_name: String,
    },
    Campaign {
        #[serde(rename = "campaignId")]
        campaign_id: String,
        #[serde(rename = "campaignName")]
        campaign_name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformSettings {
    pub enabled: bool,
    pub priority: Vec<PriorityItem>,
    pub farm_campaigns: Vec<String>,
    pub excluded_campaigns: Vec<String>,
    pub pinned_channel: Option<String>,
    #[serde(default)]
    pub prioritize_ending_soon: bool,
}

impl Default for PlatformSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            priority: Vec::new(),
            farm_campaigns: Vec::new(),
            excluded_campaigns: Vec::new(),
            pinned_channel: None,
            prioritize_ending_soon: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

fn default_true() -> bool {
    true
}

fn default_quiet_start() -> u16 {
    22 * 60
}

fn default_quiet_end() -> u16 {
    8 * 60
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleSettings {
    #[serde(default)]
    pub quiet_hours_enabled: bool,
    #[serde(default = "default_quiet_start")]
    pub quiet_start_minutes: u16,
    #[serde(default = "default_quiet_end")]
    pub quiet_end_minutes: u16,
    #[serde(default = "default_true")]
    pub humanized_jitter: bool,
}

impl Default for ScheduleSettings {
    fn default() -> Self {
        Self {
            quiet_hours_enabled: false,
            quiet_start_minutes: default_quiet_start(),
            quiet_end_minutes: default_quiet_end(),
            humanized_jitter: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookSettings {
    #[serde(default)]
    pub discord_enabled: bool,
    #[serde(default = "default_true")]
    pub on_drop_claimed: bool,
    #[serde(default = "default_true")]
    pub on_action_required: bool,
    #[serde(default = "default_true")]
    pub on_campaign_completed: bool,
    #[serde(default = "default_true")]
    pub on_epic_free_games: bool,
}

impl Default for WebhookSettings {
    fn default() -> Self {
        Self {
            discord_enabled: false,
            on_drop_claimed: true,
            on_action_required: true,
            on_campaign_completed: true,
            on_epic_free_games: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub schema_version: u32,
    pub theme: ThemeMode,
    #[serde(default)]
    pub font_family: FontFamily,
    #[serde(default)]
    pub font_scale: FontScale,
    pub autostart: bool,
    pub start_minimized: bool,
    pub close_to_tray: bool,
    pub notifications: bool,
    pub auto_claim: bool,
    pub idle_behavior: IdleBehavior,
    pub refresh_minutes: u32,
    pub farm_unlinked: bool,
    pub compat_watch_mode: bool,
    #[serde(default = "default_true")]
    pub update_check: bool,
    #[serde(default = "default_true")]
    pub auto_install_updates: bool,
    #[serde(default = "default_true")]
    pub remote_config_enabled: bool,
    #[serde(default)]
    pub schedule: ScheduleSettings,
    #[serde(default)]
    pub webhooks: WebhookSettings,
    pub disclaimer_accepted_version: u32,
    pub log_level: LogLevelSetting,
    pub mock_mode: bool,
    pub window_bounds: Option<WindowBounds>,
    pub twitch: PlatformSettings,
    pub kick: PlatformSettings,
    #[serde(default)]
    pub profiles: Vec<AccountProfile>,
    #[serde(default)]
    pub active_twitch_profile_id: String,
    #[serde(default)]
    pub active_kick_profile_id: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            theme: ThemeMode::Dark,
            font_family: FontFamily::Plex,
            font_scale: FontScale::Default,
            autostart: false,
            start_minimized: false,
            close_to_tray: true,
            notifications: true,
            auto_claim: true,
            idle_behavior: IdleBehavior::StayIdle,
            refresh_minutes: 30,
            farm_unlinked: false,
            compat_watch_mode: false,
            update_check: true,
            auto_install_updates: true,
            remote_config_enabled: true,
            schedule: ScheduleSettings::default(),
            webhooks: WebhookSettings::default(),
            disclaimer_accepted_version: 0,
            log_level: LogLevelSetting::Info,
            mock_mode: cfg!(feature = "mock"),
            window_bounds: None,
            twitch: PlatformSettings::default(),
            kick: PlatformSettings::default(),
            profiles: vec![
                AccountProfile {
                    id: DEFAULT_TWITCH_PROFILE_ID.to_string(),
                    label: "Main".to_string(),
                    platform: PlatformId::Twitch,
                    settings: PlatformSettings::default(),
                },
                AccountProfile {
                    id: DEFAULT_KICK_PROFILE_ID.to_string(),
                    label: "Main".to_string(),
                    platform: PlatformId::Kick,
                    settings: PlatformSettings::default(),
                },
            ],
            active_twitch_profile_id: DEFAULT_TWITCH_PROFILE_ID.to_string(),
            active_kick_profile_id: DEFAULT_KICK_PROFILE_ID.to_string(),
        }
    }
}

impl Settings {
    pub fn sanitize(&mut self) {
        self.schema_version = CURRENT_SCHEMA_VERSION;
        self.refresh_minutes = self
            .refresh_minutes
            .clamp(MIN_REFRESH_MINUTES, MAX_REFRESH_MINUTES);
        self.schedule.quiet_start_minutes %= 24 * 60;
        self.schedule.quiet_end_minutes %= 24 * 60;
        if !cfg!(feature = "mock") {
            self.mock_mode = false;
        }
        if let Some(ref mut bounds) = self.window_bounds {
            bounds.width = bounds.width.clamp(840, 3840);
            bounds.height = bounds.height.clamp(560, 2160);
        }

        // Ensure at least one Twitch profile and one Kick profile always exist.
        // When migrating from single-account settings.json (profiles empty), carry over
        // legacy `settings.twitch` / `settings.kick` into the default profiles.
        if !self.profiles.iter().any(|p| p.platform == PlatformId::Twitch) {
            self.profiles.push(AccountProfile {
                id: DEFAULT_TWITCH_PROFILE_ID.to_string(),
                label: "Main".to_string(),
                platform: PlatformId::Twitch,
                settings: self.twitch.clone(),
            });
        }
        if !self.profiles.iter().any(|p| p.platform == PlatformId::Kick) {
            self.profiles.push(AccountProfile {
                id: DEFAULT_KICK_PROFILE_ID.to_string(),
                label: "Main".to_string(),
                platform: PlatformId::Kick,
                settings: self.kick.clone(),
            });
        }

        // Deduplicate IDs and sanitize blank labels.
        let mut seen_ids = std::collections::HashSet::new();
        self.profiles.retain(|p| !p.id.trim().is_empty() && seen_ids.insert(p.id.clone()));
        for prof in &mut self.profiles {
            let trimmed = prof.label.trim();
            if trimmed.is_empty() {
                prof.label = if is_default_profile(&prof.id) {
                    "Main".to_string()
                } else {
                    "Account".to_string()
                };
            } else if trimmed != prof.label {
                prof.label = trimmed.to_string();
            }
        }

        // Ensure active profile IDs always point to a valid profile on the matching platform.
        if !self
            .profiles
            .iter()
            .any(|p| p.platform == PlatformId::Twitch && p.id == self.active_twitch_profile_id)
        {
            if let Some(first) = self.profiles.iter().find(|p| p.platform == PlatformId::Twitch) {
                self.active_twitch_profile_id = first.id.clone();
            }
        }
        if !self
            .profiles
            .iter()
            .any(|p| p.platform == PlatformId::Kick && p.id == self.active_kick_profile_id)
        {
            if let Some(first) = self.profiles.iter().find(|p| p.platform == PlatformId::Kick) {
                self.active_kick_profile_id = first.id.clone();
            }
        }

        self.sync_legacy_mirrors();
    }

    pub fn sync_legacy_mirrors(&mut self) {
        if let Some(tw) = self.profile_settings(&self.active_twitch_profile_id).cloned() {
            self.twitch = tw;
        }
        if let Some(kk) = self.profile_settings(&self.active_kick_profile_id).cloned() {
            self.kick = kk;
        }
    }

    pub fn active_profile_id(&self, platform: PlatformId) -> &str {
        match platform {
            PlatformId::Twitch => {
                if self.active_twitch_profile_id.is_empty() {
                    DEFAULT_TWITCH_PROFILE_ID
                } else {
                    &self.active_twitch_profile_id
                }
            }
            PlatformId::Kick => {
                if self.active_kick_profile_id.is_empty() {
                    DEFAULT_KICK_PROFILE_ID
                } else {
                    &self.active_kick_profile_id
                }
            }
        }
    }

    pub fn set_active_profile_id(&mut self, platform: PlatformId, profile_id: String) {
        match platform {
            PlatformId::Twitch => self.active_twitch_profile_id = profile_id,
            PlatformId::Kick => self.active_kick_profile_id = profile_id,
        }
    }

    pub fn profile_by_id(&self, profile_id: &str) -> Option<&AccountProfile> {
        self.profiles.iter().find(|p| p.id == profile_id)
    }

    pub fn profile_by_id_mut(&mut self, profile_id: &str) -> Option<&mut AccountProfile> {
        self.profiles.iter_mut().find(|p| p.id == profile_id)
    }

    pub fn profile_settings(&self, profile_id: &str) -> Option<&PlatformSettings> {
        self.profiles.iter().find(|p| p.id == profile_id).map(|p| &p.settings)
    }

    pub fn profile_settings_mut(&mut self, profile_id: &str) -> Option<&mut PlatformSettings> {
        self.profiles.iter_mut().find(|p| p.id == profile_id).map(|p| &mut p.settings)
    }

    pub fn profiles_for(&self, platform: PlatformId) -> Vec<&AccountProfile> {
        self.profiles.iter().filter(|p| p.platform == platform).collect()
    }

    pub fn disclaimer_accepted(&self) -> bool {
        self.disclaimer_accepted_version >= CURRENT_DISCLAIMER_VERSION
    }
}

pub fn appdata_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Dropilot")
}

fn legacy_settings_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("DropPilot")
        .join("settings.json")
}

pub fn settings_path() -> PathBuf {
    appdata_dir().join("settings.json")
}

pub fn load_settings() -> Settings {
    let path = settings_path();
    if path.exists() {
        return load_from_path(&path);
    }
    let legacy = legacy_settings_path();
    if legacy.exists() {
        let migrated = load_from_path(&legacy);
        let _ = save_atomic_to_path(&path, &migrated);
        return migrated;
    }
    Settings::default()
}

pub fn save_settings(settings: &Settings) -> Result<(), String> {
    save_atomic_to_path(&settings_path(), settings)
}

pub fn load_from_path(path: &Path) -> Settings {
    if !path.exists() {
        return Settings::default();
    }

    let raw = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) => {
            warn!(error = %err, "Failed reading settings.json; using defaults");
            return Settings::default();
        }
    };

    let mut value: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(err) => {
            backup_corrupt_file(path);
            warn!(error = %err, "Corrupt settings.json backed up and reset to defaults");
            return Settings::default();
        }
    };

    let migrated = match migrate_value(&mut value) {
        Ok(m) => m,
        Err(err) => {
            backup_corrupt_file(path);
            warn!(error = %err, "Unmigrated settings.json backed up and reset to defaults");
            return Settings::default();
        }
    };

    match serde_json::from_value::<Settings>(value) {
        Ok(mut parsed) => {
            parsed.sanitize();
            if migrated {
                let _ = save_atomic_to_path(path, &parsed);
            }
            parsed
        }
        Err(err) => {
            backup_corrupt_file(path);
            warn!(error = %err, "Invalid settings schema backed up and reset to defaults");
            Settings::default()
        }
    }
}

pub fn save_atomic_to_path(path: &Path, settings: &Settings) -> Result<(), String> {
    let mut sanitized = settings.clone();
    sanitized.sanitize();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create settings directory: {e}"))?;
    }

    let json = serde_json::to_string_pretty(&sanitized)
        .map_err(|e| format!("Failed to serialize settings: {e}"))?;

    let tmp_path = path.with_extension("json.tmp");
    {
        use std::io::Write;
        let mut file = fs::File::create(&tmp_path)
            .map_err(|e| format!("Failed to create temp settings file: {e}"))?;
        file.write_all(json.as_bytes())
            .map_err(|e| format!("Failed to write temp settings file: {e}"))?;
        file.sync_all()
            .map_err(|e| format!("Failed to sync temp settings file: {e}"))?;
    }

    // Windows atomic replace via rename when target exists may require removing backup or rename directly.
    #[cfg(windows)]
    if path.exists() {
        let prev = path.with_extension("json.prev");
        let _ = fs::remove_file(&prev);
        fs::rename(path, &prev).map_err(|e| format!("Failed rotating previous settings: {e}"))?;
        match fs::rename(&tmp_path, path) {
            Ok(()) => {
                let _ = fs::remove_file(&prev);
            }
            Err(e) => {
                let _ = fs::rename(&prev, path);
                return Err(format!("Failed replacing settings file: {e}"));
            }
        }
    } else {
        fs::rename(&tmp_path, path).map_err(|e| format!("Failed committing settings file: {e}"))?;
    }

    #[cfg(not(windows))]
    fs::rename(&tmp_path, path).map_err(|e| format!("Failed committing settings file: {e}"))?;

    Ok(())
}

fn migrate_value(value: &mut serde_json::Value) -> Result<bool, String> {
    let obj = value
        .as_object_mut()
        .ok_or_else(|| "settings root is not a JSON object".to_string())?;

    let version = obj
        .get("schemaVersion")
        .or_else(|| obj.get("schema_version"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    let mut migrated = false;

    if version < 1 {
        // Fill missing v1 defaults cleanly while keeping any pre-existing user preferences.
        let defaults = serde_json::to_value(Settings::default())
            .map_err(|e| format!("default serialization error: {e}"))?;
        if let Some(default_obj) = defaults.as_object() {
            for (k, v) in default_obj {
                obj.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        migrated = true;
        info!("Migrated settings schema v0 -> v1");
    }

    if version < 2 {
        obj.insert("updateCheck".to_string(), serde_json::Value::Bool(true));
        obj.insert(
            "autoInstallUpdates".to_string(),
            serde_json::Value::Bool(true),
        );
        obj.insert(
            "schemaVersion".to_string(),
            serde_json::Value::from(CURRENT_SCHEMA_VERSION),
        );
        migrated = true;
        info!(
            from_version = version,
            "Migrated settings schema -> v2 (auto-update enabled)"
        );
    }

    Ok(migrated)
}

fn backup_corrupt_file(path: &Path) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup_name = format!("settings.json.corrupt.{ts}.bak");
    let backup_path = path.with_file_name(backup_name);
    if let Err(err) = fs::rename(path, &backup_path) {
        warn!(error = %err, "Could not rename corrupt settings file");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock valid")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("dropilot_test_{name}_{ts}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn atomic_save_and_load_roundtrip_clamps_refresh() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("settings.json");

        let mut s = Settings::default();
        s.refresh_minutes = 2; // below minimum 10
        s.disclaimer_accepted_version = 1;
        save_atomic_to_path(&path, &s).expect("save succeeds");

        let loaded = load_from_path(&path);
        assert_eq!(loaded.refresh_minutes, MIN_REFRESH_MINUTES);
        assert_eq!(loaded.disclaimer_accepted_version, 1);
        assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrates_v0_partial_json_to_v1() {
        let dir = temp_dir("migrate");
        let path = dir.join("settings.json");
        fs::write(&path, r#"{ "theme": "light", "refreshMinutes": 45 }"#).expect("write v0");

        let loaded = load_from_path(&path);
        assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(loaded.theme, ThemeMode::Light);
        assert_eq!(loaded.refresh_minutes, 45);
        assert!(loaded.close_to_tray);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn backs_up_corrupt_settings_and_resets() {
        let dir = temp_dir("corrupt");
        let path = dir.join("settings.json");
        fs::write(&path, "{ invalid json !!!").expect("write corrupt");

        let loaded = load_from_path(&path);
        assert_eq!(loaded.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(!path.exists());

        let has_backup = fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(Result::ok)
            .any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("settings.json.corrupt.")
            });
        assert!(has_backup);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn font_family_and_scale_default_and_roundtrip() {
        let dir = temp_dir("font_prefs");
        let path = dir.join("settings.json");

        // Existing v1 settings without fontFamily/fontScale should default cleanly.
        let defaults_json = serde_json::json!({
            "schemaVersion": CURRENT_SCHEMA_VERSION,
            "theme": "dark",
            "autostart": false,
            "startMinimized": false,
            "closeToTray": true,
            "notifications": true,
            "autoClaim": true,
            "idleBehavior": "stay_idle",
            "refreshMinutes": 30,
            "farmUnlinked": false,
            "compatWatchMode": false,
            "updateCheck": false,
            "disclaimerAcceptedVersion": 1,
            "logLevel": "info",
            "mockMode": false,
            "windowBounds": null,
            "twitch": PlatformSettings::default(),
            "kick": PlatformSettings::default()
        });
        fs::write(
            &path,
            serde_json::to_string_pretty(&defaults_json).expect("serialize"),
        )
        .expect("write v1");

        let migrated = load_from_path(&path);
        assert_eq!(migrated.font_family, FontFamily::Plex);
        assert_eq!(migrated.font_scale, FontScale::Default);

        // Save custom font preferences and verify round-trip.
        let mut custom = migrated;
        custom.font_family = FontFamily::Inter;
        custom.font_scale = FontScale::Comfortable;
        save_atomic_to_path(&path, &custom).expect("save custom font prefs");

        let reloaded = load_from_path(&path);
        assert_eq!(reloaded.font_family, FontFamily::Inter);
        assert_eq!(reloaded.font_scale, FontScale::Comfortable);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrates_v1_to_v2_enabling_auto_update_and_preserves_subsequent_opt_out() {
        let dir = temp_dir("migrate_v1_v2");
        let path = dir.join("settings.json");

        let v1_json = serde_json::json!({
            "schemaVersion": 1,
            "theme": "light",
            "autostart": false,
            "startMinimized": false,
            "closeToTray": true,
            "notifications": true,
            "autoClaim": true,
            "idleBehavior": "stay_idle",
            "refreshMinutes": 45,
            "farmUnlinked": false,
            "compatWatchMode": false,
            "updateCheck": false,
            "disclaimerAcceptedVersion": 1,
            "logLevel": "info",
            "mockMode": false,
            "windowBounds": null,
            "twitch": PlatformSettings::default(),
            "kick": PlatformSettings::default()
        });
        fs::write(
            &path,
            serde_json::to_string_pretty(&v1_json).expect("serialize v1"),
        )
        .expect("write v1");

        let mut migrated = load_from_path(&path);
        assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(migrated.update_check);
        assert!(migrated.auto_install_updates);
        assert_eq!(migrated.theme, ThemeMode::Light);
        assert_eq!(migrated.refresh_minutes, 45);

        // User explicitly disables auto-updates under v2; preference must stick on next load.
        migrated.update_check = false;
        migrated.auto_install_updates = false;
        save_atomic_to_path(&path, &migrated).expect("save opted-out v2");

        let reloaded = load_from_path(&path);
        assert_eq!(reloaded.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(!reloaded.update_check);
        assert!(!reloaded.auto_install_updates);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrates_single_account_settings_into_default_profiles() {
        let dir = temp_dir("migrate_profiles");
        let path = dir.join("settings.json");

        let legacy_v2 = serde_json::json!({
            "schemaVersion": CURRENT_SCHEMA_VERSION,
            "theme": "dark",
            "autostart": false,
            "startMinimized": false,
            "closeToTray": true,
            "notifications": true,
            "autoClaim": true,
            "idleBehavior": "stay_idle",
            "refreshMinutes": 30,
            "farmUnlinked": false,
            "compatWatchMode": false,
            "updateCheck": true,
            "autoInstallUpdates": true,
            "remoteConfigEnabled": true,
            "disclaimerAcceptedVersion": 1,
            "logLevel": "info",
            "mockMode": false,
            "windowBounds": null,
            "twitch": {
                "enabled": true,
                "priority": [{ "kind": "game", "gameId": "263490", "gameName": "Rust" }],
                "farmCampaigns": ["tw-rust-1"],
                "excludedCampaigns": ["tw-ignored"],
                "pinnedChannel": "hJune",
                "prioritizeEndingSoon": true
            },
            "kick": {
                "enabled": false,
                "priority": [],
                "farmCampaigns": ["kk-1"],
                "excludedCampaigns": [],
                "pinnedChannel": null,
                "prioritizeEndingSoon": false
            }
        });
        fs::write(
            &path,
            serde_json::to_string_pretty(&legacy_v2).expect("serialize legacy"),
        )
        .expect("write legacy");

        let loaded = load_from_path(&path);
        assert_eq!(loaded.active_profile_id(PlatformId::Twitch), DEFAULT_TWITCH_PROFILE_ID);
        assert_eq!(loaded.active_profile_id(PlatformId::Kick), DEFAULT_KICK_PROFILE_ID);
        assert_eq!(loaded.profiles_for(PlatformId::Twitch).len(), 1);
        assert_eq!(loaded.profiles_for(PlatformId::Kick).len(), 1);

        let tw_prof = loaded.profile_by_id(DEFAULT_TWITCH_PROFILE_ID).expect("default twitch");
        assert_eq!(tw_prof.label, "Main");
        assert_eq!(tw_prof.settings.farm_campaigns, vec!["tw-rust-1"]);
        assert_eq!(tw_prof.settings.excluded_campaigns, vec!["tw-ignored"]);
        assert_eq!(tw_prof.settings.pinned_channel.as_deref(), Some("hJune"));
        assert!(tw_prof.settings.prioritize_ending_soon);
        assert_eq!(tw_prof.settings.priority.len(), 1);

        let kk_prof = loaded.profile_by_id(DEFAULT_KICK_PROFILE_ID).expect("default kick");
        assert_eq!(kk_prof.label, "Main");
        assert!(!kk_prof.settings.enabled);
        assert_eq!(kk_prof.settings.farm_campaigns, vec!["kk-1"]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn adding_and_removing_extra_profiles_preserves_mirrors_and_validates_active_ids() {
        let mut settings = Settings::default();
        assert_eq!(settings.profiles_for(PlatformId::Twitch).len(), 1);

        let alt = AccountProfile {
            id: "twitch-alt-1".to_string(),
            label: "Alt".to_string(),
            platform: PlatformId::Twitch,
            settings: PlatformSettings {
                farm_campaigns: vec!["alt-camp".to_string()],
                prioritize_ending_soon: true,
                ..Default::default()
            },
        };
        settings.profiles.push(alt);
        settings.set_active_profile_id(PlatformId::Twitch, "twitch-alt-1".to_string());
        settings.sanitize();

        assert_eq!(settings.profiles_for(PlatformId::Twitch).len(), 2);
        assert_eq!(settings.active_profile_id(PlatformId::Twitch), "twitch-alt-1");
        assert_eq!(settings.twitch.farm_campaigns, vec!["alt-camp"]);
        assert!(settings.twitch.prioritize_ending_soon);

        // Removing the active alt profile should cleanly fall back to Main.
        settings.profiles.retain(|p| p.id != "twitch-alt-1");
        settings.sanitize();
        assert_eq!(settings.active_profile_id(PlatformId::Twitch), DEFAULT_TWITCH_PROFILE_ID);
        assert!(settings.twitch.farm_campaigns.is_empty());

        // Removing the last remaining profile on a platform restores default Main automatically.
        settings.profiles.retain(|p| p.platform != PlatformId::Kick);
        settings.sanitize();
        assert_eq!(settings.profiles_for(PlatformId::Kick).len(), 1);
        assert_eq!(settings.active_profile_id(PlatformId::Kick), DEFAULT_KICK_PROFILE_ID);
    }
}
