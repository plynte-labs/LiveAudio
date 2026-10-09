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
