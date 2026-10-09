// SPDX-License-Identifier: MIT

//! LiveAudio Configuration loader, validator, normalizer, and atomic saver.
//!
//! Fully compatible with LiveAudio's `config.json` schema, locking mechanism,
//! and migration rules.

use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const LOCK_STALE_AFTER_SECONDS: u64 = 30;

pub static VALID_DEVICES: &[&str] = &["cpu", "cuda"];
pub static VALID_MODELS: &[&str] = &[
    "tiny (Más rápido, baja precisión)",
    "base (Rápido)",
    "small (Balance CPU)",
    "turbo (Máxima precisión GPU)",
];
pub static VALID_SUBTITLE_STYLES: &[&str] = &[
    "default",
    "karaoke",
    "neon",
    "minimal",
    "bold",
    "rgb",
    "typewriter",
];
pub static VALID_SUBTITLE_DISPLAY_MODES: &[&str] = &["single", "ribbon", "adaptive"];
pub static VALID_BACKLOG_POLICIES: &[&str] = &["auto", "live_only", "send_all"];
pub static VALID_DIAGNOSTICS_LEVELS: &[&str] = &["off", "minimal", "deep"];
pub static VALID_TRANSCRIPTION_PURPOSES: &[&str] = &["subtitles", "transcription", "combined"];

pub const DEFAULT_BLACKLIST: &str =
    "amara.org, subtítulos por, suscríbete, dale like, gracias por ver, memos, gracias, activar la campanita";

pub const AUDIO_QUEUE_MAXSIZE: usize = 100;
pub const AUDIO_QUEUE_BUDGET_SEC: f64 = 60.0;
pub const AUDIO_FRAME_DURATION_SEC: f64 = 512.0 / 16000.0;
pub const ASR_DECODE_TIMEOUT_MIN_SEC: u32 = 5;
pub const ASR_DECODE_TIMEOUT_MAX_SEC: u32 = 120;
pub const ASR_DECODE_TIMEOUT_DEFAULT_SEC: u32 = 15;

/// Resolution for the LiveAudio data home directory.
///
/// Resolution order:
/// 1. `LIVEAUDIO_HOME` environment variable (explicit override / tests).
/// 2. `%APPDATA%\LiveAudio` on Windows.
/// 3. `$XDG_CONFIG_HOME/liveaudio` or `~/.config/liveaudio` elsewhere.
pub fn get_data_home() -> PathBuf {
    if let Ok(home) = std::env::var("LIVEAUDIO_HOME") {
        if !home.trim().is_empty() {
            return PathBuf::from(home);
        }
    }

    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("LiveAudio");
        }
        if let Ok(userprofile) = std::env::var("USERPROFILE") {
            return PathBuf::from(userprofile).join(".config").join("LiveAudio");
        }
        PathBuf::from("LiveAudio")
    }

    #[cfg(not(windows))]
    {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            return PathBuf::from(xdg).join("liveaudio");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".config").join("liveaudio");
        }
        PathBuf::from("liveaudio")
    }
}

pub fn config_file_path() -> PathBuf {
    get_data_home().join("config.json")
}

pub fn lock_file_path() -> PathBuf {
    get_data_home().join("config.json.lock")
}

pub fn get_models_home() -> PathBuf {
    get_data_home().join("models")
}

pub fn get_hf_home() -> PathBuf {
    get_models_home().join("hf")
}

pub fn get_torch_home() -> PathBuf {
    get_models_home().join("torch")
}

fn default_cpu_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| (n.get() / 2).max(1))
        .unwrap_or(2)
}

fn max_cpu_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().max(1))
        .unwrap_or(4)
}

fn normalize_model_size(val: &str) -> String {
    let key = val.split_whitespace().next().unwrap_or("");
    for &m in VALID_MODELS {
        if m.starts_with(key) {
            return m.to_string();
        }
    }
    "small (Balance CPU)".to_string()
}

