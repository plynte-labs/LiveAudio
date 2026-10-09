// SPDX-License-Identifier: MIT

use std::future::Future;
use std::pin::Pin;

use crate::error::NetworkError;
use liveaudio_core::TranscriptionCue;

pub type NetworkBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Abstract network server delivering live subtitles to browser sources and clients.
///
/// Thread safety: All implementations must be `Send + Sync + 'static`.
pub trait NetworkServer: Send + Sync + 'static {
    /// Bind and start the server, scanning candidate fallback ports starting from `base_port`.
    /// Returns the successfully bound port.
    fn start(&mut self, base_port: u16) -> NetworkBoxFuture<'_, Result<u16, NetworkError>>;

    /// Stop listening and disconnect active clients cleanly.
    fn stop(&mut self) -> NetworkBoxFuture<'_, Result<(), NetworkError>>;

    /// Broadcast a single transcription cue to all connected WebSocket clients.
    fn broadcast_cue(&self, cue: &TranscriptionCue) -> Result<(), NetworkError>;

    /// Count of currently connected WebSocket clients.
    fn client_count(&self) -> usize;

    /// Read the active bound port, or None if not started.
    fn bound_port(&self) -> Option<u16>;
}
