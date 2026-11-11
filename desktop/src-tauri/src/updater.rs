// SPDX-License-Identifier: MIT

//! In-App Updater management, stream safety gate, and background scheduler.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager, Url};
use tauri_plugin_updater::UpdaterExt;

use crate::dto::{UpdateMetadataDto, UpdaterStatusDto};
use crate::state::AppState;

pub const DEFAULT_UPDATER_ENDPOINT: &str =
    "https://github.com/plynte-labs/LiveAudio/releases/latest/download/latest.json";
pub const UPDATER_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
pub const STARTUP_CHECK_DELAY: Duration = Duration::from_secs(10);

/// Parse and validate an updater endpoint URL.
///
/// Compile-time or configuration overrides MUST use HTTPS; HTTP is rejected.
pub fn parse_updater_endpoint(override_endpoint: Option<&str>) -> Result<Url, String> {
    if let Some(endpoint) = override_endpoint {
        let trimmed = endpoint.trim();
        if !trimmed.is_empty() {
            let parsed = Url::parse(trimmed)
                .map_err(|e| format!("Invalid updater endpoint URL: {e}"))?;
            if parsed.scheme() != "https" {
                return Err("Updater endpoint must use HTTPS".to_string());
            }
            return Ok(parsed);
        }
    }
    Url::parse(DEFAULT_UPDATER_ENDPOINT)
        .map_err(|e| format!("Invalid default updater URL: {e}"))
}

/// Resolves the effective updater endpoint URL, checking compile-time override first.
pub fn resolve_updater_endpoint() -> Result<Url, String> {
    parse_updater_endpoint(option_env!("LIVEAUDIO_UPDATER_ENDPOINT"))
}

/// Evaluates if a remote SemVer version string is strictly greater than the current version.
pub fn is_update_greater(current: &str, remote: &str) -> Result<bool, String> {
    let current_v = semver::Version::parse(current.trim().trim_start_matches('v'))
        .map_err(|e| format!("Invalid current version '{current}': {e}"))?;
    let remote_v = semver::Version::parse(remote.trim().trim_start_matches('v'))
        .map_err(|e| format!("Invalid remote version '{remote}': {e}"))?;
    Ok(remote_v > current_v)
}

/// Evaluates stream safety rules: checks if the application is busy.
pub fn evaluate_stream_safety(
    is_running: bool,
    ws_clients: usize,
    has_foreign_service: bool,
) -> Result<(), &'static str> {
    if is_running {
        return Err("transcription service is running");
    }
    if ws_clients > 0 {
        return Err("OBS clients connected");
    }
    if has_foreign_service {
        return Err("external service monitored");
    }
    Ok(())
}

/// RAII in-flight lock guard for idempotent update operations.
#[derive(Debug)]
pub struct InFlightGuard<'a> {
    flag: &'a AtomicBool,
}

impl<'a> InFlightGuard<'a> {
    pub fn try_acquire(flag: &'a AtomicBool) -> Result<Self, String> {
        if flag.swap(true, Ordering::SeqCst) {
            Err("AlreadyInProgress".to_string())
        } else {
            Ok(Self { flag })
        }
    }
}

impl<'a> Drop for InFlightGuard<'a> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::SeqCst);
    }
}

