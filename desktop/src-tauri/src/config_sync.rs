// SPDX-License-Identifier: MIT

//! Keeps the desktop's in-memory config in step with `config.json` when another writer (the MCP
//! server, a text editor) changes it, and saves settings-form edits without reverting those changes.

use crate::commands::keep_subtitle_style;
use crate::state::AppState;
use liveaudio_core::config::patch::{save_ui_changes, RESTART_REQUIRED_KEYS};
use liveaudio_core::config::{config_file_path, load_from_path, LiveAudioConfig};
use serde::Serialize;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter, Manager};

/// Event emitted to the frontend when the config changed on disk.
pub(crate) const CONFIG_CHANGED_EVENT: &str = "config_changed";

/// An external change adopted into the running app.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct ExternalConfigChange {
    pub config: LiveAudioConfig,
    pub changed_keys: Vec<String>,
    /// True when a changed key only takes effect after the ASR service restarts.
    pub restart_required: bool,
}

/// Adopts `disk` as the current config when it differs. Returns the change to announce, if any.
pub(crate) fn adopt_disk_config(
    current: &mut LiveAudioConfig,
    disk: LiveAudioConfig,
) -> Option<ExternalConfigChange> {
    let mut disk = disk;
    disk.normalize();
    if *current == disk {
        return None;
    }
    let before = serde_json::to_value(&*current).unwrap_or_default();
    let after = serde_json::to_value(&disk).unwrap_or_default();
    let mut changed_keys: Vec<String> = after
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(key, value)| before.get(key.as_str()) != Some(value))
        .map(|(key, _)| key.clone())
        .collect();
    changed_keys.sort();
    let restart_required = changed_keys
        .iter()
        .any(|key| RESTART_REQUIRED_KEYS.contains(&key.as_str()));
    *current = disk.clone();
    Some(ExternalConfigChange {
        config: disk,
        changed_keys,
        restart_required,
    })
}

/// Saves a settings form under the runtime config lock. `base` is the config the form was loaded
/// from; `None` means the current in-memory config.
pub(crate) async fn save_settings_form(
    state: &AppState,
    base: Option<LiveAudioConfig>,
    next: LiveAudioConfig,
) -> Result<LiveAudioConfig, String> {
    let mut guard = state.config.write().await;
    // The subtitle style belongs to save_subtitle_style: the form never diffs or overwrites it.
    let stored = guard.clone();
    let base = keep_subtitle_style(base.unwrap_or_else(|| stored.clone()), &stored);
    let next = keep_subtitle_style(next, &stored);
    let saved = save_ui_changes(&base, &next)
        .map_err(|e| format!("Failed to save config: {e}"))?
        .ok_or_else(|| "Failed to save config: config lock unavailable".to_string())?;
    replace_runtime_config(state, &mut guard, saved.clone()).await;
    Ok(saved)
}

/// The one place that pushes a subtitle theme to OBS. Every path that replaces the in-memory config
/// (form save, style save, reset, external edit) calls it with the theme and style it replaced, so the overlay
/// and late joiners never keep an older theme or style than the one the app holds.
pub(crate) async fn push_theme_if_changed(
    state: &AppState,
    previous_theme: &std::collections::BTreeMap<String, String>,
    previous_style: &str,
    current: &LiveAudioConfig,
) {
    if *previous_theme == current.subtitle_theme && previous_style == current.subtitle_style {
        return;
    }
    if let Some(ws) = state.ws_server.lock().await.as_ref() {
        crate::commands::broadcast_subtitle_theme(ws, current);
    }
}

/// Replaces the in-memory config held under `guard` and pushes a changed theme or style to OBS.
pub(crate) async fn replace_runtime_config(
    state: &AppState,
    guard: &mut LiveAudioConfig,
    next: LiveAudioConfig,
) {
    let previous = std::mem::replace(guard, next);
    push_theme_if_changed(
        state,
        &previous.subtitle_theme,
        &previous.subtitle_style,
        guard,
    )
    .await;
}

/// Saves only the subtitle style, theme and presets. The file on disk is the base, so general settings
/// changed there (for example by an MCP write) survive the save.
pub(crate) async fn save_style_fields(
    state: &AppState,
    theme: std::collections::BTreeMap<String, String>,
    presets: Vec<liveaudio_core::config::SubtitlePreset>,
    subtitle_style: Option<String>,
) -> Result<LiveAudioConfig, String> {
    let mut guard = state.config.write().await;
    let stored = guard.clone();
    let next = crate::commands::with_subtitle_style(&stored, theme, presets, subtitle_style);
    let saved = save_ui_changes(&stored, &next)
        .map_err(|e| format!("Failed to save config: {e}"))?
        .ok_or_else(|| "Failed to save config: config lock unavailable".to_string())?;
    replace_runtime_config(state, &mut guard, saved.clone()).await;
    Ok(saved)
}

