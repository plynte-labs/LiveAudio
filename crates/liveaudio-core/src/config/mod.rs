// SPDX-License-Identifier: MIT

//! LiveAudio Configuration loader, validator, normalizer, and atomic saver.
//!
//! Fully compatible with LiveAudio's `config.json` schema, locking mechanism,
//! and migration rules.

pub mod subtitle_theme;
pub use subtitle_theme::SubtitlePreset;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub mod patch;

pub const LOCK_STALE_AFTER_SECONDS: u64 = 30;

pub static VALID_DEVICES: &[&str] = &["cpu", "cuda"];
pub static VALID_ASR_BACKENDS: &[&str] = &["auto", "ort", "ct2_cuda"];
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

pub const MAX_ALLOWED_LOOPBACK_PORTS: usize = 32;

pub const AUDIO_QUEUE_MAXSIZE: usize = 100;
pub const AUDIO_QUEUE_BUDGET_SEC: f64 = 60.0;
pub const AUDIO_FRAME_DURATION_SEC: f64 = 512.0 / 16000.0;
pub const ASR_DECODE_TIMEOUT_MIN_SEC: u32 = 5;
pub const ASR_DECODE_TIMEOUT_MAX_SEC: u32 = 120;
pub const ASR_DECODE_TIMEOUT_DEFAULT_SEC: u32 = 15;

pub const UPDATE_CHECK_INTERVAL_SEC: i64 = 86_400;
pub const UPDATE_RETRY_INTERVAL_SEC: i64 = 3_600;
pub const UPDATE_FAILURE_BACKOFF_OFFSET_SEC: i64 =
    UPDATE_CHECK_INTERVAL_SEC - UPDATE_RETRY_INTERVAL_SEC;

/// Determines if an update check is due based on 24-hour throttling and clock skew resilience.
pub fn should_check_for_updates(last_check: i64, now: i64) -> bool {
    last_check <= 0 || now < last_check || now.saturating_sub(last_check) >= UPDATE_CHECK_INTERVAL_SEC
}

/// Computes the timestamp to record on update check failure so retry is permitted after 1 hour.
pub fn update_check_failure_timestamp(now: i64) -> i64 {
    now.saturating_sub(UPDATE_FAILURE_BACKOFF_OFFSET_SEC).max(0)
}

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

    /// Worker backend: `auto` (default), `ort` or `ct2_cuda` (NVIDIA only; restart required).
    #[serde(default = "default_asr_backend")]
    pub asr_backend: String,

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

    /// Custom CSS tokens layered over the built-in subtitle style.
    #[serde(default)]
    pub subtitle_theme: BTreeMap<String, String>,

    /// Named subtitle style presets saved by the user.
    #[serde(default)]
    pub subtitle_presets: Vec<SubtitlePreset>,

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

    #[serde(default = "default_ws_allowed_loopback_ports")]
    pub ws_allowed_loopback_ports: Vec<u16>,

    #[serde(default = "default_true")]
    pub obs_enabled: bool,

    #[serde(default)]
    pub rnnoise_enabled: bool,

    #[serde(default)]
    pub acoustic_events_enabled: bool,

    /// Show provisional subtitles while the speaker is still talking (desktop app only; restart required).
    #[serde(default)]
    pub live_partial_subtitles_enabled: bool,

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

    #[serde(default = "default_true", alias = "update_check_enabled")]
    pub check_updates_enabled: bool,

    #[serde(default)]
    pub last_update_check: i64,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_update_installer_path: Option<PathBuf>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_update_version: Option<String>,

    /// Low-risk opt-in for external agents (MCP): style keys only.
    /// Only the desktop UI may change it; never patchable via MCP.
    #[serde(default)]
    pub mcp_style_enabled: bool,

    /// Medium-risk opt-in for external agents (MCP): system keys.
    /// Only the desktop UI may change it; never patchable via MCP.
    #[serde(default)]
    pub mcp_system_enabled: bool,

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
    "cpu".to_string()
}
fn default_asr_backend() -> String {
    "auto".to_string()
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
    0.5
}
fn default_max_chunk_duration() -> f64 {
    3.0
}
fn default_transcription_purpose() -> String {
    "subtitles".to_string()
}

