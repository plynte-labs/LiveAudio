// SPDX-License-Identifier: MIT

//! Desktop Application State Management.

use std::sync::atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use liveaudio_core::config::LiveAudioConfig;
use liveaudio_core::sink::CompositeSink;
use liveaudio_core::supervisor::ProcessSupervisor;

/// Shared application state for LiveAudio Tauri Desktop.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<LiveAudioConfig>>,
    pub active_config: Arc<RwLock<Option<LiveAudioConfig>>>,
    pub is_running: Arc<AtomicBool>,
    pub asr_state: Arc<RwLock<String>>,
    pub vad_onset: Arc<AtomicBool>,
    pub ws_clients_count: Arc<AtomicUsize>,
    pub active_port: Arc<AtomicU16>,
    pub supervisor: Arc<Mutex<Option<Arc<ProcessSupervisor>>>>,
    pub cancel_token: Arc<Mutex<Option<CancellationToken>>>,
    pub start_time: Arc<RwLock<Option<Instant>>>,
    pub ws_server: Arc<Mutex<Option<liveaudio_network::WsServer>>>,
    pub background_tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    pub session_sink: Arc<Mutex<Option<Arc<CompositeSink>>>>,
    pub session_path: Arc<RwLock<Option<std::path::PathBuf>>>,
    pub session_error: Arc<RwLock<Option<String>>>,
    pub transition_lock: Arc<Mutex<()>>,
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
            vad_onset: Arc::new(AtomicBool::new(false)),
            ws_clients_count: Arc::new(AtomicUsize::new(0)),
            active_port: Arc::new(AtomicU16::new(port)),
            supervisor: Arc::new(Mutex::new(None)),
            cancel_token: Arc::new(Mutex::new(None)),
            start_time: Arc::new(RwLock::new(None)),
            ws_server: Arc::new(Mutex::new(None)),
            background_tasks: Arc::new(Mutex::new(Vec::new())),
            session_sink: Arc::new(Mutex::new(None)),
            session_path: Arc::new(RwLock::new(None)),
            session_error: Arc::new(RwLock::new(None)),
            transition_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    pub fn vad_onset(&self) -> bool {
        self.vad_onset.load(Ordering::SeqCst)
    }

    pub fn ws_clients(&self) -> usize {
        self.ws_clients_count.load(Ordering::SeqCst)
    }

    pub fn ws_port(&self) -> u16 {
        self.active_port.load(Ordering::SeqCst)
    }
}
