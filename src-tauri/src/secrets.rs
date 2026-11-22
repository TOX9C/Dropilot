use std::fmt;

use keyring::Entry;

use crate::platforms::{AppError, PlatformId};

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
}

#[cfg(test)]
mod tests {
    use super::SecretToken;

    #[test]
    fn debug_representation_never_exposes_secret() {
        let tok = SecretToken::new("super_secret_oauth_token".to_string());
        let formatted = format!("{tok:?}");
        assert!(!formatted.contains("super_secret_oauth_token"));
        assert!(formatted.contains("REDACTED"));
    }
}
