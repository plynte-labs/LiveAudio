// SPDX-License-Identifier: MIT

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const SERVICE_EVENT_SCHEMA: &str = "liveaudio.service.event";
pub const SERVICE_EVENT_VERSION: u32 = 1;

/// Sensitive keys that must NEVER be written to stdout or health snapshots.
pub static SCRUBBED_KEYS: &[&str] = &[
    "text",
    "transcript",
    "segments",
    "message",
    "log",
    "logs",
    "audio",
    "audio_chunk",
    "path",
    "output_dir",
    "session_dir",
    "export_dir",
    "payload",
    "blacklist",
];

/// Scrub a JSON value to strip any confidential keys.
pub fn scrub_json_value(val: &mut serde_json::Value) {
    match val {
        serde_json::Value::Object(map) => {
            let scrubbed_keys: HashSet<&str> = SCRUBBED_KEYS.iter().copied().collect();
            map.retain(|k, _| !scrubbed_keys.contains(k.as_str()));
            for (_, v) in map.iter_mut() {
                scrub_json_value(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                scrub_json_value(item);
            }
        }
        _ => {}
    }
}

/// Standardized service event emitted over stdout JSON-lines.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServiceEvent {
    pub schema: String,
    pub version: u32,
    pub service_pid: u32,
    pub parent_pid: u32,
    #[serde(rename = "type")]
    pub event_type: ServiceEventType,
    #[serde(flatten)]
    pub payload: serde_json::Map<String, serde_json::Value>,
}

impl ServiceEvent {
    pub fn new(
        service_pid: u32,
        parent_pid: u32,
        event_type: ServiceEventType,
        payload: serde_json::Map<String, serde_json::Value>,
    ) -> Self {
        let mut clean_payload = payload;
        let scrubbed_keys: HashSet<&str> = SCRUBBED_KEYS.iter().copied().collect();
        clean_payload.retain(|k, _| !scrubbed_keys.contains(k.as_str()));

        Self {
            schema: SERVICE_EVENT_SCHEMA.to_string(),
            version: SERVICE_EVENT_VERSION,
            service_pid,
            parent_pid,
            event_type,
            payload: clean_payload,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceEventType {
    Status,
    Health,
    Ready,
    Warning,
    Error,
    Stopped,
    WsPort,
    AsrState,
    ServiceState,
    Fatal,
}

/// Atomic health file snapshot content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthSnapshot {
    pub schema: String,
    pub version: u32,
    pub service_pid: u32,
    pub parent_pid: u32,
    pub state: String,
    pub ws_port: Option<u16>,
    pub ws_clients: usize,
    pub audio_capturing: bool,
    pub asr_state: String,
    pub queue_depth: usize,
    pub uptime_sec: f64,
    pub failure_count: usize,
    pub last_heartbeat_timestamp: u64,
}

impl HealthSnapshot {
    pub fn new(service_pid: u32, parent_pid: u32) -> Self {
        Self {
            schema: SERVICE_EVENT_SCHEMA.to_string(),
            version: SERVICE_EVENT_VERSION,
            service_pid,
            parent_pid,
            state: "initializing".to_string(),
            ws_port: None,
            ws_clients: 0,
            audio_capturing: false,
            asr_state: "idle".to_string(),
            queue_depth: 0,
            uptime_sec: 0.0,
            failure_count: 0,
            last_heartbeat_timestamp: 0,
        }
    }
}
