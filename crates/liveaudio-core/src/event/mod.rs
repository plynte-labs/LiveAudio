// SPDX-License-Identifier: MIT

//! Internal Event Bus and Health State Machine conforming to `liveaudio.service.event`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;

use crate::supervisor::emitter::HealthEmitter;
use liveaudio_ipc::{HealthSnapshot, ServiceEvent, ServiceEventType};

pub const DEFAULT_EVENT_BUS_CAPACITY: usize = 1024;

/// Internal publish-subscribe event bus for distributing service events across subsystems.
#[derive(Debug, Clone)]
pub struct EventBus {
    sender: broadcast::Sender<ServiceEvent>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(DEFAULT_EVENT_BUS_CAPACITY)
    }
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Subscribe to events broadcast across the pipeline.
    pub fn subscribe(&self) -> broadcast::Receiver<ServiceEvent> {
        self.sender.subscribe()
    }

    /// Publish an event to all active subscribers. Returns the number of receivers reached.
    pub fn publish(&self, event: ServiceEvent) -> usize {
        self.sender.send(event).unwrap_or(0)
    }

    /// Broadcast a generic event with given parameters.
    pub fn emit(
        &self,
        service_pid: u32,
        parent_pid: u32,
        event_type: ServiceEventType,
        payload: serde_json::Map<String, serde_json::Value>,
    ) -> usize {
        let event = ServiceEvent::new(service_pid, parent_pid, event_type, payload);
        self.publish(event)
    }

    pub fn emit_status(
        &self,
        service_pid: u32,
        parent_pid: u32,
        payload: serde_json::Map<String, serde_json::Value>,
    ) -> usize {
        self.emit(service_pid, parent_pid, ServiceEventType::Status, payload)
    }

    pub fn emit_health(
        &self,
        service_pid: u32,
        parent_pid: u32,
        payload: serde_json::Map<String, serde_json::Value>,
    ) -> usize {
        self.emit(service_pid, parent_pid, ServiceEventType::Health, payload)
    }

    pub fn emit_ready(
        &self,
        service_pid: u32,
        parent_pid: u32,
        payload: serde_json::Map<String, serde_json::Value>,
    ) -> usize {
        self.emit(service_pid, parent_pid, ServiceEventType::Ready, payload)
    }

    pub fn emit_warning(
        &self,
        service_pid: u32,
        parent_pid: u32,
        payload: serde_json::Map<String, serde_json::Value>,
    ) -> usize {
        self.emit(service_pid, parent_pid, ServiceEventType::Warning, payload)
    }

    pub fn emit_error(
        &self,
        service_pid: u32,
        parent_pid: u32,
        payload: serde_json::Map<String, serde_json::Value>,
    ) -> usize {
        self.emit(service_pid, parent_pid, ServiceEventType::Error, payload)
    }

    pub fn emit_stopped(
        &self,
        service_pid: u32,
        parent_pid: u32,
        payload: serde_json::Map<String, serde_json::Value>,
    ) -> usize {
        self.emit(service_pid, parent_pid, ServiceEventType::Stopped, payload)
    }
}

/// Formally modeled discrete states for the LiveAudio service lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthState {
    Initializing,
    Ready,
    Running,
    Degraded,
    Stopping,
    Stopped,
    Failed,
}

impl HealthState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Initializing => "initializing",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Degraded => "degraded",
            Self::Stopping => "stopping",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
        }
    }
}

/// Health state machine managing atomic health snapshot files and status broadcasts.
pub struct HealthStateMachine {
    current_state: HealthState,
    snapshot: HealthSnapshot,
    emitter: HealthEmitter,
    event_bus: Arc<EventBus>,
    start_time: Instant,
    service_pid: u32,
    parent_pid: u32,
}

impl HealthStateMachine {
    pub fn new(
        service_pid: u32,
        parent_pid: u32,
        health_file: Option<PathBuf>,
        event_bus: Arc<EventBus>,
    ) -> Self {
        let emitter = HealthEmitter::new(service_pid, parent_pid, health_file);
        let mut snapshot = HealthSnapshot::new(service_pid, parent_pid);
        snapshot.state = HealthState::Initializing.as_str().to_string();

        let sm = Self {
            current_state: HealthState::Initializing,
            snapshot,
            emitter,
            event_bus,
            start_time: Instant::now(),
            service_pid,
            parent_pid,
        };
        sm.emitter.write_snapshot(&sm.snapshot);
        sm
    }