/// The purpose sets the `max_chunk_duration` ceiling and its
/// default. Returns `(ceiling, default)`. Unknown purposes behave as subtitles.
/// The ceiling is the ORT worker's 30 s mel window: longer chunks would be truncated.
fn max_chunk_bounds(purpose: &str) -> (f64, f64) {
    match purpose {
        "transcription" | "combined" => (30.0, 30.0),
        _ => (15.0, default_max_chunk_duration()),
    }
}

/// Rounds to one decimal with half-to-even ties on the exact binary value(round-half-even).
fn round_to_tenth(value: f64) -> f64 {
    format!("{value:.1}").parse().unwrap_or(value)
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
fn default_ws_allowed_loopback_ports() -> Vec<u16> {
    vec![1420]
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
            asr_backend: default_asr_backend(),
            cpu_threads: default_cpu_threads(),
            model_size: default_model_size(),
            blacklist: default_blacklist(),
            continuous_session: true,
            subtitle_style: default_subtitle_style(),
            subtitle_display_mode: default_subtitle_display_mode(),
            subtitle_theme: BTreeMap::new(),
            subtitle_presets: Vec::new(),
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
            ws_allowed_loopback_ports: default_ws_allowed_loopback_ports(),
            obs_enabled: true,
            rnnoise_enabled: false,
            acoustic_events_enabled: false,
            live_partial_subtitles_enabled: false,
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
            check_updates_enabled: true,
            last_update_check: 0,
            pending_update_installer_path: None,
            pending_update_version: None,
            mcp_style_enabled: false,
            mcp_system_enabled: false,
            vad_speech_pad_ms: default_vad_speech_pad_ms(),
            vad_threshold: default_vad_threshold(),
            extra: serde_json::Map::new(),
        }
    }
}

impl LiveAudioConfig {
    /// Capture device id for the persisted `audio_device` selection.
    ///
    /// `None` means the OS default microphone; the legacy `wasapi_loopback` id
    /// maps to `default_loopback`.
    pub fn capture_device_id(&self) -> Option<String> {
        let id = self.audio_device.as_ref()?.get("id")?.as_str()?;
        match id {
            "default_microphone" => None,
            "wasapi_loopback" => Some("default_loopback".to_string()),
            other => Some(other.to_string()),
        }
    }

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