/// Comprehensive, strongly-typed representation of `config.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiveAudioConfig {
    #[serde(default = "default_output_dir")]
    pub output_dir: PathBuf,

    #[serde(default = "default_device")]
    pub device: String,

    #[serde(default = "default_cpu_threads")]
    pub cpu_threads: usize,

    #[serde(default = "default_model_size")]
    pub model_size: String,

    #[serde(default = "default_blacklist")]
    pub blacklist: String,

    #[serde(default = "default_true")]
    pub continuous_session: bool,

    #[serde(default = "default_subtitle_style")]
    pub subtitle_style: String,

    #[serde(default = "default_subtitle_display_mode")]
    pub subtitle_display_mode: String,

    #[serde(default = "default_subtitle_ribbon_max_lines")]
    pub subtitle_ribbon_max_lines: u32,

    #[serde(default = "default_subtitle_backlog_policy")]
    pub subtitle_backlog_policy: String,

    #[serde(default = "default_subtitle_max_live_delay_sec")]
    pub subtitle_max_live_delay_sec: f64,

    #[serde(default = "default_subtitle_catchup_interval_sec")]
    pub subtitle_catchup_interval_sec: f64,

    #[serde(default = "default_silence_timeout")]
    pub silence_timeout: f64,

    #[serde(default = "default_max_chunk_duration")]
    pub max_chunk_duration: f64,

    #[serde(default = "default_transcription_purpose")]
    pub transcription_purpose: String,

    #[serde(default = "default_asr_decode_timeout_sec")]
    pub asr_decode_timeout_sec: u32,

    #[serde(default)]
    pub audio_device: Option<serde_json::Value>,

    #[serde(default = "default_selected_profile_id")]
    pub selected_profile_id: String,

    #[serde(default = "default_profile_mode")]
    pub profile_mode: String,

    #[serde(default = "default_ws_port")]
    pub ws_port: u16,

    #[serde(default = "default_true")]
    pub obs_enabled: bool,

    #[serde(default = "default_true")]
    pub prewarm: bool,

    #[serde(default = "default_true")]
    pub save_transcript_enabled: bool,

    #[serde(default = "default_true")]
    pub save_vtt_enabled: bool,

    #[serde(default)]
    pub whisper_context_prompt_es: String,

    #[serde(default)]
    pub whisper_context_prompt_en: String,

    #[serde(default = "default_asr_language")]
    pub asr_language: String,

    #[serde(default = "default_settings_navigation_mode")]
    pub settings_navigation_mode: String,

    #[serde(default)]
    pub language: Option<String>,

    #[serde(default)]
    pub diagnostics_enabled: bool,

    #[serde(default = "default_diagnostics_level")]
    pub diagnostics_level: String,

    #[serde(default)]
    pub diagnostics_export_dir: Option<PathBuf>,

    #[serde(default)]
    pub last_update_check: i64,

    #[serde(default = "default_vad_speech_pad_ms")]
    pub vad_speech_pad_ms: u32,

    #[serde(default = "default_vad_threshold")]
    pub vad_threshold: f32,

    /// Extra user/plugin fields preserved without loss.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn default_output_dir() -> PathBuf {
    get_data_home().join("sessions")
}
fn default_device() -> String {
    "cuda".to_string()
}
fn default_model_size() -> String {
    "small (Balance CPU)".to_string()
}
fn default_blacklist() -> String {
    DEFAULT_BLACKLIST.to_string()
}
fn default_true() -> bool {
    true
}
fn default_subtitle_style() -> String {
    "default".to_string()
}
fn default_subtitle_display_mode() -> String {
    "adaptive".to_string()
}
fn default_subtitle_ribbon_max_lines() -> u32 {
    3
}
fn default_subtitle_backlog_policy() -> String {
    "auto".to_string()
}
fn default_subtitle_max_live_delay_sec() -> f64 {
    10.0
}
fn default_subtitle_catchup_interval_sec() -> f64 {
    1.5
}
fn default_silence_timeout() -> f64 {
    0.8
}
fn default_max_chunk_duration() -> f64 {
    5.0
}
fn default_transcription_purpose() -> String {
    "subtitles".to_string()
}
fn default_asr_decode_timeout_sec() -> u32 {
    ASR_DECODE_TIMEOUT_DEFAULT_SEC
}
fn default_selected_profile_id() -> String {
    "balanced".to_string()
}
fn default_profile_mode() -> String {
    "preset".to_string()
}
fn default_ws_port() -> u16 {
    8765
}
fn default_asr_language() -> String {
    "es".to_string()
}
fn default_settings_navigation_mode() -> String {
    "tabs".to_string()
}
fn default_diagnostics_level() -> String {
    "minimal".to_string()
}
fn default_vad_speech_pad_ms() -> u32 {
    200
}
fn default_vad_threshold() -> f32 {
    0.5
}

