// SPDX-License-Identifier: MIT

//! LiveAudio Desktop Tauri 2 Application Backend Library.

pub mod cli;
pub mod commands;
pub mod config_sync;
pub mod diagnostics;
pub mod dto;
pub mod logging;
pub mod state;
pub mod updater;

#[cfg(test)]
pub mod tests;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use liveaudio_core::config::load_config;
use state::AppState;
use tauri::{Manager, RunEvent};

/// Safety net over the stop path's own sink/worker deadlines so exit never hangs.
const EXIT_STOP_DEADLINE: Duration = Duration::from_secs(10);

/// Brings the running instance's main window to front when a second launch is attempted.
fn focus_main_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    for result in [window.unminimize(), window.show(), window.set_focus()] {
        if let Err(error) = result {
            tracing::warn!("Could not focus the main window: {error}");
        }
    }
}

/// Initialize and execute the Tauri 2 desktop application.
pub fn run() {
    let _log_file = logging::init_logging();
    let corrupt_backup = config_sync::quarantine_corrupt_config();
    let initial_config = load_config().unwrap_or_default();
    let app_state = AppState::new(initial_config);
    *app_state.startup_warning.lock().unwrap() = corrupt_backup;
    let exit_stop_done = Arc::new(AtomicBool::new(false));
    let autostart = cli::autostart_requested(std::env::args());

    // Must be the first plugin. A second desktop instance hands off to the first one and exits
    // inside `build()`, before it reaches the run loop (no exit-stop handler). The service lock is
    // taken by `start_service`, so a desktop opened while a headless service runs still opens.
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            focus_main_window(app)
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(app_state)
        .setup(move |app| {
            tauri::async_runtime::spawn(commands::watch_foreign_service(app.handle().clone()));
            tauri::async_runtime::spawn(config_sync::watch_external_config(app.handle().clone()));
            tauri::async_runtime::spawn(updater::schedule_startup_update_check(app.handle().clone()));
            if autostart {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let state = handle.state::<AppState>();
                    match commands::start_service(handle.clone(), state).await {
                        Ok(status) => tracing::info!(
                            "Autostart: servicio iniciado (is_running={})",
                            status.is_running
                        ),
                        Err(error) => tracing::error!("Autostart: error iniciando el servicio: {}", error),
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::get_profile_presets,
            commands::save_config,
            commands::save_subtitle_style,
            commands::reset_config_defaults,
            commands::get_audio_devices,
            commands::get_service_status,
            commands::start_service,
            commands::stop_service,
            commands::restart_service,
            commands::prepare_asr_runtime,
            commands::cancel_runtime_setup,
            commands::get_gpu_backend_status,
            commands::list_system_fonts,
            commands::install_nvidia_runtime,
            commands::cancel_nvidia_runtime_install,
            commands::get_obs_overlay_info,
            commands::export_diagnostics,
            commands::send_test_subtitle,
            commands::open_session_folder,
            commands::open_author_profile,
            commands::open_nvidia_license_url,
            commands::open_support_link,
            commands::get_usage_stats,
            commands::get_support_environment,
            commands::get_recent_logs,
            commands::check_for_updates,
            commands::install_update_now,
            commands::set_install_on_close,
            commands::get_updater_status,
            commands::get_allowed_connections,
            commands::allow_loopback_port,
            commands::revoke_loopback_port,
            commands::take_startup_warning,
        ])
        .build(tauri::generate_context!())
        .expect("error while building LiveAudio Desktop application");
    app.run(move |app, event| {
        // Stop the service once before exiting so session sinks flush and the worker is reaped.
        if let RunEvent::ExitRequested { api, .. } = event {
            if exit_stop_done.swap(true, Ordering::SeqCst) {
                return;
            }
            api.prevent_exit();
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<AppState>().inner().clone();
                let stop = commands::stop_service_for_exit(&app, &state);
                if tokio::time::timeout(EXIT_STOP_DEADLINE, stop)
                    .await
                    .is_err()
                {
                    tracing::warn!("Service stop on exit exceeded {EXIT_STOP_DEADLINE:?}");
                }
                if updater::is_install_on_close_armed(&state) {
                    updater::execute_install_on_close(&app, &state).await;
                }
                app.exit(0);
            });
        }
    });
}