        // Legacy MCP switch migration: mcp_enabled -> mcp_style_enabled (mcp_system_enabled stays false)
        if let Some(old_mcp) = self.extra.remove("mcp_enabled") {
            updated = true;
            if old_mcp.as_bool() == Some(true) {
                self.mcp_style_enabled = true;
                self.mcp_system_enabled = false;
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

        if !VALID_ASR_BACKENDS.contains(&self.asr_backend.as_str()) {
            self.asr_backend = default_asr_backend();
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

        // max_chunk_duration: the purpose sets the ceiling; the purpose itself is never changed here.
        let (max_allowed, purpose_default) = max_chunk_bounds(&self.transcription_purpose);
        let duration = if self.max_chunk_duration.is_finite() {
            self.max_chunk_duration.clamp(1.0, max_allowed)
        } else {
            purpose_default
        };
        let rounded_duration = round_to_tenth(duration);
        if self.max_chunk_duration.is_nan()
            || (self.max_chunk_duration - rounded_duration).abs() > f64::EPSILON
        {
            self.max_chunk_duration = rounded_duration;
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

        // subtitle_theme and subtitle_presets
        updated |= subtitle_theme::sanitize_tokens(&mut self.subtitle_theme);
        updated |= subtitle_theme::sanitize_presets(&mut self.subtitle_presets);

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

        // ws_allowed_loopback_ports (1 to 65535, deduped, max 32)
        let mut cleaned_ports = Vec::new();
        for &port in &self.ws_allowed_loopback_ports {
            if port > 0 && !cleaned_ports.contains(&port) {
                cleaned_ports.push(port);
            }
        }
        if cleaned_ports.len() > MAX_ALLOWED_LOOPBACK_PORTS {
            cleaned_ports.truncate(MAX_ALLOWED_LOOPBACK_PORTS);
        }
        if cleaned_ports != self.ws_allowed_loopback_ports {
            self.ws_allowed_loopback_ports = cleaned_ports;
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

        // last_update_check (must be non-negative)
        if self.last_update_check < 0 {
            self.last_update_check = 0;
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

/// Calculate nominal audio queue capacity from the chunk/queue settings.
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

    let invalid = |e: serde_json::Error| io::Error::new(io::ErrorKind::InvalidData, e.to_string());
    let serde_json::Value::Object(mut fields) = serde_json::from_str(&content).map_err(invalid)?
    else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "config root is not a JSON object",
        ));
    };
    // Per-key tolerance (legacy-config tolerance): a value that fails to deserialize on its
    // own falls back to that key's serde default; unknown keys land in `extra`.
    fields.retain(|key, value| {
        let single = serde_json::Map::from_iter([(key.clone(), value.clone())]);
        let ok = serde_json::from_value::<LiveAudioConfig>(single.into()).is_ok();
        if !ok {
            tracing::warn!(key = %key, "Invalid config value; using default for this key");
        }
        ok
    });
    // A missing or invalid max_chunk_duration takes the default of the purpose.
    // Invalid keys were just removed above, so absence here means absent or invalid.
    if !fields.contains_key("max_chunk_duration") {
        let purpose = fields
            .get("transcription_purpose")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        fields.insert(
            "max_chunk_duration".to_string(),
            serde_json::json!(max_chunk_bounds(purpose).1),
        );
    }
    serde_json::from_value(fields.into()).map_err(invalid)
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
    fn capture_device_id_maps_persisted_audio_device_to_capture_id() {
        let with_id = |id: &str| LiveAudioConfig {
            audio_device: Some(serde_json::json!({ "id": id, "name": "ignored" })),
            ..LiveAudioConfig::default()
        };
        assert_eq!(LiveAudioConfig::default().capture_device_id(), None);
        assert_eq!(with_id("default_microphone").capture_device_id(), None);
        assert_eq!(
            with_id("wasapi_loopback").capture_device_id().as_deref(),
            Some("default_loopback")
        );
        assert_eq!(
            with_id("loopback:Speakers").capture_device_id().as_deref(),
            Some("loopback:Speakers")
        );
        assert_eq!(
            with_id("Microphone (USB)").capture_device_id().as_deref(),
            Some("Microphone (USB)")
        );
        let non_string = LiveAudioConfig {
            audio_device: Some(serde_json::json!({ "id": 7 })),
            ..LiveAudioConfig::default()
        };
        assert_eq!(non_string.capture_device_id(), None);
    }

    #[test]
    fn test_default_config_fields() {
        let cfg = LiveAudioConfig::default();
        assert_eq!(cfg.device, "cpu");
        assert_eq!(cfg.ws_port, 8765);
        assert_eq!(cfg.ws_allowed_loopback_ports, vec![1420]);
        assert_eq!(cfg.subtitle_style, "default");
        assert_eq!(cfg.subtitle_display_mode, "adaptive");
        assert_eq!(cfg.subtitle_ribbon_max_lines, 3);
        assert_eq!(cfg.vad_speech_pad_ms, 200);
        assert_eq!(cfg.vad_threshold, 0.5);
        assert_eq!(cfg.silence_timeout, 0.5);
        assert_eq!(cfg.max_chunk_duration, 3.0);
        assert!(!cfg.rnnoise_enabled);
        assert!(!cfg.acoustic_events_enabled);
        assert!(!cfg.live_partial_subtitles_enabled);
        assert!(cfg.check_updates_enabled);
        assert_eq!(cfg.last_update_check, 0);
        assert!(!cfg.mcp_style_enabled);
        assert!(!cfg.mcp_system_enabled);
        assert!(cfg.subtitle_theme.is_empty());
        assert!(cfg.subtitle_presets.is_empty());
    }

    #[test]
    fn test_legacy_mcp_enabled_migration() {
        let json_enabled = r#"{"mcp_enabled": true}"#;
        let mut cfg_enabled: LiveAudioConfig = serde_json::from_str(json_enabled).unwrap();
        assert!(cfg_enabled.extra.contains_key("mcp_enabled"));
        assert!(!cfg_enabled.mcp_style_enabled);
        assert!(!cfg_enabled.mcp_system_enabled);
        assert!(cfg_enabled.normalize());
        assert!(cfg_enabled.mcp_style_enabled);
        assert!(!cfg_enabled.mcp_system_enabled);
        assert!(!cfg_enabled.extra.contains_key("mcp_enabled"));

        let json_disabled = r#"{"mcp_enabled": false}"#;
        let mut cfg_disabled: LiveAudioConfig = serde_json::from_str(json_disabled).unwrap();
        assert!(cfg_disabled.normalize());
        assert!(!cfg_disabled.mcp_style_enabled);
        assert!(!cfg_disabled.mcp_system_enabled);
        assert!(!cfg_disabled.extra.contains_key("mcp_enabled"));
    }

    #[test]
    fn test_ws_allowed_loopback_ports_normalization() {
        let mut cfg = LiveAudioConfig {
            ws_allowed_loopback_ports: vec![1420, 0, 5173, 1420, 8080, 0],
            ..LiveAudioConfig::default()
        };
        cfg.normalize();
        assert_eq!(cfg.ws_allowed_loopback_ports, vec![1420, 5173, 8080]);

        // Max cap of 32
        let mut many_ports: Vec<u16> = (1..=50).collect();
        cfg.ws_allowed_loopback_ports = many_ports.clone();
        cfg.normalize();
        assert_eq!(cfg.ws_allowed_loopback_ports.len(), 32);
        many_ports.truncate(32);
        assert_eq!(cfg.ws_allowed_loopback_ports, many_ports);
    }

    #[test]
    fn asr_backend_defaults_to_auto_round_trips_and_clamps_invalid_values() {
        assert_eq!(LiveAudioConfig::default().asr_backend, "auto");
        let legacy: LiveAudioConfig = serde_json::from_str("{}").expect("legacy config");
        assert_eq!(legacy.asr_backend, "auto");
        let mut config = LiveAudioConfig { asr_backend: "ct2_cuda".into(), ..LiveAudioConfig::default() };
        let restored: LiveAudioConfig =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(restored.asr_backend, "ct2_cuda");
        assert!(!config.normalize());
        config.asr_backend = "tensorrt".into();
        assert!(config.normalize());
        assert_eq!(config.asr_backend, "auto");
    }

    #[test]
    fn persisted_cuda_choice_survives_deserialization_and_normalization() {
        let mut config: LiveAudioConfig =
            serde_json::from_str(r#"{"device":"cuda"}"#).expect("saved config");
        assert_eq!(config.device, "cuda");
        config.normalize();
        assert_eq!(config.device, "cuda");
    }

    #[test]
    fn optional_audio_flags_default_off_for_legacy_and_round_trip_when_explicit() {
        let legacy: LiveAudioConfig = serde_json::from_str("{}").expect("legacy config");
        assert!(!legacy.rnnoise_enabled);
        assert!(!legacy.acoustic_events_enabled);
        assert!(!legacy.live_partial_subtitles_enabled);

        let mut config = LiveAudioConfig::default();
        config.rnnoise_enabled = true;
        config.acoustic_events_enabled = false;
        config.live_partial_subtitles_enabled = true;
        let serialized = serde_json::to_string(&config).expect("serialize config");
        let restored: LiveAudioConfig =
            serde_json::from_str(&serialized).expect("deserialize config");
        assert!(restored.rnnoise_enabled);
        assert!(!restored.acoustic_events_enabled);
        assert!(restored.live_partial_subtitles_enabled);
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

        assert_eq!(cfg.device, "cpu");
        assert_eq!(cfg.ws_port, 8765);
        assert_eq!(cfg.ws_allowed_loopback_ports, vec![1420]);
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
        // Per-process name: concurrent test runs share the temp dir.
        let unicode_dir = base_tmp.join(format!(
            "LiveAudio Test José Ñandú 🎙️ carpeta con espacios {}",
            std::process::id()
        ));
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

    fn load_json(name: &str, json: &str) -> io::Result<LiveAudioConfig> {
        let dir = std::env::temp_dir().join(format!(
            "liveaudio-cfg-tolerant-{}-{}",
            name,
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, json).unwrap();
        let res = load_from_path(&path);
        let _ = std::fs::remove_dir_all(&dir);
        res
    }

    #[test]
    fn normalize_keeps_subtitles_purpose_and_clamps_duration_to_its_ceiling() {
        let mut cfg = LiveAudioConfig::default();
        cfg.max_chunk_duration = 30.0;
        cfg.normalize();
        assert_eq!(cfg.transcription_purpose, "subtitles");
        assert_eq!(cfg.max_chunk_duration, 15.0);
    }

    #[test]
    fn normalize_clamps_transcription_duration_to_the_30s_worker_window() {
        for purpose in ["transcription", "combined"] {
            let mut cfg = LiveAudioConfig::default();
            cfg.transcription_purpose = purpose.into();
            cfg.max_chunk_duration = 60.0;
            cfg.normalize();
            assert_eq!(cfg.max_chunk_duration, 30.0, "purpose {purpose}");
        }
    }

    #[test]
    fn normalize_rounds_half_to_even_like_python() {
        let mut cfg = LiveAudioConfig::default();
        cfg.max_chunk_duration = 12.25;
        cfg.normalize();
        assert_eq!(cfg.max_chunk_duration, 12.2);
    }

    #[test]
    fn normalize_replaces_non_finite_duration_with_purpose_default() {
        let mut cfg = LiveAudioConfig::default();
        cfg.transcription_purpose = "transcription".into();
        cfg.max_chunk_duration = f64::NAN;
        cfg.normalize();
        assert_eq!(cfg.max_chunk_duration, 30.0);
    }

    #[test]
    fn missing_or_invalid_duration_takes_the_purpose_default_on_load() {
        let missing = load_json(
            "duration-missing",
            r#"{"transcription_purpose": "transcription"}"#,
        )
        .expect("tolerant load");
        assert_eq!(missing.max_chunk_duration, 30.0);

        let invalid = load_json(
            "duration-null",
            r#"{"transcription_purpose": "combined", "max_chunk_duration": null}"#,
        )
        .expect("tolerant load");
        assert_eq!(invalid.max_chunk_duration, 30.0);

        let subtitles = load_json("duration-subtitles", r#"{"max_chunk_duration": "x"}"#)
            .expect("tolerant load");
        assert_eq!(subtitles.max_chunk_duration, 3.0);

        let explicit = load_json(
            "duration-explicit",
            r#"{"transcription_purpose": "transcription", "max_chunk_duration": 5.0}"#,
        )
        .expect("tolerant load");
        assert_eq!(explicit.max_chunk_duration, 5.0);
    }

    #[test]
    fn wrong_typed_value_falls_back_for_that_key_only() {
        let cfg = load_json(
            "wrong-type",
            r#"{"silence_timeout": "0.8", "model_size": "medium", "ws_port": 9001,
                "custom_addon": {"on": true}}"#,
        )
        .expect("tolerant load");
        assert_eq!(cfg.silence_timeout, default_silence_timeout());
        assert_eq!(cfg.model_size, "medium");
        assert_eq!(cfg.ws_port, 9001);
        assert_eq!(
            cfg.extra.get("custom_addon"),
            Some(&serde_json::json!({"on": true}))
        );
    }

    #[test]
    fn null_nan_like_and_bool_numbers_fall_back_per_key() {
        let cfg = load_json(
            "null-nan-bool",
            r#"{"max_chunk_duration": null, "vad_threshold": "NaN", "cpu_threads": true,
                "device": "cuda", "obs_enabled": false}"#,
        )
        .expect("tolerant load");
        assert_eq!(cfg.max_chunk_duration, default_max_chunk_duration());
        assert_eq!(cfg.vad_threshold, default_vad_threshold());
        assert_eq!(cfg.cpu_threads, default_cpu_threads());
        assert_eq!(cfg.device, "cuda");
        assert!(!cfg.obs_enabled);
    }

    #[test]
    fn out_of_range_ws_port_falls_back_to_default() {
        let cfg =
            load_json("ws-port", r#"{"ws_port": 70000, "language": "en"}"#).expect("tolerant load");
        assert_eq!(cfg.ws_port, 8765);
        assert_eq!(cfg.language.as_deref(), Some("en"));
    }

    #[test]
    fn non_object_json_is_still_an_error() {
        assert!(load_json("array", "[1, 2]").is_err());
        assert!(load_json("garbage", "{not json").is_err());
    }

    #[test]
    fn subtitle_theme_and_presets_default_empty_and_normalize_through_config() {
        let legacy: LiveAudioConfig = serde_json::from_str("{}").expect("legacy config");
        assert!(legacy.subtitle_theme.is_empty());
        assert!(legacy.subtitle_presets.is_empty());

        let mut cfg: LiveAudioConfig = serde_json::from_str(
            r##"{"subtitle_theme": {"--sub-color": "#ff0000", "--sub-font-family": "url(http://x/y.woff)"},
                "subtitle_presets": [{"name": " Clean ", "tokens": {"--sub-bg": "url(x)", "--sub-radius": "99px"}}]}"##,
        )
        .expect("subtitle style config");
        assert!(cfg.normalize());
        assert_eq!(
            cfg.subtitle_theme.get("--sub-color").map(String::as_str),
            Some("#ff0000")
        );
        assert!(!cfg.subtitle_theme.contains_key("--sub-font-family"));
        assert_eq!(cfg.subtitle_presets.len(), 1);
        assert_eq!(cfg.subtitle_presets[0].name, "Clean");
        assert_eq!(
            cfg.subtitle_presets[0]
                .tokens
                .get("--sub-radius")
                .map(String::as_str),
            Some("50px")
        );
        assert!(!cfg.subtitle_presets[0].tokens.contains_key("--sub-bg"));
    }

    #[test]
    fn test_update_check_config_serde_defaults_and_roundtrip() {
        let from_empty: LiveAudioConfig = serde_json::from_str("{}").expect("empty json");
        assert!(from_empty.check_updates_enabled);
        assert_eq!(from_empty.last_update_check, 0);

        let mut cfg = LiveAudioConfig::default();
        cfg.check_updates_enabled = false;
        cfg.last_update_check = 1_700_000_000;
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: LiveAudioConfig = serde_json::from_str(&json).expect("deserialize");
        assert!(!restored.check_updates_enabled);
        assert_eq!(restored.last_update_check, 1_700_000_000);
    }

    #[test]
    fn test_update_check_normalization_clamping() {
        let mut cfg = LiveAudioConfig::default();
        cfg.last_update_check = -500;
        assert!(cfg.normalize());
        assert_eq!(cfg.last_update_check, 0);

        let mut cfg2 = LiveAudioConfig::default();
        cfg2.last_update_check = 12345;
        assert!(!cfg2.normalize());
        assert_eq!(cfg2.last_update_check, 12345);
    }

    #[test]
    fn test_should_check_for_updates_throttle_and_clock_skew() {
        let now = 1_700_000_000;

        assert!(should_check_for_updates(0, now));
        assert!(should_check_for_updates(-1, now));
        assert!(!should_check_for_updates(now, now));
        assert!(!should_check_for_updates(now, now + 86_399));
        assert!(should_check_for_updates(now, now + 86_400));
        assert!(should_check_for_updates(now, now + 100_000));
        assert!(should_check_for_updates(now + 3600, now));
    }

    #[test]
    fn test_update_check_failure_backoff() {
        let now = 1_700_000_000;
        let failure_ts = update_check_failure_timestamp(now);

        assert_eq!(failure_ts, now - 82_800);
        assert!(!should_check_for_updates(failure_ts, now));
        assert!(!should_check_for_updates(failure_ts, now + 3599));
        assert!(should_check_for_updates(failure_ts, now + 3600));
        assert_eq!(update_check_failure_timestamp(500), 0);
    }
}
