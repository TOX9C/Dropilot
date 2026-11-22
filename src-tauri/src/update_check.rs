use semver::Version;
use serde::{Deserialize, Serialize};

use crate::net::allowlist::{UPDATE_CHECK_HOST, UPDATE_CHECK_PATH};
use crate::net::client::AllowlistedClient;
use crate::platforms::AppError;

pub const RELEASES_PAGE_URL: &str = "https://github.com/theflash1211/dropilot/releases";

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    published_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub available: bool,
    pub current_version: String,
    pub latest_version: String,
    pub release_url: String,
    pub published_at: Option<String>,
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
        .starts_with("https://github.com/theflash1211/dropilot/")
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
    })
}
