use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::oneshot;
use tokio::time::{sleep, timeout};
use tracing::{info, warn};
use url::Url;

use crate::net::allowlist::{validate_url, AllowlistKind};
use crate::platforms::{AppError, PlatformId};
use crate::secrets::SecretStore;
use crate::state::{AppState, BridgeWaiters, MinerControl};
use crate::windows::{
    apply_low_memory_mode, resolve_tauri_theme, webview_data_dir, PLATFORM_WEBVIEW_BROWSER_ARGS,
};

const KICK_BRIDGE_SCRIPT: &str = r##"
(function () {
  if (window.location.hostname !== "kick.com") return;
  if (window.__dpKickBridgeInstalled) return;
  window.__dpKickBridgeInstalled = true;

  function isChallengePage() {
    try {
      const title = (document.title || "").toLowerCase();
      if (title.includes("just a moment") || title.includes("attention required")) {
        return true;
      }
      if (document.querySelector("#challenge-running, #cf-challenge-running")) {
        return true;
      }
    } catch (_) {}
    return false;
  }

  function readCookie(name) {
    try {
      const prefix = name + "=";
      const parts = (document.cookie || "").split(";");
      for (let i = 0; i < parts.length; i++) {
        const part = parts[i].trim();
        if (part.startsWith(prefix)) {
          const raw = part.slice(prefix.length);
          try {
            return decodeURIComponent(raw);
          } catch (_) {
            return raw;
          }
        }
      }
    } catch (_) {}
    return "";
  }

  let readySent = false;
  function maybeSignalReady() {
    if (readySent) return true;
    if (document.readyState === "loading" || isChallengePage()) return false;
    if (window.chrome && window.chrome.webview) {
      readySent = true;
      window.chrome.webview.postMessage(JSON.stringify({ dpKickReady: true }));
      return true;
    }
    return false;
  }

  if (!maybeSignalReady()) {
    let attempts = 0;
    const timer = setInterval(function () {
      attempts += 1;
      if (maybeSignalReady() || attempts >= 25) {
        clearInterval(timer);
      }
    }, 400);
  }

  if (window.chrome && window.chrome.webview) {
    window.chrome.webview.addEventListener("message", async function (event) {
      let msg;
      try {
        msg = typeof event.data === "string" ? JSON.parse(event.data) : event.data;
      } catch (_) {
        return;
      }
      if (!msg || msg.dpKickReq !== true || !msg.requestId || !msg.target) return;

      try {
        const parsed = new URL(msg.target);
        if (
          parsed.protocol !== "https:" ||
          (parsed.hostname !== "kick.com" &&
            parsed.hostname !== "web.kick.com" &&
            parsed.hostname !== "websockets.kick.com")
        ) {
          throw new Error("Blocked non-Kick bridge target");
        }

        const method = String(msg.method || "GET").toUpperCase() === "POST" ? "POST" : "GET";
        const headers = { Accept: "application/json" };
        if (parsed.hostname === "web.kick.com") {
          headers["x-app-platform"] = "web";
        }
        if (msg.bearer) {
          headers["Authorization"] = "Bearer " + msg.bearer;
        }
        if (parsed.hostname === "kick.com") {
          const xsrf = readCookie("XSRF-TOKEN");
          if (xsrf) {
            headers["X-XSRF-TOKEN"] = xsrf;
          }
        }
        const init = {
          method,
          credentials: "include",
          headers
        };
        if (msg.body && method === "POST") {
          headers["Content-Type"] = "application/json";
          init.body = msg.body;
        }

        let resp = await fetch(parsed.toString(), init);

        if (
          parsed.hostname === "kick.com" &&
          (resp.status === 401 || resp.status === 403) &&
          headers["Authorization"]
        ) {
          const cookieOnlyHeaders = Object.assign({}, headers);
          delete cookieOnlyHeaders["Authorization"];
          const retryInit = {
            method,
            credentials: "include",
            headers: cookieOnlyHeaders
          };
          if (init.body) {
            retryInit.body = init.body;
          }
          resp = await fetch(parsed.toString(), retryInit);
        }

        if (resp.status === 403) {
          await new Promise(function (resolve) {
            setTimeout(resolve, 1200);
          });
          resp = await fetch(parsed.toString(), init);
        }

        const text = await resp.text();
        window.chrome.webview.postMessage(
          JSON.stringify({
            dpKickBridge: true,
            requestId: msg.requestId,
            ok: resp.ok,
            status: resp.status,
            body: text
          })
        );
      } catch (err) {
        window.chrome.webview.postMessage(
          JSON.stringify({
            dpKickBridge: true,
            requestId: msg.requestId,
            ok: false,
            status: 0,
            body: String(err && err.message ? err.message : err)
          })
        );
      }
    });
  }
})();
"##;

const COMPAT_WATCH_SCRIPT: &str = r##"
(function () {
  if (window.__dpCompatWatchInstalled) return;
  window.__dpCompatWatchInstalled = true;

  // Prefer lowest video quality & unmuted player state inside the DOM.
  // Audio is muted natively at the WebView2 tab level (ICoreWebView2_8::SetIsMuted(true))
  // so the user hears zero sound while Twitch/Kick still credit drop watch time.
  try {
    const host = (window.location.hostname || "").toLowerCase();
    if (host.endsWith("twitch.tv")) {
      window.localStorage.setItem("video-quality", JSON.stringify({ default: "160p30" }));
      window.localStorage.setItem("video-muted", JSON.stringify({ default: false }));
      window.localStorage.setItem("volume", "0.5");
      window.localStorage.setItem("mature", "true");
    }
  } catch (_) {}

  function clickGates() {
    try {
      const selectors = [
        '[data-a-target="content-classification-gate-overlay-start-watching-button"]',
        '[data-a-target="player-overlay-mature-accept"]',
        '[data-a-target="player-overlay-click-handler"] button[aria-label*="Play" i]',
        'button[data-testid="mature-content-accept"]',
        'button[data-testid="accept-cookies"]'
      ];
      for (let i = 0; i < selectors.length; i++) {
        const btn = document.querySelector(selectors[i]);
        if (btn instanceof HTMLElement) {
          btn.click();
        }
      }
      // Kick & generic mature/start-watching overlays.
      const buttons = document.querySelectorAll("button");
      for (let i = 0; i < buttons.length; i++) {
        const b = buttons[i];
        const txt = (b.textContent || "").trim().toLowerCase();
        if (
          txt === "start watching" ||
          txt === "accept & continue" ||
          txt === "i am 18+" ||
          txt === "i'm 18+"
        ) {
          b.click();
        }
      }
    } catch (_) {}
  }

  function keepPlaybackAlive() {
    clickGates();
    try {
      const videos = document.querySelectorAll("video");
      videos.forEach(function (video) {
        if (video.paused && !video.ended) {
          video.volume = 0.5;
          video.muted = false;
          const p = video.play();
          if (p && typeof p.catch === "function") {
            p.catch(function () {
              // If autoplay policy demands a muted start, start muted then unmute once playing
              // (the WebView2 tab itself remains muted at the OS/Chromium level).
              video.muted = true;
              const p2 = video.play();
              if (p2 && typeof p2.then === "function") {
                p2.then(function () {
                  setTimeout(function () {
                    try {
                      video.muted = false;
                      video.volume = 0.5;
                    } catch (_) {}
                  }, 1200);
                }).catch(function () {});
              }
            });
          }
        } else if (!video.paused) {
          if (video.muted || video.volume === 0) {
            video.muted = false;
            video.volume = 0.5;
          }
        }
      });
    } catch (_) {}
  }

  setInterval(keepPlaybackAlive, 3000);
  document.addEventListener("DOMContentLoaded", keepPlaybackAlive);
  setTimeout(keepPlaybackAlive, 1500);
})();
"##;

const TWITCH_OPEN_LOGIN_MODAL_SCRIPT: &str = r##"
(function () {
  if (window.__dpTwitchModalOpened) return;
  if (!window.location.hostname.endsWith("twitch.tv")) return;

  window.__dpTwitchModalOpened = true;
  let attempts = 0;
  const timer = setInterval(function () {
    attempts += 1;
    if (attempts > 32) {
      clearInterval(timer);
      return;
    }
    try {
      if (document.querySelector('[data-a-target="user-menu-toggle"]')) {
        clearInterval(timer);
        return;
      }
      const btn = document.querySelector('[data-a-target="login-button"]');
      if (btn instanceof HTMLElement) {
        btn.click();
        clearInterval(timer);
      }
    } catch (_) {}
  }, 250);
})();
"##;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KickBridgeResult {
    pub status: u16,
    pub body: String,
}

