use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

use crate::logging::LogEntry;
use crate::platforms::AppSnapshot;
use crate::settings::Settings;

pub const EVENT_SNAPSHOT: &str = "dropilot://snapshot";
pub const EVENT_LOG: &str = "dropilot://log";
pub const EVENT_SETTINGS: &str = "dropilot://settings";

#[derive(Debug)]
struct ThrottleState {
    last_sent_at: Option<Instant>,
    last_payload: Option<Arc<AppSnapshot>>,
}

/// Throttles snapshot events to at most 1/sec, deduplicates identical payloads,
/// and suppresses all emissions when the main window is destroyed (tray mode).
#[derive(Clone, Debug)]
pub struct EventHub {
    inner: Arc<Mutex<ThrottleState>>,
}

impl Default for EventHub {
    fn default() -> Self {
        Self::new()
    }
}

impl EventHub {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ThrottleState {
                last_sent_at: None,
                last_payload: None,
            })),
        }
    }

    pub async fn emit_snapshot_throttled(
        &self,
        app: &AppHandle,
        snapshot: Arc<AppSnapshot>,
        force: bool,
    ) {
        if app.get_webview_window("main").is_none() {
            return;
        }

        let should_emit = {
            let mut guard = self.inner.lock().await;
            let changed = guard.last_payload.as_deref() != Some(snapshot.as_ref());
            if !changed && !force {
                false
            } else {
                let elapsed_ok = match guard.last_sent_at {
                    None => true,
                    Some(prev) => prev.elapsed() >= Duration::from_secs(1),
                };
                if elapsed_ok || force {
                    guard.last_sent_at = Some(Instant::now());
                    guard.last_payload = Some(snapshot.clone());
                    true
                } else {
                    false
                }
            }
        };

        if should_emit {
            let _ = app.emit_to("main", EVENT_SNAPSHOT, snapshot.as_ref());
        }
    }

    pub fn emit_log_if_open(app: &AppHandle, entry: &LogEntry) {
        if app.get_webview_window("main").is_some() {
            let _ = app.emit_to("main", EVENT_LOG, entry);
        }
    }

    pub fn emit_settings_if_open(app: &AppHandle, settings: &Settings) {
        if app.get_webview_window("main").is_some() {
            let _ = app.emit_to("main", EVENT_SETTINGS, settings);
        }
    }

    pub async fn reset_cache(&self) {
        let mut guard = self.inner.lock().await;
        guard.last_payload = None;
        guard.last_sent_at = None;
    }
}