/// Persists only `last_update_check`, under the write lock and through the disk merge, so a stale
/// in-memory config never reverts external edits and an unreadable file is never overwritten.
pub(crate) async fn record_update_check(state: &AppState, timestamp: i64) {
    let _write_guard = state.config_write_lock.lock().await;
    let mut guard = state.config.write().await;
    let stored = guard.clone();
    let next = LiveAudioConfig {
        last_update_check: timestamp,
        ..stored.clone()
    };
    match save_ui_changes(&stored, &next) {
        Ok(Some(saved)) => replace_runtime_config(state, &mut guard, saved).await,
        Ok(None) => tracing::warn!("Update check time not saved: config lock unavailable"),
        Err(error) => tracing::warn!("Update check time not saved: {error}"),
    }
}

/// Moves an unreadable `config.json` aside as `config.json.corrupt-<unix-ts>` so the defaults used
/// next never overwrite the user's file. Returns the backup file name, or `None` when the file is
/// missing or valid.
pub(crate) fn quarantine_corrupt_config() -> Option<String> {
    let path = config_file_path();
    if !path.exists() || load_from_path(&path).is_ok() {
        return None;
    }
    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let name = format!("config.json.corrupt-{ts}");
    match std::fs::rename(&path, path.with_file_name(&name)) {
        Ok(()) => {
            tracing::error!("config.json is unreadable; moved to {name}, starting with defaults");
            Some(name)
        }
        Err(error) => {
            tracing::error!("config.json is unreadable and could not be moved aside: {error}");
            None
        }
    }
}

/// Loads `config.json` at `path` and adopts it into the running app. Returns the change to announce.
pub(crate) async fn apply_external_config(
    state: &AppState,
    path: &std::path::Path,
) -> Option<ExternalConfigChange> {
    // Same lock order as the saves: the write lock first, then the runtime config.
    let _write_guard = state.config_write_lock.lock().await;
    let disk = match load_from_path(path) {
        Ok(disk) => disk,
        Err(error) => {
            tracing::warn!("Ignoring unreadable config.json: {error}");
            return None;
        }
    };
    let mut guard = state.config.write().await;
    let previous_theme = guard.subtitle_theme.clone();
    let previous_style = guard.subtitle_style.clone();
    let change = adopt_disk_config(&mut guard, disk)?;
    push_theme_if_changed(state, &previous_theme, &previous_style, &guard).await;
    Some(change)
}

