// SPDX-License-Identifier: MIT

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use liveaudio_core::TranscriptionCue;

pub const WS_PROTO_VERSION: u32 = 1;
pub const WS_APP_NAME: &str = "liveaudio";

/// Handshake hello frame emitted to every newly accepted client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelloMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub app: String,
    pub proto: u32,
    pub port: u16,
}

impl HelloMessage {
    pub fn new(port: u16) -> Self {
        Self {
            message_type: "hello".to_string(),
            app: WS_APP_NAME.to_string(),
            proto: WS_PROTO_VERSION,
            port,
        }
    }
}

/// Exact wire format for live subtitle events broadcast to OBS.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubtitleWireMessage {
    pub id: String,
    pub text: String,
    pub style: String,
    pub created_at: f64,
    pub processed_at: f64,
    pub queue_delay: f64,
    pub total_delay: f64,
    pub latency: f64,
    pub is_replay: bool,
    pub catchup_interval_sec: f64,
}

impl SubtitleWireMessage {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        text: impl Into<String>,
        style: impl Into<String>,
        created_at: f64,
        processed_at: f64,
        queue_delay: f64,
        total_delay: f64,
        latency: f64,
        is_replay: bool,
        catchup_interval_sec: f64,
    ) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            style: style.into(),
            created_at,
            processed_at,
            queue_delay,
            total_delay,
            latency,
            is_replay,
            catchup_interval_sec,
        }
    }

    /// Construct a wire message from a pipeline `TranscriptionCue`.
    pub fn from_cue(
        cue: &TranscriptionCue,
        style: Option<&str>,
        is_replay: bool,
        catchup_interval_sec: f64,
    ) -> Self {
        let style_str = style.unwrap_or("default").to_string();
        let cue_id = cue
            .telemetry
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                let ms = cue.emitted_at_ns / 1_000_000;
                format!("{}-{}", ms, cue.cue_counter)
            });

        let latency = cue.raw_response.latency_sec as f64;
        let queue_delay = cue
            .telemetry
            .get("queue_delay")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let total_delay = cue
            .telemetry
            .get("total_delay")
            .and_then(|v| v.as_f64())
            .unwrap_or_else(|| {
                (cue.end_timestamp_sec - cue.start_timestamp_sec).max(0.0) + latency
            });

        Self {
            id: cue_id,
            text: cue.text.clone(),
            style: style_str,
            created_at: cue.start_timestamp_sec,
            processed_at: cue.end_timestamp_sec,
            queue_delay,
            total_delay,
            latency,
            is_replay,
            catchup_interval_sec,
        }
    }
}

/// Dynamic CSS token theme update wire event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThemeWireMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub tokens: HashMap<String, String>,
}

impl ThemeWireMessage {
    pub fn new(tokens: HashMap<String, String>) -> Self {
        Self {
            message_type: "theme".to_string(),
            tokens,
        }
    }
}

/// Scrub `_telemetry` or internal diagnostic fields before wire serialization.
pub fn scrub_wire_json(mut val: serde_json::Value) -> serde_json::Value {
    if let serde_json::Value::Object(ref mut map) = val {
        map.remove("_telemetry");
    }
    val
}

/// Status event for UI / log queue emission.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatusWireEvent {
    #[serde(rename = "type")]
    pub message_type: String,
    pub key: String,
    pub text: String,
    pub state: String,
}

impl StatusWireEvent {
    pub fn new(key: impl Into<String>, text: impl Into<String>, state: impl Into<String>) -> Self {
        Self {
            message_type: "status".to_string(),
            key: key.into(),
            text: text.into(),
            state: state.into(),
        }
    }
}

/// Port discovery event for GUI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WsPortWireEvent {
    #[serde(rename = "type")]
    pub message_type: String,
    pub port: u16,
    pub base: u16,
}

impl WsPortWireEvent {
    pub fn new(port: u16, base: u16) -> Self {
        Self {
            message_type: "ws_port".to_string(),
            port,
            base,
        }
    }
}
