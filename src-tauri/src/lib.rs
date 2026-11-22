pub mod auth;
pub mod autostart;
pub mod commands;
pub mod events;
pub mod logging;
pub mod miner;
pub mod net;
pub mod platforms;
pub mod secrets;
pub mod settings;
pub mod state;
pub mod tray;
pub mod update_check;
pub mod windows;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::{Manager, RunEvent, WindowEvent};
use tracing::{info, warn};

use crate::events::EventHub;
use crate::logging::{init_logging, LogBuffer};
use crate::miner::engine::{spawn_miner_supervisor, PlatformBackend};
use crate::net::client::AllowlistedClient;
use crate::platforms::kick::KickPlatform;
#[cfg(feature = "mock")]
use crate::platforms::mock::MockPlatform;
use crate::platforms::twitch::TwitchPlatform;
use crate::platforms::PlatformId;
use crate::settings::load_settings;
use crate::state::{AppState, MinerControl};
use crate::tray::setup_tray;
use crate::windows::{capture_bounds_sync, ensure_main_window};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let settings = load_settings();
    let log_buffer = LogBuffer::new();
    let log_handle = init_logging(settings.log_level, log_buffer.clone()).ok();

    let http = match AllowlistedClient::new() {
        Ok(client) => Arc::new(client),
        Err(err) => {
            eprintln!("Failed initializing HTTP client: {err}");
            return;
        }
    };

    let app_state = Arc::new(AppState::new(settings.clone(), http.clone(), log_buffer));
    if let Some(handle) = log_handle {
        app_state.set_log_handle(handle);
    }

    let launched_via_autostart = std::env::args().any(|a| a == "--autostart");
    let start_minimized = launched_via_autostart && settings.start_minimized;

    let state_for_single = app_state.clone();
    let state_for_setup = app_state.clone();
    let state_for_window = app_state.clone();
    let state_for_run = app_state.clone();

    let app_result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(move |app, _args, _cwd| {
            let app_c = app.clone();
            let st = state_for_single.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(err) = ensure_main_window(&app_c, &st).await {
                    warn!(error = %err, "Failed focusing existing instance window");
                }
            });
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(app_state.clone())
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::login,
            commands::logout,
            commands::set_campaign_farm,
            commands::set_game_farm,
            commands::set_priority,
            commands::pause,
            commands::resume,
            commands::switch_channel,
            commands::pin_channel,
            commands::retry_platform,
            commands::open_drops_inventory,
            commands::fetch_campaign_details,
            commands::get_settings,
            commands::update_settings,
            commands::accept_disclaimer,
            commands::clear_app_data,
            commands::get_logs,
            commands::open_url,
            commands::open_log_folder,
            commands::open_data_folder,
            commands::check_update,
            commands::is_mock_available,
            commands::report_user_agent,
        ])
        .setup(move |app| {
            let app_handle = app.handle().clone();

            // Forward redacted in-memory log entries to the open UI window.
            let log_app = app_handle.clone();
            state_for_setup.logs.set_emitter(move |entry| {
                EventHub::emit_log_if_open(&log_app, entry);
            });

            if let Err(err) = setup_tray(&app_handle, state_for_setup.clone()) {
                warn!(error = %err, "Failed initializing system tray");
            }

            let os_autostart = crate::autostart::is_autostart_enabled(&app_handle);
            let sync_state = state_for_setup.clone();
            tauri::async_runtime::spawn(async move {
                let mut guard = sync_state.settings.write().await;
                if guard.autostart != os_autostart {
                    guard.autostart = os_autostart;
                }
            });

            // Spawn platform miner tasks.
            let twitch_real = Arc::new(TwitchPlatform::new(
                state_for_setup.http.clone(),
                Some(app_handle.clone()),
                state_for_setup.twitch_bridge_lock.clone(),
            ));
            let kick_real = Arc::new(KickPlatform::new(
                state_for_setup.http.clone(),
                Some(app_handle.clone()),
                state_for_setup.kick_bridge_waiters.clone(),
            ));

            #[cfg(feature = "mock")]
            let (tw_mock, kk_mock) = (
                Some(Arc::new(MockPlatform::new(PlatformId::Twitch)) as Arc<dyn platforms::Platform>),
                Some(Arc::new(MockPlatform::new(PlatformId::Kick)) as Arc<dyn platforms::Platform>),
            );
            #[cfg(not(feature = "mock"))]
            let (tw_mock, kk_mock) = (None, None);

            let tw_tx = spawn_miner_supervisor(
                PlatformId::Twitch,
                app_handle.clone(),
                state_for_setup.clone(),
                PlatformBackend {
                    real: twitch_real,
                    mock: tw_mock,
                },
            );
            let kk_tx = spawn_miner_supervisor(
                PlatformId::Kick,
                app_handle.clone(),
                state_for_setup.clone(),
                PlatformBackend {
                    real: kick_real,
                    mock: kk_mock,
                },
            );

            let reg_state = state_for_setup.clone();
            let win_app = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                reg_state.register_miner(PlatformId::Twitch, tw_tx).await;
                reg_state.register_miner(PlatformId::Kick, kk_tx).await;
                if !start_minimized {
                    if let Err(err) = ensure_main_window(&win_app, &reg_state).await {
                        warn!(error = %err, "Failed opening initial main window");
                    }
                } else {
                    info!("Started minimized to tray via autostart");
                }
            });

            Ok(())
        })
        .on_window_event(move |window, event| {
            if window.label() != "main" {
                return;
            }
            if let WindowEvent::CloseRequested { .. } = event {
                let bounds = capture_bounds_sync(window);
                let st = state_for_window.clone();
                let app_c = window.app_handle().clone();

                tauri::async_runtime::spawn(async move {
                    let close_to_tray = {
                        let mut guard = st.settings.write().await;
                        if let Some(b) = bounds {
                            guard.window_bounds = Some(b);
                        }
                        guard.close_to_tray
                    };
                    let _ = st.persist_settings().await;
                    st.events.reset_cache().await;

                    if !close_to_tray {
                        st.allow_exit.store(true, Ordering::SeqCst);
                        st.send_all_miners(MinerControl::Shutdown).await;
                        app_c.exit(0);
                    } else {
                        info!("Main window closed to tray; WebView2 destroyed to save RAM");
                    }
                });
            }
        })
        .build(tauri::generate_context!());

    let app = match app_result {
        Ok(a) => a,
        Err(err) => {
            eprintln!("Failed starting DropPilot: {err}");
            return;
        }
    };

    app.run(move |_app_handle, event| {
        if let RunEvent::ExitRequested { api, .. } = event {
            if !state_for_run.allow_exit.load(Ordering::SeqCst) {
                api.prevent_exit();
            }
        }
    });
}
