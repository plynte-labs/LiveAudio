// SPDX-License-Identifier: MIT

//! LiveAudio Desktop Logging Infrastructure.
//!
//! Provides synchronized dual-target logging:
//! 1. Live console / terminal output (stdout) for real-time developer feedback.
//! 2. Persistent file output (%APPDATA%/LiveAudio/logs/liveaudio-desktop.log) for diagnostics.

use std::fs::{create_dir_all, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing_subscriber::fmt::MakeWriter;

/// Dual-destination writer that synchronously emits logs to both stdout and a log file.
pub struct DualWriter {
    file: Option<Arc<Mutex<File>>>,
}

impl Write for DualWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stdout().write_all(buf);
        if let Some(ref f) = self.file {
            if let Ok(mut guard) = f.lock() {
                let _ = guard.write_all(buf);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::stdout().flush();
        if let Some(ref f) = self.file {
            if let Ok(mut guard) = f.lock() {
                let _ = guard.flush();
            }
        }
        Ok(())
    }
}

/// Factory for creating `DualWriter` instances for `tracing-subscriber`.
#[derive(Clone)]
pub struct DualWriterMaker {
    file: Option<Arc<Mutex<File>>>,
}

impl<'a> MakeWriter<'a> for DualWriterMaker {
    type Writer = DualWriter;

    fn make_writer(&'a self) -> Self::Writer {
        DualWriter {
            file: self.file.clone(),
        }
    }
}

/// Initializes structured logging for LiveAudio Desktop.
///
/// Returns the canonical path to the persistent log file.
pub fn init_logging() -> PathBuf {
    let data_home = liveaudio_core::config::get_data_home();
    let logs_dir = data_home.join("logs");
    let _ = create_dir_all(&logs_dir);
    let log_file_path = logs_dir.join("liveaudio-desktop.log");

    let file_handle = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file_path)
        .ok()
        .map(|f| Arc::new(Mutex::new(f)));

    let maker = DualWriterMaker { file: file_handle };

    let subscriber = tracing_subscriber::fmt()
        .with_writer(maker)
        .with_ansi(true)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .compact()
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);

    tracing::info!("============================================================");
    tracing::info!("🚀 LiveAudio Desktop Logging Initialized");
    tracing::info!("📁 Log file: {}", log_file_path.display());
    tracing::info!("============================================================");

    log_file_path
}
