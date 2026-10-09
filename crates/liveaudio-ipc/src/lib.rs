// SPDX-License-Identifier: MIT

pub mod commands;
pub mod protocol;

pub use commands::{DaemonCommand, DaemonResponse};
pub use protocol::{
    scrub_json_value, HealthSnapshot, ServiceEvent, ServiceEventType, SCRUBBED_KEYS,
    SERVICE_EVENT_SCHEMA, SERVICE_EVENT_VERSION,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scrub_json_value_strips_sensitive_keys() {
        let mut data = serde_json::json!({
            "code": "ok",
            "text": "secret subtitle",
            "transcript": "confidential meeting notes",
            "audio": [0.1, 0.2],
            "nested": {
                "safe": 123,
                "message": "private log"
            }
        });

        scrub_json_value(&mut data);

        assert_eq!(data.get("code"), Some(&serde_json::json!("ok")));
        assert_eq!(data.get("text"), None);
        assert_eq!(data.get("transcript"), None);
        assert_eq!(data.get("audio"), None);
        assert_eq!(
            data.get("nested").and_then(|n| n.get("safe")),
            Some(&serde_json::json!(123))
        );
        assert_eq!(data.get("nested").and_then(|n| n.get("message")), None);
    }

    #[test]
    fn test_service_event_automatic_scrub() {
        let mut map = serde_json::Map::new();
        map.insert("port".to_string(), serde_json::json!(8765));
        map.insert("text".to_string(), serde_json::json!("should be scrubbed"));

        let event = ServiceEvent::new(1234, 5678, ServiceEventType::Ready, map);
        assert_eq!(event.payload.get("port"), Some(&serde_json::json!(8765)));
        assert_eq!(event.payload.get("text"), None);
    }

    #[test]
    fn test_opencohost_supervisor_event_serialization() {
        // ws_port event
        let mut port_map = serde_json::Map::new();
        port_map.insert("base_port".to_string(), serde_json::json!(8765));
        port_map.insert("effective_port".to_string(), serde_json::json!(8765));
        port_map.insert("base".to_string(), serde_json::json!(8765));
        port_map.insert("port".to_string(), serde_json::json!(8765));

        let port_event = ServiceEvent::new(1234, 5678, ServiceEventType::WsPort, port_map);
        let port_json = serde_json::to_value(&port_event).unwrap();
        assert_eq!(port_json["schema"], "liveaudio.service.event");
        assert_eq!(port_json["type"], "ws_port");
        assert_eq!(port_json["effective_port"], 8765);
        assert_eq!(port_json["port"], 8765);

        // asr_state event
        let mut asr_map = serde_json::Map::new();
        asr_map.insert("asr_state".to_string(), serde_json::json!("ready"));
        asr_map.insert("asr_state_legacy".to_string(), serde_json::json!("ready"));
        asr_map.insert("phase".to_string(), serde_json::json!("ready"));
        asr_map.insert("attempt".to_string(), serde_json::json!(1));

        let asr_event = ServiceEvent::new(1234, 5678, ServiceEventType::AsrState, asr_map);
        let asr_json = serde_json::to_value(&asr_event).unwrap();
        assert_eq!(asr_json["type"], "asr_state");
        assert_eq!(asr_json["asr_state"], "ready");

        // service_state event
        let mut state_map = serde_json::Map::new();
        state_map.insert("state".to_string(), serde_json::json!("running"));

        let state_event = ServiceEvent::new(1234, 5678, ServiceEventType::ServiceState, state_map);
        let state_json = serde_json::to_value(&state_event).unwrap();
        assert_eq!(state_json["type"], "service_state");
        assert_eq!(state_json["state"], "running");
    }
}
