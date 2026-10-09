// SPDX-License-Identifier: MIT

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AsrError {
    #[error("Failed to load ASR model '{model}': {reason}")]
    ModelLoadFailed { model: String, reason: String },

    #[error("ASR inference timed out after {timeout_sec:.1}s")]
    InferenceTimeout { timeout_sec: f32 },

    #[error("ASR worker process crashed or exited unexpectedly: {0}")]
    WorkerCrashed(String),

    #[error("ASR request queue full (backpressure drop)")]
    QueueFull,

    #[error("ASR operation cancelled")]
    Cancelled,

    #[error("Unsupported hardware device '{device}': {reason}")]
    UnsupportedDevice { device: String, reason: String },

    #[error("ASR internal error: {0}")]
    InternalError(String),
}
