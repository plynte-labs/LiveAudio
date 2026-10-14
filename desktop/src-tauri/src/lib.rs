// SPDX-License-Identifier: MIT

//! LiveAudio Desktop Tauri 2 Application Backend Library.

pub mod commands;
pub mod dto;
pub mod logging;
pub mod state;

#[cfg(test)]
pub mod tests;

use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use liveaudio_core::config::{get_data_home, load_config};
use state::AppState;
use tauri::{Manager, RunEvent};

const INSTANCE_LOCK_NAME: &str = "service.lock";
/// Safety net over the stop path's own sink/worker deadlines so exit never hangs.
const EXIT_STOP_DEADLINE: Duration = Duration::from_secs(10);

/// Takes the non-blocking OS lock on `service.lock` in `dir` (Python `lock.py` parity).
/// Returns `None` when another instance holds it; the OS releases it on process exit.
pub(crate) fn try_acquire_instance_lock(dir: &Path) -> std::io::Result<Option<File>> {
    std::fs::create_dir_all(dir)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(INSTANCE_LOCK_NAME))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

/// Initialize and execute the Tauri 2 desktop application.
pub fn run() {
    let _log_file = logging::init_logging();
    let _instance_lock = match try_acquire_instance_lock(&get_data_home()) {
        Ok(Some(lock)) => Some(lock),
        Ok(None) => {
            tracing::warn!("LiveAudio is already running; exiting this second instance");
            return;
        }
        Err(error) => {
            tracing::warn!("Instance lock unavailable, continuing without it: {error}");
            None
        }
    };
    let initial_config = load_config().unwrap_or_default();
    let app_state = AppState::new(initial_config);
    let exit_stop_done = Arc::new(AtomicBool::new(false));

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::get_profile_presets,
            commands::save_config,
            commands::reset_config_defaults,
            commands::get_audio_devices,
            commands::get_service_status,
            commands::start_service,
            commands::stop_service,
            commands::restart_service,
            commands::prepare_asr_runtime,
            commands::cancel_runtime_setup,
            commands::get_obs_overlay_info,
            commands::export_diagnostics,
            commands::send_test_subtitle,
            commands::open_session_folder,
            commands::open_author_profile,
            commands::get_recent_logs,
        ])
        .build(tauri::generate_context!())
        .expect("error while building LiveAudio Desktop application")
        .run(move |app, event| {
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
                    app.exit(0);
                });
            }
        });
}
