// SPDX-License-Identifier: MIT

//! Data Transfer Objects (DTOs) for Tauri IPC.

use serde::{Deserialize, Serialize};

/// Audio capture device representation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioDeviceDto {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub kind: String,
}

/// Service status and telemetry snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceStatusDto {
    pub is_running: bool,
    pub asr_state: String,
    pub vad_onset: bool,
    pub ws_port: u16,
    pub ws_clients: usize,
    pub active_device: Option<String>,
    pub model_size: String,
    pub device: String,
    pub uptime_sec: f64,
    pub restart_count: usize,
    pub session_path: Option<String>,
    pub session_error: Option<String>,
    pub asr_progress: Option<AsrProgressDto>,
    /// Audio capture state: capturing, standby (no input device) or reconnecting.
    pub audio_state: liveaudio_audio::AudioSourceStatus,
    /// Another process owns the running service; this desktop only monitors it.
    pub monitor_mode: bool,
    /// PID of that owner, when its lock metadata names one.
    pub foreign_service_pid: Option<u32>,
    #[serde(default)]
    pub effective_device: Option<String>,
    #[serde(default)]
    pub compute_type: Option<String>,
    #[serde(default)]
    pub fallback_occurred: bool,
    #[serde(default)]
    pub fallback_reason: Option<String>,
    /// "ort" or "ct2_cuda" while a session runs.
    #[serde(default)]
    pub active_backend: Option<String>,
    /// Why that backend runs (e.g. `nvidia_cuda_ready`, `ct2_start_failed`).
    #[serde(default)]
    pub backend_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AsrProgressDto {
    pub phase: String,
    pub text: String,
    pub percent: Option<f64>,
    pub bytes_downloaded: Option<u64>,
    pub bytes_total: Option<u64>,
    pub speed_bps: Option<u64>,
    pub eta_sec: Option<f64>,
    pub attempt: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProfilePresetDto {
    pub id: String,
    pub label: String,
    pub description: String,
    pub device: String,
    pub model_size: String,
    pub silence_timeout: f64,
    pub subtitle_backlog_policy: String,
    pub subtitle_max_live_delay_sec: f64,
    pub subtitle_catchup_interval_sec: f64,
}

/// OBS browser source configuration and guidance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObsOverlayDto {
    pub url: String,
    pub port: u16,
    pub width: u32,
    pub height: u32,
    pub instructions: String,
}

/// Allowed local loopback ports and recent rejected connection attempts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AllowedConnectionsDto {
    pub allowed_ports: Vec<u16>,
    pub recent_rejected: Vec<liveaudio_network::RejectedOriginEntry>,
}

/// Diagnostics export summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticsExportDto {
    pub path: String,
    pub timestamp: String,
    pub success: bool,
}

/// Real-time subtitle cue for preview and broadcast.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubtitleCueDto {
    pub id: u64,
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub style: String,
    pub timestamp_ms: u64,
}

/// Information about an available application update.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdateMetadataDto {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
    pub pub_date: Option<String>,
}

/// In-app updater status snapshot for the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UpdaterStatusDto {
    pub current_version: String,
    pub available_update: Option<UpdateMetadataDto>,
    pub install_on_close: bool,
    pub is_checking: bool,
    pub is_installing: bool,
    pub stream_busy: bool,
    pub last_check: i64,
}

/// Detected GPU adapter for the backend status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GpuAdapterDto {
    /// "nvidia", "amd", "intel" or "other".
    pub vendor: String,
    pub name: String,
    pub dedicated_vram_mb: u64,
}

/// GPU detection, NVIDIA runtime install state and the backend auto-selection result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GpuBackendStatusDto {
    pub adapters: Vec<GpuAdapterDto>,
    pub cuda_driver_version: Option<i32>,
    pub nvidia_runtime: liveaudio_core::NvidiaRuntimeStatus,
    /// NVIDIA license pages to show before the user starts the download.
    pub license_urls: Vec<String>,
    /// "ort" or "ct2_cuda".
    pub selected_backend: String,
    pub reason: String,
}
