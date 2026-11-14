// SPDX-License-Identifier: MIT

//! Local usage counters (`stats.json` in the data home). Integers and dates only, never text.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Minimum spacing between the throttled writes triggered by recording.
const FLUSH_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UsageStats {
    /// Sum of accepted final utterance durations.
    pub audio_seconds: f64,
    /// Accepted final subtitles (partials excluded).
    pub subtitles: u64,
    /// Service starts.
    pub sessions: u64,
    /// Unix seconds of the first recorded activity (0 = never used).
    pub first_used: u64,
}

impl UsageStats {
    /// Reads the file; a missing or corrupt file yields zeroed stats.
    pub fn load(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(self)?)?;
        std::fs::rename(&tmp, path)
    }

    fn touch(&mut self, now_unix: u64) {
        if self.first_used == 0 {
            self.first_used = now_unix;
        }
    }
}

struct Inner {
    stats: UsageStats,
    dirty: bool,
    last_flush: Option<Instant>,
}

/// Thread-safe in-memory counters persisted to `path` (throttled, plus explicit `flush`).
pub struct UsageTracker {
    path: PathBuf,
    inner: Mutex<Inner>,
}

impl UsageTracker {
    pub fn new(path: PathBuf) -> Self {
        let stats = UsageStats::load(&path);
        Self {
            path,
            inner: Mutex::new(Inner { stats, dirty: false, last_flush: None }),
        }
    }

    pub fn snapshot(&self) -> UsageStats {
        self.lock().stats
    }

    pub fn record_session(&self, now_unix: u64) {
        self.record(now_unix, |s| s.sessions += 1);
    }

    pub fn record_subtitle(&self, duration_sec: f64, now_unix: u64) {
        self.record(now_unix, |s| {
            s.subtitles += 1;
            if duration_sec.is_finite() && duration_sec > 0.0 {
                s.audio_seconds += duration_sec;
            }
        });
    }

    /// Persists pending changes now; failures are logged, never raised.
    pub fn flush(&self) {
        let mut inner = self.lock();
        Self::write(&self.path, &mut inner);
    }

    fn record(&self, now_unix: u64, apply: impl FnOnce(&mut UsageStats)) {
        let mut inner = self.lock();
        inner.stats.touch(now_unix);
        apply(&mut inner.stats);
        inner.dirty = true;
        if inner.last_flush.map_or(true, |t| t.elapsed() >= FLUSH_INTERVAL) {
            Self::write(&self.path, &mut inner);
        }
    }

    fn write(path: &std::path::Path, inner: &mut Inner) {
        if !inner.dirty {
            return;
        }
        inner.last_flush = Some(Instant::now());
        match inner.stats.save(path) {
            Ok(()) => inner.dirty = false,
            Err(error) => tracing::warn!("Could not save usage stats: {error}"),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn temp_path() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "liveaudio-usage-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("stats.json")
    }

    #[test]
    fn missing_or_corrupt_file_starts_from_zero() {
        let path = temp_path();
        assert_eq!(UsageStats::load(&path), UsageStats::default());
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(UsageStats::load(&path), UsageStats::default());
    }

    #[test]
    fn records_accumulate_and_first_used_is_set_once() {
        let tracker = UsageTracker::new(temp_path());
        tracker.record_session(1_000);
        tracker.record_subtitle(2.5, 2_000);
        tracker.record_subtitle(1.5, 3_000);
        let s = tracker.snapshot();
        assert_eq!((s.sessions, s.subtitles, s.first_used), (1, 2, 1_000));
        assert!((s.audio_seconds - 4.0).abs() < 1e-9);
    }

    #[test]
    fn invalid_durations_are_ignored_but_the_subtitle_counts() {
        let tracker = UsageTracker::new(temp_path());
        tracker.record_subtitle(f64::NAN, 1);
        tracker.record_subtitle(-3.0, 1);
        let s = tracker.snapshot();
        assert_eq!(s.subtitles, 2);
        assert_eq!(s.audio_seconds, 0.0);
    }

    #[test]
    fn flush_round_trips_atomically_and_only_holds_numbers() {
        let path = temp_path();
        let tracker = UsageTracker::new(path.clone());
        tracker.record_session(10);
        tracker.record_subtitle(3.0, 10);
        tracker.flush();
        assert_eq!(UsageStats::load(&path), tracker.snapshot());
        assert!(!path.with_extension("json.tmp").exists());
        let raw = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(value.as_object().unwrap().values().all(|v| v.is_number()));
    }

    #[test]
    fn flush_skips_the_write_when_nothing_changed() {
        let path = temp_path();
        let tracker = UsageTracker::new(path.clone());
        tracker.flush();
        assert!(!path.exists());
    }

    #[test]
    fn recording_flushes_at_most_once_per_interval() {
        let path = temp_path();
        let tracker = UsageTracker::new(path.clone());
        tracker.record_subtitle(1.0, 1);
        assert!(path.exists(), "first change is persisted right away");
        tracker.record_subtitle(1.0, 2);
        assert_eq!(UsageStats::load(&path).subtitles, 1, "throttled");
        tracker.flush();
        assert_eq!(UsageStats::load(&path).subtitles, 2);
    }
}
