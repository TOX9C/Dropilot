use std::collections::VecDeque;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::reload;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, Registry};

use crate::net::redact::redact;
use crate::platforms::PlatformId;
use crate::settings::LogLevelSetting;

pub const MAX_IN_MEMORY_LOGS: usize = 1_000;
const MAX_SINGLE_LOG_BYTES: u64 = 5 * 1024 * 1024;
const MAX_LOG_FILES: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: u64,
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub platform: Option<PlatformId>,
    pub message: String,
}

#[derive(Debug)]
struct InnerRing {
    next_id: u64,
    entries: VecDeque<LogEntry>,
}

type LogEmitter = Box<dyn Fn(&LogEntry) + Send + Sync>;

#[derive(Clone)]
pub struct LogBuffer {
    inner: Arc<Mutex<InnerRing>>,
    on_entry: Arc<Mutex<Option<LogEmitter>>>,
}

impl std::fmt::Debug for LogBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogBuffer").finish_non_exhaustive()
    }
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl LogBuffer {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(InnerRing {
                next_id: 1,
                entries: VecDeque::with_capacity(MAX_IN_MEMORY_LOGS),
            })),
            on_entry: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_emitter<F>(&self, emitter: F)
    where
        F: Fn(&LogEntry) + Send + Sync + 'static,
    {
        if let Ok(mut guard) = self.on_entry.lock() {
            *guard = Some(Box::new(emitter));
        }
    }

    pub fn push(&self, level: LogLevel, platform: Option<PlatformId>, raw_message: &str) {
        let clean = redact(raw_message);
        let entry = {
            let Ok(mut guard) = self.inner.lock() else {
                return;
            };
            let id = guard.next_id;
            guard.next_id = guard.next_id.saturating_add(1);
            let entry = LogEntry {
                id,
                timestamp: Utc::now(),
                level,
                platform,
                message: clean,
            };
            if guard.entries.len() >= MAX_IN_MEMORY_LOGS {
                guard.entries.pop_front();
            }
            guard.entries.push_back(entry.clone());
            entry
        };

        if let Ok(cb_guard) = self.on_entry.lock() {
            if let Some(ref cb) = *cb_guard {
                cb(&entry);
            }
        }
    }

    /// Returns newest-first log lines filtered optionally by minimum/exact level and platform.
    pub fn list(&self, level: Option<LogLevel>, platform: Option<PlatformId>) -> Vec<LogEntry> {
        let Ok(guard) = self.inner.lock() else {
            return Vec::new();
        };
        guard
            .entries
            .iter()
            .rev()
            .filter(|entry| match level {
                Some(filter_lvl) => entry.level == filter_lvl,
                None => true,
            })
            .filter(|entry| match platform {
                Some(filter_plat) => entry.platform == Some(filter_plat),
                None => true,
            })
            .cloned()
            .collect()
    }
}

pub struct LogHandle {
    filter_reload: reload::Handle<EnvFilter, Registry>,
    _guard: WorkerGuard,
}

fn env_filter_directive(level: LogLevelSetting) -> &'static str {
    match level {
        LogLevelSetting::Info => "dropilot_lib=info,dropilot=info,tauri_plugin_updater=off,warn",
        LogLevelSetting::Debug => "dropilot_lib=debug,dropilot=debug,tauri_plugin_updater=off,info",
    }
}

impl LogHandle {
    pub fn set_level(&self, level: LogLevelSetting) {
        let directive = env_filter_directive(level);
        if let Ok(filter) = EnvFilter::try_new(directive) {
            let _ = self.filter_reload.reload(filter);
        }
    }
}

pub fn log_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Dropilot")
        .join("logs")
}

fn enforce_log_size_cap(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.starts_with("dropilot.log") {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            if meta.len() > MAX_SINGLE_LOG_BYTES {
                let overflow_bak = path.with_extension("oversized.bak");
                let _ = fs::remove_file(&overflow_bak);
                let _ = fs::rename(&path, &overflow_bak);
            }
        }
    }
}

struct RedactingWriter<W>(W);

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_all(buf)?;
        Ok(buf.len())
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        // The fmt layer emits one complete UTF-8 event per write_all.
        let clean = redact(&String::from_utf8_lossy(buf));
        self.0.write_all(clean.as_bytes())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

struct MemoryLayer {
    buffer: LogBuffer,
}

#[derive(Default)]
struct EventVisitor {
    message: String,
    platform: Option<PlatformId>,
    fields: Vec<String>,
}

fn should_skip_field(name: &str) -> bool {
    // Strip tracing-log bridge metadata (log.file, log.line, log.target, log.module_path)
    // so local build paths like C:\Users\...\.cargo\registry never leak into in-app logs.
    name.starts_with("log.")
}