impl Default for LiveAudioConfig {
    fn default() -> Self {
        Self {
            output_dir: default_output_dir(),
            device: default_device(),
            cpu_threads: default_cpu_threads(),
            model_size: default_model_size(),
            blacklist: default_blacklist(),
            continuous_session: true,
            subtitle_style: default_subtitle_style(),
            subtitle_display_mode: default_subtitle_display_mode(),
            subtitle_ribbon_max_lines: default_subtitle_ribbon_max_lines(),
            subtitle_backlog_policy: default_subtitle_backlog_policy(),
            subtitle_max_live_delay_sec: default_subtitle_max_live_delay_sec(),
            subtitle_catchup_interval_sec: default_subtitle_catchup_interval_sec(),
            silence_timeout: default_silence_timeout(),
            max_chunk_duration: default_max_chunk_duration(),
            transcription_purpose: default_transcription_purpose(),
            asr_decode_timeout_sec: default_asr_decode_timeout_sec(),
            audio_device: None,
            selected_profile_id: default_selected_profile_id(),
            profile_mode: default_profile_mode(),
            ws_port: default_ws_port(),
            obs_enabled: true,
            prewarm: true,
            save_transcript_enabled: true,
            save_vtt_enabled: true,
            whisper_context_prompt_es: String::new(),
            whisper_context_prompt_en: String::new(),
            asr_language: default_asr_language(),
            settings_navigation_mode: default_settings_navigation_mode(),
            language: None,
            diagnostics_enabled: false,
            diagnostics_level: default_diagnostics_level(),
            diagnostics_export_dir: None,
            last_update_check: 0,
            vad_speech_pad_ms: default_vad_speech_pad_ms(),
            vad_threshold: default_vad_threshold(),
            extra: serde_json::Map::new(),
        }
    }
}

