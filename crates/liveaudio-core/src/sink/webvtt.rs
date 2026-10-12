// SPDX-License-Identifier: MIT

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::sink::error::SinkError;
use crate::sink::traits::{BoxFuture, TranscriptionSink};
use crate::sink::types::{SinkStats, TranscriptionCue};

/// Convert floating-point seconds to standard WebVTT timestamp format `HH:MM:SS.mmm`.
pub fn format_vtt_timestamp(seconds: f64) -> String {
    let non_neg = seconds.max(0.0);
    let total_secs = non_neg.floor() as u64;
    let millis = ((non_neg - non_neg.floor()) * 1000.0).round() as u64;

    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    format!(
        "{:02}:{:02}:{:02}.{:03}",
        hours,
        minutes,
        secs,
        millis.min(999)
    )
}

/// Asynchronous, bounded WebVTT file writer sink (ADR-005).
pub struct WebVttSink {
    sender: mpsc::Sender<TranscriptionCue>,
    pending: Arc<AtomicU64>,
    saved: Arc<AtomicU64>,
    failed: Arc<AtomicU64>,
    rejected: Arc<AtomicU64>,
}

impl WebVttSink {
    pub fn new(path: PathBuf, capacity: usize) -> Self {
        let (sender, mut receiver) = mpsc::channel::<TranscriptionCue>(capacity.max(1));
        let pending = Arc::new(AtomicU64::new(0));
        let saved = Arc::new(AtomicU64::new(0));
        let failed = Arc::new(AtomicU64::new(0));
        let rejected = Arc::new(AtomicU64::new(0));

        let p = Arc::clone(&pending);
        let s = Arc::clone(&saved);
        let f = Arc::clone(&failed);

        tokio::spawn(async move {
            if !path.exists() {
                if let Ok(mut file) = OpenOptions::new().create(true).write(true).open(&path) {
                    let _ = writeln!(file, "WEBVTT\n");
                }
            }

            while let Some(cue) = receiver.recv().await {
                let start_str = format_vtt_timestamp(cue.start_timestamp_sec);
                let end_str = format_vtt_timestamp(cue.end_timestamp_sec);
                let entry = format!(
                    "{}\n{} --> {}\n{}\n\n#cue:{}\n\n",
                    cue.cue_counter, start_str, end_str, cue.text, cue.cue_counter
                );

                let write_res = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .and_then(|mut file| file.write_all(entry.as_bytes()));

                p.fetch_sub(1, Ordering::SeqCst);
                if write_res.is_ok() {
                    s.fetch_add(1, Ordering::SeqCst);
                } else {
                    f.fetch_add(1, Ordering::SeqCst);
                }
            }
        });

        Self {
            sender,
            pending,
            saved,
            failed,
            rejected,
        }
    }
}

impl TranscriptionSink for WebVttSink {
    fn emit<'a>(&'a self, cue: &'a TranscriptionCue) -> BoxFuture<'a, Result<(), SinkError>> {
        Box::pin(async move {
            self.pending.fetch_add(1, Ordering::SeqCst);
            match self.sender.try_send(cue.clone()) {
                Ok(()) => Ok(()),
                Err(mpsc::error::TrySendError::Full(_)) => {
                    self.pending.fetch_sub(1, Ordering::SeqCst);
                    self.rejected.fetch_add(1, Ordering::SeqCst);
                    Err(SinkError::QueueFull("webvtt".to_string()))
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    self.pending.fetch_sub(1, Ordering::SeqCst);
                    self.rejected.fetch_add(1, Ordering::SeqCst);
                    Err(SinkError::Closed)
                }
            }
        })
    }

    fn flush(&self, flush_timeout: Duration) -> BoxFuture<'_, Result<(), SinkError>> {
        Box::pin(async move {
            let check = async {
                while self.pending.load(Ordering::SeqCst) > 0 {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            };

            timeout(flush_timeout, check)
                .await
                .map_err(|_| SinkError::DrainTimeout)
        })
    }

    fn stop(&self, stop_timeout: Duration) -> BoxFuture<'_, Result<(), SinkError>> {
        Box::pin(async move {
            self.flush(stop_timeout).await?;
            let stats = self.stats();
            if stats.failed > 0 || stats.rejected > 0 {
                return Err(SinkError::StorageError(format!(
                    "WebVTT output failed: {} write(s), {} rejected",
                    stats.failed, stats.rejected
                )));
            }
            Ok(())
        })
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
