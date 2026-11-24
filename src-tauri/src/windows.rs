use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, Theme, WebviewUrl, WebviewWindowBuilder};
use tracing::{info, warn};

use crate::settings::{
    is_default_profile, ThemeMode, WindowBounds,
};
use crate::state::AppState;

/// Auxiliary WebView2 window labels that must stay alive if open when `"main"` is
/// minimized or hidden to the system tray (login modals, background API bridges,
/// drops inventory views, Epic claim modal, and compatibility watch windows).
pub const AUX_WEBVIEW_LABELS: &[&str] = &[
    "login-twitch",
    "login-kick",
    "kick-bridge",
    "twitch-bridge",
    "inventory-twitch",
    "inventory-kick",
    "epic-claim",
    "compat-watch-twitch",
    "compat-watch-kick",
];

pub const AUX_WEBVIEW_PREFIXES: &[&str] = &[
    "login-",
    "inventory-",
    "kick-bridge",
    "twitch-bridge",
    "epic-claim",
    "compat-watch-",
];

static LAST_MAIN_BOUNDS: Mutex<Option<WindowBounds>> = Mutex::new(None);
static MAIN_WEBVIEW_SUSPENDED: AtomicBool = AtomicBool::new(false);
static MAIN_WEBVIEW_LOW_MEM: AtomicBool = AtomicBool::new(false);

/// Ultra-light Chromium flags for Dropilot's local Svelte UI window (`main`).
/// Disables the separate GPU & out-of-process audio processes, caps disk/media caches
/// and V8 heap, and disables unused Chromium background features while preserving WRY's defaults.
pub const UI_WEBVIEW_BROWSER_ARGS: &str = concat!(
    "--disable-gpu ",
    "--disable-gpu-compositing ",
    "--disable-gpu-shader-disk-cache ",
    "--renderer-process-limit=1 ",
    "--disable-extensions ",
    "--disable-background-networking ",
    "--disable-sync ",
    "--disable-component-update ",
    "--disable-default-apps ",
    "--disable-domain-reliability ",
    "--no-first-run ",
    "--disk-cache-size=4194304 ",
    "--media-cache-size=1048576 ",
    "--js-flags=--max-old-space-size=48 ",
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,Translate,BackForwardCache,MediaRouter,OptimizationHints,InterestFeedContentSuggestions,AutofillServerCommunication,HeavyAdIntervention,AudioServiceOutOfProcess"
);

/// Lean Chromium flags shared across ALL platform windows using `webview_data_dir()`
/// (login, inventory, and rare fallback bridges). Keeps GPU intact for official login/security checks
/// and disables occluded-window throttling so short-lived hidden bridges render React pages promptly.
pub const PLATFORM_WEBVIEW_BROWSER_ARGS: &str = concat!(
    "--autoplay-policy=no-user-gesture-required ",
    "--disable-extensions ",
    "--disable-background-networking ",
    "--disable-background-timer-throttling ",
    "--disable-renderer-backgrounding ",
    "--disable-backgrounding-occluded-windows ",
    "--disable-sync ",
    "--disable-component-update ",
    "--disable-default-apps ",
    "--no-first-run ",
    "--disk-cache-size=16777216 ",
    "--media-cache-size=2097152 ",
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,Translate,BackForwardCache,MediaRouter,OptimizationHints,InterestFeedContentSuggestions"
);

/// Isolated WebView2 user-data directory solely for Dropilot's local Svelte UI (`main`).
/// Separate from `webview_data_dir()` so `main` can use stricter low-RAM flags (`--disable-gpu`)
/// and never accumulates Twitch/Kick site caches or service workers.
pub fn ui_webview_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Dropilot")
        .join("WebView2-UI")
}

/// Isolated WebView2 user-data directory for official Twitch/Kick login & inventory sessions.
pub fn webview_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Dropilot")
        .join("WebView2")
}

pub fn sanitize_profile_id_for_fs(profile_id: &str) -> String {
    let cleaned: String = profile_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if cleaned.is_empty() {
        "profile".to_string()
    } else {
        cleaned
    }
}

