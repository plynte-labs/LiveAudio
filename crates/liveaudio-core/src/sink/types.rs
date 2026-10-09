// SPDX-License-Identifier: MIT

use serde::{Deserialize, Serialize};

use crate::asr::AsrTranscriptionResponse;

/// Structured cue containing transcription data and timing for delivery to sinks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TranscriptionCue {
    /// Monotonically increasing cue index (1-based for WebVTT cues).
    pub cue_counter: u64,
    /// Start offset in seconds from session epoch.
    pub start_timestamp_sec: f64,
    /// End offset in seconds from session epoch.
    pub end_timestamp_sec: f64,
    /// Sanitized display text for subtitles and transcript.
    pub text: String,
    /// Full original ASR response payload.
    pub raw_response: AsrTranscriptionResponse,
    /// Monotonic timestamp when this cue was emitted from the pipeline.
    pub emitted_at_ns: u64,
    /// Telemetry data (latency, capture monotonic stamps, model name).
    pub telemetry: serde_json::Map<String, serde_json::Value>,
}

impl TranscriptionCue {
    pub fn new(
        cue_counter: u64,
        start_timestamp_sec: f64,
        end_timestamp_sec: f64,
        text: String,
        raw_response: AsrTranscriptionResponse,
    ) -> Self {
        let emitted_at_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);

        Self {
            cue_counter,
            start_timestamp_sec,
            end_timestamp_sec,
            text,
            raw_response,
            emitted_at_ns,
            telemetry: serde_json::Map::new(),
        }
    }
}

/// Outcome counters and health metrics for a transcription sink.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SinkStats {
    pub pending: u64,
    pub saved: u64,
    pub failed: u64,
    pub rejected: u64,
}