impl LiveAudioConfig {
    /// Validates types, bounds, and normalizes values in-place.
    /// Returns `true` if any field was updated or clamped.
    pub fn normalize(&mut self) -> bool {
        let mut updated = false;

        // Legacy prompt migration: whisper_context_prompt -> whisper_context_prompt_es
        if let Some(old_prompt) = self.extra.remove("whisper_context_prompt") {
            updated = true;
            if let Some(s) = old_prompt.as_str() {
                if !s.trim().is_empty() && self.whisper_context_prompt_es.trim().is_empty() {
                    self.whisper_context_prompt_es = s.trim().to_string();
                }
            }
        }

        // output_dir
        if self.output_dir.as_os_str().is_empty() {
            self.output_dir = default_output_dir();
            updated = true;
        } else if !self.output_dir.is_absolute() {
            if let Ok(abs) = std::fs::canonicalize(&self.output_dir) {
                self.output_dir = abs;
                updated = true;
            } else if let Ok(cwd) = std::env::current_dir() {
                self.output_dir = cwd.join(&self.output_dir);
                updated = true;
            }
        }

        // device
        if !VALID_DEVICES.contains(&self.device.as_str()) {
            self.device = default_device();
            updated = true;
        }

        // model_size
        let normalized_model = normalize_model_size(&self.model_size);
        if self.model_size != normalized_model {
            self.model_size = normalized_model;
            updated = true;
        }

        // cpu_threads
        let max_threads = max_cpu_threads();
        let clamped_threads = self.cpu_threads.clamp(1, max_threads);
        if self.cpu_threads != clamped_threads {
            self.cpu_threads = clamped_threads;
            updated = true;
        }

        // silence_timeout (0.3 to 2.0, round 1 decimal)
        let clamped_silence = (self.silence_timeout.clamp(0.3, 2.0) * 10.0).round() / 10.0;
        if (self.silence_timeout - clamped_silence).abs() > f64::EPSILON {
            self.silence_timeout = clamped_silence;
            updated = true;
        }

        // transcription_purpose
        if !VALID_TRANSCRIPTION_PURPOSES.contains(&self.transcription_purpose.as_str()) {
            self.transcription_purpose = default_transcription_purpose();
            updated = true;
        }

        // max_chunk_duration
        if self.max_chunk_duration > 15.0
            && self.max_chunk_duration <= 60.0
            && self.transcription_purpose == "subtitles"
        {
            self.transcription_purpose = "transcription".to_string();
            updated = true;
        }
        let max_allowed = if self.transcription_purpose == "transcription"
            || self.transcription_purpose == "combined"
        {
            60.0
        } else {
            15.0
        };
        let clamped_duration =
            (self.max_chunk_duration.clamp(1.0, max_allowed) * 10.0).round() / 10.0;
        if (self.max_chunk_duration - clamped_duration).abs() > f64::EPSILON {
            self.max_chunk_duration = clamped_duration;
            updated = true;
        }

        // asr_decode_timeout_sec
        let clamped_timeout = self
            .asr_decode_timeout_sec
            .clamp(ASR_DECODE_TIMEOUT_MIN_SEC, ASR_DECODE_TIMEOUT_MAX_SEC);
        if self.asr_decode_timeout_sec != clamped_timeout {
            self.asr_decode_timeout_sec = clamped_timeout;
            updated = true;
        }

        // vad_speech_pad_ms (0 to 500)
        let clamped_pad = self.vad_speech_pad_ms.clamp(0, 500);
        if self.vad_speech_pad_ms != clamped_pad {
            self.vad_speech_pad_ms = clamped_pad;
            updated = true;
        }

        // vad_threshold (0.1 to 0.9, round 2 decimals)
        let clamped_thresh = ((self.vad_threshold.clamp(0.1, 0.9) * 100.0).round()) / 100.0;
        if (self.vad_threshold - clamped_thresh).abs() > f32::EPSILON {
            self.vad_threshold = clamped_thresh;
            updated = true;
        }

        // blacklist
        if self.blacklist.trim().is_empty() {
            self.blacklist = default_blacklist();
            updated = true;
        }

        // asr_language
        if self.asr_language != "es" && self.asr_language != "en" {
            self.asr_language = default_asr_language();
            updated = true;
        }

        // subtitle_style
        if !VALID_SUBTITLE_STYLES.contains(&self.subtitle_style.as_str()) {
            self.subtitle_style = default_subtitle_style();
            updated = true;
        }

        // subtitle_display_mode
        if !VALID_SUBTITLE_DISPLAY_MODES.contains(&self.subtitle_display_mode.as_str()) {
            self.subtitle_display_mode = default_subtitle_display_mode();
            updated = true;
        }

        // subtitle_ribbon_max_lines (1 to 8)
        let clamped_lines = self.subtitle_ribbon_max_lines.clamp(1, 8);
        if self.subtitle_ribbon_max_lines != clamped_lines {
            self.subtitle_ribbon_max_lines = clamped_lines;
            updated = true;
        }

        // subtitle_backlog_policy
        if !VALID_BACKLOG_POLICIES.contains(&self.subtitle_backlog_policy.as_str()) {
            self.subtitle_backlog_policy = default_subtitle_backlog_policy();
            updated = true;
        }

        // subtitle_max_live_delay_sec (1.0 to 120.0)
        let clamped_delay =
            (self.subtitle_max_live_delay_sec.clamp(1.0, 120.0) * 10.0).round() / 10.0;
        if (self.subtitle_max_live_delay_sec - clamped_delay).abs() > f64::EPSILON {
            self.subtitle_max_live_delay_sec = clamped_delay;
            updated = true;
        }

        // subtitle_catchup_interval_sec (0.0 to 10.0)
        let clamped_interval =
            (self.subtitle_catchup_interval_sec.clamp(0.0, 10.0) * 10.0).round() / 10.0;
        if (self.subtitle_catchup_interval_sec - clamped_interval).abs() > f64::EPSILON {
            self.subtitle_catchup_interval_sec = clamped_interval;
            updated = true;
        }

        // audio_device (must be Object or None)
        if let Some(ref val) = self.audio_device {
            if !val.is_object() {
                self.audio_device = None;
                updated = true;
            }
        }

        // profile_mode
        if self.profile_mode != "preset" && self.profile_mode != "custom" {
            self.profile_mode = default_profile_mode();
            updated = true;
        }

        // ws_port (1 to 65535)
        if self.ws_port == 0 {
            self.ws_port = default_ws_port();
            updated = true;
        }

        // settings_navigation_mode
        if self.settings_navigation_mode != "tabs" && self.settings_navigation_mode != "dropdown" {
            self.settings_navigation_mode = default_settings_navigation_mode();
            updated = true;
        }

        // language
        if let Some(ref lang) = self.language {
            if lang != "es" && lang != "en" {
                self.language = None;
                updated = true;
            }
        }

        // diagnostics_level
        if !VALID_DIAGNOSTICS_LEVELS.contains(&self.diagnostics_level.as_str()) {
            self.diagnostics_level = default_diagnostics_level();
            updated = true;
        }

        // diagnostics_export_dir
        if let Some(ref dir) = self.diagnostics_export_dir {
            if dir.as_os_str().is_empty() {
                self.diagnostics_export_dir = None;
                updated = true;
            } else if !dir.is_absolute() {
                if let Ok(abs) = std::fs::canonicalize(dir) {
                    self.diagnostics_export_dir = Some(abs);
                    updated = true;
                } else if let Ok(cwd) = std::env::current_dir() {
                    self.diagnostics_export_dir = Some(cwd.join(dir));
                    updated = true;
                }
            }
        }

        updated
    }
}

