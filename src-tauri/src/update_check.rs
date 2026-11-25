use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;
use tracing::{debug, info, warn};

use crate::auth::login_window::{
    close_epic_claim_window, close_kick_bridge_window, close_twitch_bridge_window,
};
use crate::net::allowlist::{UPDATE_CHECK_HOST, UPDATE_CHECK_PATH};
use crate::net::client::AllowlistedClient;
use crate::platforms::AppError;
use crate::state::{AppState, MinerControl};

pub const RELEASES_PAGE_URL: &str = "https://github.com/theflash1211/dropilot-releases/releases";
pub const UPDATE_PROGRESS_EVENT: &str = "dropilot://update-progress";

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    published_at: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub available: bool,
    pub current_version: String,
    pub latest_version: String,
    pub release_url: String,
    pub published_at: Option<String>,
    pub notes: Option<String>,
    pub signed_update_ready: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgressEvent {
    pub stage: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub percent: Option<u8>,
}

pub fn updates_allowed(mock_active: bool) -> bool {
    !mock_active
}

pub fn offline_update_status() -> UpdateStatus {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    UpdateStatus {
        available: false,
        latest_version: current_version.clone(),
        current_version,
        release_url: RELEASES_PAGE_URL.to_string(),
        published_at: None,
        notes: None,
        signed_update_ready: false,
    }
}

/// Checks for a signed Tauri update manifest (`latest.json`) first, falling back to
/// the GitHub Releases API if no signed manifest is published yet.
pub async fn check_signed_update(
    app: &AppHandle,
    state: &AppState,
) -> Result<UpdateStatus, AppError> {
    if !updates_allowed(state.mock_active.load(Ordering::Relaxed)) {
        return Ok(offline_update_status());
    }
    let current_str = env!("CARGO_PKG_VERSION").to_string();

    let mut signed_err: Option<String> = None;
    if let Ok(updater) = app.updater() {
        let signed_check = updater.check().await;
        if !updates_allowed(state.mock_active.load(Ordering::Relaxed)) {
            return Ok(offline_update_status());
        }
        match signed_check {
            Ok(Some(update)) => {
                let published_at = update.date.map(|d| d.to_string());
                info!(
                    current = %current_str,
                    latest = %update.version,
                    "Signed Dropilot update available"
                );
                return Ok(UpdateStatus {
                    available: true,
                    current_version: current_str,
                    latest_version: update.version,
                    release_url: RELEASES_PAGE_URL.to_string(),
                    published_at,
                    notes: update.body.filter(|b| !b.trim().is_empty()),
                    signed_update_ready: true,
                });
            }
            Ok(None) => {
                return Ok(UpdateStatus {
                    available: false,
                    current_version: current_str.clone(),
                    latest_version: current_str,
                    release_url: RELEASES_PAGE_URL.to_string(),
                    published_at: None,
                    notes: None,
                    signed_update_ready: true,
                });
            }
            Err(err) => {
                signed_err = Some(err.to_string());
                debug!(error = %err, "Signed updater manifest unavailable; checking GitHub Releases API");
            }
        }
    }

    if !updates_allowed(state.mock_active.load(Ordering::Relaxed)) {
        return Ok(offline_update_status());
    }
    match check_for_updates(&state.http).await {
        Ok(status) => Ok(status),
        Err(AppError::Network(msg)) if msg.contains("HTTP 404") => Ok(UpdateStatus {
            available: false,
            current_version: current_str.clone(),
            latest_version: current_str,
            release_url: RELEASES_PAGE_URL.to_string(),
            published_at: None,
            notes: None,
            signed_update_ready: false,
        }),
        Err(err) => {
            if let Some(ref signed) = signed_err {
                warn!(signed_error = %signed, fallback_error = %err, "Update check failed");
            }
            Err(err)
        }
    }
}

