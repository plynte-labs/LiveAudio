// SPDX-License-Identifier: MIT

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use liveaudio_ipc::{scrub_json_value, HealthSnapshot, ServiceEvent, ServiceEventType};

#[cfg(test)]
mod tests {
    use super::legacy_asr_state;

    #[test]
    fn legacy_asr_state_matches_python_mirror_and_defaults_to_loading() {
        for state in [
            "downloading",
            "loading",
            "transcribing",
            "stalled",
            "starting",
        ] {
            assert_eq!(legacy_asr_state(state), "loading", "state={state}");
        }
        assert_eq!(legacy_asr_state("ready"), "ready");
        assert_eq!(legacy_asr_state("failed"), "failed");
        assert_eq!(legacy_asr_state("unknown"), "loading");
    }
}

fn legacy_asr_state(state: &str) -> &'static str {
    match state {
        "ready" => "ready",
        "failed" => "failed",
        "downloading" | "loading" | "transcribing" | "stalled" => "loading",
        _ => "loading",
    }
}

/// Health and status emitter producing stdout JSON-lines and atomic health snapshot files.
pub struct HealthEmitter {
    service_pid: u32,
    parent_pid: u32,
    health_file: Option<PathBuf>,
    health_file_warned: AtomicBool,
}

impl HealthEmitter {
    pub fn new(service_pid: u32, parent_pid: u32, health_file: Option<PathBuf>) -> Self {
        Self {
            service_pid,
            parent_pid,
            health_file,
            health_file_warned: AtomicBool::new(false),
        }
    }

    /// Emit a single structured JSON line event to stdout.
    /// Never panics and never writes to disk.
    pub fn emit(
        &self,
        event_type: ServiceEventType,
        payload: serde_json::Map<String, serde_json::Value>,
    ) {
        let event = ServiceEvent::new(self.service_pid, self.parent_pid, event_type, payload);
        if let Ok(line) = serde_json::to_string(&event) {
            println!("{}", line);
            let _ = std::io::stdout().flush();
        }
    }

    /// Emit a ws_port event over stdout JSON-lines for supervisor discovery (e.g. OpenCohost).
    pub fn emit_ws_port(&self, base_port: u16, effective_port: u16) {
        let mut payload = serde_json::Map::new();
        payload.insert("base_port".to_string(), serde_json::json!(base_port));
        payload.insert(
            "effective_port".to_string(),
            serde_json::json!(effective_port),
        );
        payload.insert("base".to_string(), serde_json::json!(base_port));
        payload.insert("port".to_string(), serde_json::json!(effective_port));
        self.emit(ServiceEventType::WsPort, payload);
    }

    /// Emit an asr_state event over stdout JSON-lines (e.g. starting, loading, ready).
    pub fn emit_asr_state(&self, asr_state: &str, phase: Option<&str>, attempt: Option<u32>) {
        let mut payload = serde_json::Map::new();
        payload.insert("asr_state".to_string(), serde_json::json!(asr_state));
        payload.insert(
            "asr_state_legacy".to_string(),
            serde_json::json!(legacy_asr_state(asr_state)),
        );
        payload.insert(
            "phase".to_string(),
            serde_json::json!(phase.unwrap_or(asr_state)),
        );
        payload.insert(
            "attempt".to_string(),
            serde_json::json!(attempt.unwrap_or(1)),
        );
        self.emit(ServiceEventType::AsrState, payload);
    }

    /// Emit a service_state event over stdout JSON-lines (e.g. starting, running, stopping).
    pub fn emit_service_state(&self, state: &str) {
        let mut payload = serde_json::Map::new();
        payload.insert("state".to_string(), serde_json::json!(state));
        self.emit(ServiceEventType::ServiceState, payload);
    }

    /// Atomically writes the health snapshot to the designated health file.
    /// Uses a temporary file sibling, flushes, syncs, and renames.
    pub fn write_snapshot(&self, snapshot: &HealthSnapshot) -> bool {
        let health_file = match &self.health_file {
            Some(path) => path,
            None => return true,
        };

        let mut val = match serde_json::to_value(snapshot) {
            Ok(v) => v,
            Err(_) => return false,
        };
        scrub_json_value(&mut val);

        let parent_dir = health_file.parent().unwrap_or_else(|| Path::new("."));
        let _ = std::fs::create_dir_all(parent_dir);

        let tmp_file_path = parent_dir.join(format!(
            ".health-{}-{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));

        let write_op = (|| -> std::io::Result<()> {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&tmp_file_path)?;

            let bytes = serde_json::to_vec_pretty(&val)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

            file.write_all(&bytes)?;
            file.flush()?;
            file.sync_all()?;
            drop(file);

            std::fs::rename(&tmp_file_path, health_file)?;
            Ok(())
        })();

        match write_op {
            Ok(()) => true,
            Err(_) => {
                let _ = std::fs::remove_file(&tmp_file_path);
                if !self.health_file_warned.swap(true, Ordering::SeqCst) {
                    let mut payload = serde_json::Map::new();
                    payload.insert(
                        "code".to_string(),
                        serde_json::Value::String("health-file-unwritable".to_string()),
                    );
                    self.emit(ServiceEventType::Warning, payload);
                }
                false
            }
        }
    }
}