/// Polls `config.json` once a second and adopts external edits. Only the file's modification time
/// is checked per tick; a changed file is parsed and compared with the in-memory config, so our
/// own saves (which produce the same config) announce nothing.
pub(crate) async fn watch_external_config(app: AppHandle) {
    let path = config_file_path();
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    let mut seen: Option<SystemTime> = None;
    loop {
        interval.tick().await;
        let Some(modified) = std::fs::metadata(&path).and_then(|m| m.modified()).ok() else {
            continue;
        };
        if seen == Some(modified) {
            continue;
        }
        seen = Some(modified);
        let state = app.state::<AppState>();
        let change = apply_external_config(state.inner(), &path).await;
        if let Some(change) = change {
            let _ = app.emit(CONFIG_CHANGED_EVENT, &change);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::{Mutex, MutexGuard};

    /// Serializes tests that point `LIVEAUDIO_HOME` at a temp dir; removes it on drop.
    static HOME_LOCK: Mutex<()> = Mutex::new(());

    struct TestHome {
        dir: PathBuf,
        _guard: MutexGuard<'static, ()>,
    }

    impl TestHome {
        fn new(name: &str) -> Self {
            let guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let dir = std::env::temp_dir().join(format!(
                "liveaudio-sync-{}-{}",
                name,
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::env::set_var("LIVEAUDIO_HOME", &dir);
            Self { dir, _guard: guard }
        }
    }

    impl Drop for TestHome {
        fn drop(&mut self) {
            std::env::remove_var("LIVEAUDIO_HOME");
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn enabled() -> LiveAudioConfig {
        LiveAudioConfig {
            mcp_style_enabled: true,
            mcp_system_enabled: true,
            ..LiveAudioConfig::default()
        }
    }

    #[test]
    fn external_change_is_adopted_once_and_names_the_changed_keys() {
        let mut current = LiveAudioConfig::default();
        let mut disk = current.clone();
        disk.subtitle_style = "neon".to_string();

        let change = adopt_disk_config(&mut current, disk.clone()).expect("change adopted");
        assert_eq!(change.changed_keys, vec!["subtitle_style".to_string()]);
        assert!(!change.restart_required, "subtitle style is live");
        assert_eq!(current, disk);

        assert!(
            adopt_disk_config(&mut current, disk).is_none(),
            "same file again is a no-op"
        );
    }

    #[test]
    fn runtime_key_change_is_flagged_for_service_restart() {
        let mut current = LiveAudioConfig::default();
        let mut disk = current.clone();
        disk.ws_port = 9100;
        let change = adopt_disk_config(&mut current, disk).expect("change adopted");
        assert!(change.restart_required);
        assert_eq!(current.ws_port, 9100);
    }

    #[tokio::test]
    async fn form_save_keeps_external_edit_and_updates_runtime_state() {
        let home = TestHome::new("form");
        let base = enabled();
        liveaudio_core::config::save_to_path(
            &LiveAudioConfig {
                subtitle_style: "neon".to_string(),
                ..base.clone()
            },
            &home.dir.join("config.json"),
        )
        .unwrap();
        let state = AppState::new(base.clone());
        let form = LiveAudioConfig {
            ws_port: 9000,
            ..base.clone()
        };

        let saved = save_settings_form(&state, Some(base), form)
            .await
            .expect("form saved");

        assert_eq!(saved.subtitle_style, "neon");
        assert_eq!(saved.ws_port, 9000);
        assert_eq!(*state.config.read().await, saved);
    }

    #[tokio::test]
    async fn form_save_never_changes_the_saved_subtitle_style() {
        let _home = TestHome::new("style-owner");
        let stored = LiveAudioConfig {
            subtitle_style: "neon".to_string(),
            subtitle_theme: [("--sub-color".to_string(), "#ff0000".to_string())]
                .into_iter()
                .collect(),
            subtitle_presets: vec![liveaudio_core::config::SubtitlePreset {
                name: "Night".to_string(),
                tokens: Default::default(),
                ..Default::default()
            }],
            ..enabled()
        };
        let state = AppState::new(stored.clone());
        // The form was loaded before the style was saved, so its base and its copy differ from the store.
        let base = LiveAudioConfig {
            subtitle_theme: Default::default(),
            subtitle_presets: Vec::new(),
            ..stored.clone()
        };
        let form = LiveAudioConfig {
            ws_port: 9000,
            subtitle_style: "default".to_string(),
            subtitle_theme: [("--sub-color".to_string(), "#00ff00".to_string())]
                .into_iter()
                .collect(),
            subtitle_presets: Vec::new(),
            ..base.clone()
        };

        let saved = save_settings_form(&state, Some(base), form)
            .await
            .expect("form saved");

        assert_eq!(saved.subtitle_theme, stored.subtitle_theme);
        assert_eq!(saved.subtitle_presets, stored.subtitle_presets);
        assert_eq!(saved.subtitle_style, "neon");
        assert_eq!(saved.ws_port, 9000);
    }

    /// Starts an OBS overlay server on a free OS-assigned port and makes it the running one for `state`.
    async fn running_obs(state: &AppState) -> liveaudio_network::WsServer {
        use liveaudio_network::{NetworkServer, WsServer, WsServerConfig};
        let base_port = std::net::TcpListener::bind(("127.0.0.1", 0))
            .expect("reserve a free port")
            .local_addr()
            .expect("local addr")
            .port();
        let mut ws = WsServer::new(WsServerConfig {
            base_port,
            ..Default::default()
        });
        NetworkServer::start(&mut ws, base_port)
            .await
            .expect("bind obs server");
        *state.ws_server.lock().await = Some(ws.clone());
        ws
    }

    fn broadcasts(ws: &liveaudio_network::WsServer) -> u64 {
        ws.metrics()
            .broadcast_messages_total
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    #[tokio::test]
    async fn form_save_pushes_the_theme_it_adopts_from_disk_to_obs() {
        let home = TestHome::new("form-theme");
        let base = enabled();
        let disk_theme: std::collections::BTreeMap<String, String> =
            [("--sub-color".to_string(), "#ff0000".to_string())]
                .into_iter()
                .collect();
        // An MCP write put this theme on disk; memory still has the older (empty) theme.
        liveaudio_core::config::save_to_path(
            &LiveAudioConfig {
                subtitle_theme: disk_theme.clone(),
                ..base.clone()
            },
            &home.dir.join("config.json"),
        )
        .unwrap();
        let state = AppState::new(base.clone());
        let mut ws = running_obs(&state).await;
        let before = broadcasts(&ws);

        let form = LiveAudioConfig {
            ws_port: 9000,
            ..base.clone()
        };
        let saved = save_settings_form(&state, Some(base), form)
            .await
            .expect("form saved");

        assert_eq!(saved.subtitle_theme, disk_theme, "the disk theme is kept");
        assert_eq!(broadcasts(&ws), before + 1, "the adopted theme reaches OBS");
        liveaudio_network::NetworkServer::stop(&mut ws)
            .await
            .expect("stop obs server");
    }

    #[tokio::test]
    async fn saving_an_unchanged_theme_does_not_broadcast_again() {
        let home = TestHome::new("dedup");
        let theme: std::collections::BTreeMap<String, String> =
            [("--sub-color".to_string(), "#ff0000".to_string())]
                .into_iter()
                .collect();
        let stored = LiveAudioConfig {
            subtitle_theme: theme,
            ..enabled()
        };
        liveaudio_core::config::save_to_path(&stored, &home.dir.join("config.json")).unwrap();
        let state = AppState::new(stored.clone());
        let mut ws = running_obs(&state).await;
        let before = broadcasts(&ws);

        save_settings_form(
            &state,
            Some(stored.clone()),
            LiveAudioConfig {
                ws_port: 9000,
                ..stored.clone()
            },
        )
        .await
        .expect("form saved");

        assert_eq!(
            broadcasts(&ws),
            before,
            "an unchanged theme is not sent again"
        );
        liveaudio_network::NetworkServer::stop(&mut ws)
            .await
            .expect("stop obs server");
    }

    #[tokio::test]
    async fn reset_pushes_the_cleared_theme_to_obs_once() {
        let state = AppState::new(LiveAudioConfig {
            subtitle_theme: [("--sub-color".to_string(), "#ff0000".to_string())]
                .into_iter()
                .collect(),
            ..LiveAudioConfig::default()
        });
        let mut ws = running_obs(&state).await;
        let before = broadcasts(&ws);

        let defaults = crate::commands::reset_config_to_defaults(&state, |_| Ok(true))
            .await
            .expect("reset");
        assert!(defaults.subtitle_theme.is_empty());
        assert_eq!(
            broadcasts(&ws),
            before + 1,
            "the cleared theme is sent once"
        );

        crate::commands::reset_config_to_defaults(&state, |_| Ok(true))
            .await
            .expect("second reset");
        assert_eq!(
            broadcasts(&ws),
            before + 1,
            "an already empty theme is not sent again"
        );
        liveaudio_network::NetworkServer::stop(&mut ws)
            .await
            .expect("stop obs server");
    }

    #[tokio::test]
    async fn style_save_keeps_general_changes_made_on_disk() {
        let home = TestHome::new("style-merge");
        let base = enabled();
        let path = home.dir.join("config.json");
        // An MCP patch changed a general setting on disk; memory is still stale.
        liveaudio_core::config::save_to_path(
            &LiveAudioConfig {
                ws_port: 9100,
                ..base.clone()
            },
            &path,
        )
        .unwrap();
        let state = AppState::new(LiveAudioConfig {
            ws_port: 9000,
            ..base.clone()
        });
        let theme: std::collections::BTreeMap<String, String> =
            [("--sub-color".to_string(), "#00ff00".to_string())]
                .into_iter()
                .collect();

        let saved = save_style_fields(&state, theme.clone(), Vec::new(), None)
            .await
            .expect("style saved");

        assert_eq!(
            saved.ws_port, 9100,
            "the general change made on disk survives"
        );
        assert_eq!(saved.subtitle_theme, theme);
        assert_eq!(load_from_path(&path).unwrap().ws_port, 9100);
    }

    #[tokio::test]
    async fn style_save_writes_the_built_in_style_to_disk_and_memory() {
        let home = TestHome::new("style-built-in");
        let base = enabled();
        let path = home.dir.join("config.json");
        liveaudio_core::config::save_to_path(&base, &path).unwrap();
        let state = AppState::new(base);

        let saved = save_style_fields(
            &state,
            Default::default(),
            Vec::new(),
            Some("neon".to_string()),
        )
        .await
        .expect("style saved");

        assert_eq!(saved.subtitle_style, "neon");
        assert_eq!(load_from_path(&path).unwrap().subtitle_style, "neon");
        assert_eq!(state.config.read().await.subtitle_style, "neon");
    }

    #[tokio::test]
    async fn external_adoption_waits_for_the_write_lock() {
        let home = TestHome::new("watch-lock");
        let base = enabled();
        let path = home.dir.join("config.json");
        liveaudio_core::config::save_to_path(
            &LiveAudioConfig {
                ws_port: 9100,
                ..base.clone()
            },
            &path,
        )
        .unwrap();
        let state = std::sync::Arc::new(AppState::new(base));
        let guard = state.config_write_lock.lock().await;
        let adopt = {
            let state = std::sync::Arc::clone(&state);
            let path = path.clone();
            tokio::spawn(async move { apply_external_config(&state, &path).await })
        };
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(
            !adopt.is_finished(),
            "external adoption ran while a config save held the write lock"
        );
        drop(guard);
        let change = adopt.await.expect("adopt task").expect("external change");
        assert_eq!(state.config.read().await.ws_port, 9100);
        assert!(change.changed_keys.contains(&"ws_port".to_string()));
    }

    #[tokio::test]
    async fn external_theme_change_is_pushed_to_the_running_obs_server() {
        use liveaudio_network::NetworkServer;
        let home = TestHome::new("watch-theme");
        let base = enabled();
        let path = home.dir.join("config.json");
        liveaudio_core::config::save_to_path(
            &LiveAudioConfig {
                subtitle_theme: [("--sub-color".to_string(), "#ff0000".to_string())]
                    .into_iter()
                    .collect(),
                ..base.clone()
            },
            &path,
        )
        .unwrap();
        let state = AppState::new(base);
        let mut ws = running_obs(&state).await;
        let before = ws
            .metrics()
            .broadcast_messages_total
            .load(std::sync::atomic::Ordering::SeqCst);

        apply_external_config(&state, &path)
            .await
            .expect("external change");

        assert_eq!(
            ws.metrics()
                .broadcast_messages_total
                .load(std::sync::atomic::Ordering::SeqCst),
            before + 1,
            "a changed subtitle theme is sent to OBS"
        );
        NetworkServer::stop(&mut ws).await.expect("stop obs server");
    }

    #[tokio::test]
    async fn own_form_save_is_not_announced_back_as_an_external_change() {
        let _home = TestHome::new("echo");
        let base = enabled();
        let state = AppState::new(base.clone());
        let form = LiveAudioConfig {
            ws_port: 9002,
            ..base.clone()
        };
        let saved = save_settings_form(&state, Some(base), form)
            .await
            .expect("form saved");
        let disk = load_from_path(&config_file_path()).unwrap();
        let mut current = saved;
        assert!(adopt_disk_config(&mut current, disk).is_none());
    }

    #[tokio::test]
    async fn style_save_pushes_the_changed_style_to_obs() {
        let home = TestHome::new("style-broadcast");
        let base = enabled();
        let path = home.dir.join("config.json");
        liveaudio_core::config::save_to_path(&base, &path).unwrap();
        let state = AppState::new(base);
        let mut ws = running_obs(&state).await;
        let before = broadcasts(&ws);

        let saved = save_style_fields(
            &state,
            Default::default(),
            Vec::new(),
            Some("neon".to_string()),
        )
        .await
        .expect("style saved");

        assert_eq!(saved.subtitle_style, "neon");
        assert_eq!(
            broadcasts(&ws),
            before + 1,
            "a changed subtitle style is sent to OBS"
        );

        // Saving the same style again is deduped
        save_style_fields(
            &state,
            Default::default(),
            Vec::new(),
            Some("neon".to_string()),
        )
        .await
        .expect("style saved again");

        assert_eq!(
            broadcasts(&ws),
            before + 1,
            "an unchanged style is not sent again"
        );
        liveaudio_network::NetworkServer::stop(&mut ws)
            .await
            .expect("stop obs server");
    }

    #[tokio::test]
    async fn external_style_change_is_pushed_to_the_running_obs_server() {
        use liveaudio_network::NetworkServer;
        let home = TestHome::new("watch-style");
        let base = enabled();
        let path = home.dir.join("config.json");
        liveaudio_core::config::save_to_path(
            &LiveAudioConfig {
                subtitle_style: "neon".to_string(),
                ..base.clone()
            },
            &path,
        )
        .unwrap();
        let state = AppState::new(base);
        let mut ws = running_obs(&state).await;
        let before = ws
            .metrics()
            .broadcast_messages_total
            .load(std::sync::atomic::Ordering::SeqCst);

        apply_external_config(&state, &path)
            .await
            .expect("external change");

        assert_eq!(
            ws.metrics()
                .broadcast_messages_total
                .load(std::sync::atomic::Ordering::SeqCst),
            before + 1,
            "a changed subtitle style from disk is sent to OBS"
        );
        NetworkServer::stop(&mut ws).await.expect("stop obs server");
    }

    #[test]
    fn corrupt_config_is_moved_aside_not_overwritten() {
        let home = TestHome::new("corrupt");
        let path = home.dir.join("config.json");
        std::fs::write(&path, "{ not json").unwrap();

        let name = quarantine_corrupt_config().expect("corrupt file quarantined");

        assert!(name.starts_with("config.json.corrupt-"), "{name}");
        assert!(!path.exists());
        assert_eq!(std::fs::read_to_string(home.dir.join(&name)).unwrap(), "{ not json");
    }

    #[test]
    fn missing_or_valid_config_is_not_quarantined() {
        let home = TestHome::new("not-corrupt");
        assert!(quarantine_corrupt_config().is_none(), "first run has no file");
        let path = home.dir.join("config.json");
        liveaudio_core::config::save_to_path(&enabled(), &path).unwrap();
        assert!(quarantine_corrupt_config().is_none());
        assert!(path.exists());
    }

    #[tokio::test]
    async fn update_check_time_is_saved_without_reverting_external_edit() {
        let home = TestHome::new("update-check");
        liveaudio_core::config::save_to_path(
            &LiveAudioConfig {
                subtitle_style: "neon".to_string(),
                ..enabled()
            },
            &home.dir.join("config.json"),
        )
        .unwrap();
        let state = AppState::new(enabled());

        record_update_check(&state, 123).await;

        let disk = load_from_path(&home.dir.join("config.json")).unwrap();
        assert_eq!(disk.last_update_check, 123);
        assert_eq!(disk.subtitle_style, "neon", "external edit survives");
        assert_eq!(state.config.read().await.last_update_check, 123);
    }

    #[tokio::test]
    async fn update_check_time_never_overwrites_an_unreadable_config() {
        let home = TestHome::new("update-check-corrupt");
        let path = home.dir.join("config.json");
        std::fs::write(&path, "{ not json").unwrap();
        let state = AppState::new(enabled());

        record_update_check(&state, 123).await;

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
    }

    #[tokio::test]
    async fn test_save_allowed_ports_workflow() {
        let home = TestHome::new("allowed-ports");
        let base = enabled();
        let path = home.dir.join("config.json");
        liveaudio_core::config::save_to_path(&base, &path).unwrap();
        let state = AppState::new(base);

        assert_eq!(state.config.read().await.ws_allowed_loopback_ports, vec![1420]);

        // Record a rejected attempt
        state.rejected_origins.lock().unwrap().record("http://localhost:5173");
        assert_eq!(state.rejected_origins.lock().unwrap().len(), 1);

        // Allow 5173
        let mut ports = state.config.read().await.ws_allowed_loopback_ports.clone();
        ports.push(5173);
        let saved = crate::commands::save_allowed_ports(&state, ports).await.expect("save");
        assert_eq!(saved.ws_allowed_loopback_ports, vec![1420, 5173]);
        assert_eq!(state.config.read().await.ws_allowed_loopback_ports, vec![1420, 5173]);

        // Tracking removal on allow
        state.rejected_origins.lock().unwrap().remove_by_port(5173);
        assert_eq!(state.rejected_origins.lock().unwrap().len(), 0);

        // Revoke 5173
        let mut ports = state.config.read().await.ws_allowed_loopback_ports.clone();
        ports.retain(|&p| p != 5173);
        let saved = crate::commands::save_allowed_ports(&state, ports).await.expect("revoke");
        assert_eq!(saved.ws_allowed_loopback_ports, vec![1420]);
        assert_eq!(state.config.read().await.ws_allowed_loopback_ports, vec![1420]);
    }
}
