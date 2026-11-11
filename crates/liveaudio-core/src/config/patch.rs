// SPDX-License-Identifier: MIT

//! Allowlisted, validated partial updates of `config.json` for external agents (MCP).
//!
//! Trust boundary: only keys in [`MCP_PATCHABLE_KEYS`] can change, values are type-checked by
//! serde, enum-checked against the `VALID_*` tables, and normalized before the atomic save.
//! The `mcp_style_enabled` and `mcp_system_enabled` opt-in flags are deliberately not patchable:
//! only the desktop UI can flip them.

use super::{
    acquire_lock, config_file_path, load_from_path, release_lock, save_to_path, subtitle_theme,
    LiveAudioConfig, VALID_ASR_BACKENDS, VALID_BACKLOG_POLICIES, VALID_DEVICES, VALID_DIAGNOSTICS_LEVELS, VALID_MODELS,
    VALID_SUBTITLE_DISPLAY_MODES, VALID_SUBTITLE_STYLES, VALID_TRANSCRIPTION_PURPOSES,
};
use serde_json::{Map, Value};
use std::io;
use std::time::Duration;

/// Keys an external agent may read and change through the patch API.
pub const MCP_PATCHABLE_KEYS: &[&str] = &[
    "device",
    "asr_backend",
    "cpu_threads",
    "model_size",
    "blacklist",
    "continuous_session",
    "subtitle_style",
    "subtitle_display_mode",
    "subtitle_ribbon_max_lines",
    "subtitle_backlog_policy",
    "subtitle_max_live_delay_sec",
    "subtitle_catchup_interval_sec",
    "silence_timeout",
    "max_chunk_duration",
    "transcription_purpose",
    "asr_decode_timeout_sec",
    "ws_port",
    "obs_enabled",
    "rnnoise_enabled",
    "acoustic_events_enabled",
    "live_partial_subtitles_enabled",
    "prewarm",
    "save_transcript_enabled",
    "save_vtt_enabled",
    "whisper_context_prompt_es",
    "whisper_context_prompt_en",
    "asr_language",
    "settings_navigation_mode",
    "language",
    "diagnostics_enabled",
    "diagnostics_level",
    "vad_speech_pad_ms",
    "vad_threshold",
    "subtitle_theme",
    "subtitle_presets",
];

/// Style keys an external agent may read and change when `mcp_style_enabled` is active (Group A, low risk).
pub const MCP_STYLE_KEYS: &[&str] = &[
    "subtitle_theme",
    "subtitle_presets",
    "subtitle_style",
    "subtitle_display_mode",
];

/// Returns `true` if the key belongs to the style group (Group A).
pub fn is_style_key(key: &str) -> bool {
    MCP_STYLE_KEYS.contains(&key)
}

/// Returns `true` if the key is authorized by the current active MCP switches.
pub fn is_key_enabled(key: &str, style_enabled: bool, system_enabled: bool) -> bool {
    if is_style_key(key) {
        style_enabled
    } else {
        system_enabled
    }
}

/// Patchable keys that only take effect after the ASR service restarts (mirrors the desktop
/// `RUNTIME_SETTINGS` list). Changing any of them reports `restart_required`.
pub const RESTART_REQUIRED_KEYS: &[&str] = &[
    "device",
    "asr_backend",
    "model_size",
    "ws_port",
    "asr_language",
    "language",
    "whisper_context_prompt_es",
    "whisper_context_prompt_en",
    "silence_timeout",
    "max_chunk_duration",
    "vad_speech_pad_ms",
    "vad_threshold",
    "subtitle_backlog_policy",
    "subtitle_max_live_delay_sec",
    "subtitle_catchup_interval_sec",
    "continuous_session",
    "save_transcript_enabled",
    "save_vtt_enabled",
    "rnnoise_enabled",
    "acoustic_events_enabled",
    "live_partial_subtitles_enabled",
];

/// Longest text value accepted from a patch (characters). Generous for blacklists and prompts.
pub const MAX_PATCH_TEXT_CHARS: usize = 4096;

/// Lock wait for a patch write, same budget as the desktop `save_config` path.
const PATCH_LOCK_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub enum PatchError {
    /// MCP integration is off.
    Disabled,
    /// Access to a specific setting group (style or system) is disabled.
    GroupDisabled(String),
    /// Key does not exist in the config schema (includes `extra` keys).
    UnknownKey(String),
    /// Key exists in the config but is outside the allowlist (paths, opt-in flag, UI state).
    NotAllowed(String),
    /// Value has the wrong JSON type or is outside the enum set.
    InvalidValue { key: String, reason: String },
    /// Another process holds the config lock.
    LockUnavailable,
    /// Existing config could not be read; it is left untouched.
    Io(io::Error),
}

