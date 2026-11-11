// SPDX-License-Identifier: MIT

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
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

/// Cue text cannot contain the `-->` timing separator or blank lines: either would end the cue
/// early and corrupt the file. The single left-to-right pass is enough, because the inserted
/// `- ->` never forms a new `-->`. Blank lines are dropped and the remaining lines are kept.
pub fn vtt_cue_text(text: &str) -> String {
    text.replace("-->", "- ->")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("
")
}

/// Same as [`format_vtt_timestamp`] with the comma separator SubRip (`.srt`) requires.
pub fn format_srt_timestamp(seconds: f64) -> String {
    format_vtt_timestamp(seconds).replace('.', ",")
}

/// Last cue number already written to a WebVTT or SRT file, so a resumed session keeps
/// numbering sequentially. Missing file yields 0. A cue number is a numeric line directly
/// followed by a timing line, so numeric cue text is never mistaken for one. Legacy files
/// with `#cue:N` markers are still honored.
pub fn last_cue_counter(path: &Path) -> u64 {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = content.lines().map(str::trim).collect();
    let mut counter = 0;
    for (i, line) in lines.iter().enumerate() {
        if let Some(Ok(marker)) = line.strip_prefix("#cue:").map(str::parse::<u64>) {
            counter = marker;
        } else if let Ok(cue) = line.parse::<u64>() {
            if lines.get(i + 1).is_some_and(|next| next.contains(" --> ")) {
                counter = counter.max(cue);
            }
        }
    }
    counter
}

type CueFormatter = Box<dyn FnMut(&TranscriptionCue) -> String + Send>;

/// Asynchronous, bounded cue file writer sink (ADR-005) producing WebVTT or SRT.
pub struct CueFileSink {
    label: &'static str,
    sender: mpsc::Sender<TranscriptionCue>,
    pending: Arc<AtomicU64>,
    saved: Arc<AtomicU64>,
    failed: Arc<AtomicU64>,
    rejected: Arc<AtomicU64>,
}

impl CueFileSink {
    /// Standard WebVTT; each cue is identified by the pipeline's cue counter.
    pub fn webvtt(path: PathBuf, capacity: usize) -> Self {
        Self::spawn("WebVTT", path, capacity, "WEBVTT

", |cue| {
            format!(
                "{}
{} --> {}
{}

",
                cue.cue_counter,
                format_vtt_timestamp(cue.start_timestamp_sec),
                format_vtt_timestamp(cue.end_timestamp_sec),
                vtt_cue_text(&cue.text)
            )
        })
    }

    /// SubRip. Blocks are numbered from 1, continuing after the last block of an existing file.
    pub fn srt(path: PathBuf, capacity: usize) -> Self {
        let mut index = last_cue_counter(&path);
        Self::spawn("SRT", path, capacity, "", move |cue| {
            index += 1;
            format!(
                "{}
{} --> {}
{}

",
                index,
                format_srt_timestamp(cue.start_timestamp_sec),
                format_srt_timestamp(cue.end_timestamp_sec),
                vtt_cue_text(&cue.text)
            )
        })
    }

    fn spawn(
        label: &'static str,
        path: PathBuf,
        capacity: usize,
        header: &'static str,
        format_entry: impl FnMut(&TranscriptionCue) -> String + Send + 'static,
    ) -> Self {
        let mut format_entry: CueFormatter = Box::new(format_entry);
        let (sender, mut receiver) = mpsc::channel::<TranscriptionCue>(capacity.max(1));
        let pending = Arc::new(AtomicU64::new(0));
        let saved = Arc::new(AtomicU64::new(0));
        let failed = Arc::new(AtomicU64::new(0));
        let rejected = Arc::new(AtomicU64::new(0));

        let p = Arc::clone(&pending);
        let s = Arc::clone(&saved);
        let f = Arc::clone(&failed);

        tokio::spawn(async move {
            if !header.is_empty() && !path.exists() {
                if let Ok(mut file) = OpenOptions::new().create(true).write(true).truncate(true).open(&path) {
                    let _ = file.write_all(header.as_bytes());
                }
            }

            while let Some(cue) = receiver.recv().await {
                let entry = format_entry(&cue);

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
            label,
            sender,
            pending,
            saved,
            failed,
            rejected,
        }
    }
}

impl TranscriptionSink for CueFileSink {
    fn emit<'a>(&'a self, cue: &'a TranscriptionCue) -> BoxFuture<'a, Result<(), SinkError>> {
        Box::pin(async move {
            self.pending.fetch_add(1, Ordering::SeqCst);
            match self.sender.try_send(cue.clone()) {
                Ok(()) => Ok(()),
                Err(mpsc::error::TrySendError::Full(_)) => {
                    self.pending.fetch_sub(1, Ordering::SeqCst);
                    self.rejected.fetch_add(1, Ordering::SeqCst);
                    Err(SinkError::QueueFull(self.label.to_lowercase()))
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
                    "{} output failed: {} write(s), {} rejected",
                    self.label, stats.failed, stats.rejected
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_cue_counter_resumes_from_existing_cues() {
        let dir = std::env::temp_dir().join(format!("liveaudio-vtt-resume-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("subtitles.vtt");
        assert_eq!(last_cue_counter(&path), 0, "missing file starts at zero");

        std::fs::write(
            &path,
            [
                "WEBVTT",
                "",
                "1",
                "00:00:00.000 --> 00:00:01.000",
                "uno",
                "",
                "#cue:1",
                "",
                "7",
                "00:00:02.000 --> 00:00:03.000",
                "siete",
                "",
                "#cue:7",
                "",
            ]
            .join("\n"),
        )
        .unwrap();
        assert_eq!(last_cue_counter(&path), 7);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn cue(counter: u64, start: f64, end: f64, text: &str) -> TranscriptionCue {
        let response = crate::asr::AsrTranscriptionResponse {
            request_id: String::new(),
            text: text.to_string(),
            language: "es".to_string(),
            duration_sec: 0.0,
            latency_sec: 0.0,
            segments: Vec::new(),
        };
        TranscriptionCue::new(counter, start, end, text.to_string(), response)
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("liveaudio-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn last_cue_counter_reads_standard_identifiers_and_ignores_numeric_text() {
        let dir = temp_dir("vtt-standard");
        let path = dir.join("subtitles.vtt");
        std::fs::write(
            &path,
            "WEBVTT

3
00:00:00.000 --> 00:00:01.000
999

4
00:00:01.000 --> 00:00:02.000
cuatro

",
        )
        .unwrap();
        assert_eq!(last_cue_counter(&path), 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn vtt_sink_writes_standard_webvtt_without_cue_markers() {
        let dir = temp_dir("vtt-sink");
        let path = dir.join("subtitles.vtt");
        let sink = CueFileSink::webvtt(path.clone(), 8);
        sink.emit(&cue(5, 1.0, 2.5, "hola

mundo --> fin")).await.unwrap();
        sink.stop(Duration::from_secs(2)).await.unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            content,
            "WEBVTT

5
00:00:01.000 --> 00:00:02.500
hola
mundo - -> fin

"
        );
        assert_eq!(last_cue_counter(&path), 5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn srt_sink_numbers_sequentially_and_resumes_after_last_block() {
        let dir = temp_dir("srt-sink");
        let path = dir.join("subtitles.srt");
        let sink = CueFileSink::srt(path.clone(), 8);
        sink.emit(&cue(41, 0.0, 1.25, "uno")).await.unwrap();
        sink.emit(&cue(42, 3661.5, 3662.0, "dos

tres --> x")).await.unwrap();
        sink.stop(Duration::from_secs(2)).await.unwrap();
        let expected = "1
00:00:00,000 --> 00:00:01,250
uno

2
01:01:01,500 --> 01:01:02,000
dos
tres - -> x

";
        assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);

        let resumed = CueFileSink::srt(path.clone(), 8);
        resumed.emit(&cue(99, 4.0, 5.0, "cuatro")).await.unwrap();
        resumed.stop(Duration::from_secs(2)).await.unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.ends_with("3
00:00:04,000 --> 00:00:05,000
cuatro

"), "{content:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn last_cue_counter_still_reads_legacy_cue_markers() {
        let dir = temp_dir("vtt-legacy");
        let path = dir.join("subtitles.vtt");
        std::fs::write(
            &path,
            "WEBVTT

8
00:00:00.000 --> 00:00:01.000
ocho

#cue:8

",
        )
        .unwrap();
        assert_eq!(last_cue_counter(&path), 8);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cue_text_never_contains_the_vtt_arrow() {
        assert_eq!(vtt_cue_text("plain text"), "plain text");
        for text in ["see --> there", "--->", "a-->b-->c", "-->-->"] {
            let cleaned = vtt_cue_text(text);
            assert!(!cleaned.contains("-->"), "{cleaned:?} from {text:?}");
        }
    }
}
