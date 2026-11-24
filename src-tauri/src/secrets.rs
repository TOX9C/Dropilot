use std::fmt;

use keyring::Entry;

use crate::platforms::{AppError, PlatformId};
use crate::settings::{DEFAULT_KICK_PROFILE_ID, DEFAULT_TWITCH_PROFILE_ID};

const SERVICE_NAME: &str = "Dropilot";
const LEGACY_SERVICE_NAME: &str = "DropPilot";

#[derive(Clone)]
pub struct SecretToken(String);

impl SecretToken {
    pub fn new(token: String) -> Self {
        Self(token)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretToken([REDACTED])")
    }
}

pub struct SecretStore;

impl SecretStore {
    fn entry_for_service(service: &str, platform: PlatformId) -> Result<Entry, AppError> {
        let target = format!("{service}/{}", platform.as_str());
        Entry::new_with_target(&target, service, platform.as_str())
            .map_err(|e| AppError::Internal(format!("Credential Manager entry error: {e}")))
    }

    fn entry(platform: PlatformId) -> Result<Entry, AppError> {
        Self::entry_for_service(SERVICE_NAME, platform)
    }

    pub fn save_token(platform: PlatformId, token: &str) -> Result<(), AppError> {
        let entry = Self::entry(platform)?;
        entry
            .set_password(token)
            .map_err(|e| AppError::Internal(format!("Failed saving token to Credential Manager: {e}")))
    }

    pub fn load_token(platform: PlatformId) -> Result<Option<SecretToken>, AppError> {
        let entry = Self::entry(platform)?;
        match entry.get_password() {
            Ok(tok) if !tok.trim().is_empty() => return Ok(Some(SecretToken::new(tok))),
            Ok(_) | Err(keyring::Error::NoEntry) => {}
            Err(e) => {
                return Err(AppError::Internal(format!(
                    "Failed reading token from Credential Manager: {e}"
                )));
            }
        }

        // Seamless one-time migration from legacy "DropPilot" Credential Manager entry.
        if let Ok(legacy_entry) = Self::entry_for_service(LEGACY_SERVICE_NAME, platform) {
            if let Ok(tok) = legacy_entry.get_password() {
                if !tok.trim().is_empty() {
                    let _ = Self::save_token(platform, &tok);
                    return Ok(Some(SecretToken::new(tok)));
                }
            }
        }

        Ok(None)
    }

    pub fn delete_token(platform: PlatformId) -> Result<(), AppError> {
        if let Ok(legacy_entry) = Self::entry_for_service(LEGACY_SERVICE_NAME, platform) {
            let _ = legacy_entry.delete_credential();
        }
        let entry = Self::entry(platform)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AppError::Internal(format!(
                "Failed deleting token from Credential Manager: {e}"
            ))),
        }
    }

    fn named_entry(key: &str) -> Result<Entry, AppError> {
        let target = format!("{SERVICE_NAME}/{key}");
        Entry::new_with_target(&target, SERVICE_NAME, key)
            .map_err(|e| AppError::Internal(format!("Credential Manager entry error: {e}")))
    }

    pub fn save_named_secret(key: &str, value: &str) -> Result<(), AppError> {
        let entry = Self::named_entry(key)?;
        entry
            .set_password(value)
            .map_err(|e| AppError::Internal(format!("Failed saving secret to Credential Manager: {e}")))
    }

    pub fn load_named_secret(key: &str) -> Result<Option<SecretToken>, AppError> {
        let entry = Self::named_entry(key)?;
        match entry.get_password() {
            Ok(val) if !val.trim().is_empty() => Ok(Some(SecretToken::new(val))),
            Ok(_) | Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AppError::Internal(format!(
                "Failed reading secret from Credential Manager: {e}"
            ))),
        }
    }

    pub fn delete_named_secret(key: &str) -> Result<(), AppError> {
        let entry = Self::named_entry(key)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AppError::Internal(format!(
                "Failed deleting secret from Credential Manager: {e}"
            ))),
        }
    }

    fn is_legacy_default_profile(platform: PlatformId, profile_id: &str) -> bool {
        matches!(
            (platform, profile_id),
            (PlatformId::Twitch, DEFAULT_TWITCH_PROFILE_ID)
                | (PlatformId::Kick, DEFAULT_KICK_PROFILE_ID)
        )
    }

    pub fn profile_secret_key(platform: PlatformId, profile_id: &str) -> String {
        if Self::is_legacy_default_profile(platform, profile_id) {
            platform.as_str().to_string()
        } else {
            format!("{}:{profile_id}", platform.as_str())
        }
    }

    pub fn save_profile_token(
        platform: PlatformId,
        profile_id: &str,
        token: &str,
    ) -> Result<(), AppError> {
        if Self::is_legacy_default_profile(platform, profile_id) {
            Self::save_token(platform, token)
        } else {
            Self::save_named_secret(&Self::profile_secret_key(platform, profile_id), token)
        }
    }

    pub fn load_profile_token(
        platform: PlatformId,
        profile_id: &str,
    ) -> Result<Option<SecretToken>, AppError> {
        if Self::is_legacy_default_profile(platform, profile_id) {
            Self::load_token(platform)
        } else {
            Self::load_named_secret(&Self::profile_secret_key(platform, profile_id))
        }
    }

    pub fn delete_profile_token(platform: PlatformId, profile_id: &str) -> Result<(), AppError> {
        if Self::is_legacy_default_profile(platform, profile_id) {
            Self::delete_token(platform)
        } else {
            Self::delete_named_secret(&Self::profile_secret_key(platform, profile_id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SecretStore, SecretToken};
    use crate::platforms::PlatformId;
    use crate::settings::{DEFAULT_KICK_PROFILE_ID, DEFAULT_TWITCH_PROFILE_ID};

    #[test]
    fn debug_representation_never_exposes_secret() {
        let tok = SecretToken::new("super_secret_oauth_token".to_string());
        let formatted = format!("{tok:?}");
        assert!(!formatted.contains("super_secret_oauth_token"));
        assert!(formatted.contains("REDACTED"));
    }

    #[test]
    fn profile_secret_keys_preserve_legacy_defaults_and_scope_extras() {
        assert_eq!(
            SecretStore::profile_secret_key(PlatformId::Twitch, DEFAULT_TWITCH_PROFILE_ID),
            "twitch"
        );
        assert_eq!(
            SecretStore::profile_secret_key(PlatformId::Kick, DEFAULT_KICK_PROFILE_ID),
            "kick"
        );
        assert_eq!(
            SecretStore::profile_secret_key(PlatformId::Twitch, "twitch-alt-1"),
            "twitch:twitch-alt-1"
        );
        assert_eq!(
            SecretStore::profile_secret_key(PlatformId::Kick, "kick-alt-2"),
            "kick:kick-alt-2"
        );
    }
}