impl std::fmt::Display for PatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => write!(
                f,
                "MCP access is disabled; enable it in LiveAudio settings (Advanced)"
            ),
            Self::GroupDisabled(group) => write!(
                f,
                "MCP access to {group} settings is disabled; enable it in LiveAudio settings (Advanced)"
            ),
            Self::UnknownKey(key) => write!(f, "unknown config key '{key}'"),
            Self::NotAllowed(key) => write!(f, "config key '{key}' cannot be changed via MCP"),
            Self::InvalidValue { key, reason } => write!(f, "invalid value for '{key}': {reason}"),
            Self::LockUnavailable => write!(f, "config is locked by another process; retry"),
            Self::Io(e) => write!(f, "config could not be read: {e}"),
        }
    }
}

impl std::error::Error for PatchError {}

#[derive(Debug, Clone, PartialEq)]
pub struct PatchOutcome {
    /// Normalized values now stored for every patched key.
    pub applied: Map<String, Value>,
    pub restart_required: bool,
}

/// Returns the allowlisted subset of `config` as JSON, for read-only agent access.
/// Only keys authorized by the active switches (`mcp_style_enabled`, `mcp_system_enabled`) are included.
pub fn patchable_values(config: &LiveAudioConfig) -> Map<String, Value> {
    let full = serde_json::to_value(config).unwrap_or(Value::Null);
    MCP_PATCHABLE_KEYS
        .iter()
        .filter(|key| is_key_enabled(key, config.mcp_style_enabled, config.mcp_system_enabled))
        .filter_map(|key| full.get(*key).map(|v| ((*key).to_string(), v.clone())))
        .collect()
}

/// Applies a partial update to the on-disk config under the config lock.
pub fn apply_patch(patch: &Map<String, Value>) -> Result<PatchOutcome, PatchError> {
    let defaults = serde_json::to_value(LiveAudioConfig::default()).unwrap_or(Value::Null);
    for (key, value) in patch {
        if !MCP_PATCHABLE_KEYS.contains(&key.as_str()) {
            return Err(if defaults.get(key).is_some() {
                PatchError::NotAllowed(key.clone())
            } else {
                PatchError::UnknownKey(key.clone())
            });
        }
        check_enum(key, value)?;
        if value
            .as_str()
            .is_some_and(|text| text.chars().count() > MAX_PATCH_TEXT_CHARS)
        {
            return Err(PatchError::InvalidValue {
                key: key.clone(),
                reason: format!("text longer than {MAX_PATCH_TEXT_CHARS} characters"),
            });
        }
    }

    if !acquire_lock(PATCH_LOCK_TIMEOUT).map_err(PatchError::Io)? {
        return Err(PatchError::LockUnavailable);
    }
    let outcome = write_patch(patch);
    release_lock();
    outcome
}

/// Closed value sets for enum-typed keys (`None` for free-form or numeric keys).
fn enum_values(key: &str) -> Option<&'static [&'static str]> {
    Some(match key {
        "device" => VALID_DEVICES,
        "asr_backend" => VALID_ASR_BACKENDS,
        "model_size" => VALID_MODELS,
        "subtitle_style" => VALID_SUBTITLE_STYLES,
        "subtitle_display_mode" => VALID_SUBTITLE_DISPLAY_MODES,
        "subtitle_backlog_policy" => VALID_BACKLOG_POLICIES,
        "transcription_purpose" => VALID_TRANSCRIPTION_PURPOSES,
        "diagnostics_level" => VALID_DIAGNOSTICS_LEVELS,
        "asr_language" | "language" => &["es", "en"],
        "settings_navigation_mode" => &["tabs", "dropdown"],
        _ => return None,
    })
}

/// Enum-typed keys accept only the values the UI offers; `language` also accepts `null`.
fn check_enum(key: &str, value: &Value) -> Result<(), PatchError> {
    let Some(allowed) = enum_values(key) else {
        return Ok(());
    };
    if key == "language" && value.is_null() {
        return Ok(());
    }
    match value.as_str() {
        Some(s) if allowed.contains(&s) => Ok(()),
        _ => Err(PatchError::InvalidValue {
            key: key.to_string(),
            reason: format!("expected one of {allowed:?}"),
        }),
    }
}

