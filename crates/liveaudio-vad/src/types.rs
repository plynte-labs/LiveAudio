// SPDX-License-Identifier: MIT

use serde::{Deserialize, Serialize};

/// Configuration parameters for Voice Activity Detection and speech segmentation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VadConfig {
    /// Audio sample rate in Hz (default: 16000).
    pub sample_rate: u32,
    /// Chunk size in samples per VAD evaluation (default: 512 = 32ms).
    pub chunk_size: usize,
    /// Minimum speech probability to consider active speech (range: 0.0 - 1.0, default: 0.5).
    pub threshold: f32,
    /// Duration of pre-roll silence buffer to recover initial word onsets (default: 96ms).
    pub speech_pad_ms: u32,
    /// Consecutive silence duration before triggering phrase termination (default: 700ms).
    pub silence_timeout_ms: u32,
    /// Hard cap on single phrase duration before forcing closure (default: 15.0s).
    pub max_chunk_duration_sec: f32,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            chunk_size: 512,
            threshold: 0.5,
            speech_pad_ms: 96,
            silence_timeout_ms: 700,
            max_chunk_duration_sec: 15.0,
        }
    }
}

impl VadConfig {
    /// Calculate integer silence chunk count before triggering phrase boundary.
    pub fn silence_limit_chunks(&self) -> usize {
        let chunks_per_sec = self.sample_rate as f64 / self.chunk_size as f64;
        let silence_sec = self.silence_timeout_ms as f64 / 1000.0;
        (chunks_per_sec * silence_sec) as usize
    }

    /// Calculate ceiling chunk limit for phrase duration cap.
    pub fn max_chunks_limit(&self) -> usize {
        let total_samples = self.max_chunk_duration_sec as f64 * self.sample_rate as f64;
        let chunks = (total_samples / self.chunk_size as f64).ceil() as usize;
        chunks.max(1)
    }

    /// Calculate pre-buffer capacity in chunks from speech_pad_ms.
    pub fn pre_buffer_chunks(&self) -> usize {
        crate::pre_buffer::PreBuffer::chunks_from_pad(
            self.speech_pad_ms,
            self.sample_rate,
            self.chunk_size,
        )
    }
}

/// Reason why an ongoing speech segment was finalized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechEndReason {
    /// Phrase terminated due to silence timeout threshold.
    SilenceTimeout,
    /// Phrase terminated due to max chunk duration cap.
    MaxDurationReached,
}

/// Transition state emitted on phrase boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum VadTransition {
    None,
    SpeechStart,
    SpeechEnd {
        duration_ms: u64,
        reason: SpeechEndReason,
    },
}

/// Decision outcome for an evaluated audio chunk.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct VadFrameDecision {
    /// Raw probability evaluated by the VAD model (0.0 .. 1.0).
    pub speech_probability: f32,
    /// Hysteresis-filtered decision on whether the speaker is currently active.
    pub is_speech: bool,
    /// Boundary transition state (start, end, or none).
    pub transition: VadTransition,
}

impl VadFrameDecision {
    pub fn new(speech_probability: f32, is_speech: bool, transition: VadTransition) -> Self {
        Self {
            speech_probability,
            is_speech,
            transition,
        }
    }
}
