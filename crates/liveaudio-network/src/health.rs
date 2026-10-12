// SPDX-License-Identifier: MIT

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use liveaudio_ipc::{HealthSnapshot, ServiceEvent};

/// Real-time atomic health metrics for the WebSocket server.
#[derive(Debug, Default)]
pub struct NetworkMetrics {
    pub client_count: AtomicUsize,
    pub total_accepted_clients: AtomicU64,
    pub rejected_clients: AtomicU64,
    pub replay_buffer_size: AtomicUsize,
    pub replay_drops: AtomicU64,
    pub retry_buffer_size: AtomicUsize,
    pub retry_drops: AtomicU64,
    pub backpressure_events: AtomicU64,
    pub broadcast_messages_total: AtomicU64,
}

impl NetworkMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(
        &self,
        bound_port: Option<u16>,
        start_instant: std::time::Instant,
    ) -> NetworkHealthReport {
        NetworkHealthReport {
            bound_port,
            client_count: self.client_count.load(Ordering::SeqCst),
            total_accepted_clients: self.total_accepted_clients.load(Ordering::SeqCst),
            rejected_clients: self.rejected_clients.load(Ordering::SeqCst),
            replay_buffer_size: self.replay_buffer_size.load(Ordering::SeqCst),
            replay_drops: self.replay_drops.load(Ordering::SeqCst),
            retry_buffer_size: self.retry_buffer_size.load(Ordering::SeqCst),
            retry_drops: self.retry_drops.load(Ordering::SeqCst),
            backpressure_events: self.backpressure_events.load(Ordering::SeqCst),
            broadcast_messages_total: self.broadcast_messages_total.load(Ordering::SeqCst),
            uptime_sec: start_instant.elapsed().as_secs_f64(),
        }
    }
}

/// Serializable health snapshot for networking subsystem.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkHealthReport {
    pub bound_port: Option<u16>,
    pub client_count: usize,
    pub total_accepted_clients: u64,
    pub rejected_clients: u64,
    pub replay_buffer_size: usize,
    pub replay_drops: u64,
    pub retry_buffer_size: usize,
    pub retry_drops: u64,
    pub backpressure_events: u64,
    pub broadcast_messages_total: u64,
    pub uptime_sec: f64,
}

impl NetworkHealthReport {
    pub fn to_health_snapshot(&self, service_pid: u32, parent_pid: u32) -> HealthSnapshot {
        let mut snap = HealthSnapshot::new(service_pid, parent_pid);
        snap.state = if self.bound_port.is_some() {
            "running".to_string()
        } else {
            "stopped".to_string()
        };
        snap.ws_port = self.bound_port;
        snap.ws_clients = self.client_count;
        snap.uptime_sec = self.uptime_sec;
        snap.last_heartbeat_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        snap
    }
}

/// Atomic file writer for health snapshots.
pub struct HealthFileWriter {
    target_path: PathBuf,
}

impl HealthFileWriter {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            target_path: path.into(),
        }
    }

    /// Atomically write serializable data to disk via temporary file rename.
    pub fn write_atomic<T: Serialize>(&self, data: &T) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(data)?;
        let tmp_path = self.target_path.with_extension("tmp");
        if let Some(parent) = self.target_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&tmp_path, json.as_bytes())?;
        fs::rename(&tmp_path, &self.target_path)?;
        Ok(())
    }

    pub fn target_path(&self) -> &Path {
        &self.target_path
    }
}

/// Format a `ServiceEvent` as a single Newline-Delimited JSON (NDJSON) string.
pub fn format_ndjson_event(event: &ServiceEvent) -> Result<String, serde_json::Error> {
    let mut s = serde_json::to_string(event)?;
    s.push('\n');
    Ok(s)
}
