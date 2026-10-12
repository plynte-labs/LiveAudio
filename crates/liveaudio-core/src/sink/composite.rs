// SPDX-License-Identifier: MIT

use std::sync::Arc;
use std::time::Duration;

use crate::sink::error::SinkError;
use crate::sink::traits::{BoxFuture, TranscriptionSink};
use crate::sink::types::{SinkStats, TranscriptionCue};

/// Fan-out sink distributing cues to multiple underlying sinks with fault isolation.
pub struct CompositeSink {
    sinks: Vec<Arc<dyn TranscriptionSink>>,
}

impl CompositeSink {
    pub fn new(sinks: Vec<Arc<dyn TranscriptionSink>>) -> Self {
        Self { sinks }
    }

    pub fn add_sink(&mut self, sink: Arc<dyn TranscriptionSink>) {
        self.sinks.push(sink);
    }
}

impl TranscriptionSink for CompositeSink {
    fn emit<'a>(&'a self, cue: &'a TranscriptionCue) -> BoxFuture<'a, Result<(), SinkError>> {
        Box::pin(async move {
            let mut first_error = None;
            for sink in &self.sinks {
                if let Err(e) = sink.emit(cue).await {
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
            }
            match first_error {
                Some(e) => Err(e),
                None => Ok(()),
            }
        })
    }

    fn flush(&self, timeout: Duration) -> BoxFuture<'_, Result<(), SinkError>> {
        Box::pin(async move {
            let mut first_error = None;
            for sink in &self.sinks {
                if let Err(e) = sink.flush(timeout).await {
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
            }
            match first_error {
                Some(e) => Err(e),
                None => Ok(()),
            }
        })
    }

    fn stop(&self, timeout: Duration) -> BoxFuture<'_, Result<(), SinkError>> {
        Box::pin(async move {
            let mut first_error = None;
            for sink in &self.sinks {
                if let Err(e) = sink.stop(timeout).await {
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
            }
            match first_error {
                Some(e) => Err(e),
                None => Ok(()),
            }
        })
    }

    fn stats(&self) -> SinkStats {
        let mut combined = SinkStats::default();
        for sink in &self.sinks {
            let s = sink.stats();
            combined.pending += s.pending;
            combined.saved += s.saved;
            combined.failed += s.failed;
            combined.rejected += s.rejected;
        }
        combined
    }
}