pub fn login_window_label(platform: PlatformId) -> &'static str {
    match platform {
        PlatformId::Twitch => "login-twitch",
        PlatformId::Kick => "login-kick",
    }
}

pub fn compat_window_label(platform: PlatformId) -> &'static str {
    match platform {
        PlatformId::Twitch => "compat-watch-twitch",
        PlatformId::Kick => "compat-watch-kick",
    }
}

fn platform_login_url(platform: PlatformId) -> &'static str {
    match platform {
        PlatformId::Twitch => "https://www.twitch.tv",
        PlatformId::Kick => "https://kick.com",
    }
}

fn platform_cookie_url(platform: PlatformId) -> &'static str {
    match platform {
        PlatformId::Twitch => "https://www.twitch.tv",
        PlatformId::Kick => "https://kick.com",
    }
}

fn platform_token_cookie_name(platform: PlatformId) -> &'static str {
    match platform {
        PlatformId::Twitch => "auth-token",
        PlatformId::Kick => "session_token",
    }
}

pub async fn open_login_window(
    app: &AppHandle,
    state: Arc<AppState>,
    platform: PlatformId,
) -> Result<(), AppError> {
    let label = login_window_label(platform);
    if let Some(existing) = app.get_webview_window(label) {
        let _ = existing.show();
        let _ = existing.set_focus();
        return Ok(());
    }

    let start_url = Url::parse(platform_login_url(platform))
        .map_err(|e| AppError::Internal(format!("Invalid login URL: {e}")))?;

    let theme = state.settings.read().await.theme;
    let title = format!("Sign in to {} — DropPilot", platform.display_name());

    let mut builder = WebviewWindowBuilder::new(app, label, WebviewUrl::External(start_url))
        .title(&title)
        .inner_size(1060.0, 760.0)
        .min_inner_size(880.0, 640.0)
        .center()
        .resizable(true)
        .theme(resolve_tauri_theme(theme))
        .data_directory(webview_data_dir())
        .additional_browser_args(PLATFORM_WEBVIEW_BROWSER_ARGS)
        .on_navigation(|url| handle_restricted_navigation(url, AllowlistKind::LoginNavigation));

    if platform == PlatformId::Twitch {
        builder = builder.initialization_script(TWITCH_OPEN_LOGIN_MODAL_SCRIPT);
    }

    builder
        .build()
        .map_err(|e| AppError::Internal(format!("Failed opening login window: {e}")))?;

    info!(platform = platform.as_str(), "Opened official login window");

    // Poll DropPilot's own isolated WebView2 cookie jar off the UI thread.
    let app_clone = app.clone();
    let state_clone = state.clone();
    let label_string = label.to_string();

    tauri::async_runtime::spawn(async move {
        let cookie_url = match Url::parse(platform_cookie_url(platform)) {
            Ok(u) => u,
            Err(_) => return,
        };
        let target_cookie = platform_token_cookie_name(platform);

        loop {
            sleep(Duration::from_millis(1500)).await;

            let Some(win) = app_clone.get_webview_window(&label_string) else {
                break;
            };

            let url_clone = cookie_url.clone();
            let win_clone = win.clone();
            let cookies_res =
                tokio::task::spawn_blocking(move || win_clone.cookies_for_url(url_clone)).await;

            let Ok(Ok(cookies)) = cookies_res else {
                continue;
            };

            let token_opt = cookies.into_iter().find_map(|c| {
                if c.name() == target_cookie && !c.value().trim().is_empty() {
                    Some(c.value().to_string())
                } else {
                    None
                }
            });

            if let Some(token) = token_opt {
                if let Err(err) = SecretStore::save_token(platform, &token) {
                    warn!(platform = platform.as_str(), error = %err, "Failed saving login token");
                    continue;
                }
                info!(
                    platform = platform.as_str(),
                    "Captured official session cookie into Credential Manager"
                );
                if platform == PlatformId::Kick {
                    sleep(Duration::from_millis(600)).await;
                }
                if let Some(login_win) = app_clone.get_webview_window(&label_string) {
                    let _ = login_win.close();
                }
                state_clone
                    .send_miner(platform, MinerControl::LoginComplete)
                    .await;
                break;
            }
        }
    });

    Ok(())
}

pub async fn open_drops_inventory_window(
    app: &AppHandle,
    state: Arc<AppState>,
    platform: PlatformId,
) -> Result<(), AppError> {
    let label = match platform {
        PlatformId::Twitch => "inventory-twitch",
        PlatformId::Kick => "inventory-kick",
    };

    if let Some(existing) = app.get_webview_window(label) {
        let _ = existing.show();
        let _ = existing.set_focus();
        return Ok(());
    }

    let url_str = match platform {
        PlatformId::Twitch => "https://www.twitch.tv/drops/inventory",
        PlatformId::Kick => "https://kick.com/drops/inventory",
    };
    let parsed = Url::parse(url_str)
        .map_err(|e| AppError::Internal(format!("Invalid inventory URL: {e}")))?;

    let theme = state.settings.read().await.theme;
    let title = format!("{} Drops Inventory — DropPilot", platform.display_name());

    WebviewWindowBuilder::new(app, label, WebviewUrl::External(parsed))
        .title(&title)
        .inner_size(980.0, 720.0)
        .center()
        .theme(resolve_tauri_theme(theme))
        .data_directory(webview_data_dir())
        .additional_browser_args(PLATFORM_WEBVIEW_BROWSER_ARGS)
        .on_navigation(|url| handle_restricted_navigation(url, AllowlistKind::LoginNavigation))
        .build()
        .map_err(|e| AppError::Internal(format!("Failed opening drops inventory window: {e}")))?;

    Ok(())
}

pub async fn clear_webview_browsing_data(app: &AppHandle) -> Result<(), AppError> {
    // Clear UI cache if main window is open (separate lightweight UI WebView2 profile).
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.clear_all_browsing_data();
    }

    // Clear platform session cookies (Twitch/Kick) in `webview_data_dir()`.
    for label in [
        "login-twitch",
        "login-kick",
        "inventory-twitch",
        "inventory-kick",
        "kick-bridge",
        "twitch-bridge",
    ] {
        if let Some(existing) = app.get_webview_window(label) {
            return existing.clear_all_browsing_data().map_err(|e| {
                AppError::Internal(format!("Failed clearing WebView2 browsing data: {e}"))
            });
        }
    }

    // Create a temporary hidden window on our isolated platform profile only if none is open, clear, and close.
    let tmp = WebviewWindowBuilder::new(app, "clear-data-tmp", WebviewUrl::default())
        .visible(false)
        .data_directory(webview_data_dir())
        .additional_browser_args(PLATFORM_WEBVIEW_BROWSER_ARGS)
        .build()
        .map_err(|e| AppError::Internal(format!("Failed opening cleanup helper: {e}")))?;

    let res = tmp
        .clear_all_browsing_data()
        .map_err(|e| AppError::Internal(format!("Failed clearing WebView2 browsing data: {e}")));
    let _ = tmp.close();
    res
}

#[cfg(windows)]
fn configure_hidden_muted_webview(win: &tauri::WebviewWindow) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_8;
    use windows::core::Interface;

    let _ = win.with_webview(|wv| unsafe {
        let controller = wv.controller();
        let _ = controller.SetIsVisible(true);
        if let Ok(core) = controller.CoreWebView2() {
            if let Ok(core8) = core.cast::<ICoreWebView2_8>() {
                let _ = core8.SetIsMuted(true);
            }
        }
    });
}

fn normalized_watch_path(url: &Url) -> String {
    url.path().trim_matches('/').to_ascii_lowercase()
}