/// Isolated WebView2 user-data directory for a specific account profile.
/// Default profiles reuse `webview_data_dir()` to preserve existing cookies;
/// additional profiles get their own isolated directory under `WebView2-Profiles`.
pub fn profile_webview_data_dir(profile_id: &str) -> PathBuf {
    if is_default_profile(profile_id) {
        return webview_data_dir();
    }
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Dropilot")
        .join("WebView2-Profiles")
        .join(sanitize_profile_id_for_fs(profile_id))
}

/// Returns `true` if `label` identifies an auxiliary/login/bridge/inventory/compat WebView2 window.
pub fn is_aux_webview_label(label: &str) -> bool {
    AUX_WEBVIEW_LABELS.contains(&label)
        || AUX_WEBVIEW_PREFIXES
            .iter()
            .any(|prefix| label.starts_with(prefix))
}

/// Returns `true` if any auxiliary/login/bridge label matching `is_aux_webview_label` is reported open.
pub fn any_aux_label_active<F>(exists: F) -> bool
where
    F: Fn(&str) -> bool,
{
    AUX_WEBVIEW_LABELS.iter().any(|label| exists(label))
}

/// Returns `true` when any login, bridge, inventory, Epic claim, or compat-watch WebView2 window is open.
pub fn has_active_aux_webview(app: &AppHandle) -> bool {
    app.webview_windows()
        .keys()
        .any(|label| is_aux_webview_label(label))
}

/// Stores validated non-minimized `"main"` window bounds in memory for restore after minimize-to-tray.
pub fn store_cached_main_bounds(bounds: Option<WindowBounds>) {
    if let Some(b) = bounds {
        if let Ok(mut guard) = LAST_MAIN_BOUNDS.lock() {
            *guard = Some(b);
        }
    }
}

/// Captures and caches `"main"` window bounds only when the window is not minimized.
pub fn update_cached_main_bounds(window: &tauri::Window) {
    if window.is_minimized().unwrap_or(false) {
        return;
    }
    store_cached_main_bounds(capture_bounds_sync(window));
}

/// Returns the last-known valid `"main"` window bounds cached in memory.
pub fn cached_main_bounds() -> Option<WindowBounds> {
    LAST_MAIN_BOUNDS.lock().ok().and_then(|g| g.clone())
}

/// Returns the current Windows Package Family Name (e.g. `io.dropilot.app_...`) when running
/// with sparse/full MSIX Package Identity, or `None` when unpackaged.
#[cfg(windows)]
pub fn current_package_family_name() -> Option<String> {
    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS};
    use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFamilyName;

    let mut len: u32 = 128;
    let mut buf = vec![0u16; len as usize];
    let mut status = unsafe { GetCurrentPackageFamilyName(&mut len, buf.as_mut_ptr()) };
    if status == ERROR_INSUFFICIENT_BUFFER && len > 0 {
        buf.resize(len as usize, 0);
        status = unsafe { GetCurrentPackageFamilyName(&mut len, buf.as_mut_ptr()) };
    }
    if status != ERROR_SUCCESS || len <= 1 {
        return None;
    }
    let trimmed_len = (len as usize).saturating_sub(1).min(buf.len());
    let name = String::from_utf16_lossy(&buf[..trimmed_len]);
    let trimmed = name.trim_end_matches('\0').trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(not(windows))]
pub fn current_package_family_name() -> Option<String> {
    None
}