/// Checks for updates against the configured endpoint.
///
/// When `force` is false, throttles according to configuration (24h) and fails silently.
pub async fn check_for_updates_internal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &AppState,
    force: bool,
) -> Result<Option<UpdateMetadataDto>, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    if !force {
        let (enabled, last_check) = {
            let cfg = state.config.read().await;
            (cfg.check_updates_enabled, cfg.last_update_check)
        };
        if !enabled || !liveaudio_core::config::should_check_for_updates(last_check, now) {
            return Ok(None);
        }
    }

    let _guard = InFlightGuard::try_acquire(&state.updater_check_in_progress)?;

    let endpoint = resolve_updater_endpoint()?;
    let updater = app
        .updater_builder()
        .timeout(UPDATER_REQUEST_TIMEOUT)
        .endpoints(vec![endpoint])
        .map_err(|e| e.to_string())?
        .version_comparator(|current, remote| remote.version > current)
        .build()
        .map_err(|e| e.to_string())?;

    match updater.check().await {
        Ok(Some(update)) => {
            crate::config_sync::record_update_check(state, now).await;

            let metadata = UpdateMetadataDto {
                version: update.version.clone(),
                current_version: update.current_version.clone(),
                notes: update.body.clone(),
                pub_date: update.date.map(|d| d.to_string()),
            };

            *state.updater_available.write().await = Some(metadata.clone());
            *state.updater_pending_update.lock().await = Some(update);

            let _ = app.emit("update-available", &metadata);
            Ok(Some(metadata))
        }
        Ok(None) => {
            crate::config_sync::record_update_check(state, now).await;
            *state.updater_available.write().await = None;
            *state.updater_pending_update.lock().await = None;
            Ok(None)
        }
        Err(e) => {
            let failure_ts = liveaudio_core::config::update_check_failure_timestamp(now);
            crate::config_sync::record_update_check(state, failure_ts).await;
            if !force {
                tracing::debug!("Background update check failed: {e}");
                Ok(None)
            } else {
                Err(format!("Unable to check for updates: {e}"))
            }
        }
    }
}

/// Immediately downloads and installs the pending or checked update if completely idle.
pub async fn install_update_now_internal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &AppState,
) -> Result<(), String> {
    if let Err(reason) = state.check_stream_safety().await {
        return Err(format!("Cannot install update: {reason}"));
    }

    let _guard = InFlightGuard::try_acquire(&state.updater_install_in_progress)?;

    if let Err(reason) = state.check_stream_safety().await {
        return Err(format!("Cannot install update: {reason}"));
    }

    let update = {
        let mut pending = state.updater_pending_update.lock().await;
        if let Some(update) = pending.take() {
            update
        } else {
            let endpoint = resolve_updater_endpoint()?;
            let updater = app
                .updater_builder()
                .timeout(UPDATER_REQUEST_TIMEOUT)
                .endpoints(vec![endpoint])
                .map_err(|e| e.to_string())?
                .version_comparator(|current, remote| remote.version > current)
                .build()
                .map_err(|e| e.to_string())?;

            updater
                .check()
                .await
                .map_err(|e| format!("Check failed: {e}"))?
                .ok_or_else(|| "No update available".to_string())?
        }
    };

    if let Err(reason) = state.check_stream_safety().await {
        *state.updater_pending_update.lock().await = Some(update);
        return Err(format!("Cannot install update: {reason}"));
    }

    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|e| format!("Failed to download and install update: {e}"))?;

    Ok(())
}

/// Returns whether the install-on-close flag is armed.
pub fn is_install_on_close_armed(state: &AppState) -> bool {
    state.updater_install_on_close.load(Ordering::SeqCst)
}

