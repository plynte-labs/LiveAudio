// SPDX-License-Identifier: MIT

pub mod error;
pub mod health;
pub mod protocol;
pub mod replay;
pub mod security;
pub mod server;
pub mod sink_adapter;
pub mod traits;

pub use error::NetworkError;
pub use health::{format_ndjson_event, HealthFileWriter, NetworkHealthReport, NetworkMetrics};
pub use protocol::{
    scrub_wire_json, HelloMessage, SubtitleWireMessage, ThemeWireMessage, WsPortWireEvent,
    WS_APP_NAME, WS_PROTO_VERSION,
};
pub use replay::{
    decide_obs_emit, subtitle_from_worker_result, BacklogPolicy, CompletedUtteranceTiming,
    ObsEmitDecision, PendingUtteranceTracker, ReplayBuffer, RetryBuffer, WireReplayBuffer,
    HIGH_WATER_MARK_BYTES, MAX_RETRY_BUFFER, REPLAY_BUFFER_MAX,
};
pub use security::{
    candidate_ports, extract_loopback_port, is_origin_allowed, is_remote_addr_allowed,
    validate_origin_headers, RejectedOriginEntry, RejectedOriginTracker, MAX_REJECTED_ORIGINS,
    MAX_REJECTED_ORIGIN_LEN, WS_ALLOWED_ORIGINS, WS_LOOPBACK_HOSTS, WS_PORT_FALLBACK_RANGE,
};
pub use server::{start_if_enabled, WsServer, WsServerConfig};
pub use sink_adapter::WebSocketTranscriptionSink;
pub use traits::{NetworkBoxFuture, NetworkServer};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_candidate_ports_calculation() {
        let ports = candidate_ports(8765, Some(10));
        assert_eq!(ports.len(), 10);
        assert_eq!(ports[0], 8765);
        assert_eq!(ports[9], 8774);
    }

    #[test]
    fn test_origin_security_rules() {
        // CEF OBS Browser Source
        assert!(is_origin_allowed(Some("http://absolute"), 8765, &[]));

        // Local loopback origins: on bound port or in allowed list
        assert!(is_origin_allowed(Some("http://localhost:8765"), 8765, &[]));
        assert!(is_origin_allowed(Some("http://127.0.0.1:8765"), 8765, &[]));
        assert!(is_origin_allowed(Some("http://[::1]:8765"), 8765, &[]));
        assert!(!is_origin_allowed(Some("http://localhost:1420"), 8765, &[]));
        assert!(is_origin_allowed(Some("http://localhost:1420"), 8765, &[1420]));
        assert!(!is_origin_allowed(Some("http://127.0.0.1:8080"), 8765, &[]));

        // Native client / empty origin
        assert!(is_origin_allowed(None, 8765, &[]));
        assert!(is_origin_allowed(Some(""), 8765, &[]));

        // Malicious / external origins strictly rejected
        assert!(!is_origin_allowed(Some("https://malicious.com"), 8765, &[]));
        assert!(!is_origin_allowed(Some("http://localhost.attacker.com"), 8765, &[]));
        assert!(!is_origin_allowed(Some("https://attacker.org:8765"), 8765, &[]));
    }

    #[test]
    fn test_replay_buffer_drop_oldest() {
        let mut buffer = ReplayBuffer::new(2);
        let dummy_asr = liveaudio_core::AsrTranscriptionResponse {
            request_id: "req1".to_string(),
            text: "test".to_string(),
            language: "es".to_string(),
            duration_sec: 1.0,
            latency_sec: 0.1,
            segments: vec![],
        };

        let cue1 = liveaudio_core::TranscriptionCue::new(
            1,
            0.0,
            1.0,
            "one".to_string(),
            dummy_asr.clone(),
        );
        let cue2 = liveaudio_core::TranscriptionCue::new(
            2,
            1.0,
            2.0,
            "two".to_string(),
            dummy_asr.clone(),
        );
        let cue3 =
            liveaudio_core::TranscriptionCue::new(3, 2.0, 3.0, "three".to_string(), dummy_asr);

        buffer.push(cue1);
        buffer.push(cue2);
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.dropped_count(), 0);

        // Third push should drop the oldest (cue1)
        buffer.push(cue3);
        assert_eq!(buffer.len(), 2);
        assert_eq!(buffer.dropped_count(), 1);
    }
}
