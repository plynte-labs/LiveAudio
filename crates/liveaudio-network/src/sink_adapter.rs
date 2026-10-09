// SPDX-License-Identifier: MIT

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

use liveaudio_core::{BoxFuture, SinkError, SinkStats, TranscriptionCue, TranscriptionSink};

use crate::server::WsServer;
use crate::traits::NetworkServer;

enum SinkTarget {
    Broadcast(broadcast::Sender<TranscriptionCue>),
    Server(Arc<WsServer>),
}

/// Adapter bridging pipeline transcription output into network broadcast channels or WsServer.
pub struct WebSocketTranscriptionSink {
    target: SinkTarget,
    pending: Arc<AtomicU64>,
    saved: Arc<AtomicU64>,
    failed: Arc<AtomicU64>,
    rejected: Arc<AtomicU64>,
}

impl WebSocketTranscriptionSink {
    /// Create sink routing to an async broadcast channel.
    pub fn new(sender: broadcast::Sender<TranscriptionCue>) -> Self {
        Self {
            target: SinkTarget::Broadcast(sender),
            pending: Arc::new(AtomicU64::new(0)),
            saved: Arc::new(AtomicU64::new(0)),
            failed: Arc::new(AtomicU64::new(0)),
            rejected: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Create sink routing directly to a production WsServer instance.
    pub fn from_server(server: Arc<WsServer>) -> Self {
        Self {
            target: SinkTarget::Server(server),
            pending: Arc::new(AtomicU64::new(0)),
            saved: Arc::new(AtomicU64::new(0)),
            failed: Arc::new(AtomicU64::new(0)),
            rejected: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl TranscriptionSink for WebSocketTranscriptionSink {
    fn emit<'a>(&'a self, cue: &'a TranscriptionCue) -> BoxFuture<'a, Result<(), SinkError>> {
        Box::pin(async move {
            match &self.target {
                SinkTarget::Broadcast(sender) => match sender.send(cue.clone()) {
                    Ok(_) | Err(_) => {
                        self.saved.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    }
                },
                SinkTarget::Server(server) => match server.broadcast_cue(cue) {
                    Ok(_) => {
                        self.saved.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    }
                    Err(e) => {
                        self.failed.fetch_add(1, Ordering::SeqCst);
                        Err(SinkError::StorageError(e.to_string()))
                    }
                },
            }
        })
    }

    fn flush(&self, _timeout: Duration) -> BoxFuture<'_, Result<(), SinkError>> {
        Box::pin(async move { Ok(()) })
    }

    fn stop(&self, _timeout: Duration) -> BoxFuture<'_, Result<(), SinkError>> {
        Box::pin(async move { Ok(()) })
    }

    fn stats(&self) -> SinkStats {
        SinkStats {
            pending: self.pending.load(Ordering::SeqCst),
            saved: self.saved.load(Ordering::SeqCst),
            failed: self.failed.load(Ordering::SeqCst),
            rejected: self.rejected.load(Ordering::SeqCst),
        }
    }
}
