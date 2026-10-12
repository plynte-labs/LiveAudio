// SPDX-License-Identifier: MIT

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("Failed to bind server to port {port}: {reason}")]
    BindError { port: u16, reason: String },

    #[error("All candidate ports in range {base}..{top} are unavailable")]
    PortExhaustion { base: u16, top: u16 },

    #[error("Server is already running on port {0}")]
    AlreadyRunning(u16),

    #[error("Server is not running")]
    NotRunning,

    #[error("Client handshake rejected: foreign origin '{0}'")]
    ForbiddenOrigin(String),

    #[error("Client connection rejected: non-loopback remote address '{0}'")]
    ForbiddenRemote(String),

    #[error("Broadcast channel error: {0}")]
    BroadcastError(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("WebSocket protocol error: {0}")]
    WebSocketError(String),

    #[error("Network I/O error: {0}")]
    IoError(#[from] std::io::Error),
}