/// Documented numeric bounds; `normalize()` is the enforcement (values are clamped on save).
fn numeric_bounds(key: &str) -> Option<(f64, f64)> {
    Some(match key {
        "cpu_threads" => (
            1.0,
            std::thread::available_parallelism().map_or(4, |n| n.get()) as f64,
        ),
        "subtitle_ribbon_max_lines" => (1.0, 8.0),
        "subtitle_max_live_delay_sec" => (1.0, 120.0),
        "subtitle_catchup_interval_sec" => (0.0, 10.0),
        "silence_timeout" => (0.3, 2.0),
        "max_chunk_duration" => (1.0, 30.0),
        "asr_decode_timeout_sec" => (5.0, 120.0),
        "ws_port" => (1.0, 65535.0),
        "vad_speech_pad_ms" => (0.0, 500.0),
        "vad_threshold" => (0.1, 0.9),
        _ => return None,
    })
}

/// JSON Schema-like description of patchable keys for active switches, for `get_config_schema`.
pub fn config_schema_for(style_enabled: bool, system_enabled: bool) -> Value {
    let defaults = serde_json::to_value(LiveAudioConfig::default()).unwrap_or(Value::Null);
    let fields = MCP_PATCHABLE_KEYS
        .iter()
        .filter(|key| is_key_enabled(key, style_enabled, system_enabled))
        .map(|key| {
            let mut entry =
                subtitle_style_schema(key).unwrap_or_else(|| scalar_schema(key, &defaults));
            entry.insert(
                "restart_required".into(),
                Value::from(RESTART_REQUIRED_KEYS.contains(key)),
            );
            ((*key).to_string(), Value::Object(entry))
        })
        .collect();
    Value::Object(fields)
}

/// JSON Schema-like description of all patchable keys, for `get_config_schema`.
pub fn config_schema() -> Value {
    config_schema_for(true, true)
}

/// Schema for the subtitle style keys, which are objects and arrays rather than scalars.
fn subtitle_style_schema(key: &str) -> Option<Map<String, Value>> {
    let tokens = subtitle_theme::token_names();
    let schema = match key {
        "subtitle_theme" => serde_json::json!({
            "type": "object",
            "propertyNames": { "enum": tokens },
            "additionalProperties": { "type": "string", "maxLength": 200 },
            "description": "CSS token values for the OBS subtitles. Invalid values are dropped; sizes are clamped."
        }),
        "subtitle_presets" => serde_json::json!({
            "type": "array",
            "maxItems": subtitle_theme::MAX_SUBTITLE_PRESETS,
            "items": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "maxLength": subtitle_theme::MAX_PRESET_NAME_CHARS },
                    "style": { "type": "string", "description": "Built-in base style the preset was saved with." },
                    "tokens": {
                        "type": "object",
                        "propertyNames": { "enum": tokens },
                        "additionalProperties": { "type": "string", "maxLength": 200 }
                    }
                }
            },
            "description": "Named style presets. Duplicate, empty, and over-long names are dropped."
        }),
        _ => return None,
    };
    schema.as_object().cloned()
}

/// Schema for a scalar setting, derived from its default value and the documented enum and bounds.
fn scalar_schema(key: &str, defaults: &Value) -> Map<String, Value> {
    let mut entry = Map::new();
    let kind = match defaults.get(key) {
        Some(Value::Bool(_)) => "boolean",
        Some(Value::Number(n)) if n.is_f64() => "number",
        Some(Value::Number(_)) => "integer",
        _ => "string",
    };
    if let Some(values) = enum_values(key) {
        entry.insert("enum".into(), serde_json::json!(values));
    }
    if key == "language" {
        // The patch API accepts null here (no language override), so the schema says so.
        entry.insert("type".into(), serde_json::json!(["string", "null"]));
        if let Some(Value::Array(values)) = entry.get_mut("enum") {
            values.push(Value::Null);
        }
    } else {
        entry.insert("type".into(), Value::from(kind));
    }
    if let Some((min, max)) = numeric_bounds(key) {
        entry.insert("minimum".into(), serde_json::json!(min));
        entry.insert("maximum".into(), serde_json::json!(max));
    }
    entry
}

