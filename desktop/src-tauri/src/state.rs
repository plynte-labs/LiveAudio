// SPDX-License-Identifier: MIT

//! Desktop Application State Management.

use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use crate::dto::{AsrProgressDto, UpdateMetadataDto};
use liveaudio_core::config::{get_data_home, LiveAudioConfig};
use liveaudio_core::usage_stats::UsageTracker;
use liveaudio_core::service_lock::{ServiceLock, ServiceLockHolder};
use liveaudio_core::sink::CompositeSink;
use liveaudio_core::supervisor::ProcessSupervisor;
use liveaudio_network::NetworkServer;

/// Shared application state for LiveAudio Tauri Desktop.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<LiveAudioConfig>>,
    pub active_config: Arc<RwLock<Option<LiveAudioConfig>>>,
    pub is_running: Arc<AtomicBool>,
    pub asr_state: Arc<RwLock<String>>,
    pub asr_progress: Arc<RwLock<Option<AsrProgressDto>>>,
    pub vad_onset: Arc<AtomicBool>,
    pub active_port: Arc<AtomicU16>,
    pub supervisor: Arc<Mutex<Option<Arc<ProcessSupervisor>>>>,
    pub cancel_token: Arc<Mutex<Option<CancellationToken>>>,
    pub start_time: Arc<RwLock<Option<Instant>>>,
    pub ws_server: Arc<Mutex<Option<liveaudio_network::WsServer>>>,
    pub background_tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    pub session_sink: Arc<Mutex<Option<Arc<CompositeSink>>>>,
    pub session_path: Arc<RwLock<Option<std::path::PathBuf>>>,
    /// Timeline origin of the active session; capture offsets are measured from it.
    pub session_origin: Arc<RwLock<Option<Instant>>>,
    /// Last cue number handed to the session sinks.
    pub session_cue_counter: Arc<AtomicU64>,
    pub session_error: Arc<RwLock<Option<String>>>,
    pub transition_lock: Arc<Mutex<()>>,
    /// Serializes read-modify-write saves of the config (general form and subtitle style).
    pub config_write_lock: Arc<Mutex<()>>,
    pub runtime_setup_token: Arc<Mutex<Option<CancellationToken>>>,
    /// Cancel flag of the running NVIDIA runtime install (`Some` while one is running).
    pub nvidia_install_cancel: Arc<Mutex<Option<Arc<AtomicBool>>>>,
    pub startup_cancel_token: Arc<Mutex<Option<CancellationToken>>>,
    pub previous_transcript: Arc<RwLock<Option<String>>>,
    pub audio_state: Arc<RwLock<liveaudio_audio::AudioSourceStatus>>,
    /// Service lock held while this desktop runs the pipeline (released on stop).
    pub service_lock: Arc<Mutex<Option<ServiceLock>>>,
    /// Live holder of the service lock when another process runs the service (Monitor Mode).
    pub foreign_service: Arc<RwLock<Option<ServiceLockHolder>>>,
    pub updater_check_in_progress: Arc<AtomicBool>,
    pub updater_install_in_progress: Arc<AtomicBool>,
    pub updater_install_on_close: Arc<AtomicBool>,
    pub updater_available: Arc<RwLock<Option<UpdateMetadataDto>>>,
    pub updater_pending_update: Arc<Mutex<Option<tauri_plugin_updater::Update>>>,
    pub effective_device: Arc<RwLock<Option<String>>>,
    pub compute_type: Arc<RwLock<Option<String>>>,
    pub fallback_occurred: Arc<AtomicBool>,
    pub fallback_reason: Arc<RwLock<Option<String>>>,
    /// Worker backend of the running session (`ort` / `ct2_cuda`) and why it was chosen.
    pub active_backend: Arc<RwLock<Option<(String, String)>>>,
    /// Final utterances sent to the worker whose result has not been delivered yet.
    pub pending_utterances: Arc<std::sync::Mutex<liveaudio_network::PendingUtteranceTracker>>,
    /// Stops the worker-event task; cancelled by shutdown only after pending finals were delivered.
    pub events_cancel_token: Arc<Mutex<Option<CancellationToken>>>,
    /// One-time startup warning for the UI (backup name of a corrupt `config.json`).
    pub startup_warning: Arc<std::sync::Mutex<Option<String>>>,
    /// Local usage counters persisted to `stats.json` in the data home.
    pub usage_stats: Arc<UsageTracker>,
    /// Bounded in-memory tracker for recent rejected connection attempts.
    pub rejected_origins: Arc<std::sync::Mutex<liveaudio_network::RejectedOriginTracker>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new(LiveAudioConfig::default())
    }
}

