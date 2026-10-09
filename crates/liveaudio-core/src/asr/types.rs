// SPDX-License-Identifier: MIT

use serde::{Deserialize, Serialize};

/// Configuration for loading or switching an ASR model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AsrModelConfig {
    /// Model identifier or path (e.g., "small", "turbo", "large-v3").
    pub model_name: String,
    /// Execution device: "cuda", "cpu", "auto".
    pub device: String,
    /// Quantization / compute type: "float16", "int8", "float32".
    pub compute_type: String,
    /// Language code (e.g. "es", "en") or None for auto-detection.
    pub language: Option<String>,
    /// Optional context prompt to prime vocabulary and reduce hallucinations (ADR-008).
    pub initial_prompt: Option<String>,
    /// Beam size for decoding (default: 1 or 5).
    pub beam_size: u32,
    /// Sampling temperature (default: 0.0 for deterministic output).
    pub temperature: f32,
}

impl Default for AsrModelConfig {
    fn default() -> Self {
        Self {
            model_name: "small".to_string(),
            device: "auto".to_string(),
            compute_type: "float16".to_string(),
            language: Some("es".to_string()),
            initial_prompt: None,
            beam_size: 1,
            temperature: 0.0,
        }
    }
}

/// Request to transcribe a finalized speech audio segment.
#[derive(Debug, Clone, PartialEq)]
pub struct AsrTranscriptionRequest {
    /// Unique identifier for this transcription request.
    pub request_id: String,
    /// 16kHz mono PCM float samples.
    pub audio_samples: Vec<f32>,
    /// Sample rate (expected: 16000).
    pub sample_rate: u32,
    /// Optional overriding context prompt.
    pub prompt: Option<String>,
    /// Optional overriding language code.
    pub language: Option<String>,
    /// Temperature override.
    pub temperature: f32,
    /// Capture start monotonic timestamp in milliseconds.
    pub timestamp_ms: u64,
}

/// A timed word or subtitle segment within a transcript.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TranscriptionSegment {
    pub id: u64,
    pub start_sec: f64,
    pub end_sec: f64,
    pub text: String,
    pub confidence: f32,
}

/// Finalized transcription response returned by the ASR backend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AsrTranscriptionResponse {
    pub request_id: String,
    pub text: String,
    pub language: String,
    pub duration_sec: f32,
    pub latency_sec: f32,
    pub segments: Vec<TranscriptionSegment>,
}

/// Current health and operational state of the ASR backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AsrHealthStatus {
    Uninitialized,
    Initializing { progress: f32 },
    Idle,
    Transcribing,
    HotSwapping,
    Errored { error: String },
}