fn write_patch(patch: &Map<String, Value>) -> Result<PatchOutcome, PatchError> {
    let path = config_file_path();
    let mut cfg = if path.exists() {
        load_from_path(&path).map_err(PatchError::Io)?
    } else {
        LiveAudioConfig::default()
    };
    for (key, _) in patch {
        if is_style_key(key) {
            if !cfg.mcp_style_enabled {
                return Err(PatchError::GroupDisabled("style".into()));
            }
        } else if !cfg.mcp_system_enabled {
            return Err(PatchError::GroupDisabled("system".into()));
        }
    }
    if !cfg.mcp_style_enabled && !cfg.mcp_system_enabled {
        return Err(PatchError::Disabled);
    }

    let mut merged = serde_json::to_value(&cfg)
        .map_err(|e| PatchError::Io(io::Error::new(io::ErrorKind::InvalidData, e)))?;
    let fields = merged.as_object_mut().ok_or_else(|| {
        PatchError::Io(io::Error::new(
            io::ErrorKind::InvalidData,
            "config is not an object",
        ))
    })?;
    for (key, value) in patch {
        // Type-check each key on its own so the error names the offending key.
        let mut single = fields.clone();
        single.insert(key.clone(), value.clone());
        serde_json::from_value::<LiveAudioConfig>(Value::Object(single)).map_err(|e| {
            PatchError::InvalidValue {
                key: key.clone(),
                reason: e.to_string(),
            }
        })?;
        fields.insert(key.clone(), value.clone());
    }
    cfg = serde_json::from_value(merged)
        .map_err(|e| PatchError::Io(io::Error::new(io::ErrorKind::InvalidData, e)))?;
    cfg.normalize();
    save_to_path(&cfg, &path).map_err(PatchError::Io)?;

    let stored = serde_json::to_value(&cfg).unwrap_or(Value::Null);
    let applied = patch
        .keys()
        .map(|key| (key.clone(), stored.get(key).cloned().unwrap_or(Value::Null)))
        .collect();
    let restart_required = patch
        .keys()
        .any(|key| RESTART_REQUIRED_KEYS.contains(&key.as_str()));
    Ok(PatchOutcome {
        applied,
        restart_required,
    })
}

/// Saves a settings form: fields the form changed (`next` differs from `base`) take the form
/// value; every other field keeps what is on disk, so stale form state never reverts external
/// edits. Returns the saved config, or `None` when the config lock is busy.
pub fn save_ui_changes(
    base: &LiveAudioConfig,
    next: &LiveAudioConfig,
) -> io::Result<Option<LiveAudioConfig>> {
    if !acquire_lock(PATCH_LOCK_TIMEOUT)? {
        return Ok(None);
    }
    let outcome = write_ui_changes(base, next);
    release_lock();
    outcome.map(Some)
}

