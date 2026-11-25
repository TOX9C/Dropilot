pub mod auth;
pub mod autostart;
pub mod commands;
pub mod edition;
pub mod events;
pub mod history;
pub mod logging;
pub mod miner;
pub mod net;
pub mod notifications;
pub mod platforms;
pub mod remote_config;
pub mod rewards;
pub mod secrets;
pub mod settings;
pub mod state;
pub mod tray;
pub mod update_check;
pub mod webhooks;
pub mod windows;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::{Manager, RunEvent, WindowEvent};
use tracing::{info, warn};

use crate::auth::login_window::close_all_aux_windows;
use crate::events::EventHub;
use crate::logging::{init_logging, LogBuffer};
use crate::miner::engine::{build_platform_backend, spawn_miner_supervisor};
use crate::net::client::AllowlistedClient;
use crate::settings::{load_settings, save_settings};
use crate::state::AppState;
use crate::tray::setup_tray;
use crate::windows::{
    cached_main_bounds, capture_bounds_sync, destroy_main_to_tray, ensure_main_window,
    has_active_aux_webview, is_valid_window_bounds, migrate_legacy_webview_dir, resume_webview,
    set_main_webview_low_memory, suspend_webview, update_cached_main_bounds,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    migrate_legacy_webview_dir();
    let mut settings = load_settings();
    // Always start on Auto channel by default instead of carrying over stale pins from prior sessions.
    let had_pins = settings.twitch.pinned_channel.is_some()
        || settings.kick.pinned_channel.is_some()
        || settings
            .profiles
            .iter()
            .any(|p| p.settings.pinned_channel.is_some());
    if had_pins {
        settings.twitch.pinned_channel = None;
        settings.kick.pinned_channel = None;
        for prof in &mut settings.profiles {
            prof.settings.pinned_channel = None;
        }
        let _ = save_settings(&settings);
    }
    let log_buffer = LogBuffer::new();
    let log_handle = init_logging(settings.log_level, log_buffer.clone()).ok();
    windows::configure_windows_process_group();
    notifications::register_windows_toast_identity();

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
    let launched_in_tray_arg = std::env::args().any(|a| a == "--minimized" || a == "--tray");
    let start_in_tray = settings.start_in_tray
        || launched_in_tray_arg
        || (launched_via_autostart && settings.start_minimized);

    let state_for_single = app_state.clone();
    let state_for_setup = app_state.clone();
    let state_for_window = app_state.clone();
    let state_for_run = app_state.clone();

    let app_result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(move |app, args, _cwd| {
            if args.iter().any(|a| a == "--minimized" || a == "--tray") {
                return;
            }
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(app_state.clone())
        .invoke_handler(tauri::generate_handler![
            commands::get_edition_info,
            commands::get_snapshot,
            commands::add_account_profile,
            commands::remove_account_profile,
            commands::set_active_profile,
            commands::rename_account_profile,
            commands::login,
            commands::logout,
            commands::set_campaign_farm,
            commands::set_game_farm,
            commands::set_priority,
            commands::set_prioritize_ending_soon,
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
            commands::install_update,
            commands::is_mock_available,
            commands::report_user_agent,
            commands::hide_to_tray,
            commands::get_remote_config_status,
            commands::refresh_remote_config,
            commands::get_webhook_secrets_status,
            commands::save_webhook_secrets,
            commands::test_webhook,
            commands::get_history_stats,
            commands::export_history,
            commands::clear_history,
            commands::get_epic_free_games,
            commands::refresh_epic_free_games,
            commands::open_epic_claim,
            commands::open_epic_launcher,
            commands::mark_epic_game_claimed,
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
                let remote_enabled = {
                    let mut guard = sync_state.settings.write().await;
                    if guard.autostart != os_autostart {
                        guard.autostart = os_autostart;
                    }
                    guard.remote_config_enabled
                };
                if remote_enabled {
                    let _ = sync_state.remote_config.refresh_if_stale(&sync_state.http).await;
                }
            });

            crate::rewards::epic::spawn_epic_poll_loop(
                app_handle.clone(),
                state_for_setup.clone(),
            );

            // Spawn platform miner tasks for every edition-allowed profile with staggered startup.
            let initial_profiles: Vec<_> = settings
                .profiles
                .iter()
                .filter(|p| crate::edition::is_profile_allowed(p.platform, &p.id))
                .cloned()
                .collect();
            let mut spawned_miners = Vec::with_capacity(initial_profiles.len());
            for (idx, profile) in initial_profiles.iter().enumerate() {
                let backend = build_platform_backend(
                    &profile.id,
                    profile.platform,
                    &app_handle,
                    &state_for_setup,
                );
                let (tx, task) = spawn_miner_supervisor(
                    profile.id.clone(),
                    profile.platform,
                    (idx as u64) * 350,
                    app_handle.clone(),
                    state_for_setup.clone(),
                    backend,
                );
                tauri::async_runtime::block_on(state_for_setup.register_miner_for_profile(
                    &profile.id,
                    tx,
                    task,
                ));
            }

            let reg_state = state_for_setup.clone();
            let win_app = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                if !start_in_tray {
                    if let Err(err) = ensure_main_window(&win_app, &reg_state).await {
                        warn!(error = %err, "Failed opening initial main window");
                    }
                } else {
                    info!("Started in system tray (main window hidden until opened)");
                }
            });

            Ok(())
        })
        .on_window_event(move |window, event| {
            if window.label() != "main" {
                return;
            }
            match event {
                WindowEvent::Moved(_) => {
                    update_cached_main_bounds(window);
                }
                WindowEvent::Resized(_) => {
                    if window.is_minimized().unwrap_or(false) {
                        let cached_bounds = cached_main_bounds();
                        let app_c = window.app_handle().clone();
                        let st = state_for_window.clone();
                        if has_active_aux_webview(&app_c) {
                            if let Some(wv) = app_c.get_webview_window("main") {
                                suspend_webview(&wv);
                            }
                            info!(
                                "Main window minimized while auxiliary/login/bridge window is open; suspended WebView2 instead of destroying"
                            );
                        } else {
                            tauri::async_runtime::spawn(async move {
                                destroy_main_to_tray(&app_c, &st, cached_bounds).await;
                            });
                        }
                    } else {
                        update_cached_main_bounds(window);
                    }
                }
                WindowEvent::Focused(true) => {
                    if let Some(wv) = window.app_handle().get_webview_window("main") {
                        resume_webview(&wv);
                        set_main_webview_low_memory(&wv, false);
                    }
                }
                WindowEvent::Focused(false) => {
                    if let Some(wv) = window.app_handle().get_webview_window("main") {
                        set_main_webview_low_memory(&wv, true);
                    }
                }
                WindowEvent::CloseRequested { api, .. } => {
                    let close_to_tray = state_for_window.close_to_tray.load(Ordering::Relaxed);
                    let app_c = window.app_handle().clone();
                    let st = state_for_window.clone();
                    let bounds = capture_bounds_sync(window)
                        .or_else(cached_main_bounds)
                        .filter(is_valid_window_bounds);

                    if close_to_tray && has_active_aux_webview(&app_c) {
                        api.prevent_close();
                        let _ = window.hide();
                        if let Some(wv) = app_c.get_webview_window("main") {
                            suspend_webview(&wv);
                        }
                        tauri::async_runtime::spawn(async move {
                            if let Some(b) = bounds {
                                {
                                    let mut guard = st.settings.write().await;
                                    guard.window_bounds = Some(b);
                                }
                                let _ = st.persist_settings().await;
                            }
                            info!(
                                "Close to tray requested while auxiliary/login/bridge window is open; hid main window without destroying WebView2"
                            );
                        });
                    } else if close_to_tray {
                        tauri::async_runtime::spawn(async move {
                            if let Some(b) = bounds {
                                {
                                    let mut guard = st.settings.write().await;
                                    guard.window_bounds = Some(b);
                                }
                                let _ = st.persist_settings().await;
                            }
                            st.events.reset_cache().await;
                            info!(
                                "Main window closed to tray; UI WebView2 destroyed and only dropilot.exe remains"
                            );
                        });
                    } else {
                        tauri::async_runtime::spawn(async move {
                            if let Some(b) = bounds {
                                {
                                    let mut guard = st.settings.write().await;
                                    guard.window_bounds = Some(b);
                                }
                                let _ = st.persist_settings().await;
                            }
                            st.events.reset_cache().await;
                            st.allow_exit.store(true, Ordering::SeqCst);
                            st.shutdown().await;
                            close_all_aux_windows(&app_c);
                            app_c.exit(0);
                        });
                    }
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!());

    let app = match app_result {
        Ok(a) => a,
        Err(err) => {
            eprintln!("Failed starting Dropilot: {err}");
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
