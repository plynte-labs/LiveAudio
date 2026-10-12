// SPDX-License-Identifier: MIT

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::sink::error::SinkError;
use crate::sink::types::{SinkStats, TranscriptionCue};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Abstract sink destination for transcription output (JSONL, WebVTT, WebSocket bus, etc.).
///
/// Thread safety: All implementations must be `Send + Sync + 'static`.
/// Methods returning `BoxFuture` are dyn-compatible, allowing `Arc<dyn TranscriptionSink>`.
pub trait TranscriptionSink: Send + Sync + 'static {
    /// Emit a transcription record/cue to the destination without blocking the ASR inference path.
    fn emit<'a>(&'a self, cue: &'a TranscriptionCue) -> BoxFuture<'a, Result<(), SinkError>>;

    /// Flush all buffered records to durable storage or network with a bounded timeout.
    fn flush(&self, timeout: Duration) -> BoxFuture<'_, Result<(), SinkError>>;

    /// Orderly stop of the sink worker, draining pending records up to timeout.
    fn stop(&self, timeout: Duration) -> BoxFuture<'_, Result<(), SinkError>>;

    /// Read outcome statistics (pending, saved, failed, rejected).
    fn stats(&self) -> SinkStats;
}
