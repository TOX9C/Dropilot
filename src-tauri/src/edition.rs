use serde::Serialize;

use crate::platforms::PlatformId;
use crate::settings::is_default_profile;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditionInfo {
    pub edition: &'static str,
    pub is_free: bool,
    pub twitch_enabled: bool,
    pub kick_enabled: bool,
    pub epic_enabled: bool,
    pub multi_account_enabled: bool,
    pub webhooks_enabled: bool,
    pub quiet_hours_enabled: bool,
    pub max_profiles_per_platform: usize,
}

pub const fn current_edition() -> EditionInfo {
    #[cfg(feature = "free-edition")]
    {
        EditionInfo {
            edition: "free",
            is_free: true,
            twitch_enabled: true,
            kick_enabled: false,
            epic_enabled: true,
            multi_account_enabled: false,
            webhooks_enabled: true,
            quiet_hours_enabled: true,
            max_profiles_per_platform: 1,
        }
    }

    #[cfg(not(feature = "free-edition"))]
    {
        EditionInfo {
            edition: "full",
            is_free: false,
            twitch_enabled: true,
            kick_enabled: true,
            epic_enabled: true,
            multi_account_enabled: true,
            webhooks_enabled: true,
            quiet_hours_enabled: true,
            max_profiles_per_platform: 10,
        }
    }
}

pub const fn is_platform_enabled(platform: PlatformId) -> bool {
    let info = current_edition();
    match platform {
        PlatformId::Twitch => info.twitch_enabled,
        PlatformId::Kick => info.kick_enabled,
    }
}

pub const fn is_multi_account_enabled() -> bool {
    current_edition().multi_account_enabled
}

pub fn is_profile_allowed(platform: PlatformId, profile_id: &str) -> bool {
    if !is_platform_enabled(platform) {
        return false;
    }
    if is_multi_account_enabled() {
        return true;
    }
    is_default_profile(profile_id)
}

pub fn ensure_platform_enabled(platform: PlatformId) -> Result<(), String> {
    if is_platform_enabled(platform) {
        Ok(())
    } else {
        Err(format!(
            "{} is not included in Dropilot Free Edition.",
            platform.display_name()
        ))
    }
}

pub fn ensure_profile_allowed(platform: PlatformId, profile_id: &str) -> Result<(), String> {
    ensure_platform_enabled(platform)?;
    if is_profile_allowed(platform, profile_id) {
        Ok(())
    } else {
        Err("Multiple accounts are not included in Dropilot Free Edition.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{DEFAULT_KICK_PROFILE_ID, DEFAULT_TWITCH_PROFILE_ID};

    #[test]
    fn edition_flags_match_compile_feature() {
        let info = current_edition();
        assert!(info.twitch_enabled);
        assert!(info.epic_enabled);

        if cfg!(feature = "free-edition") {
            assert_eq!(info.edition, "free");
            assert!(info.is_free);
            assert!(!info.kick_enabled);
            assert!(!info.multi_account_enabled);
            assert_eq!(info.max_profiles_per_platform, 1);
            assert!(is_profile_allowed(PlatformId::Twitch, DEFAULT_TWITCH_PROFILE_ID));
            assert!(!is_profile_allowed(PlatformId::Twitch, "twitch-alt-1"));
            assert!(!is_profile_allowed(PlatformId::Kick, DEFAULT_KICK_PROFILE_ID));
        } else {
            assert_eq!(info.edition, "full");
            assert!(!info.is_free);
            assert!(info.kick_enabled);
            assert!(info.multi_account_enabled);
            assert!(is_profile_allowed(PlatformId::Twitch, "twitch-alt-1"));
            assert!(is_profile_allowed(PlatformId::Kick, DEFAULT_KICK_PROFILE_ID));
        }
    }
}