/// Configures Windows process identity (`AppUserModelID`) and assigns Dropilot to an
/// anonymous Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` so all child
/// `msedgewebview2.exe` processes terminate automatically when Dropilot exits or crashes.
#[cfg(windows)]
pub fn configure_windows_process_group() {
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    use windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    static PROCESS_JOB_HANDLE: OnceLock<isize> = OnceLock::new();

    match current_package_family_name() {
        Some(pfn) => {
            info!(
                package_family_name = %pfn,
                "Running with Windows Package Identity"
            );
        }
        None => {
            info!("Running unpackaged without Windows Package Identity");
            let app_id: Vec<u16> = "io.dropilot.app\0".encode_utf16().collect();
            let hr = unsafe { SetCurrentProcessExplicitAppUserModelID(app_id.as_ptr()) };
            if hr < 0 {
                warn!(hresult = hr, "Failed setting Windows AppUserModelID");
            }
        }
    }

    if PROCESS_JOB_HANDLE.get().is_some() {
        return;
    }

    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            warn!(
                error = GetLastError(),
                "Failed creating Windows Job Object for WebView2 subprocess cleanup"
            );
            return;
        }

        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        let ok = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION as *const std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        if ok == 0 {
            warn!(
                error = GetLastError(),
                "Failed configuring KILL_ON_JOB_CLOSE on Windows Job Object"
            );
            CloseHandle(job);
            return;
        }

        if AssignProcessToJobObject(job, GetCurrentProcess()) == 0 {
            warn!(
                error = GetLastError(),
                "Could not assign Dropilot to Job Object (process may already be in a restricted nested job)"
            );
            CloseHandle(job);
            return;
        }

        if PROCESS_JOB_HANDLE.set(job as isize).is_err() {
            CloseHandle(job);
            return;
        }

        info!("Configured Windows KILL_ON_JOB_CLOSE Job Object");
    }
}

#[cfg(not(windows))]
pub fn configure_windows_process_group() {}

/// One-time migration of the isolated Twitch/Kick WebView2 session folder from legacy `DropPilot`
/// to `Dropilot` so existing logged-in browser sessions carry over automatically.
pub fn migrate_legacy_webview_dir() {
    let new_dir = webview_data_dir();
    if new_dir.exists() {
        return;
    }
    let legacy_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("DropPilot")
        .join("WebView2");
    if !legacy_dir.exists() {
        return;
    }
    if let Some(parent) = new_dir.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::rename(&legacy_dir, &new_dir);
}

#[cfg(windows)]
pub fn apply_low_memory_mode(win: &tauri::WebviewWindow) {
    set_webview_memory_level(win, true);
}

#[cfg(not(windows))]
pub fn apply_low_memory_mode(_win: &tauri::WebviewWindow) {}

/// Adjusts the `"main"` WebView2 memory usage target level only when transitioning
/// between active focus and background/Alt-Tab blur, avoiding redundant COM calls.
#[cfg(windows)]
pub fn set_main_webview_low_memory(win: &tauri::WebviewWindow, low: bool) {
    if win.label() != "main" {
        return;
    }
    if MAIN_WEBVIEW_LOW_MEM.swap(low, Ordering::Relaxed) == low {
        return;
    }
    set_webview_memory_level(win, low);
}

#[cfg(not(windows))]
pub fn set_main_webview_low_memory(_win: &tauri::WebviewWindow, _low: bool) {}

#[cfg(windows)]
fn set_webview_memory_level(win: &tauri::WebviewWindow, low: bool) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    };
    use windows::core::Interface;

    let level = if low {
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
    } else {
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
    };
    let _ = win.with_webview(move |wv| unsafe {
        if let Ok(core) = wv.controller().CoreWebView2() {
            if let Ok(core19) = core.cast::<ICoreWebView2_19>() {
                let _ = core19.SetMemoryUsageTargetLevel(level);
            }
        }
    });
}

/// Suspends the WebView2 renderer when minimized or hidden with an active auxiliary window.
#[cfg(windows)]
pub fn suspend_webview(win: &tauri::WebviewWindow) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_19, ICoreWebView2_3, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
    };
    use webview2_com::TrySuspendCompletedHandler;
    use windows::core::Interface;

    if win.label() == "main" {
        MAIN_WEBVIEW_SUSPENDED.store(true, Ordering::Relaxed);
        MAIN_WEBVIEW_LOW_MEM.store(true, Ordering::Relaxed);
    }

    let _ = win.with_webview(|wv| unsafe {
        let controller = wv.controller();
        let _ = controller.SetIsVisible(false);
        if let Ok(core) = controller.CoreWebView2() {
            if let Ok(core19) = core.cast::<ICoreWebView2_19>() {
                let _ = core19
                    .SetMemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW);
            }
            if let Ok(core3) = core.cast::<ICoreWebView2_3>() {
                let handler =
                    TrySuspendCompletedHandler::create(Box::new(|_hr, _succeeded| Ok(())));
                let _ = core3.TrySuspend(&handler);
            }
        }
    });
}

#[cfg(not(windows))]
pub fn suspend_webview(_win: &tauri::WebviewWindow) {}