impl AppState {
    pub fn new(config: LiveAudioConfig) -> Self {
        let port = config.ws_port;
        Self {
            config: Arc::new(RwLock::new(config)),
            active_config: Arc::new(RwLock::new(None)),
            is_running: Arc::new(AtomicBool::new(false)),
            asr_state: Arc::new(RwLock::new("stopped".to_string())),
            asr_progress: Arc::new(RwLock::new(None)),
            vad_onset: Arc::new(AtomicBool::new(false)),
            active_port: Arc::new(AtomicU16::new(port)),
            supervisor: Arc::new(Mutex::new(None)),
            cancel_token: Arc::new(Mutex::new(None)),
            start_time: Arc::new(RwLock::new(None)),
            ws_server: Arc::new(Mutex::new(None)),
            background_tasks: Arc::new(Mutex::new(Vec::new())),
            session_sink: Arc::new(Mutex::new(None)),
            session_path: Arc::new(RwLock::new(None)),
            session_origin: Arc::new(RwLock::new(None)),
            session_cue_counter: Arc::new(AtomicU64::new(0)),
            session_error: Arc::new(RwLock::new(None)),
            transition_lock: Arc::new(Mutex::new(())),
            config_write_lock: Arc::new(Mutex::new(())),
            runtime_setup_token: Arc::new(Mutex::new(None)),
            nvidia_install_cancel: Arc::new(Mutex::new(None)),
            startup_cancel_token: Arc::new(Mutex::new(None)),
            previous_transcript: Arc::new(RwLock::new(None)),
            audio_state: Arc::new(RwLock::new(liveaudio_audio::AudioSourceStatus::Stopped)),
            service_lock: Arc::new(Mutex::new(None)),
            foreign_service: Arc::new(RwLock::new(None)),
            updater_check_in_progress: Arc::new(AtomicBool::new(false)),
            updater_install_in_progress: Arc::new(AtomicBool::new(false)),
            updater_install_on_close: Arc::new(AtomicBool::new(false)),
            updater_available: Arc::new(RwLock::new(None)),
            updater_pending_update: Arc::new(Mutex::new(None)),
            effective_device: Arc::new(RwLock::new(None)),
            compute_type: Arc::new(RwLock::new(None)),
            fallback_occurred: Arc::new(AtomicBool::new(false)),
            fallback_reason: Arc::new(RwLock::new(None)),
            active_backend: Arc::new(RwLock::new(None)),
            pending_utterances: Arc::new(std::sync::Mutex::new(Default::default())),
            events_cancel_token: Arc::new(Mutex::new(None)),
            startup_warning: Arc::new(std::sync::Mutex::new(None)),
            usage_stats: Arc::new(UsageTracker::new(get_data_home().join("stats.json"))),
            rejected_origins: Arc::new(std::sync::Mutex::new(liveaudio_network::RejectedOriginTracker::new())),
        }
    }

    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    pub fn vad_onset(&self) -> bool {
        self.vad_onset.load(Ordering::SeqCst)
    }

    pub async fn ws_clients(&self) -> usize {
        self.ws_server
            .lock()
            .await
            .as_ref()
            .map_or(0, |ws| ws.client_count())
    }

    pub async fn is_busy(&self) -> bool {
        if self.is_running() {
            return true;
        }
        if self.ws_clients().await > 0 {
            return true;
        }
        self.foreign_service.read().await.is_some()
    }

    pub async fn check_stream_safety(&self) -> Result<(), &'static str> {
        let running = self.is_running();
        let clients = self.ws_clients().await;
        let foreign = self.foreign_service.read().await.is_some();
        crate::updater::evaluate_stream_safety(running, clients, foreign)
    }

    pub fn ws_port(&self) -> u16 {
        self.active_port.load(Ordering::SeqCst)
    }
}