fn write_ui_changes(base: &LiveAudioConfig, next: &LiveAudioConfig) -> io::Result<LiveAudioConfig> {
    let path = config_file_path();
    let disk = if path.exists() {
        load_from_path(&path)?
    } else {
        next.clone()
    };
    let invalid = |e: serde_json::Error| io::Error::new(io::ErrorKind::InvalidData, e);
    let mut merged = serde_json::to_value(&disk).map_err(invalid)?;
    let base_values = serde_json::to_value(base).map_err(invalid)?;
    let next_values = serde_json::to_value(next).map_err(invalid)?;
    if let (Some(fields), Some(next_fields)) = (merged.as_object_mut(), next_values.as_object()) {
        for (key, value) in next_fields {
            if base_values.get(key) != Some(value) {
                fields.insert(key.clone(), value.clone());
            }
        }
    }
    let mut cfg: LiveAudioConfig = serde_json::from_value(merged).map_err(invalid)?;
    cfg.normalize();
    save_to_path(&cfg, &path)?;
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_home<T>(name: &str, f: impl FnOnce(&std::path::Path) -> T) -> T {
        // `LIVEAUDIO_HOME` is process-wide: hold the crate-wide env lock for the whole override.
        let _env = crate::test_env::lock();
        let dir =
            std::env::temp_dir().join(format!("liveaudio-patch-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("LIVEAUDIO_HOME", &dir);
        let out = f(&dir);
        std::env::remove_var("LIVEAUDIO_HOME");
        let _ = std::fs::remove_dir_all(&dir);
        out
    }

    fn write_config(dir: &std::path::Path, cfg: &LiveAudioConfig) {
        save_to_path(cfg, &dir.join("config.json")).unwrap();
    }

    fn enabled_config() -> LiveAudioConfig {
        LiveAudioConfig {
            mcp_style_enabled: true,
            mcp_system_enabled: true,
            ..LiveAudioConfig::default()
        }
    }

    fn patch(pairs: &[(&str, Value)]) -> Map<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn mcp_flag_defaults_off_and_is_not_patchable() {
        assert!(!LiveAudioConfig::default().mcp_style_enabled);
        assert!(!LiveAudioConfig::default().mcp_system_enabled);
        assert!(!MCP_PATCHABLE_KEYS.contains(&"mcp_enabled"));
        assert!(!MCP_PATCHABLE_KEYS.contains(&"mcp_style_enabled"));
        assert!(!MCP_PATCHABLE_KEYS.contains(&"mcp_system_enabled"));
        assert!(!MCP_PATCHABLE_KEYS.contains(&"output_dir"));
        assert!(!MCP_PATCHABLE_KEYS.contains(&"diagnostics_export_dir"));
        assert!(!MCP_PATCHABLE_KEYS.contains(&"last_update_check"));
        assert!(!MCP_PATCHABLE_KEYS.contains(&"ws_allowed_loopback_ports"));
    }

    #[test]
    fn asr_backend_is_patchable_enum_and_restart_required() {
        assert!(MCP_PATCHABLE_KEYS.contains(&"asr_backend"));
        assert!(RESTART_REQUIRED_KEYS.contains(&"asr_backend"));
        assert_eq!(enum_values("asr_backend"), Some(VALID_ASR_BACKENDS));
    }

    #[test]
    fn live_partial_subtitles_flag_is_patchable_and_restart_required() {
        assert!(MCP_PATCHABLE_KEYS.contains(&"live_partial_subtitles_enabled"));
        assert!(RESTART_REQUIRED_KEYS.contains(&"live_partial_subtitles_enabled"));
    }

    #[test]
    fn patchable_values_expose_only_allowlisted_keys() {
        let values = patchable_values(&enabled_config());
        assert!(values.contains_key("subtitle_style"));
        assert!(!values.contains_key("output_dir"));
        assert!(!values.contains_key("mcp_enabled"));
        assert!(!values.contains_key("mcp_style_enabled"));
        assert!(!values.contains_key("mcp_system_enabled"));
        assert!(!values.contains_key("ws_allowed_loopback_ports"));
        assert_eq!(values.len(), MCP_PATCHABLE_KEYS.len());

        let default_values = patchable_values(&LiveAudioConfig::default());
        assert!(default_values.is_empty(), "when both switches are off, no values are patchable");

        let style_only = LiveAudioConfig {
            mcp_style_enabled: true,
            ..LiveAudioConfig::default()
        };
        let style_values = patchable_values(&style_only);
        assert_eq!(style_values.len(), MCP_STYLE_KEYS.len());
        for k in MCP_STYLE_KEYS {
            assert!(style_values.contains_key(*k));
        }

        let system_only = LiveAudioConfig {
            mcp_system_enabled: true,
            ..LiveAudioConfig::default()
        };
        let sys_values = patchable_values(&system_only);
        assert_eq!(sys_values.len(), MCP_PATCHABLE_KEYS.len() - MCP_STYLE_KEYS.len());
        for k in MCP_STYLE_KEYS {
            assert!(!sys_values.contains_key(*k));
        }
    }

    #[test]
    fn refuses_every_patch_while_mcp_is_disabled_and_leaves_file_unchanged() {
        with_home("disabled", |dir| {
            write_config(dir, &LiveAudioConfig::default());
            let before = std::fs::read(dir.join("config.json")).unwrap();

            let err = apply_patch(&patch(&[("subtitle_style", Value::from("neon"))])).unwrap_err();

            assert!(
                matches!(err, PatchError::GroupDisabled(ref g) if g == "style"),
                "got {err:?}"
            );
            assert_eq!(
                err.to_string(),
                "MCP access to style settings is disabled; enable it in LiveAudio settings (Advanced)"
            );
            assert_eq!(std::fs::read(dir.join("config.json")).unwrap(), before);
        });
    }

    #[test]
    fn applies_allowed_key_preserves_other_fields_and_reports_live_change() {
        with_home("apply", |dir| {
            let mut cfg = enabled_config();
            cfg.extra.insert("user_note".into(), Value::from("keep me"));
            write_config(dir, &cfg);

            let outcome = apply_patch(&patch(&[("subtitle_style", Value::from("neon"))])).unwrap();

            assert_eq!(
                outcome.applied.get("subtitle_style"),
                Some(&Value::from("neon"))
            );
            assert!(!outcome.restart_required);
            let saved = load_from_path(&dir.join("config.json")).unwrap();
            assert_eq!(saved.subtitle_style, "neon");
            assert!(saved.mcp_style_enabled && saved.mcp_system_enabled, "patch must not flip the opt-in flags");
            assert_eq!(saved.extra.get("user_note"), Some(&Value::from("keep me")));
        });
    }

    #[test]
    fn reports_restart_required_for_runtime_keys() {
        with_home("restart", |dir| {
            write_config(dir, &enabled_config());
            let outcome = apply_patch(&patch(&[("ws_port", Value::from(9123))])).unwrap();
            assert!(outcome.restart_required);
            assert_eq!(
                load_from_path(&dir.join("config.json")).unwrap().ws_port,
                9123
            );
        });
    }

    #[test]
    fn rejects_unknown_extra_and_disallowed_keys_without_writing() {
        with_home("keys", |dir| {
            write_config(dir, &enabled_config());
            let before = std::fs::read(dir.join("config.json")).unwrap();

            for key in [
                "not_a_setting",
                "output_dir",
                "diagnostics_export_dir",
                "mcp_enabled",
                "mcp_style_enabled",
                "mcp_system_enabled",
                "last_update_check",
                "audio_device",
                "ws_allowed_loopback_ports",
            ] {
                let err = apply_patch(&patch(&[(key, Value::Null)])).unwrap_err();
                assert!(
                    matches!(err, PatchError::UnknownKey(_) | PatchError::NotAllowed(_)),
                    "{key}: got {err:?}"
                );
            }
            assert_eq!(std::fs::read(dir.join("config.json")).unwrap(), before);
        });
    }

    #[test]
    fn rejects_ws_allowed_loopback_ports_with_not_allowed() {
        with_home("loopback-ports", |dir| {
            write_config(dir, &enabled_config());
            let err = apply_patch(&patch(&[("ws_allowed_loopback_ports", serde_json::json!([5173]))])).unwrap_err();
            assert!(
                matches!(err, PatchError::NotAllowed(ref k) if k == "ws_allowed_loopback_ports"),
                "expected NotAllowed for ws_allowed_loopback_ports, got {err:?}"
            );
        });
    }

    #[test]
    fn rejects_mixed_patch_atomically_if_one_group_is_disabled() {
        with_home("mixed", |dir| {
            let mut cfg = LiveAudioConfig::default();
            cfg.mcp_style_enabled = true;
            cfg.mcp_system_enabled = false;
            cfg.subtitle_style = "default".to_string();
            cfg.ws_port = 8765;
            write_config(dir, &cfg);
            let before = std::fs::read(dir.join("config.json")).unwrap();

            let err = apply_patch(&patch(&[
                ("subtitle_style", Value::from("neon")),
                ("ws_port", Value::from(9000)),
            ]))
            .unwrap_err();

            assert!(
                matches!(err, PatchError::GroupDisabled(ref g) if g == "system"),
                "got {err:?}"
            );
            assert_eq!(
                err.to_string(),
                "MCP access to system settings is disabled; enable it in LiveAudio settings (Advanced)"
            );
            assert_eq!(std::fs::read(dir.join("config.json")).unwrap(), before);
            let reloaded = load_from_path(&dir.join("config.json")).unwrap();
            assert_eq!(reloaded.subtitle_style, "default");
            assert_eq!(reloaded.ws_port, 8765);
        });
    }

    #[test]
    fn rejects_style_key_when_only_system_enabled() {
        with_home("style-disabled", |dir| {
            let mut cfg = LiveAudioConfig::default();
            cfg.mcp_style_enabled = false;
            cfg.mcp_system_enabled = true;
            write_config(dir, &cfg);

            let err = apply_patch(&patch(&[("subtitle_style", Value::from("neon"))])).unwrap_err();
            assert!(
                matches!(err, PatchError::GroupDisabled(ref g) if g == "style"),
                "got {err:?}"
            );
            assert_eq!(
                err.to_string(),
                "MCP access to style settings is disabled; enable it in LiveAudio settings (Advanced)"
            );
        });
    }

    #[test]
    fn rejects_wrong_type_and_unknown_enum_values() {
        with_home("types", |dir| {
            write_config(dir, &enabled_config());
            let before = std::fs::read(dir.join("config.json")).unwrap();

            let wrong_type = apply_patch(&patch(&[("silence_timeout", Value::from("fast"))]));
            let bad_enum = apply_patch(&patch(&[("subtitle_style", Value::from("rainbow"))]));

            assert!(matches!(wrong_type, Err(PatchError::InvalidValue { .. })));
            assert!(matches!(bad_enum, Err(PatchError::InvalidValue { .. })));
            assert_eq!(std::fs::read(dir.join("config.json")).unwrap(), before);
        });
    }

    #[test]
    fn out_of_range_numbers_are_clamped_by_normalize_and_reported() {
        with_home("clamp", |dir| {
            write_config(dir, &enabled_config());
            let outcome = apply_patch(&patch(&[("silence_timeout", Value::from(9.0))])).unwrap();
            assert_eq!(
                outcome.applied.get("silence_timeout"),
                Some(&Value::from(2.0))
            );
            assert_eq!(
                load_from_path(&dir.join("config.json"))
                    .unwrap()
                    .silence_timeout,
                2.0
            );
        });
    }

    #[test]
    fn refuses_to_overwrite_an_unreadable_config() {
        with_home("corrupt", |dir| {
            std::fs::write(dir.join("config.json"), "{ not json").unwrap();
            let err = apply_patch(&patch(&[("subtitle_style", Value::from("neon"))])).unwrap_err();
            assert!(matches!(err, PatchError::Io(_)), "got {err:?}");
            assert_eq!(
                std::fs::read_to_string(dir.join("config.json")).unwrap(),
                "{ not json"
            );
        });
    }

    #[test]
    fn subtitle_style_is_patchable_without_restart_and_described_in_the_schema() {
        assert!(MCP_PATCHABLE_KEYS.contains(&"subtitle_theme"));
        assert!(MCP_PATCHABLE_KEYS.contains(&"subtitle_presets"));
        let schema = config_schema();
        assert_eq!(schema["subtitle_theme"]["restart_required"], false);
        assert_eq!(schema["subtitle_presets"]["restart_required"], false);
        assert_eq!(schema["subtitle_theme"]["type"], "object");
        assert_eq!(
            schema["subtitle_theme"]["propertyNames"]["enum"]
                .as_array()
                .map(Vec::len),
            Some(15),
            "the schema names the 15 accepted tokens"
        );
        assert_eq!(schema["subtitle_presets"]["type"], "array");
        assert_eq!(schema["subtitle_presets"]["maxItems"], 20);
        assert_eq!(
            schema["subtitle_presets"]["items"]["properties"]["name"]["maxLength"],
            40
        );
        assert_eq!(
            schema["subtitle_presets"]["items"]["properties"]["style"]["type"],
            "string"
        );
    }

    #[test]
    fn patch_stores_the_subtitle_style_after_validation() {
        with_home("style", |dir| {
            write_config(dir, &enabled_config());
            let outcome = apply_patch(&patch(&[
                (
                    "subtitle_theme",
                    serde_json::json!({"--sub-color": "#ff0000", "--sub-bg": "url(http://evil.test/x)"}),
                ),
                (
                    "subtitle_presets",
                    serde_json::json!([{"name": "  Night  ", "tokens": {"--sub-color": "#00ff00"}}]),
                ),
            ]))
            .expect("style patch applied");

            let saved = load_from_path(&dir.join("config.json")).unwrap();
            assert_eq!(
                saved.subtitle_theme,
                [("--sub-color".to_string(), "#ff0000".to_string())]
                    .into_iter()
                    .collect()
            );
            assert_eq!(saved.subtitle_presets.len(), 1);
            assert_eq!(saved.subtitle_presets[0].name, "Night");
            assert!(!outcome.restart_required);
            assert_eq!(
                outcome.applied["subtitle_theme"],
                serde_json::json!({"--sub-color": "#ff0000"})
            );
        });
    }

    #[test]
    fn patch_rejects_a_subtitle_theme_of_the_wrong_json_type_without_writing() {
        with_home("style-type", |dir| {
            write_config(dir, &enabled_config());
            let before = std::fs::read(dir.join("config.json")).unwrap();
            let err =
                apply_patch(&patch(&[("subtitle_theme", serde_json::json!("red"))])).unwrap_err();
            assert!(
                matches!(err, PatchError::InvalidValue { ref key, .. } if key == "subtitle_theme"),
                "got {err:?}"
            );
            assert_eq!(std::fs::read(dir.join("config.json")).unwrap(), before);
        });
    }

    #[test]
    fn schema_describes_every_patchable_key_and_no_restricted_one() {
        let schema = config_schema();
        let fields = schema.as_object().expect("schema is an object");
        assert_eq!(fields.len(), MCP_PATCHABLE_KEYS.len());
        for key in MCP_PATCHABLE_KEYS {
            assert!(fields.contains_key(*key), "missing schema entry for {key}");
        }
        assert!(!fields.contains_key("output_dir"));
        assert!(!fields.contains_key("mcp_enabled"));
        assert!(!fields.contains_key("mcp_style_enabled"));
        assert!(!fields.contains_key("mcp_system_enabled"));
        assert!(!fields.contains_key("ws_allowed_loopback_ports"));
        assert_eq!(
            fields["subtitle_style"]["enum"],
            serde_json::json!(VALID_SUBTITLE_STYLES)
        );
        assert_eq!(fields["silence_timeout"]["type"], "number");
        assert_eq!(fields["silence_timeout"]["minimum"], 0.3);
        assert_eq!(fields["silence_timeout"]["maximum"], 2.0);
        assert_eq!(fields["obs_enabled"]["type"], "boolean");
        assert_eq!(fields["ws_port"]["restart_required"], true);
        assert_eq!(fields["subtitle_style"]["restart_required"], false);

        let style_schema = config_schema_for(true, false);
        assert_eq!(style_schema.as_object().unwrap().len(), MCP_STYLE_KEYS.len());
        for key in MCP_STYLE_KEYS {
            assert!(style_schema.get(*key).is_some());
        }

        let sys_schema = config_schema_for(false, true);
        assert_eq!(sys_schema.as_object().unwrap().len(), MCP_PATCHABLE_KEYS.len() - MCP_STYLE_KEYS.len());
        for key in MCP_STYLE_KEYS {
            assert!(sys_schema.get(*key).is_none());
        }
    }

    #[test]
    fn form_save_keeps_external_edits_on_fields_the_form_did_not_change() {
        with_home("merge", |dir| {
            let base = enabled_config();
            let mut external = base.clone();
            external.subtitle_style = "neon".to_string();
            external.blacklist = "external blacklist".to_string();
            write_config(dir, &external);

            let mut form = base.clone();
            form.ws_port = 9000;
            form.blacklist = base.blacklist.clone();

            let saved = save_ui_changes(&base, &form).unwrap().expect("lock free");

            assert_eq!(saved.ws_port, 9000, "field changed by the form is saved");
            assert_eq!(
                saved.subtitle_style, "neon",
                "external edit survives the form save"
            );
            assert_eq!(saved.blacklist, "external blacklist");
            assert_eq!(load_from_path(&dir.join("config.json")).unwrap(), saved);
        });
    }

    #[test]
    fn form_save_reports_busy_lock_without_writing() {
        with_home("merge-locked", |dir| {
            write_config(dir, &enabled_config());
            std::fs::write(dir.join("config.json.lock"), "held").unwrap();
            let base = enabled_config();
            let mut form = base.clone();
            form.ws_port = 9001;
            assert!(save_ui_changes(&base, &form).unwrap().is_none());
            assert_eq!(
                load_from_path(&dir.join("config.json")).unwrap().ws_port,
                8765
            );
        });
    }

    #[test]
    fn rejects_oversized_text_values_without_writing() {
        with_home("text-cap", |dir| {
            write_config(dir, &enabled_config());
            let before = std::fs::read(dir.join("config.json")).unwrap();
            let huge = "x".repeat(MAX_PATCH_TEXT_CHARS + 1);
            let err = apply_patch(&patch(&[("blacklist", Value::from(huge))])).unwrap_err();
            assert!(
                matches!(err, PatchError::InvalidValue { .. }),
                "got {err:?}"
            );
            assert_eq!(std::fs::read(dir.join("config.json")).unwrap(), before);

            let at_cap = "x".repeat(MAX_PATCH_TEXT_CHARS);
            apply_patch(&patch(&[("blacklist", Value::from(at_cap))])).expect("cap is inclusive");
        });
    }

    #[test]
    fn schema_language_admits_null_like_the_patch_api() {
        let schema = config_schema();
        let values = schema["language"]["enum"].as_array().unwrap();
        assert!(
            values.contains(&Value::Null),
            "language enum must include null"
        );
        assert_eq!(
            schema["language"]["type"],
            serde_json::json!(["string", "null"])
        );
    }

    #[test]
    fn reports_lock_unavailable_when_another_writer_holds_the_lock() {
        with_home("locked", |dir| {
            write_config(dir, &enabled_config());
            std::fs::write(dir.join("config.json.lock"), "held").unwrap();
            let err = apply_patch(&patch(&[("subtitle_style", Value::from("neon"))])).unwrap_err();
            assert!(matches!(err, PatchError::LockUnavailable), "got {err:?}");
        });
    }
}