/// Calculate nominal audio queue capacity matching Python formula.
pub fn audio_queue_capacity(config: &LiveAudioConfig) -> usize {
    let phrase_budget = config.max_chunk_duration
        + config.silence_timeout
        + (config.vad_speech_pad_ms as f64 / 1000.0)
        + AUDIO_FRAME_DURATION_SEC;
    if phrase_budget <= 0.0 {
        return AUDIO_QUEUE_MAXSIZE;
    }
    let capacity = (AUDIO_QUEUE_BUDGET_SEC / phrase_budget) as usize;
    capacity.clamp(1, AUDIO_QUEUE_MAXSIZE)
}

/// Acquires the lock file with stale detection and retry.
pub fn acquire_lock(timeout: Duration) -> io::Result<bool> {
    let data_home = get_data_home();
    let _ = std::fs::create_dir_all(&data_home);
    let lock_path = lock_file_path();
    let start = Instant::now();

    loop {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(_) => return Ok(true),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                if is_lock_stale(&lock_path) {
                    let _ = std::fs::remove_file(&lock_path);
                    continue;
                }
                if start.elapsed() >= timeout {
                    return Ok(false);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e),
        }
    }
}

pub fn release_lock() {
    let _ = std::fs::remove_file(lock_file_path());
}

fn is_lock_stale(lock_path: &Path) -> bool {
    if let Ok(metadata) = std::fs::metadata(lock_path) {
        if let Ok(modified) = metadata.modified() {
            if let Ok(elapsed) = modified.elapsed() {
                return elapsed.as_secs() > LOCK_STALE_AFTER_SECONDS;
            }
        }
    }
    false
}

/// Information returned by `load_config_readonly`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigReadonlyInfo {
    pub source: &'static str,
    pub error_code: Option<&'static str>,
}

/// Loads config in read-only mode, never creates or alters files on disk.
pub fn load_config_readonly() -> (LiveAudioConfig, ConfigReadonlyInfo) {
    let path = config_file_path();
    if !path.exists() {
        let mut cfg = LiveAudioConfig::default();
        cfg.normalize();
        return (
            cfg,
            ConfigReadonlyInfo {
                source: "defaults-missing-file",
                error_code: Some("config-missing-defaults"),
            },
        );
    }

    match load_from_path(&path) {
        Ok(mut cfg) => {
            cfg.normalize();
            (
                cfg,
                ConfigReadonlyInfo {
                    source: "file",
                    error_code: None,
                },
            )
        }
        Err(_) => {
            let mut cfg = LiveAudioConfig::default();
            cfg.normalize();
            (
                cfg,
                ConfigReadonlyInfo {
                    source: "defaults-corrupt-file",
                    error_code: Some("config-corrupt-defaults"),
                },
            )
        }
    }
}

