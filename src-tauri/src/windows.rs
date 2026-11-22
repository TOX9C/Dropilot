use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, Theme, WebviewUrl, WebviewWindowBuilder};
use tracing::warn;

use crate::settings::{ThemeMode, WindowBounds};
use crate::state::AppState;

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
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
    };
    use windows::core::Interface;

    let _ = win.with_webview(|wv| unsafe {
        if let Ok(core) = wv.controller().CoreWebView2() {
            if let Ok(core19) = core.cast::<ICoreWebView2_19>() {
                let _ = core19
                    .SetMemoryUsageTargetLevel(COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW);
            }
        }
    });
}

#[cfg(not(windows))]
pub fn apply_low_memory_mode(_win: &tauri::WebviewWindow) {}

/// Suspends the WebView2 renderer when minimized to taskbar (0% CPU, aggressively trimmed RAM).
#[cfg(windows)]
pub fn suspend_webview(win: &tauri::WebviewWindow) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_19, ICoreWebView2_3, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
    };
    use webview2_com::TrySuspendCompletedHandler;
    use windows::core::Interface;

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
        let _ = existing.unminimize();
        let _ = existing.show();
        let _ = existing.set_focus();
        resume_webview(&existing);
        return Ok(());
    }

    let (bounds, theme) = {
        let guard = state.settings.read().await;
        (guard.window_bounds.clone(), guard.theme)
    };

    let (width, height) = bounds
        .as_ref()
        .map(|b| (b.width as f64, b.height as f64))
        .unwrap_or((1040.0, 700.0));

    let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("Dropilot")
        .inner_size(width, height)
        .min_inner_size(840.0, 560.0)
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

    apply_low_memory_mode(&win);

    state.events.reset_cache().await;
    Ok(())
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
        width: logical_w.clamp(840, 3840),
        height: logical_h.clamp(560, 2160),
    })
}

pub fn apply_window_theme(app: &AppHandle, mode: ThemeMode) {
    let resolved = resolve_tauri_theme(mode);
    for label in ["main", "login-twitch", "login-kick", "inventory-twitch", "inventory-kick"] {
        if let Some(win) = app.get_webview_window(label) {
            if let Err(err) = win.set_theme(resolved) {
                warn!(window = label, error = %err, "Failed updating window theme");
            }
        }
    }
}