impl Visit for EventVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let name = field.name();
        if should_skip_field(name) {
            return;
        }
        let formatted = format!("{value:?}");
        let trimmed = formatted.trim_matches('"').to_string();
        if name == "message" {
            self.message = trimmed;
        } else if name == "platform" {
            self.platform = match trimmed.as_str() {
                "twitch" => Some(PlatformId::Twitch),
                "kick" => Some(PlatformId::Kick),
                _ => None,
            };
        } else {
            self.fields.push(format!("{name}={trimmed}"));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        let name = field.name();
        if should_skip_field(name) {
            return;
        }
        if name == "message" {
            self.message = value.to_string();
        } else if name == "platform" {
            self.platform = match value {
                "twitch" => Some(PlatformId::Twitch),
                "kick" => Some(PlatformId::Kick),
                _ => None,
            };
        } else {
            self.fields.push(format!("{name}={value}"));
        }
    }
}

impl<S: Subscriber> Layer<S> for MemoryLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let level = match *event.metadata().level() {
            Level::ERROR => LogLevel::Error,
            Level::WARN => LogLevel::Warn,
            Level::INFO => LogLevel::Info,
            Level::DEBUG | Level::TRACE => LogLevel::Debug,
        };

        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);

        let full = if visitor.fields.is_empty() {
            visitor.message
        } else if visitor.message.is_empty() {
            visitor.fields.join(" ")
        } else {
            format!("{} ({})", visitor.message, visitor.fields.join(", "))
        };

        if !full.is_empty() {
            self.buffer.push(level, visitor.platform, &full);
        }
    }
}

pub fn init_logging(initial_level: LogLevelSetting, buffer: LogBuffer) -> Result<LogHandle, String> {
    init_logging_at(&log_dir(), initial_level, buffer)
}

fn init_logging_at(
    dir: &Path,
    initial_level: LogLevelSetting,
    buffer: LogBuffer,
) -> Result<LogHandle, String> {
    fs::create_dir_all(dir).map_err(|e| format!("Failed creating log directory: {e}"))?;
    enforce_log_size_cap(dir);

    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("dropilot.log")
        .max_log_files(MAX_LOG_FILES)
        .build(dir)
        .map_err(|e| format!("Failed initializing rolling log appender: {e}"))?;

    let (non_blocking, guard) = tracing_appender::non_blocking(RedactingWriter(file_appender));

    let directive = env_filter_directive(initial_level);
    let filter = EnvFilter::try_new(directive).unwrap_or_else(|_| EnvFilter::new("info"));
    let (filter_layer, filter_reload) = reload::Layer::new(filter);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(false);

    let mem_layer = MemoryLayer { buffer };

    let registry = Registry::default()
        .with(filter_layer)
        .with(file_layer)
        .with(mem_layer);

    let _ = registry.try_init();

    Ok(LogHandle {
        filter_reload,
        _guard: guard,
    })
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::webhooks::WebhookDispatcher;

    #[test]
    fn skips_tracing_log_metadata_fields_and_silences_updater_noise() {
        assert!(should_skip_field("log.file"));
        assert!(should_skip_field("log.line"));
        assert!(should_skip_field("log.target"));
        assert!(should_skip_field("log.module_path"));
        assert!(!should_skip_field("error"));
        assert!(!should_skip_field("channel"));

        assert!(env_filter_directive(LogLevelSetting::Info).contains("tauri_plugin_updater=off"));
        assert!(env_filter_directive(LogLevelSetting::Debug).contains("tauri_plugin_updater=off"));
    }

    #[test]
    fn webhook_tokens_and_auth_headers_stay_out_of_all_log_sinks() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "dropilot-log-redaction-{}-{unique}",
            std::process::id()
        ));
        let buffer = LogBuffer::new();
        let handle = init_logging_at(&dir, LogLevelSetting::Info, buffer.clone())
            .expect("initialize logging");

        assert!(WebhookDispatcher::save_discord_url(
            "https://discord.com/api/v8/webhooks/123456789012345678/SECRETv8tok"
        )
        .is_err());
        assert!(WebhookDispatcher::save_discord_url(
            "https://discord.com/api/webhooks/notdigits/SECRETndtok"
        )
        .is_err());
        tracing::warn!(
            authorization = "Bearer SECRETheader",
            "header redaction check"
        );

        drop(handle);

        let mut file_logs = String::new();
        for entry in fs::read_dir(&dir).expect("read temporary log directory") {
            let path = entry.expect("log directory entry").path();
            if path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("dropilot.log"))
            {
                file_logs.push_str(&fs::read_to_string(path).expect("read log file"));
            }
        }
        let memory_logs = buffer
            .list(None, None)
            .into_iter()
            .map(|entry| entry.message)
            .collect::<Vec<_>>()
            .join("\n");

        fs::remove_dir_all(&dir).expect("remove temporary log directory");

        for secret in ["SECRETv8tok", "SECRETndtok", "SECRETheader"] {
            assert!(
                !file_logs.contains(secret),
                "file log leaked {secret}:\n{file_logs}"
            );
            assert!(
                !memory_logs.contains(secret),
                "memory log leaked {secret}:\n{memory_logs}"
            );
        }
    }
}
