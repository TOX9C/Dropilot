use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;
use tracing::{info, warn};

pub fn sync_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    let current = manager.is_enabled().unwrap_or(false);
    if current == enabled {
        return Ok(());
    }

    if enabled {
        manager
            .enable()
            .map_err(|e| format!("Failed enabling Start with Windows: {e}"))?;
        info!("Enabled Start with Windows (HKCU Run)");
    } else {
        manager
            .disable()
            .map_err(|e| format!("Failed disabling Start with Windows: {e}"))?;
        info!("Disabled Start with Windows");
    }

    Ok(())
}

pub fn is_autostart_enabled(app: &AppHandle) -> bool {
    match app.autolaunch().is_enabled() {
        Ok(val) => val,
        Err(err) => {
            warn!(error = %err, "Failed reading autostart state");
            false
        }
    }
}