/// Downloads, cryptographically verifies, and installs the latest signed update,
/// emitting progress events to the UI and cleanly restarting Dropilot when finished.
pub async fn install_signed_update(app: &AppHandle, state: &Arc<AppState>) -> Result<(), String> {
    if !updates_allowed(state.mock_active.load(Ordering::Relaxed)) {
        return Err("Updates are disabled in offline mock mode.".to_string());
    }
    let updater = app
        .updater()
        .map_err(|e| format!("Updater unavailable: {e}"))?;

    let update = updater
        .check()
        .await
        .map_err(|e| format!("Failed checking signed update manifest: {e}"))?;
    if !updates_allowed(state.mock_active.load(Ordering::Relaxed)) {
        return Err("Updates are disabled in offline mock mode.".to_string());
    }
    let Some(update) = update else {
        return Err("Dropilot is already on the latest version.".to_string());
    };

    info!(version = %update.version, "Starting signed update download and verification");
    emit_progress(app, "downloading", 0, None);

    let downloaded = Arc::new(AtomicU64::new(0));
    let dl_counter = downloaded.clone();
    let app_for_chunk = app.clone();
    let app_for_finish = app.clone();

    update
        .download_and_install(
            move |chunk_len, content_len| {
                let total_dl = dl_counter.fetch_add(chunk_len as u64, Ordering::Relaxed)
                    + chunk_len as u64;
                emit_progress(&app_for_chunk, "downloading", total_dl, content_len);
            },
            move || {
                emit_progress(&app_for_finish, "installing", 0, None);
            },
        )
        .await
        .map_err(|e| {
            emit_progress(app, "error", 0, None);
            format!("Update download or signature verification failed: {e}")
        })?;

    emit_progress(app, "restarting", 0, None);
    info!("Signed update installed; shutting down miners and restarting Dropilot");

    // Shut down miners and bridge windows cleanly so the updated executable launches smoothly.
    state.allow_exit.store(true, Ordering::SeqCst);
    state.send_all_miners(MinerControl::Shutdown).await;
    close_kick_bridge_window(app);
    close_twitch_bridge_window(app);
    close_epic_claim_window(app);

    app.restart();
}

fn emit_progress(app: &AppHandle, stage: &str, downloaded_bytes: u64, total_bytes: Option<u64>) {
    if app.get_webview_window("main").is_none() {
        return;
    }
    let percent = total_bytes.and_then(|total| {
        downloaded_bytes
            .saturating_mul(100)
            .checked_div(total)
            .map(|p| p.min(100) as u8)
    });
    let _ = app.emit(
        UPDATE_PROGRESS_EVENT,
        UpdateProgressEvent {
            stage: stage.to_string(),
            downloaded_bytes,
            total_bytes,
            percent,
        },
    );
}

pub async fn check_for_updates(http: &AllowlistedClient) -> Result<UpdateStatus, AppError> {
    let endpoint = format!("https://{UPDATE_CHECK_HOST}{UPDATE_CHECK_PATH}");
    let release: GitHubRelease = http.fetch_update_json(&endpoint).await?;

    let current_str = env!("CARGO_PKG_VERSION");
    let latest_clean = release.tag_name.trim_start_matches('v');

    let current_ver = Version::parse(current_str)
        .map_err(|e| AppError::Internal(format!("Invalid crate version: {e}")))?;
    let latest_ver = Version::parse(latest_clean)
        .map_err(|e| AppError::PlatformChanged(format!("Invalid GitHub release tag: {e}")))?;

    let safe_url = if release
        .html_url
        .starts_with("https://github.com/theflash1211/dropilot-releases/")
    {
        release.html_url
    } else {
        RELEASES_PAGE_URL.to_string()
    };

    Ok(UpdateStatus {
        available: latest_ver > current_ver,
        current_version: current_str.to_string(),
        latest_version: latest_ver.to_string(),
        release_url: safe_url,
        published_at: release.published_at,
        notes: release.body.filter(|b| !b.trim().is_empty()),
        signed_update_ready: false,
    })
}

#[cfg(test)]
mod tests {
    use super::{offline_update_status, updates_allowed};

    #[test]
    fn mock_mode_disables_update_network_requests() {
        assert!(!updates_allowed(true));
        assert!(updates_allowed(false));
    }

    #[test]
    fn offline_update_status_cannot_trigger_an_install() {
        let status = offline_update_status();
        assert!(!status.available);
        assert!(!status.signed_update_ready);
        assert_eq!(status.current_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(status.latest_version, status.current_version);
    }
}