/// Standard config loader with automatic normalization, migration, and persistence.
pub fn load_config() -> io::Result<LiveAudioConfig> {
    let locked = acquire_lock(Duration::from_secs(2))?;
    let path = config_file_path();

    let result = (|| -> io::Result<LiveAudioConfig> {
        if !path.exists() {
            let mut default_cfg = LiveAudioConfig::default();
            default_cfg.normalize();
            if locked {
                let _ = save_to_path(&default_cfg, &path);
            }
            return Ok(default_cfg);
        }

        let mut cfg = load_from_path(&path)?;
        let updated = cfg.normalize();
        if updated && locked {
            let _ = save_to_path(&cfg, &path);
        }
        Ok(cfg)
    })();

    if locked {
        release_lock();
    }
    result
}

/// Saves the given config to the active data home `config.json` atomically.
pub fn save_config(config: &LiveAudioConfig) -> io::Result<bool> {
    let locked = acquire_lock(Duration::from_secs(2))?;
    if !locked {
        tracing::warn!("Failed to acquire config lock; save aborted");
        return Ok(false);
    }

    let path = config_file_path();
    let res = save_to_path(config, &path);
    release_lock();

    res.map(|_| true)
}

/// Loads config from an arbitrary file path.
pub fn load_from_path(path: &Path) -> io::Result<LiveAudioConfig> {
    let mut file = OpenOptions::new().read(true).open(path)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    serde_json::from_str(&content)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
}

