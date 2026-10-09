// SPDX-License-Identifier: MIT

use liveaudio_core::asr::AsrTranscriptionResponse;
use liveaudio_core::TranscriptionCue;
use liveaudio_network::protocol::{
    scrub_wire_json, HelloMessage, SubtitleWireMessage, ThemeWireMessage, WsPortWireEvent,
};
use std::collections::HashMap;

#[test]
fn test_golden_hello_wire_parity() {
    let python_hello_raw = r#"{"type": "hello", "app": "liveaudio", "proto": 1, "port": 8766}"#;
    let python_val: serde_json::Value = serde_json::from_str(python_hello_raw).unwrap();

    let rust_hello = HelloMessage::new(8766);
    let rust_json = serde_json::to_string(&rust_hello).unwrap();
    let rust_val: serde_json::Value = serde_json::from_str(&rust_json).unwrap();

    assert_eq!(rust_val, python_val);
    assert_eq!(rust_val["type"], "hello");
    assert_eq!(rust_val["app"], "liveaudio");
    assert_eq!(rust_val["proto"], 1);
    assert_eq!(rust_val["port"], 8766);
    assert!(
        rust_val.get("text").is_none(),
        "hello frame must not contain text"
    );
}

#[test]
fn test_golden_subtitle_wire_parity() {
    let python_wire_raw = r#"{
        "id": "1760000000000-12",
        "text": "Hola a todos, bienvenidos al stream",
        "style": "default",
        "created_at": 1760000000.0,
        "processed_at": 1760000001.3,
        "queue_delay": 0.2,
        "total_delay": 1.3,
        "latency": 1.1,
        "is_replay": false,
        "catchup_interval_sec": 0.0
    }"#;
    let python_val: serde_json::Value = serde_json::from_str(python_wire_raw).unwrap();

    let rust_msg = SubtitleWireMessage::new(
        "1760000000000-12",
        "Hola a todos, bienvenidos al stream",
        "default",
        1760000000.0,
        1760000001.3,
        0.2,
        1.3,
        1.1,
        false,
        0.0,
    );
    let rust_json = serde_json::to_string(&rust_msg).unwrap();
    let rust_val: serde_json::Value = serde_json::from_str(&rust_json).unwrap();

    assert_eq!(rust_val, python_val);

    // Verify all wire keys required by WEBSOCKET_OBS.md
    let expected_keys = [
        "id",
        "text",
        "style",
        "created_at",
        "processed_at",
        "queue_delay",
        "total_delay",
        "latency",
        "is_replay",
        "catchup_interval_sec",
    ];

    let rust_obj = rust_val.as_object().unwrap();
    for key in expected_keys {
        assert!(rust_obj.contains_key(key), "Missing wire field: {}", key);
    }
    assert_eq!(rust_obj.len(), expected_keys.len());
}

#[test]
fn test_golden_theme_wire_parity() {
    let python_theme_raw = r##"{
        "type": "theme",
        "tokens": {
            "--sub-bg": "rgba(0, 0, 0, 0.7)",
            "--sub-color": "#ffffff",
            "--sub-font-size": "42px"
        }
    }"##;
    let python_val: serde_json::Value = serde_json::from_str(python_theme_raw).unwrap();

    let mut tokens = HashMap::new();
    tokens.insert("--sub-bg".to_string(), "rgba(0, 0, 0, 0.7)".to_string());
    tokens.insert("--sub-color".to_string(), "#ffffff".to_string());
    tokens.insert("--sub-font-size".to_string(), "42px".to_string());

    let rust_theme = ThemeWireMessage::new(tokens);
    let rust_json = serde_json::to_string(&rust_theme).unwrap();
    let rust_val: serde_json::Value = serde_json::from_str(&rust_json).unwrap();

    assert_eq!(rust_val, python_val);
}

#[test]
fn test_golden_ws_port_discovery_event_parity() {
    let python_ws_port_raw = r#"{"type": "ws_port", "port": 8766, "base": 8765}"#;
    let python_val: serde_json::Value = serde_json::from_str(python_ws_port_raw).unwrap();

    let rust_event = WsPortWireEvent::new(8766, 8765);
    let rust_json = serde_json::to_string(&rust_event).unwrap();
    let rust_val: serde_json::Value = serde_json::from_str(&rust_json).unwrap();

    assert_eq!(rust_val, python_val);
}

#[test]
fn test_telemetry_scrubbing_wire_parity() {
    let raw_with_telemetry = serde_json::json!({
        "id": "cue-42",
        "text": "test audio text",
        "style": "karaoke",
        "_telemetry": {
            "attempt": 1,
            "sequence": 42,
            "queue_enqueued_monotonic": 12345.67
        }
    });

    let scrubbed = scrub_wire_json(raw_with_telemetry);
    let obj = scrubbed.as_object().unwrap();
    assert!(
        !obj.contains_key("_telemetry"),
        "_telemetry must be scrubbed"
    );
    assert_eq!(obj.get("id").unwrap(), "cue-42");
    assert_eq!(obj.get("text").unwrap(), "test audio text");
    assert_eq!(obj.get("style").unwrap(), "karaoke");
}

#[test]
fn test_transcription_cue_to_wire_conversion() {
    let dummy_asr = AsrTranscriptionResponse {
        request_id: "req-101".to_string(),
        text: "transcription result".to_string(),
        language: "es".to_string(),
        duration_sec: 2.5,
        latency_sec: 0.35,
        segments: vec![],
    };

    let mut cue =
        TranscriptionCue::new(7, 10.0, 12.5, "transcription result".to_string(), dummy_asr);
    cue.telemetry
        .insert("queue_delay".to_string(), serde_json::json!(0.15));
    cue.telemetry
        .insert("total_delay".to_string(), serde_json::json!(0.5));

    let wire = SubtitleWireMessage::from_cue(&cue, Some("neon"), true, 1.5);
    assert_eq!(wire.text, "transcription result");
    assert_eq!(wire.style, "neon");
    assert_eq!(wire.created_at, 10.0);
    assert_eq!(wire.processed_at, 12.5);
    assert_eq!(wire.queue_delay, 0.15);
    assert_eq!(wire.total_delay, 0.5);
    assert!((wire.latency - 0.35).abs() < 1e-5);
    assert!(wire.is_replay);
    assert_eq!(wire.catchup_interval_sec, 1.5);
}
