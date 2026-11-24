use std::sync::{Arc, Mutex};

use tauri::menu::{MenuBuilder, MenuItem, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
use tracing::warn;

use crate::auth::login_window::{
    close_epic_claim_window, close_kick_bridge_window, close_twitch_bridge_window,
};
use crate::platforms::{MinerState, PlatformId, PlatformSnapshot};
use crate::state::{AppState, MinerControl};
use crate::windows::ensure_main_window;

pub struct TrayHandles {
    pub tray: TrayIcon<Wry>,
    pub toggle_pause: MenuItem<Wry>,
    pub status_twitch: MenuItem<Wry>,
    pub status_kick: MenuItem<Wry>,
}

pub struct TrayState(pub Mutex<TrayHandles>);

pub fn setup_tray(app: &AppHandle, state: Arc<AppState>) -> Result<(), String> {
    let open_item = MenuItemBuilder::with_id("open", "Open Dropilot")
        .build(app)
        .map_err(|e| format!("Tray menu open item error: {e}"))?;
    let toggle_pause = MenuItemBuilder::with_id("toggle-pause", "Pause all")
        .build(app)
        .map_err(|e| format!("Tray menu pause item error: {e}"))?;
    let sep1 = PredefinedMenuItem::separator(app)
        .map_err(|e| format!("Tray separator error: {e}"))?;
    let status_twitch = MenuItemBuilder::with_id("status-twitch", "Twitch: Logged out")
        .enabled(false)
        .build(app)
        .map_err(|e| format!("Tray twitch status item error: {e}"))?;
    let status_kick = MenuItemBuilder::with_id("status-kick", "Kick: Logged out")
        .enabled(false)
        .build(app)
        .map_err(|e| format!("Tray kick status item error: {e}"))?;
    let sep2 = PredefinedMenuItem::separator(app)
        .map_err(|e| format!("Tray separator error: {e}"))?;
    let quit_item = MenuItemBuilder::with_id("quit", "Quit Dropilot")
        .build(app)
        .map_err(|e| format!("Tray quit item error: {e}"))?;

    let menu = MenuBuilder::new(app)
        .items(&[
            &open_item,
            &toggle_pause,
            &sep1,
            &status_twitch,
            &status_kick,
            &sep2,
            &quit_item,
        ])
        .build()
        .map_err(|e| format!("Failed building tray menu: {e}"))?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .tooltip("Dropilot")
        .show_menu_on_left_click(false);

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    let state_menu = state.clone();
    let state_click = state.clone();

    let tray = builder
        .on_menu_event(move |app_handle, event| {
            let st = state_menu.clone();
            let app_c = app_handle.clone();
            match event.id().as_ref() {
                "open" => {
                    tauri::async_runtime::spawn(async move {
                        if let Err(err) = ensure_main_window(&app_c, &st).await {
                            warn!(error = %err, "Failed opening main window from tray");
                        }
                    });
                }
                "toggle-pause" => {
                    tauri::async_runtime::spawn(async move {
                        let tw_paused = st.twitch_snapshot.read().await.state == MinerState::Paused;
                        let kk_paused = st.kick_snapshot.read().await.state == MinerState::Paused;
                        if tw_paused && kk_paused {
                            st.send_all_miners(MinerControl::Resume).await;
                        } else {
                            st.send_all_miners(MinerControl::Pause).await;
                        }
                    });
                }
                "quit" => {
                    st.allow_exit.store(true, std::sync::atomic::Ordering::SeqCst);
                    tauri::async_runtime::spawn(async move {
                        st.send_all_miners(MinerControl::Shutdown).await;
                        close_kick_bridge_window(&app_c);
                        close_twitch_bridge_window(&app_c);
                        close_epic_claim_window(&app_c);
                        app_c.exit(0);
                    });
                }
                _ => {}
            }
        })
        .on_tray_icon_event(move |tray_icon, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app_c = tray_icon.app_handle().clone();
                let st = state_click.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(err) = ensure_main_window(&app_c, &st).await {
                        warn!(error = %err, "Failed opening main window on tray click");
                    }
                });
            }
        })
        .build(app)
        .map_err(|e| format!("Failed creating tray icon: {e}"))?;

    app.manage(TrayState(Mutex::new(TrayHandles {
        tray,
        toggle_pause,
        status_twitch,
        status_kick,
    })));

    Ok(())
}

pub async fn update_tray_from_state(app: &AppHandle, state: &AppState) {
    let tw = state.twitch_snapshot.read().await.clone();
    let kk = state.kick_snapshot.read().await.clone();

    let tw_line = format_platform_status(&tw);
    let kk_line = format_platform_status(&kk);
    let tooltip = format!("{tw_line} · {kk_line}");
    let all_paused = tw.state == MinerState::Paused && kk.state == MinerState::Paused;

    if let Some(tray_state) = app.try_state::<TrayState>() {
        if let Ok(handles) = tray_state.0.lock() {
            let _ = handles.status_twitch.set_text(&tw_line);
            let _ = handles.status_kick.set_text(&kk_line);
            let _ = handles.toggle_pause.set_text(if all_paused {
                "Resume all"
            } else {
                "Pause all"
            });
            let _ = handles.tray.set_tooltip(Some(&tooltip));
        }
    }
}

fn format_platform_status(snap: &PlatformSnapshot) -> String {
    let name = match snap.platform {
        PlatformId::Twitch => "Twitch",
        PlatformId::Kick => "Kick",
    };
    match snap.state {
        MinerState::LoggedOut => format!("{name}: Logged out"),
        MinerState::Idle => format!("{name}: Idle"),
        MinerState::Selecting => format!("{name}: Selecting"),
        MinerState::Paused => format!("{name}: Paused"),
        MinerState::Error => format!("{name}: Error"),
        MinerState::Claiming => format!("{name}: Claiming"),
        MinerState::Mining => {
            if let Some(camp_id) = snap.current_campaign_id.as_deref() {
                if let Some(camp) = snap.campaigns.iter().find(|c| c.id == camp_id) {
                    if let Some(drop_id) = snap.current_drop_id.as_deref() {
                        if let Some(drop) = camp.drops.iter().find(|d| d.id == drop_id) {
                            let pct = if drop.required_minutes > 0 {
                                ((drop.current_minutes as f64 / drop.required_minutes as f64) * 100.0)
                                    .round()
                                    .clamp(0.0, 100.0) as u32
                            } else {
                                0
                            };
                            return format!("{name}: Mining - {} ({pct}%)", camp.game.name);
                        }
                    }
                    return format!("{name}: Mining - {}", camp.game.name);
                }
            }
            format!("{name}: Mining")
        }
    }
}