    pub fn current_state(&self) -> HealthState {
        self.current_state
    }

    pub fn current_snapshot(&self) -> &HealthSnapshot {
        &self.snapshot
    }

    /// Perform a state transition, updating snapshot and notifying event bus & health file.
    pub fn transition_to(&mut self, next: HealthState, reason: Option<&str>) {
        if self.current_state == next {
            return;
        }

        self.current_state = next;
        self.snapshot.state = next.as_str().to_string();
        self.snapshot.uptime_sec = self.start_time.elapsed().as_secs_f64();
        self.snapshot.last_heartbeat_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let mut payload = serde_json::Map::new();
        payload.insert(
            "state".to_string(),
            serde_json::Value::String(next.as_str().to_string()),
        );
        if let Some(r) = reason {
            payload.insert(
                "reason".to_string(),
                serde_json::Value::String(r.to_string()),
            );
        }

        let event_type = match next {
            HealthState::Ready => ServiceEventType::Ready,
            HealthState::Stopped => ServiceEventType::Stopped,
            HealthState::Failed => ServiceEventType::Error,
            HealthState::Degraded => ServiceEventType::Warning,
            _ => ServiceEventType::Status,
        };

        self.event_bus.emit(
            self.service_pid,
            self.parent_pid,
            event_type,
            payload.clone(),
        );
        self.emitter.emit(event_type, payload);
        self.emitter.write_snapshot(&self.snapshot);
    }

    /// Update dynamic metrics.
    pub fn update_metrics(
        &mut self,
        ws_port: Option<u16>,
        ws_clients: usize,
        audio_capturing: bool,
        asr_state: &str,
        queue_depth: usize,
    ) {
        if let Some(port) = ws_port {
            self.snapshot.ws_port = Some(port);
        }
        self.snapshot.ws_clients = ws_clients;
        self.snapshot.audio_capturing = audio_capturing;
        self.snapshot.asr_state = asr_state.to_string();
        self.snapshot.queue_depth = queue_depth;
        self.snapshot.uptime_sec = self.start_time.elapsed().as_secs_f64();
        self.snapshot.last_heartbeat_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        self.emitter.write_snapshot(&self.snapshot);
    }

    /// Record a subsystem failure.
    pub fn record_failure(&mut self) {
        self.snapshot.failure_count += 1;
        self.emitter.write_snapshot(&self.snapshot);
    }

    /// Trigger periodic heartbeat snapshot emission.
    pub fn heartbeat_tick(&mut self) {
        self.snapshot.uptime_sec = self.start_time.elapsed().as_secs_f64();
        self.snapshot.last_heartbeat_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let mut payload = serde_json::Map::new();
        payload.insert(
            "uptime_sec".to_string(),
            serde_json::Value::from(self.snapshot.uptime_sec),
        );
        payload.insert(
            "state".to_string(),
            serde_json::Value::String(self.snapshot.state.clone()),
        );

        self.event_bus
            .emit_health(self.service_pid, self.parent_pid, payload);
        self.emitter.write_snapshot(&self.snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_bus_broadcast() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        let mut payload = serde_json::Map::new();
        payload.insert(
            "code".to_string(),
            serde_json::Value::String("ok".to_string()),
        );

        let reached = bus.emit_status(100, 200, payload);
        assert_eq!(reached, 1);

        let event = rx.recv().await.expect("event should be received");
        assert_eq!(event.service_pid, 100);
        assert_eq!(event.parent_pid, 200);
        assert_eq!(event.event_type, ServiceEventType::Status);
    }

    #[test]
    fn test_health_state_machine_transitions() {
        let bus = Arc::new(EventBus::new(16));
        let tmp_dir =
            std::env::temp_dir().join(format!("liveaudio-sm-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp_dir);
        let health_file = tmp_dir.join("service_health.json");

        let mut sm = HealthStateMachine::new(1234, 5678, Some(health_file.clone()), bus);
        assert_eq!(sm.current_state(), HealthState::Initializing);

        sm.transition_to(HealthState::Ready, Some("Startup completed"));
        assert_eq!(sm.current_state(), HealthState::Ready);
        assert_eq!(sm.current_snapshot().state, "ready");
        assert!(health_file.exists());

        sm.transition_to(HealthState::Running, None);
        assert_eq!(sm.current_state(), HealthState::Running);

        sm.transition_to(HealthState::Stopped, Some("Clean shutdown"));
        assert_eq!(sm.current_state(), HealthState::Stopped);

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