/// Executes install-on-close when the application is closing after service stop.
pub async fn execute_install_on_close<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    state: &AppState,
) {
    let update = {
        let mut pending = state.updater_pending_update.lock().await;
        pending.take()
    };

    let update = match update {
        Some(u) => Some(u),
        None => {
            if let Ok(endpoint) = resolve_updater_endpoint() {
                if let Ok(builder) = app
                    .updater_builder()
                    .timeout(UPDATER_REQUEST_TIMEOUT)
                    .endpoints(vec![endpoint])
                {
                    if let Ok(updater) = builder
                        .version_comparator(|current, remote| remote.version > current)
                        .build()
                    {
                        updater.check().await.ok().flatten()
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        }
    };

    if let Some(update) = update {
        tracing::info!("Installing update v{} on close", update.version);
        if let Err(e) = update.download_and_install(|_, _| {}, || {}).await {
            tracing::error!("Failed to install update on close: {e}");
        }
    }
}

/// Background task scheduled at application launch: delays 10 seconds and checks for updates.
pub async fn schedule_startup_update_check<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    tokio::time::sleep(STARTUP_CHECK_DELAY).await;
    let state = app.state::<AppState>().inner().clone();
    let _ = check_for_updates_internal(&app, &state, false).await;
}

/// Gets the updater status snapshot for the frontend.
pub async fn get_updater_status_internal(state: &AppState) -> UpdaterStatusDto {
    let available_update = state.updater_available.read().await.clone();
    let install_on_close = state.updater_install_on_close.load(Ordering::SeqCst);
    let is_checking = state.updater_check_in_progress.load(Ordering::SeqCst);
    let is_installing = state.updater_install_in_progress.load(Ordering::SeqCst);
    let stream_busy = state.is_busy().await;
    let last_check = state.config.read().await.last_update_check;
    let current_version = env!("CARGO_PKG_VERSION").to_string();

    UpdaterStatusDto {
        current_version,
        available_update,
        install_on_close,
        is_checking,
        is_installing,
        stream_busy,
        last_check,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_updater_endpoint() {
        // Default when None or empty
        let def = parse_updater_endpoint(None).unwrap();
        assert_eq!(def.as_str(), DEFAULT_UPDATER_ENDPOINT);

        let empty = parse_updater_endpoint(Some("   ")).unwrap();
        assert_eq!(empty.as_str(), DEFAULT_UPDATER_ENDPOINT);

        // Custom HTTPS
        let custom =
            parse_updater_endpoint(Some("https://example.com/test/latest.json")).unwrap();
        assert_eq!(custom.as_str(), "https://example.com/test/latest.json");

        // Reject plain HTTP
        let http_err = parse_updater_endpoint(Some("http://example.com/latest.json"));
        assert!(http_err.is_err());
        assert!(http_err.unwrap_err().contains("must use HTTPS"));

        // Reject invalid URL
        assert!(parse_updater_endpoint(Some("not-a-url")).is_err());
    }

    #[test]
    fn test_version_monotonicity_greater() {
        assert!(is_update_greater("1.6.0", "1.6.1").unwrap());
        assert!(is_update_greater("1.5.1", "1.6.0").unwrap());
        assert!(is_update_greater("v1.5.1", "v1.6.0").unwrap());
        assert!(is_update_greater("1.6.0-rc.1", "1.6.0").unwrap());

        // Equal versions rejected
        assert!(!is_update_greater("1.6.0", "1.6.0").unwrap());
        assert!(!is_update_greater("v1.6.0", "1.6.0").unwrap());

        // Downgrades rejected
        assert!(!is_update_greater("1.6.1", "1.6.0").unwrap());
        assert!(!is_update_greater("1.6.0", "1.5.1").unwrap());

        // Invalid semver
        assert!(is_update_greater("abc", "1.6.0").is_err());
        assert!(is_update_greater("1.6.0", "xyz").is_err());
    }

    #[test]
    fn test_evaluate_stream_safety_gating() {
        // Completely idle -> allowed
        assert!(evaluate_stream_safety(false, 0, false).is_ok());

        // Transcription running -> blocked
        let err1 = evaluate_stream_safety(true, 0, false).unwrap_err();
        assert!(err1.contains("running"));

        // OBS connected -> blocked
        let err2 = evaluate_stream_safety(false, 1, false).unwrap_err();
        assert!(err2.contains("OBS clients"));

        // Foreign service monitored -> blocked
        let err3 = evaluate_stream_safety(false, 0, true).unwrap_err();
        assert!(err3.contains("external service"));

        // Multiple active conditions -> blocked
        assert!(evaluate_stream_safety(true, 2, true).is_err());
    }

    #[test]
    fn test_in_flight_guard_idempotency() {
        let flag = AtomicBool::new(false);

        {
            let guard = InFlightGuard::try_acquire(&flag);
            assert!(guard.is_ok());
            assert!(flag.load(Ordering::SeqCst));

            // Concurrent acquire fails
            let second = InFlightGuard::try_acquire(&flag);
            assert!(second.is_err());
            assert_eq!(second.unwrap_err(), "AlreadyInProgress");
        }

        // Released after drop
        assert!(!flag.load(Ordering::SeqCst));
        let third = InFlightGuard::try_acquire(&flag);
        assert!(third.is_ok());
    }
}