/// Resumes a suspended WebView2 renderer instantaneously when restored or focused.
#[cfg(windows)]
pub fn resume_webview(win: &tauri::WebviewWindow) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
    use windows::core::Interface;

    if win.label() == "main" && !MAIN_WEBVIEW_SUSPENDED.swap(false, Ordering::Relaxed) {
        return;
    }

    let _ = win.with_webview(|wv| unsafe {
        let controller = wv.controller();
        let _ = controller.SetIsVisible(true);
        if let Ok(core) = controller.CoreWebView2() {
            if let Ok(core3) = core.cast::<ICoreWebView2_3>() {
                let _ = core3.Resume();
            }
        }
    });
}

#[cfg(not(windows))]
pub fn resume_webview(_win: &tauri::WebviewWindow) {}

pub fn resolve_tauri_theme(mode: ThemeMode) -> Option<Theme> {
    match mode {
        ThemeMode::Dark => Some(Theme::Dark),
        ThemeMode::Light => Some(Theme::Light),
        ThemeMode::System => None,
    }
}

pub async fn ensure_main_window(app: &AppHandle, state: &Arc<AppState>) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window("main") {
        if existing.is_minimized().unwrap_or(false) {
            let _ = existing.unminimize();
        }
        if !existing.is_visible().unwrap_or(true) {
            let _ = existing.show();
        }
        let _ = existing.set_focus();
        resume_webview(&existing);
        set_main_webview_low_memory(&existing, false);
        return Ok(());
    }

    MAIN_WEBVIEW_SUSPENDED.store(false, Ordering::Relaxed);
    MAIN_WEBVIEW_LOW_MEM.store(false, Ordering::Relaxed);

    let (bounds, theme) = {
        let guard = state.settings.read().await;
        (guard.window_bounds.clone(), guard.theme)
    };

    store_cached_main_bounds(bounds.clone());

    let (width, height) = bounds
        .as_ref()
        .map(|b| (b.width as f64, b.height as f64))
        .unwrap_or((1040.0, 700.0));

    let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("Dropilot")
        .inner_size(width, height)
        .min_inner_size(600.0, 480.0)
        .resizable(true)
        .theme(resolve_tauri_theme(theme))
        .data_directory(ui_webview_data_dir())
        .additional_browser_args(UI_WEBVIEW_BROWSER_ARGS);

    builder = if let Some(b) = bounds {
        builder.position(b.x as f64, b.y as f64)
    } else {
        builder.center()
    };

    let win = builder
        .build()
        .map_err(|e| format!("Failed creating main window: {e}"))?;

    update_cached_main_bounds(&win.as_ref().window());
    set_main_webview_low_memory(&win, false);

    state.events.reset_cache().await;
    Ok(())
}

/// Destroys the `"main"` UI WebView2 window to the system tray so zero `msedgewebview2.exe`
/// processes remain while miners, scheduler, webhooks, Epic polling, and tray keep running.
///
/// If any auxiliary window (`login-*`, `*-bridge`, `inventory-*`, `epic-claim`, `compat-watch-*`)
/// is currently open, destruction is skipped so those windows are never interrupted; `"main"` is
/// gently hidden/suspended instead and `false` is returned.
pub async fn destroy_main_to_tray(
    app: &AppHandle,
    state: &Arc<AppState>,
    bounds_override: Option<WindowBounds>,
) -> bool {
    if let Some(b) = bounds_override.or_else(cached_main_bounds) {
        {
            let mut guard = state.settings.write().await;
            guard.window_bounds = Some(b);
        }
        let _ = state.persist_settings().await;
    }

    if has_active_aux_webview(app) {
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.hide();
            suspend_webview(&win);
        }
        info!(
            "Skipped destroying UI WebView2 while an auxiliary/login/bridge window is open; hid main window instead"
        );
        return false;
    }

    state.events.reset_cache().await;
    MAIN_WEBVIEW_SUSPENDED.store(false, Ordering::Relaxed);
    MAIN_WEBVIEW_LOW_MEM.store(false, Ordering::Relaxed);
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.destroy();
        info!("UI WebView2 destroyed to tray; only dropilot.exe remains");
        true
    } else {
        false
    }
}