pub fn ensure_compat_watch_window(
    app: &AppHandle,
    platform: PlatformId,
    channel_login: &str,
) -> Result<(), AppError> {
    let label = compat_window_label(platform);
    let url_str = match platform {
        PlatformId::Twitch => format!("https://www.twitch.tv/{channel_login}"),
        PlatformId::Kick => format!("https://kick.com/{channel_login}"),
    };
    let parsed = Url::parse(&url_str)
        .map_err(|e| AppError::Internal(format!("Invalid compat watch URL: {e}")))?;
    validate_url(&parsed, AllowlistKind::Platform)?;

    if let Some(existing) = app.get_webview_window(label) {
        #[cfg(windows)]
        configure_hidden_muted_webview(&existing);

        let same_channel = existing
            .url()
            .ok()
            .map(|u| {
                u.host_str() == parsed.host_str()
                    && normalized_watch_path(&u) == normalized_watch_path(&parsed)
            })
            .unwrap_or(false);
        if !same_channel {
            let _ = existing.navigate(parsed);
            info!(
                platform = platform.as_str(),
                channel = %channel_login,
                "Switched hidden muted watch window"
            );
        }
        return Ok(());
    }

    let win = WebviewWindowBuilder::new(app, label, WebviewUrl::External(parsed))
        .title("DropPilot Compatibility Player")
        .inner_size(960.0, 540.0)
        .center()
        .visible(false)
        .focused(false)
        .skip_taskbar(true)
        .data_directory(webview_data_dir())
        .additional_browser_args(PLATFORM_WEBVIEW_BROWSER_ARGS)
        .initialization_script(COMPAT_WATCH_SCRIPT)
        .on_navigation(|url| handle_restricted_navigation(url, AllowlistKind::Platform))
        .build()
        .map_err(|e| AppError::Internal(format!("Failed starting compatibility watch window: {e}")))?;

    apply_low_memory_mode(&win);
    #[cfg(windows)]
    configure_hidden_muted_webview(&win);

    info!(
        platform = platform.as_str(),
        channel = %channel_login,
        "Started hidden muted compatibility watch window"
    );
    Ok(())
}

pub fn close_compat_watch_window(app: &AppHandle, platform: PlatformId) {
    if let Some(win) = app.get_webview_window(compat_window_label(platform)) {
        let _ = win.close();
    }
}

#[cfg(windows)]
static KICK_BRIDGE_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
#[cfg(windows)]
static KICK_BRIDGE_IDLE_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(windows)]
fn schedule_kick_bridge_idle_close(app: &AppHandle) {
    use std::sync::atomic::Ordering;
    let gen = KICK_BRIDGE_IDLE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        sleep(Duration::from_secs(2)).await;
        if KICK_BRIDGE_IDLE_GEN.load(Ordering::SeqCst) == gen {
            close_kick_bridge_window(&app_clone);
        }
    });
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum KickBridgeMessage {
    Ready,
    Response {
        request_id: String,
        status: u16,
        body: String,
    },
}

pub(crate) fn parse_kick_bridge_message(raw: &str) -> Option<KickBridgeMessage> {
    let val: serde_json::Value = serde_json::from_str(raw).ok()?;
    if val.get("dpKickReady").and_then(|v| v.as_bool()) == Some(true) {
        return Some(KickBridgeMessage::Ready);
    }
    if val.get("dpKickBridge").and_then(|v| v.as_bool()) == Some(true) {
        let request_id = val.get("requestId")?.as_str()?.to_string();
        let status = val.get("status").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
        let body = val
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        return Some(KickBridgeMessage::Response {
            request_id,
            status,
            body,
        });
    }
    None
}

pub fn close_kick_bridge_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("kick-bridge") {
        let _ = win.close();
    }
}

