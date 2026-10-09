// SPDX-License-Identifier: MIT

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SinkError {
    #[error("Sink queue full (backpressure rejected: {0})")]
    QueueFull(String),

    #[error("Storage I/O failure: {0}")]
    StorageError(String),

    #[error("Sink drain timed out")]
    DrainTimeout,

    #[error("Sink is closed or shut down")]
    Closed,

    #[error("Format error: {0}")]
    FormatError(String),
}