pub fn capture_bounds_sync(window: &tauri::Window) -> Option<WindowBounds> {
    let scale = window.scale_factor().unwrap_or(1.0).max(0.5);
    let pos: PhysicalPosition<i32> = window.outer_position().ok()?;
    let size: PhysicalSize<u32> = window.inner_size().ok()?;
    let logical_w = ((size.width as f64) / scale).round() as u32;
    let logical_h = ((size.height as f64) / scale).round() as u32;
    let logical_x = ((pos.x as f64) / scale).round() as i32;
    let logical_y = ((pos.y as f64) / scale).round() as i32;

    if logical_w < 400 || logical_h < 300 || logical_x < -3000 || logical_y < -3000 {
        return None;
    }

    Some(WindowBounds {
        x: logical_x,
        y: logical_y,
        width: logical_w.clamp(600, 3840),
        height: logical_h.clamp(480, 2160),
    })
}

pub fn apply_window_theme(app: &AppHandle, mode: ThemeMode) {
    let resolved = resolve_tauri_theme(mode);
    for (label, win) in app.webview_windows() {
        if label == "main"
            || label.starts_with("login-")
            || label.starts_with("inventory-")
            || label == "epic-claim"
        {
            if let Err(err) = win.set_theme(resolved) {
                warn!(window = %label, error = %err, "Failed updating window theme");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aux_webview_labels_include_all_login_bridge_and_overlay_windows() {
        for expected in [
            "login-twitch",
            "login-kick",
            "kick-bridge",
            "twitch-bridge",
            "inventory-twitch",
            "inventory-kick",
            "epic-claim",
            "compat-watch-twitch",
            "compat-watch-kick",
        ] {
            assert!(
                AUX_WEBVIEW_LABELS.contains(&expected),
                "Missing expected auxiliary label: {expected}"
            );
        }
        assert_eq!(AUX_WEBVIEW_LABELS.len(), 9);
    }

    #[test]
    fn aux_guard_detects_active_aux_windows_and_ignores_main() {
        assert!(!any_aux_label_active(|label| label == "main"));
        assert!(!any_aux_label_active(|_| false));
        assert!(!is_aux_webview_label("main"));

        for label in AUX_WEBVIEW_LABELS {
            assert!(
                any_aux_label_active(|candidate| candidate == *label),
                "Aux guard should return true when {label} is active"
            );
            assert!(
                is_aux_webview_label(label),
                "Prefix guard should match {label}"
            );
        }

        for profile_label in [
            "login-twitch-alt1",
            "login-kick-alt2",
            "inventory-twitch-alt1",
            "kick-bridge-alt2",
            "twitch-bridge-alt1",
            "compat-watch-twitch-alt1",
            "compat-watch-kick-alt2",
        ] {
            assert!(
                is_aux_webview_label(profile_label),
                "Prefix guard should match multi-account window {profile_label}"
            );
        }
    }

    #[test]
    fn profile_webview_data_dir_preserves_default_and_isolates_extra_profiles() {
        let base = webview_data_dir();
        assert_eq!(
            profile_webview_data_dir(crate::settings::DEFAULT_TWITCH_PROFILE_ID),
            base
        );
        assert_eq!(
            profile_webview_data_dir(crate::settings::DEFAULT_KICK_PROFILE_ID),
            base
        );

        let alt = profile_webview_data_dir("twitch-alt-1");
        assert_ne!(alt, base);
        assert!(alt.ends_with("twitch-alt-1"));
        assert!(alt
            .parent()
            .and_then(|p| p.file_name())
            .is_some_and(|n| n == "WebView2-Profiles"));
    }

    #[test]
    fn cached_main_bounds_ignores_none_and_keeps_last_valid_bounds() {
        let sample = WindowBounds {
            x: 120,
            y: 80,
            width: 1120,
            height: 760,
        };
        store_cached_main_bounds(Some(sample.clone()));
        assert_eq!(cached_main_bounds(), Some(sample.clone()));

        // Passing None (e.g. minimized (-32000, -32000) coordinates) must preserve prior valid bounds.
        store_cached_main_bounds(None);
        assert_eq!(cached_main_bounds(), Some(sample));
    }
}