/// Atomically saves config to an arbitrary path using a sibling temporary file.
pub fn save_to_path(config: &LiveAudioConfig, path: &Path) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_path = parent.join(format!(".config-{}-{}.tmp", std::process::id(), nanos));

    let res = (|| -> io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;

        let bytes = serde_json::to_vec_pretty(config)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        file.write_all(&bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);

        std::fs::rename(&tmp_path, path)?;
        Ok(())
    })();

    if res.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_fields() {
        let cfg = LiveAudioConfig::default();
        assert_eq!(cfg.device, "cuda");
        assert_eq!(cfg.ws_port, 8765);
        assert_eq!(cfg.subtitle_style, "default");
        assert_eq!(cfg.subtitle_display_mode, "adaptive");
        assert_eq!(cfg.subtitle_ribbon_max_lines, 3);
        assert_eq!(cfg.vad_speech_pad_ms, 200);
        assert_eq!(cfg.vad_threshold, 0.5);
    }

    #[test]
    fn test_normalization_and_clamping() {
        let mut cfg = LiveAudioConfig::default();
        cfg.device = "invalid_device".to_string();
        cfg.ws_port = 0;
        cfg.subtitle_ribbon_max_lines = 99;
        cfg.silence_timeout = 10.0;
        cfg.vad_speech_pad_ms = 9999;
        cfg.vad_threshold = 1.5;
        cfg.asr_decode_timeout_sec = 500;

        assert!(cfg.normalize());

        assert_eq!(cfg.device, "cuda");
        assert_eq!(cfg.ws_port, 8765);
        assert_eq!(cfg.subtitle_ribbon_max_lines, 8);
        assert_eq!(cfg.silence_timeout, 2.0);
        assert_eq!(cfg.vad_speech_pad_ms, 500);
        assert_eq!(cfg.vad_threshold, 0.9);
        assert_eq!(cfg.asr_decode_timeout_sec, 120);
    }

    #[test]
    fn test_legacy_whisper_prompt_migration() {
        let mut cfg = LiveAudioConfig::default();
        cfg.extra.insert(
            "whisper_context_prompt".to_string(),
            serde_json::Value::String("Legacy Vocabulary".to_string()),
        );

        assert!(cfg.normalize());
        assert_eq!(cfg.whisper_context_prompt_es, "Legacy Vocabulary");
        assert!(!cfg.extra.contains_key("whisper_context_prompt"));
    }

    #[test]
    fn test_audio_queue_capacity_sizing() {
        let mut cfg = LiveAudioConfig::default();
        cfg.max_chunk_duration = 5.0;
        cfg.silence_timeout = 0.8;
        cfg.vad_speech_pad_ms = 200;
        assert_eq!(audio_queue_capacity(&cfg), 9);

        cfg.max_chunk_duration = 60.0;
        cfg.silence_timeout = 2.0;
        cfg.vad_speech_pad_ms = 500;
        assert_eq!(audio_queue_capacity(&cfg), 1);
    }

    #[test]
    fn test_atomic_file_saving_and_reading() {
        let tmp_dir =
            std::env::temp_dir().join(format!("liveaudio-cfg-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp_dir);
        let file_path = tmp_dir.join("config.json");

        let mut original = LiveAudioConfig::default();
        original.ws_port = 9876;
        original
            .extra
            .insert("custom_key".to_string(), serde_json::Value::Bool(true));

        save_to_path(&original, &file_path).expect("save should succeed");
        let loaded = load_from_path(&file_path).expect("load should succeed");

        assert_eq!(loaded.ws_port, 9876);
        assert_eq!(
            loaded.extra.get("custom_key"),
            Some(&serde_json::Value::Bool(true))
        );

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_config_v1_2_7_comprehensive_migration() {
        let v1_2_7_json = r#"{
            "output_dir": "C:\\Users\\Usuario de Prueba\\LiveAudio\\sessions",
            "device": "cuda",
            "cpu_threads": 999,
            "model_size": "small (Balance CPU)",
            "blacklist": "amara.org, subtítulos por",
            "continuous_session": true,
            "subtitle_style": "default",
            "subtitle_backlog_policy": "auto",
            "subtitle_max_live_delay_sec": 10.0,
            "subtitle_catchup_interval_sec": 1.5,
            "silence_timeout": 0.05,
            "max_chunk_duration": 99.0,
            "audio_device": null,
            "selected_profile_id": "balanced",
            "profile_mode": "preset",
            "ws_port": 8765,
            "obs_enabled": true,
            "whisper_context_prompt": "Vocabulario especializado de medicina y derecho",
            "asr_language": "es",
            "settings_navigation_mode": "tabs",
            "language": null,
            "diagnostics_enabled": false,
            "diagnostics_level": "minimal",
            "diagnostics_export_dir": null,
            "v1_2_7_custom_addon": {"enabled": true, "score": 42}
        }"#;

        let mut config: LiveAudioConfig =
            serde_json::from_str(v1_2_7_json).expect("JSON deserialization");
        let updated = config.normalize();
        assert!(updated, "Config normalization should report updates");

        // Migration of legacy context prompt
        assert_eq!(
            config.whisper_context_prompt_es,
            "Vocabulario especializado de medicina y derecho"
        );
        assert!(!config.extra.contains_key("whisper_context_prompt"));

        // Clamping of out-of-range values
        assert_eq!(
            config.silence_timeout, 0.3,
            "silence_timeout should be clamped to min 0.3"
        );
        assert_eq!(
            config.max_chunk_duration, 15.0,
            "max_chunk_duration should be clamped to max 15.0 for default subtitles purpose"
        );
        assert!(
            config.cpu_threads <= 16,
            "cpu_threads should be clamped to max parallelism"
        );

        // Defaults populated
        assert_eq!(config.vad_speech_pad_ms, 200);
        assert!((config.vad_threshold - 0.5).abs() < 0.01);

        // Extra legacy fields preserved
        assert!(config.extra.contains_key("v1_2_7_custom_addon"));
        let addon = config.extra.get("v1_2_7_custom_addon").unwrap();
        assert_eq!(addon.get("score"), Some(&serde_json::json!(42)));
    }

    #[test]
    fn test_unicode_and_spaces_path_handling() {
        let base_tmp = std::env::temp_dir();
        let unicode_dir = base_tmp.join("LiveAudio Test José Ñandú 🎙️ carpeta con espacios");
        let _ = std::fs::create_dir_all(&unicode_dir);
        let config_file = unicode_dir.join("config.json");

        let mut config = LiveAudioConfig::default();
        config.output_dir = unicode_dir.join("grabaciones subfolder");
        config.whisper_context_prompt_es =
            "Transcripción en español con acentos: á, é, í, ó, ú, ñ".to_string();

        let save_res = save_to_path(&config, &config_file);
        assert!(
            save_res.is_ok(),
            "Saving config in Unicode & spaces path should succeed: {:?}",
            save_res.err()
        );

        let load_res = load_from_path(&config_file);
        assert!(
            load_res.is_ok(),
            "Loading config from Unicode & spaces path should succeed: {:?}",
            load_res.err()
        );

        let loaded = load_res.unwrap();
        assert_eq!(
            loaded.whisper_context_prompt_es,
            config.whisper_context_prompt_es
        );
        assert_eq!(loaded.output_dir, config.output_dir);

        let _ = std::fs::remove_dir_all(&unicode_dir);
    }
}