/// Executes a same-origin/credentialed fetch from inside DropPilot's hidden `kick.com`
/// WebView2 window using native WebView2 `WebMessageReceived` / `PostWebMessageAsString`
/// messaging when direct HTTP requests receive a 403/503 or Cloudflare challenge.
/// Does not bypass anti-bot checks: if the hidden official page itself is blocked,
/// returns `AppError::Blocked`.
pub async fn kick_webview_fetch_with_waiters(
    app: &AppHandle,
    waiters: &BridgeWaiters,
    method: &str,
    target_url: &str,
    json_body: Option<&str>,
    bearer_token: Option<&str>,
) -> Result<KickBridgeResult, AppError> {
    let parsed_target = Url::parse(target_url)
        .map_err(|e| AppError::Internal(format!("Invalid Kick bridge URL: {e}")))?;
    validate_url(&parsed_target, AllowlistKind::Platform)?;
    if !matches!(
        parsed_target.host_str(),
        Some("kick.com" | "web.kick.com" | "websockets.kick.com")
    ) {
        return Err(AppError::HostNotAllowed(
            parsed_target.host_str().unwrap_or("unknown").to_string(),
        ));
    }

    #[cfg(not(windows))]
    {
        let _ = (app, waiters, method, json_body, bearer_token);
        Err(AppError::Blocked(
            "Kick official page sync requires Windows WebView2".to_string(),
        ))
    }

    #[cfg(windows)]
    {
        use std::sync::Mutex as StdMutex;
        use webview2_com::{take_pwstr, WebMessageReceivedEventHandler};
        use windows::core::{HSTRING, PWSTR};

        let _lock = KICK_BRIDGE_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await;

        let win = if let Some(existing) = app.get_webview_window("kick-bridge") {
            existing
        } else {
            let blank = Url::parse("about:blank")
                .map_err(|e| AppError::Internal(format!("Invalid about:blank URL: {e}")))?;

            let created = WebviewWindowBuilder::new(
                app,
                "kick-bridge",
                WebviewUrl::External(blank),
            )
            .title("DropPilot Kick Bridge")
            .inner_size(980.0, 680.0)
            .center()
            .visible(false)
            .skip_taskbar(true)
            .focused(false)
            .data_directory(webview_data_dir())
            .additional_browser_args(PLATFORM_WEBVIEW_BROWSER_ARGS)
            .initialization_script(KICK_BRIDGE_SCRIPT)
            .on_navigation(|url| {
                url.as_str() == "about:blank"
                    || validate_url(url, AllowlistKind::LoginNavigation).is_ok()
            })
            .build()
            .map_err(|e| AppError::Internal(format!("Failed creating Kick bridge window: {e}")))?;

            apply_low_memory_mode(&created);

            let (ready_tx, ready_rx) = oneshot::channel::<()>();
            let ready_tx_slot = Arc::new(StdMutex::new(Some(ready_tx)));
            let (attach_tx, attach_rx) = oneshot::channel::<Result<(), String>>();
            let waiters_clone = waiters.clone();
            let ready_slot_clone = ready_tx_slot.clone();

            if let Err(err) = created.with_webview(move |wv| {
                let attach_res: Result<(), String> = (|| unsafe {
                    let controller = wv.controller();
                    let _ = controller.SetIsVisible(true);
                    let core = controller
                        .CoreWebView2()
                        .map_err(|e| format!("CoreWebView2 lookup failed: {e}"))?;

                    let mut token = 0i64;
                    let handler = WebMessageReceivedEventHandler::create(Box::new(
                        move |_webview, args| {
                            let Some(args) = args else {
                                return Ok(());
                            };
                            let mut src_pwstr = PWSTR::null();
                            args.Source(&mut src_pwstr)?;
                            let src = take_pwstr(src_pwstr);
                            if !src.starts_with("https://kick.com") {
                                return Ok(());
                            }

                            let mut msg_pwstr = PWSTR::null();
                            if args.TryGetWebMessageAsString(&mut msg_pwstr).is_err() {
                                return Ok(());
                            }
                            let raw = take_pwstr(msg_pwstr);

                            match parse_kick_bridge_message(&raw) {
                                Some(KickBridgeMessage::Ready) => {
                                    if let Ok(mut guard) = ready_slot_clone.lock() {
                                        if let Some(tx) = guard.take() {
                                            let _ = tx.send(());
                                        }
                                    }
                                }
                                Some(KickBridgeMessage::Response {
                                    request_id,
                                    status,
                                    body,
                                }) => {
                                    let tx_opt = waiters_clone
                                        .lock()
                                        .ok()
                                        .and_then(|mut g| g.remove(&request_id));
                                    if let Some(tx) = tx_opt {
                                        if status == 0 {
                                            let _ = tx.send(Err(body));
                                        } else if let Ok(serialized) =
                                            serde_json::to_string(&KickBridgeResult {
                                                status,
                                                body,
                                            })
                                        {
                                            let _ = tx.send(Ok(serialized));
                                        } else {
                                            let _ = tx.send(Err(
                                                "Failed encoding Kick bridge response".to_string(),
                                            ));
                                        }
                                    }
                                }
                                None => {}
                            }
                            Ok(())
                        },
                    ));

                    core.add_WebMessageReceived(&handler, &mut token)
                        .map_err(|e| format!("add_WebMessageReceived failed: {e}"))?;
                    Ok(())
                })();

                let _ = attach_tx.send(attach_res);
            }) {
                close_kick_bridge_window(app);
                return Err(AppError::Internal(format!(
                    "Failed attaching Kick bridge listener: {err}"
                )));
            }

            match timeout(Duration::from_secs(5), attach_rx).await {
                Ok(Ok(Ok(()))) => {}
                Ok(Ok(Err(msg))) => {
                    close_kick_bridge_window(app);
                    return Err(AppError::Internal(msg));
                }
                _ => {
                    close_kick_bridge_window(app);
                    return Err(AppError::Internal(
                        "Timed out preparing Kick WebView2 listener".to_string(),
                    ));
                }
            }

            let inventory_url = Url::parse("https://kick.com/drops/inventory")
                .map_err(|e| AppError::Internal(format!("Invalid bridge base URL: {e}")))?;
            if let Err(err) = created.navigate(inventory_url) {
                close_kick_bridge_window(app);
                return Err(AppError::Internal(format!(
                    "Kick bridge navigation failed: {err}"
                )));
            }

            match timeout(Duration::from_secs(18), ready_rx).await {
                Ok(Ok(())) => {}
                _ => {
                    close_kick_bridge_window(app);
                    return Err(AppError::Blocked(
                        "Kick background page timed out (possible Cloudflare verification required); open Kick Drops Inventory to verify"
                            .to_string(),
                    ));
                }
            }

            created
        };

        let req_id = format!(
            "kb_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );

        let (tx, rx) = oneshot::channel::<Result<String, String>>();
        {
            let mut guard = waiters
                .lock()
                .map_err(|_| AppError::Internal("Bridge waiter lock poisoned".to_string()))?;
            guard.insert(req_id.clone(), tx);
        }

        let payload = serde_json::json!({
            "dpKickReq": true,
            "requestId": req_id,
            "method": method,
            "target": parsed_target.as_str(),
            "body": json_body,
            "bearer": bearer_token,
        })
        .to_string();

        let (dispatch_tx, dispatch_rx) = oneshot::channel::<Result<(), String>>();
        if let Err(err) = win.with_webview(move |wv| {
            let res: Result<(), String> = unsafe {
                let controller = wv.controller();
                let _ = controller.SetIsVisible(true);
                match controller.CoreWebView2() {
                    Ok(core) => {
                        let msg_hstring = HSTRING::from(payload.as_str());
                        core.PostWebMessageAsString(&msg_hstring)
                            .map_err(|e| format!("PostWebMessageAsString failed: {e}"))
                    }
                    Err(e) => Err(format!("CoreWebView2 lookup failed: {e}")),
                }
            };
            let _ = dispatch_tx.send(res);
        }) {
            if let Ok(mut guard) = waiters.lock() {
                guard.remove(&req_id);
            }
            close_kick_bridge_window(app);
            return Err(AppError::Internal(format!(
                "Kick bridge dispatch failed: {err}"
            )));
        }

        match timeout(Duration::from_secs(5), dispatch_rx).await {
            Ok(Ok(Ok(()))) => {}
            Ok(Ok(Err(msg))) => {
                if let Ok(mut guard) = waiters.lock() {
                    guard.remove(&req_id);
                }
                close_kick_bridge_window(app);
                return Err(AppError::Internal(msg));
            }
            _ => {
                if let Ok(mut guard) = waiters.lock() {
                    guard.remove(&req_id);
                }
                close_kick_bridge_window(app);
                return Err(AppError::Blocked(
                    "Kick bridge dispatcher timed out".to_string(),
                ));
            }
        }

        let wait_res = timeout(Duration::from_secs(15), rx).await;
        if let Ok(mut guard) = waiters.lock() {
            guard.remove(&req_id);
        }

        match wait_res {
            Ok(Ok(Ok(raw))) => {
                let parsed: KickBridgeResult = serde_json::from_str(&raw).map_err(|e| {
                    AppError::PlatformChanged(format!("Bridge JSON decode error: {e}"))
                })?;
                if parsed.status == 403 || parsed.body.contains("Just a moment...") {
                    if parsed_target.host_str() != Some("websockets.kick.com")
                        || parsed.body.contains("Just a moment...")
                    {
                        close_kick_bridge_window(app);
                    }
                    return Err(AppError::Blocked(
                        "Kick security check blocked background access; open Kick Drops Inventory to verify"
                            .to_string(),
                    ));
                }
                schedule_kick_bridge_idle_close(app);
                Ok(parsed)
            }
            Ok(Ok(Err(err_msg))) => Err(AppError::Network(format!("Kick bridge error: {err_msg}"))),
            Ok(Err(_)) => Err(AppError::Network("Kick bridge channel closed".to_string())),
            Err(_) => {
                close_kick_bridge_window(app);
                Err(AppError::Blocked(
                    "Kick background request timed out (possible Cloudflare verification required); open Kick Drops Inventory to verify"
                        .to_string(),
                ))
            }
        }
    }
}

pub fn close_twitch_bridge_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("twitch-bridge") {
        let _ = win.close();
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum TwitchGqlInspect {
    Data(serde_json::Value),
    Unauthenticated(String),
    TransientIntegrity(String),
}

pub(crate) fn inspect_twitch_gql_payload(
    raw_bytes: &[u8],
    expected_kind: &str,
) -> Option<TwitchGqlInspect> {
    let parsed: serde_json::Value = serde_json::from_slice(raw_bytes).ok()?;
    let items: &[serde_json::Value] = match parsed.as_array() {
        Some(arr) => arr.as_slice(),
        None => std::slice::from_ref(&parsed),
    };

    for item in items {
        let op_name = item
            .pointer("/extensions/operationName")
            .and_then(|s| s.as_str())
            .unwrap_or("");

        match expected_kind {
            "campaigns"
                if item
                    .pointer("/data/currentUser/dropCampaigns")
                    .and_then(|v| v.as_array())
                    .is_some() =>
            {
                return Some(TwitchGqlInspect::Data(item.clone()));
            }
            "inventory"
                if item
                    .pointer("/data/currentUser/inventory")
                    .map(|v| !v.is_null())
                    .unwrap_or(false) =>
            {
                return Some(TwitchGqlInspect::Data(item.clone()));
            }
            "directory"
                if item
                    .pointer("/data/game/streams/edges")
                    .and_then(|v| v.as_array())
                    .is_some()
                    || (op_name == "DirectoryPage_Game"
                        && item.get("data").is_some()
                        && (item
                            .pointer("/data/game")
                            .map(|v| v.is_null())
                            .unwrap_or(false)
                            || item
                                .pointer("/data/game/streams")
                                .map(|v| v.is_null())
                                .unwrap_or(false))) =>
            {
                return Some(TwitchGqlInspect::Data(item.clone()));
            }
            _ => {}
        }

        let matches_auth_op = matches!(
            (expected_kind, op_name),
            ("campaigns", "ViewerDropsDashboard") | ("inventory", "Inventory")
        );
        if matches_auth_op
            && item
                .pointer("/data/currentUser")
                .map(|v| v.is_null())
                .unwrap_or(false)
        {
            return Some(TwitchGqlInspect::Unauthenticated(
                "Twitch web session expired; please log in again".to_string(),
            ));
        }

        let matches_err_op = matches!(
            op_name,
            "ViewerDropsDashboard" | "Inventory" | "DirectoryPage_Game"
        );
        if matches_err_op {
            if let Some(errors) = item.get("errors").and_then(|e| e.as_array()) {
                for err in errors {
                    let msg = err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or_default();
                    if msg.to_ascii_lowercase().contains("integrity") {
                        return Some(TwitchGqlInspect::TransientIntegrity(msg.to_string()));
                    }
                }
            }
        }
    }

    None
}

pub(crate) fn inspect_twitch_campaign_details_payload(
    raw_bytes: &[u8],
    expected_campaign_id: &str,
) -> Option<TwitchGqlInspect> {
    let parsed: serde_json::Value = serde_json::from_slice(raw_bytes).ok()?;
    let items: &[serde_json::Value] = match parsed.as_array() {
        Some(arr) => arr.as_slice(),
        None => std::slice::from_ref(&parsed),
    };

    for item in items {
        let camp_node = item
            .pointer("/data/user/dropCampaign")
            .or_else(|| item.pointer("/data/dropCampaign"));
        if let Some(camp) = camp_node {
            if camp.get("id").and_then(|s| s.as_str()) == Some(expected_campaign_id) {
                return Some(TwitchGqlInspect::Data(item.clone()));
            }
        }

        let op_name = item
            .pointer("/extensions/operationName")
            .and_then(|s| s.as_str())
            .unwrap_or("");

        if op_name == "ViewerDropsDashboard"
            && item
                .pointer("/data/currentUser")
                .map(|v| v.is_null())
                .unwrap_or(false)
        {
            return Some(TwitchGqlInspect::Unauthenticated(
                "Twitch web session expired; please log in again".to_string(),
            ));
        }

        if op_name == "DropCampaignDetails" {
            if let Some(errors) = item.get("errors").and_then(|e| e.as_array()) {
                for err in errors {
                    let msg = err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or_default();
                    if msg.to_ascii_lowercase().contains("integrity") {
                        return Some(TwitchGqlInspect::TransientIntegrity(msg.to_string()));
                    }
                }
            }
        }
    }

    None
}

pub(crate) fn inspect_twitch_claim_payload(
    raw_bytes: &[u8],
    expected_drop_id: &str,
    expected_instance_id: Option<&str>,
) -> Option<TwitchGqlInspect> {
    let parsed: serde_json::Value = serde_json::from_slice(raw_bytes).ok()?;
    let items: &[serde_json::Value] = match parsed.as_array() {
        Some(arr) => arr.as_slice(),
        None => std::slice::from_ref(&parsed),
    };

    for item in items {
        let op_name = item
            .pointer("/extensions/operationName")
            .and_then(|s| s.as_str())
            .unwrap_or("");

        let is_claim_op = op_name == "DropsPage_ClaimDropRewards"
            || item.pointer("/data/claimDropRewards").is_some();
        if is_claim_op {
            if let Some(errors) = item.get("errors").and_then(|e| e.as_array()) {
                for err in errors {
                    let msg = err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or_default();
                    let lower = msg.to_ascii_lowercase();
                    if lower.contains("integrity") {
                        return Some(TwitchGqlInspect::TransientIntegrity(msg.to_string()));
                    }
                    if lower.contains("unauthenticated") || lower.contains("unauthorized") {
                        return Some(TwitchGqlInspect::Unauthenticated(msg.to_string()));
                    }
                }
            }

            if let Some(status) = item
                .pointer("/data/claimDropRewards/status")
                .and_then(|v| v.as_str())
            {
                if matches!(
                    status,
                    "ELIGIBLE_FOR_ALL" | "DROP_INSTANCE_ALREADY_CLAIMED" | "SUCCESS"
                ) {
                    return Some(TwitchGqlInspect::Data(item.clone()));
                }
                if status.to_ascii_lowercase().contains("integrity") {
                    return Some(TwitchGqlInspect::TransientIntegrity(status.to_string()));
                }
            }
        }

        if op_name == "Inventory" {
            if item
                .pointer("/data/currentUser")
                .map(|v| v.is_null())
                .unwrap_or(false)
            {
                return Some(TwitchGqlInspect::Unauthenticated(
                    "Twitch web session expired; please log in again".to_string(),
                ));
            }
            if let Some(errors) = item.get("errors").and_then(|e| e.as_array()) {
                for err in errors {
                    let msg = err
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or_default();
                    if msg.to_ascii_lowercase().contains("integrity") {
                        return Some(TwitchGqlInspect::TransientIntegrity(msg.to_string()));
                    }
                }
            }

            if let Some(in_progress) = item
                .pointer("/data/currentUser/inventory/dropCampaignsInProgress")
                .and_then(|v| v.as_array())
            {
                for camp in in_progress {
                    let Some(drops) = camp.get("timeBasedDrops").and_then(|v| v.as_array()) else {
                        continue;
                    };
                    for d in drops {
                        let id_match = !expected_drop_id.is_empty()
                            && d.get("id").and_then(|v| v.as_str()) == Some(expected_drop_id);
                        let inst_match = expected_instance_id.is_some()
                            && d.pointer("/self/dropInstanceID").and_then(|v| v.as_str())
                                == expected_instance_id;
                        if (id_match || inst_match)
                            && d.pointer("/self/isClaimed").and_then(|b| b.as_bool()) == Some(true)
                        {
                            return Some(TwitchGqlInspect::Data(serde_json::json!({
                                "alreadyClaimed": true
                            })));
                        }
                    }
                }
            }
        }
    }

    None
}

#[cfg(windows)]
unsafe fn read_istream_to_vec(stream: &windows::Win32::System::Com::IStream) -> Vec<u8> {
    const MAX_BYTES: usize = 16 * 1024 * 1024;
    let mut out = Vec::new();
    let mut chunk = [0u8; 16 * 1024];

    loop {
        if out.len() >= MAX_BYTES {
            break;
        }
        let mut bytes_read = 0u32;
        let hr = stream.Read(
            chunk.as_mut_ptr().cast(),
            chunk.len() as u32,
            Some(&mut bytes_read),
        );
        if hr.is_err() || bytes_read == 0 {
            break;
        }
        let take_len = (bytes_read as usize).min(MAX_BYTES.saturating_sub(out.len()));
        out.extend_from_slice(&chunk[..take_len]);
    }

    out
}

#[cfg(windows)]
type TwitchPayloadInspector = Arc<dyn Fn(&[u8]) -> Option<TwitchGqlInspect> + Send + Sync>;

fn build_twitch_expand_campaign_script(
    campaign_name: &str,
    game_id: &str,
    game_name: &str,
) -> Result<String, AppError> {
    let camp_json = serde_json::to_string(campaign_name)
        .map_err(|e| AppError::Internal(format!("Serialize campaign name failed: {e}")))?;
    let gid_json = serde_json::to_string(game_id)
        .map_err(|e| AppError::Internal(format!("Serialize game id failed: {e}")))?;
    let game_json = serde_json::to_string(game_name)
        .map_err(|e| AppError::Internal(format!("Serialize game name failed: {e}")))?;

    Ok(format!(
        r##"
(function () {{
  if (window.location.hostname !== "www.twitch.tv") return;
  if (window.__dpExpandStarted) return;
  window.__dpExpandStarted = true;

  function norm(str) {{
    return (str || "").replace(/\s+/g, " ").trim().toLowerCase();
  }}

  const targetCampaign = norm({camp_json});
  const targetGameId = norm({gid_json});
  const targetGame = norm({game_json});

  function gameMatches(text) {{
    const t = norm(text);
    if (!t || !targetGame) return false;
    if (t === targetGame) return true;
    if (t.length >= 4 && targetGame.length >= 4) {{
      return t.startsWith(targetGame) || targetGame.startsWith(t);
    }}
    return false;
  }}

  function isolateSingleRow(seed) {{
    let cur = seed;
    let lastSingle = null;
    for (let depth = 0; depth < 10 && cur && cur !== document.body; depth++) {{
      if (cur.tagName === "MAIN") break;
      const boxCount = cur.querySelectorAll('img[src*="ttv-boxart"]').length;
      if (boxCount > 1) break;
      if (boxCount === 1) lastSingle = cur;
      cur = cur.parentElement;
    }}
    return lastSingle;
  }}

  function collectCandidateRows() {{
    const rows = [];
    const seen = new Set();
    function addRow(seed) {{
      const r = isolateSingleRow(seed);
      if (r && !seen.has(r) && !r.__dpClicked) {{
        seen.add(r);
        rows.push(r);
      }}
    }}

    const imgs = Array.from(document.querySelectorAll('img[src*="ttv-boxart"]'));
    for (let i = 0; i < imgs.length; i++) {{
      const img = imgs[i];
      const src = (img.getAttribute("src") || "").toLowerCase();
      const alt = img.getAttribute("alt") || "";
      if (
        (targetGameId &&
          (src.includes("/ttv-boxart/" + targetGameId + "-") ||
            src.includes("/ttv-boxart/" + targetGameId + "_") ||
            src.includes("/ttv-boxart/" + targetGameId + "."))) ||
        gameMatches(alt)
      ) {{
        addRow(img);
      }}
    }}

    const nodes = Array.from(document.querySelectorAll("main h3, main h4, main h5, main h6, main p, main span"));
    for (let i = 0; i < nodes.length; i++) {{
      const n = nodes[i];
      if (n.children.length > 0) continue;
      const txt = norm(n.textContent);
      if (!txt || txt.length > 140) continue;
      if ((targetCampaign && txt === targetCampaign) || gameMatches(txt)) {{
        addRow(n);
      }}
    }}
    return rows;
  }}

  function expandRow(row) {{
    row.__dpClicked = true;
    row.scrollIntoView({{ block: "center" }});
    const btn =
      row.querySelector('button[aria-expanded="false"], [role="button"][aria-expanded="false"]') ||
      row.querySelector('button, [role="button"]');
    if (btn instanceof HTMLElement) {{
      btn.click();
    }}
    if (row.firstElementChild instanceof HTMLElement && row.firstElementChild !== btn) {{
      row.firstElementChild.click();
    }}
  }}

  let attempts = 0;
  let clickedTotal = 0;
  const timer = setInterval(function () {{
    attempts += 1;
    if (attempts > 36 || clickedTotal >= 8) {{
      clearInterval(timer);
      return;
    }}
    if (document.readyState === "loading") return;

    try {{
      const rows = collectCandidateRows();
      for (let i = 0; i < rows.length && clickedTotal < 8; i++) {{
        expandRow(rows[i]);
        clickedTotal += 1;
      }}
    }} catch (_) {{}}
  }}, 350);
}})();
"##
    ))
}

fn build_twitch_claim_drop_script(
    campaign_name: &str,
    game_name: &str,
    drop_name: &str,
    reward_names: &[String],
) -> Result<String, AppError> {
    let camp_json = serde_json::to_string(campaign_name)
        .map_err(|e| AppError::Internal(format!("Serialize campaign name failed: {e}")))?;
    let game_json = serde_json::to_string(game_name)
        .map_err(|e| AppError::Internal(format!("Serialize game name failed: {e}")))?;
    let drop_json = serde_json::to_string(drop_name)
        .map_err(|e| AppError::Internal(format!("Serialize drop name failed: {e}")))?;
    let rewards_json = serde_json::to_string(reward_names)
        .map_err(|e| AppError::Internal(format!("Serialize reward names failed: {e}")))?;

    Ok(format!(
        r##"
(function () {{
  if (window.location.hostname !== "www.twitch.tv") return;
  if (window.__dpClaimStarted) return;
  window.__dpClaimStarted = true;

  function norm(str) {{
    return (str || "").replace(/\s+/g, " ").trim().toLowerCase();
  }}

  const targetCampaign = norm({camp_json});
  const targetGame = norm({game_json});
  const targetDrop = norm({drop_json});
  const targetRewards = ({rewards_json}).map(norm).filter(Boolean);

  function collectClaimButtons() {{
    const seen = new Set();
    const out = [];

    function maybeAdd(btn) {{
      if (!(btn instanceof HTMLElement)) return;
      if (seen.has(btn)) return;
      if (btn.hasAttribute("disabled") || btn.getAttribute("aria-disabled") === "true") return;
      try {{
        const style = window.getComputedStyle(btn);
        if (style.display === "none" || style.visibility === "hidden") return;
      }} catch (_) {{}}
      seen.add(btn);
      out.push(btn);
    }}

    const tagged = document.querySelectorAll(
      'button[data-test-selector="DropsCampaignInProgressRewardPresentation-claim-button"], button[data-test-selector*="claim" i], [data-test-selector*="claim" i] button'
    );
    for (let i = 0; i < tagged.length; i++) {{
      maybeAdd(tagged[i]);
    }}

    const buttons = document.querySelectorAll("button");
    for (let i = 0; i < buttons.length; i++) {{
      const txt = norm(buttons[i].textContent);
      const aria = norm(buttons[i].getAttribute("aria-label"));
      if (
        txt === "claim now" ||
        txt === "claim" ||
        txt === "claim drop" ||
        aria === "claim now" ||
        aria === "claim" ||
        aria === "claim drop"
      ) {{
        maybeAdd(buttons[i]);
      }}
    }}
    return out;
  }}

  function scoreButton(btn) {{
    let score = 0;
    let cur = btn.parentElement;
    for (let depth = 0; depth < 12 && cur && cur !== document.body; depth++) {{
      if (cur.tagName === "MAIN") break;
      const text = norm(cur.textContent);
      if (depth < 6) {{
        for (let r = 0; r < targetRewards.length; r++) {{
          if (targetRewards[r] && text.includes(targetRewards[r])) {{
            score = Math.max(score, 4);
          }}
        }}
        if (targetDrop && text.includes(targetDrop)) {{
          score = Math.max(score, 4);
        }}
      }}
      if (targetCampaign && text.includes(targetCampaign)) {{
        score += 2;
        break;
      }}
      if (targetGame && text.includes(targetGame)) {{
        score += 1;
        break;
      }}
      cur = cur.parentElement;
    }}
    return score;
  }}

  let attempts = 0;
  let clickCount = 0;
  let lastClickAt = 0;
  let readyAt = 0;
  const timer = setInterval(function () {{
    attempts += 1;
    if (attempts > 45 || clickCount >= 5) {{
      clearInterval(timer);
      return;
    }}
    if (document.readyState === "loading") return;
    const now = Date.now();
    if (readyAt === 0) {{
      readyAt = now;
      return;
    }}
    if (now - readyAt < 1000) return;
    if (lastClickAt > 0 && now - lastClickAt < 3000) return;

    try {{
      const buttons = collectClaimButtons();
      if (buttons.length === 0) return;

      let bestBtn = null;
      let bestScore = -1;
      for (let i = 0; i < buttons.length; i++) {{
        const btn = buttons[i];
        const prev = btn.__dpClaimClickedAt || 0;
        if (prev > 0 && now - prev < 3500) continue;
        const s = scoreButton(btn);
        if (s > bestScore) {{
          bestScore = s;
          bestBtn = btn;
        }}
      }}

      if (bestBtn) {{
        bestBtn.__dpClaimClickedAt = now;
        lastClickAt = now;
        clickCount += 1;
        bestBtn.scrollIntoView({{ block: "center" }});
        bestBtn.focus();
        bestBtn.click();
      }}
    }} catch (_) {{}}
  }}, 400);
}})();
"##
    ))
}

#[cfg(windows)]
async fn run_twitch_bridge(
    app: &AppHandle,
    page_url: &str,
    expected_kind: &str,
    timeout_secs: u64,
    init_script: Option<String>,
    inspector: TwitchPayloadInspector,
) -> Result<serde_json::Value, AppError> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex as StdMutex;
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_2;
    use webview2_com::{
        take_pwstr, WebResourceResponseReceivedEventHandler,
        WebResourceResponseViewGetContentCompletedHandler,
    };
    use windows::core::{Interface, PWSTR};

    let parsed = Url::parse(page_url)
        .map_err(|e| AppError::Internal(format!("Invalid Twitch bridge URL: {e}")))?;
    validate_url(&parsed, AllowlistKind::Platform)?;
    if parsed.host_str() != Some("www.twitch.tv") {
        return Err(AppError::HostNotAllowed(
            parsed.host_str().unwrap_or("unknown").to_string(),
        ));
    }

    close_twitch_bridge_window(app);
    sleep(Duration::from_millis(150)).await;

    let blank = Url::parse("about:blank")
        .map_err(|e| AppError::Internal(format!("Invalid about:blank URL: {e}")))?;

    let mut builder = WebviewWindowBuilder::new(app, "twitch-bridge", WebviewUrl::External(blank))
        .title("DropPilot Twitch Sync")
        .inner_size(1200.0, 800.0)
        .center()
        .visible(false)
        .skip_taskbar(true)
        .focused(false)
        .data_directory(webview_data_dir())
        .additional_browser_args(PLATFORM_WEBVIEW_BROWSER_ARGS)
        .on_navigation(|url| {
            url.as_str() == "about:blank"
                || validate_url(url, AllowlistKind::LoginNavigation).is_ok()
        });

    if let Some(script) = init_script {
        builder = builder.initialization_script(&script);
    }

    let win = builder
        .build()
        .map_err(|e| AppError::Internal(format!("Failed creating Twitch bridge window: {e}")))?;

    apply_low_memory_mode(&win);

    let (result_tx, result_rx) = oneshot::channel::<Result<serde_json::Value, String>>();
    let result_tx_slot = Arc::new(StdMutex::new(Some(result_tx)));
    let (ready_tx, ready_rx) = oneshot::channel::<Result<(), String>>();
    let integrity_attempts = Arc::new(AtomicUsize::new(0));

    let kind_log = expected_kind.to_string();
    let attach_tx = result_tx_slot.clone();
    let attach_attempts = integrity_attempts.clone();

    if let Err(err) = win.with_webview(move |wv| {
        let attach_res: Result<(), String> = (|| unsafe {
            let controller = wv.controller();
            let _ = controller.SetIsVisible(true);
            let core = controller
                .CoreWebView2()
                .map_err(|e| format!("CoreWebView2 lookup failed: {e}"))?;
            let core2: ICoreWebView2_2 = core
                .cast()
                .map_err(|e| format!("ICoreWebView2_2 cast failed: {e}"))?;

            let mut token = 0i64;
            let handler = WebResourceResponseReceivedEventHandler::create(Box::new(
                move |_webview, args| {
                    let Some(args) = args else {
                        return Ok(());
                    };
                    let req = args.Request()?;
                    let mut uri_pwstr = PWSTR::null();
                    req.Uri(&mut uri_pwstr)?;
                    let uri = take_pwstr(uri_pwstr);
                    if uri.starts_with("https://gql.twitch.tv/integrity") {
                        let resp = args.Response()?;
                        let mut status = 0i32;
                        resp.StatusCode(&mut status)?;
                        info!(status, "Twitch official page integrity handshake completed");
                        return Ok(());
                    }
                    if !uri.starts_with("https://gql.twitch.tv/gql") {
                        return Ok(());
                    }
                    let resp = args.Response()?;
                    let mut status = 0i32;
                    resp.StatusCode(&mut status)?;
                    if status != 200 {
                        return Ok(());
                    }

                    let tx_inner = attach_tx.clone();
                    let kind_inner = kind_log.clone();
                    let attempts_inner = attach_attempts.clone();
                    let inspector_inner = inspector.clone();
                    let content_handler =
                        WebResourceResponseViewGetContentCompletedHandler::create(Box::new(
                            move |hr, stream| {
                                if hr.is_ok() {
                                    if let Some(stream) = stream {
                                        let bytes = read_istream_to_vec(&stream);
                                        match inspector_inner(&bytes) {
                                            Some(TwitchGqlInspect::Data(val)) => {
                                                if let Ok(mut guard) = tx_inner.lock() {
                                                    if let Some(tx) = guard.take() {
                                                        let _ = tx.send(Ok(val));
                                                    }
                                                }
                                            }
                                            Some(TwitchGqlInspect::Unauthenticated(msg)) => {
                                                if let Ok(mut guard) = tx_inner.lock() {
                                                    if let Some(tx) = guard.take() {
                                                        let _ = tx.send(Err(format!(
                                                            "unauthenticated:{msg}"
                                                        )));
                                                    }
                                                }
                                            }
                                            Some(TwitchGqlInspect::TransientIntegrity(msg)) => {
                                                let n = attempts_inner
                                                    .fetch_add(1, Ordering::Relaxed)
                                                    + 1;
                                                if n == 1 {
                                                    info!(
                                                        kind = %kind_inner,
                                                        "Twitch page refreshing integrity check; waiting for automatic retry"
                                                    );
                                                }
                                                if n >= 4 {
                                                    if let Ok(mut guard) = tx_inner.lock() {
                                                        if let Some(tx) = guard.take() {
                                                            let _ = tx.send(Err(format!(
                                                                "blocked:{msg}"
                                                            )));
                                                        }
                                                    }
                                                }
                                            }
                                            None => {}
                                        }
                                    }
                                }
                                Ok(())
                            },
                        ));

                    let _ = resp.GetContent(&content_handler);
                    Ok(())
                },
            ));

            core2
                .add_WebResourceResponseReceived(&handler, &mut token)
                .map_err(|e| format!("add_WebResourceResponseReceived failed: {e}"))?;
            Ok(())
        })();

        let _ = ready_tx.send(attach_res);
    }) {
        close_twitch_bridge_window(app);
        return Err(AppError::Internal(format!(
            "Failed attaching Twitch bridge listener: {err}"
        )));
    }

    match timeout(Duration::from_secs(5), ready_rx).await {
        Ok(Ok(Ok(()))) => {}
        Ok(Ok(Err(msg))) => {
            close_twitch_bridge_window(app);
            return Err(AppError::Internal(msg));
        }
        _ => {
            close_twitch_bridge_window(app);
            return Err(AppError::Internal(
                "Timed out preparing Twitch WebView2 listener".to_string(),
            ));
        }
    }

    info!(kind = expected_kind, "Syncing Twitch data via official page");
    if let Err(err) = win.navigate(parsed) {
        close_twitch_bridge_window(app);
        return Err(AppError::Internal(format!(
            "Twitch bridge navigation failed: {err}"
        )));
    }

    let wait_res = timeout(Duration::from_secs(timeout_secs), result_rx).await;
    close_twitch_bridge_window(app);

    match wait_res {
        Ok(Ok(Ok(val))) => {
            info!(kind = expected_kind, "Synced Twitch data from official page");
            Ok(val)
        }
        Ok(Ok(Err(err))) => {
            if let Some(msg) = err.strip_prefix("unauthenticated:") {
                Err(AppError::Auth(msg.to_string()))
            } else {
                Err(AppError::Blocked(
                    err.trim_start_matches("blocked:").to_string(),
                ))
            }
        }
        Ok(Err(_)) | Err(_) => {
            if integrity_attempts.load(Ordering::Relaxed) > 0 {
                Err(AppError::Blocked(
                    "Twitch integrity check did not succeed after retries; check Logs for integrity handshake status"
                        .to_string(),
                ))
            } else if expected_kind == "directory" || expected_kind == "campaign-details" {
                Err(AppError::Network(
                    "Twitch page request timed out; please retry in a moment".to_string(),
                ))
            } else {
                Err(AppError::Blocked(
                    "Twitch official drops page did not respond in time".to_string(),
                ))
            }
        }
    }
}

/// Loads an official `www.twitch.tv` drops or directory page inside a hidden isolated
/// WebView2 helper window and reads native `WebResourceResponseReceived` GQL payloads in Rust.
pub async fn twitch_bridge_capture(
    app: &AppHandle,
    page_url: &str,
    expected_kind: &str,
) -> Result<serde_json::Value, AppError> {
    #[cfg(not(windows))]
    {
        let _ = (app, page_url, expected_kind);
        Err(AppError::Blocked(
            "Twitch official page sync requires Windows WebView2".to_string(),
        ))
    }

    #[cfg(windows)]
    {
        let kind_owned = expected_kind.to_string();
        let timeout_secs = if expected_kind == "directory" { 35 } else { 25 };
        let inspector: TwitchPayloadInspector =
            Arc::new(move |bytes| inspect_twitch_gql_payload(bytes, &kind_owned));
        run_twitch_bridge(app, page_url, expected_kind, timeout_secs, None, inspector).await
    }
}

/// Opens `https://www.twitch.tv/drops/campaigns` inside the isolated hidden bridge,
/// clicks the official campaign row so Twitch fetches `DropCampaignDetails` natively,
/// and captures the response payload without any forged tokens.
pub async fn twitch_bridge_fetch_campaign_details(
    app: &AppHandle,
    campaign_id: &str,
    campaign_name: &str,
    game_id: &str,
    game_name: &str,
) -> Result<serde_json::Value, AppError> {
    #[cfg(not(windows))]
    {
        let _ = (app, campaign_id, campaign_name, game_id, game_name);
        Err(AppError::Blocked(
            "Twitch official page sync requires Windows WebView2".to_string(),
        ))
    }

    #[cfg(windows)]
    {
        let script = build_twitch_expand_campaign_script(campaign_name, game_id, game_name)?;
        let camp_id_owned = campaign_id.to_string();
        let inspector: TwitchPayloadInspector =
            Arc::new(move |bytes| inspect_twitch_campaign_details_payload(bytes, &camp_id_owned));
        run_twitch_bridge(
            app,
            "https://www.twitch.tv/drops/campaigns",
            "campaign-details",
            15,
            Some(script),
            inspector,
        )
        .await
    }
}

/// Opens `https://www.twitch.tv/drops/inventory` inside the temporary hidden WebView2 bridge,
/// clicks the official "Claim Now" button so Twitch fires `DropsPage_ClaimDropRewards` natively
/// with its own browser integrity handshake, and confirms the response before closing immediately.
pub async fn twitch_bridge_claim_drop(
    app: &AppHandle,
    campaign_name: &str,
    game_name: &str,
    drop_id: &str,
    drop_name: &str,
    reward_names: &[String],
    instance_id: Option<&str>,
) -> Result<(), AppError> {
    #[cfg(not(windows))]
    {
        let _ = (
            app,
            campaign_name,
            game_name,
            drop_id,
            drop_name,
            reward_names,
            instance_id,
        );
        Err(AppError::Blocked(
            "Twitch automatic claim requires Windows WebView2".to_string(),
        ))
    }

    #[cfg(windows)]
    {
        let script =
            build_twitch_claim_drop_script(campaign_name, game_name, drop_name, reward_names)?;
        let drop_id_owned = drop_id.to_string();
        let instance_id_owned = instance_id.map(str::to_string);
        let inspector: TwitchPayloadInspector = Arc::new(move |bytes| {
            inspect_twitch_claim_payload(bytes, &drop_id_owned, instance_id_owned.as_deref())
        });
        let _ = run_twitch_bridge(
            app,
            "https://www.twitch.tv/drops/inventory",
            "claim-drop",
            20,
            Some(script),
            inspector,
        )
        .await?;
        Ok(())
    }
}

fn handle_restricted_navigation(url: &Url, kind: AllowlistKind) -> bool {
    if validate_url(url, kind).is_ok() {
        return true;
    }

    if url.scheme() == "https" {
        let _ = tauri_plugin_opener::open_url(url.as_str(), None::<&str>);
    } else {
        warn!(scheme = %url.scheme(), "Blocked non-https navigation in embedded window");
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{
        inspect_twitch_campaign_details_payload, inspect_twitch_claim_payload,
        inspect_twitch_gql_payload, parse_kick_bridge_message, KickBridgeMessage, TwitchGqlInspect,
    };
    use serde_json::json;

    #[test]
    fn extracts_campaigns_from_batched_gql_array() {
        let batch = json!([
            {
                "extensions": { "operationName": "SideNav_FollowedChannels" },
                "data": { "currentUser": { "followedLiveUsers": { "nodes": [] } } }
            },
            {
                "extensions": { "operationName": "ViewerDropsDashboard" },
                "data": {
                    "currentUser": {
                        "dropCampaigns": [
                            { "id": "c1", "name": "Rust Drops" }
                        ]
                    }
                }
            }
        ]);
        let res = inspect_twitch_gql_payload(batch.to_string().as_bytes(), "campaigns");
        let Some(TwitchGqlInspect::Data(val)) = res else {
            panic!("expected Data variant");
        };
        assert_eq!(val["data"]["currentUser"]["dropCampaigns"][0]["id"], "c1");
    }

    #[test]
    fn extracts_inventory_with_null_in_progress() {
        let payload = json!({
            "extensions": { "operationName": "Inventory" },
            "data": {
                "currentUser": {
                    "inventory": {
                        "dropCampaignsInProgress": null
                    }
                }
            }
        });
        let res = inspect_twitch_gql_payload(payload.to_string().as_bytes(), "inventory");
        let Some(TwitchGqlInspect::Data(val)) = res else {
            panic!("expected Data variant");
        };
        assert!(val["data"]["currentUser"]["inventory"].is_object());
    }

    #[test]
    fn extracts_directory_channels() {
        let payload = json!({
            "extensions": { "operationName": "DirectoryPage_Game" },
            "data": {
                "game": {
                    "streams": {
                        "edges": [
                            { "node": { "broadcaster": { "login": "hJune" } } }
                        ]
                    }
                }
            }
        });
        let res = inspect_twitch_gql_payload(payload.to_string().as_bytes(), "directory");
        let Some(TwitchGqlInspect::Data(val)) = res else {
            panic!("expected Data variant");
        };
        assert_eq!(
            val["data"]["game"]["streams"]["edges"][0]["node"]["broadcaster"]["login"],
            "hJune"
        );
    }

    #[test]
    fn detects_unauthenticated_and_integrity_errors_scoped_by_operation() {
        let unauth = json!({
            "extensions": { "operationName": "ViewerDropsDashboard" },
            "data": { "currentUser": null }
        });
        assert!(matches!(
            inspect_twitch_gql_payload(unauth.to_string().as_bytes(), "campaigns"),
            Some(TwitchGqlInspect::Unauthenticated(_))
        ));

        // Unrelated operation with currentUser: null must be ignored
        let unrelated_unauth = json!({
            "extensions": { "operationName": "GlobalNav" },
            "data": { "currentUser": null }
        });
        assert!(inspect_twitch_gql_payload(
            unrelated_unauth.to_string().as_bytes(),
            "campaigns"
        )
        .is_none());

        let blocked = json!({
            "extensions": { "operationName": "ViewerDropsDashboard" },
            "errors": [{ "message": "failed integrity check" }]
        });
        assert!(matches!(
            inspect_twitch_gql_payload(blocked.to_string().as_bytes(), "campaigns"),
            Some(TwitchGqlInspect::TransientIntegrity(_))
        ));
    }

    #[test]
    fn ignores_unrelated_gql_payloads() {
        let other = json!({
            "extensions": { "operationName": "OnsiteNotifications_ListNotifications" },
            "data": { "currentUser": { "id": "123" } }
        });
        assert!(inspect_twitch_gql_payload(other.to_string().as_bytes(), "campaigns").is_none());
    }

    #[test]
    fn parses_kick_bridge_web_messages() {
        assert_eq!(
            parse_kick_bridge_message(r#"{"dpKickReady":true}"#),
            Some(KickBridgeMessage::Ready)
        );
        assert_eq!(
            parse_kick_bridge_message(
                r#"{"dpKickBridge":true,"requestId":"kb_1","status":200,"body":"{\"ok\":true}"}"#
            ),
            Some(KickBridgeMessage::Response {
                request_id: "kb_1".to_string(),
                status: 200,
                body: r#"{"ok":true}"#.to_string(),
            })
        );
        assert_eq!(
            parse_kick_bridge_message(r#"{"__TAURI__":true,"cmd":"foo"}"#),
            None
        );
        assert_eq!(parse_kick_bridge_message("not json"), None);
    }

    #[test]
    fn extracts_campaign_details_and_handles_null_directory() {
        let batch = json!([
            {
                "extensions": { "operationName": "DropCampaignDetails" },
                "data": { "user": { "dropCampaign": { "id": "other-camp", "name": "Other" } } }
            },
            {
                "extensions": { "operationName": "DropCampaignDetails" },
                "data": { "user": { "dropCampaign": { "id": "target-camp", "name": "Tarkov Day 6", "self": { "isAccountConnected": true } } } }
            }
        ]);
        let found = inspect_twitch_campaign_details_payload(batch.to_string().as_bytes(), "target-camp");
        let Some(TwitchGqlInspect::Data(val)) = found else {
            panic!("expected matching DropCampaignDetails Data");
        };
        assert_eq!(val["data"]["user"]["dropCampaign"]["self"]["isAccountConnected"], true);

        let err_payload = json!({
            "extensions": { "operationName": "DropCampaignDetails" },
            "errors": [{ "message": "failed integrity check" }]
        });
        assert!(matches!(
            inspect_twitch_campaign_details_payload(err_payload.to_string().as_bytes(), "target-camp"),
            Some(TwitchGqlInspect::TransientIntegrity(_))
        ));

        let null_dir = json!({
            "extensions": { "operationName": "DirectoryPage_Game" },
            "data": { "game": null }
        });
        assert!(matches!(
            inspect_twitch_gql_payload(null_dir.to_string().as_bytes(), "directory"),
            Some(TwitchGqlInspect::Data(_))
        ));
    }

    #[test]
    fn inspects_twitch_claim_responses_and_already_claimed_inventory() {
        let claimed_ok = json!([
            {
                "extensions": { "operationName": "DropsPage_ClaimDropRewards" },
                "data": { "claimDropRewards": { "status": "ELIGIBLE_FOR_ALL" } }
            }
        ]);
        assert!(matches!(
            inspect_twitch_claim_payload(claimed_ok.to_string().as_bytes(), "d1", Some("inst1")),
            Some(TwitchGqlInspect::Data(_))
        ));

        let already = json!({
            "extensions": { "operationName": "DropsPage_ClaimDropRewards" },
            "data": { "claimDropRewards": { "status": "DROP_INSTANCE_ALREADY_CLAIMED" } }
        });
        assert!(matches!(
            inspect_twitch_claim_payload(already.to_string().as_bytes(), "d1", Some("inst1")),
            Some(TwitchGqlInspect::Data(_))
        ));

        let inv_claimed = json!({
            "extensions": { "operationName": "Inventory" },
            "data": {
                "currentUser": {
                    "inventory": {
                        "dropCampaignsInProgress": [
                            {
                                "id": "camp1",
                                "timeBasedDrops": [
                                    {
                                        "id": "d1",
                                        "self": { "dropInstanceID": "inst1", "isClaimed": true }
                                    }
                                ]
                            }
                        ]
                    }
                }
            }
        });
        assert!(matches!(
            inspect_twitch_claim_payload(inv_claimed.to_string().as_bytes(), "d1", Some("inst1")),
            Some(TwitchGqlInspect::Data(_))
        ));

        let integrity_err = json!({
            "extensions": { "operationName": "DropsPage_ClaimDropRewards" },
            "errors": [{ "message": "failed integrity check" }]
        });
        assert!(matches!(
            inspect_twitch_claim_payload(integrity_err.to_string().as_bytes(), "d1", None),
            Some(TwitchGqlInspect::TransientIntegrity(_))
        ));

        let unauth_inv = json!({
            "extensions": { "operationName": "Inventory" },
            "data": { "currentUser": null }
        });
        assert!(matches!(
            inspect_twitch_claim_payload(unauth_inv.to_string().as_bytes(), "d1", None),
            Some(TwitchGqlInspect::Unauthenticated(_))
        ));
    }
}
