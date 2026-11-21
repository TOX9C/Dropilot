use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, Theme, WebviewUrl, WebviewWindowBuilder};
use tracing::warn;

use crate::settings::{ThemeMode, WindowBounds};
use crate::state::AppState;

pub fn webview_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("DropPilot")
        .join("WebView2")
}

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
        .title("DropPilot")
        .inner_size(width, height)
        .min_inner_size(840.0, 560.0)
        .resizable(true)
        .theme(resolve_tauri_theme(theme))
        .data_directory(webview_data_dir());

    builder = if let Some(b) = bounds {
        builder.position(b.x as f64, b.y as f64)
    } else {
        builder.center()
    };

    builder
        .build()
        .map_err(|e| format!("Failed creating main window: {e}"))?;

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
