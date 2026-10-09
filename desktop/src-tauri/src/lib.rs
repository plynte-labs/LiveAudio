// SPDX-License-Identifier: MIT

//! LiveAudio Desktop Tauri 2 Application Backend Library.

pub mod commands;
pub mod dto;
pub mod logging;
pub mod state;

#[cfg(test)]
pub mod tests;

use liveaudio_core::config::load_config;
use state::AppState;

/// Initialize and execute the Tauri 2 desktop application.
pub fn run() {
    let _log_file = logging::init_logging();
    let initial_config = load_config().unwrap_or_default();
    let app_state = AppState::new(initial_config);

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
            commands::get_obs_overlay_info,
            commands::export_diagnostics,
            commands::send_test_subtitle,
            commands::open_session_folder,
            commands::open_author_profile,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LiveAudio Desktop application");
}
