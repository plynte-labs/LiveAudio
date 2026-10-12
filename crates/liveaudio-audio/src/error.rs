// SPDX-License-Identifier: MIT

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("Audio device not found: {0}")]
    DeviceNotFound(String),

    #[error("Audio backend failure: {0}")]
    BackendError(String),

    #[error("Failed to build or start audio capture stream: {0}")]
    StreamError(String),

    #[error("Audio ring buffer overflow: dropped {dropped_chunks} chunks")]
    BufferOverflow { dropped_chunks: usize },

    #[error("Audio capture device was disconnected")]
    DeviceDisconnected,

    #[error("Audio source is already running")]
    AlreadyRunning,

    #[error("Audio source is not running")]
    NotRunning,

    #[error("Invalid stream parameter: {0}")]
    InvalidParameter(String),
}
