// SPDX-License-Identifier: MIT

use thiserror::Error;

#[derive(Debug, Error)]
pub enum VadError {
    #[error("Invalid chunk size: expected {expected} samples, got {actual}")]
    InvalidChunkSize { expected: usize, actual: usize },

    #[error("VAD model inference failed: {0}")]
    InferenceError(String),

    #[error("VAD model not loaded: {0}")]
    ModelNotLoaded(String),

    #[error("Invalid VAD configuration: {0}")]
    InvalidConfiguration(String),

    #[error("VAD engine internal state error: {0}")]
    StateError(String),
}
