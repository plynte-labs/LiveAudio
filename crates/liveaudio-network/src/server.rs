// SPDX-License-Identifier: MIT

use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Notify};
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

use liveaudio_core::acoustic_events::AcousticSpeechEvent;
use liveaudio_core::TranscriptionCue;

use crate::error::NetworkError;
use crate::health::{HealthFileWriter, NetworkHealthReport, NetworkMetrics};
use crate::protocol::{HelloMessage, SubtitleWireMessage, ThemeWireMessage, WsPortWireEvent};
use crate::replay::{
    BacklogPolicy, RetryBuffer, WireReplayBuffer, HIGH_WATER_MARK_BYTES, MAX_RETRY_BUFFER,
    REPLAY_BUFFER_MAX,
};
use crate::security::{
    candidate_ports, is_remote_addr_allowed, validate_origin_headers, WS_PORT_FALLBACK_RANGE,
};
use crate::traits::{NetworkBoxFuture, NetworkServer};

fn is_addr_in_use(err: &std::io::Error) -> bool {
    if err.kind() == ErrorKind::AddrInUse {
        return true;
    }
    #[cfg(windows)]
    if let Some(code) = err.raw_os_error() {
        if code == 10048 {
            return true;
        }
    }
    #[cfg(unix)]
    if let Some(code) = err.raw_os_error() {
        if code == 98 {
            return true;
        }
    }
    false
}

/// Configuration parameters for `WsServer`.
#[derive(Debug, Clone)]
pub struct WsServerConfig {
    pub base_port: u16,
    pub fallback_range: u16,
    pub backlog_policy: BacklogPolicy,
    pub max_live_delay_sec: f32,
    pub high_water_mark_bytes: usize,
    pub max_retry_buffer: usize,
    pub replay_buffer_capacity: usize,
    pub health_file_path: Option<PathBuf>,
    /// Maximum time a TCP peer may take to complete the WebSocket handshake.
    pub handshake_timeout: Duration,
    pub allowed_loopback_ports: Vec<u16>,
    pub rejected_origins: Option<Arc<Mutex<crate::security::RejectedOriginTracker>>>,
    /// Maximum simultaneous TCP connections (WebSocket clients and overlay requests); extras are dropped.
    pub max_connections: usize,
    /// Largest inbound WebSocket message/frame. Clients only receive; their frames are discarded.
    pub max_inbound_message_bytes: usize,
}

impl Default for WsServerConfig {
    fn default() -> Self {
        Self {
            base_port: 8765,
            fallback_range: WS_PORT_FALLBACK_RANGE,
            backlog_policy: BacklogPolicy::Auto,
            max_live_delay_sec: 10.0,
            high_water_mark_bytes: HIGH_WATER_MARK_BYTES,
            max_retry_buffer: MAX_RETRY_BUFFER,
            replay_buffer_capacity: REPLAY_BUFFER_MAX,
            health_file_path: None,
            handshake_timeout: Duration::from_secs(5),
            allowed_loopback_ports: vec![1420],
            rejected_origins: None,
            max_connections: MAX_CONNECTIONS,
            max_inbound_message_bytes: MAX_INBOUND_MESSAGE_BYTES,
        }
    }
}

/// Per-client queues of payloads a full client queue could not take yet, oldest first.
#[derive(Default)]
struct RetryState {
    queues: HashMap<u64, RetryBuffer>,
    /// Drops from queues already released by a disconnect, so `retry_drops` stays cumulative.
    retired_drops: usize,
}

impl RetryState {
    fn publish(&self, metrics: &NetworkMetrics) {
        let buffered: usize = self.queues.values().map(RetryBuffer::len).sum();
        let dropped: usize = self.retired_drops
            + self
                .queues
                .values()
                .map(RetryBuffer::dropped_count)
                .sum::<usize>();
        metrics.retry_buffer_size.store(buffered, Ordering::SeqCst);
        metrics.retry_drops.store(dropped as u64, Ordering::SeqCst);
    }

    fn forget(&mut self, client_id: u64) {
        if let Some(queue) = self.queues.remove(&client_id) {
            self.retired_drops += queue.dropped_count();
        }
    }
}

/// Pause between accept attempts after an error, so a persistent failure cannot spin the loop.
const MAX_CONNECTIONS: usize = 64;
const MAX_INBOUND_MESSAGE_BYTES: usize = 64 * 1024;
const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(100);

/// Decrements the live client count without wrapping below zero (`stop()` resets it).
fn decrement_client_count(metrics: &NetworkMetrics) {
    let _ = metrics
        .client_count
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
            Some(n.saturating_sub(1))
        });
}

pub const EMBEDDED_OBS_OVERLAY_HTML: &str =
    include_str!("../../../assets/subtitulos_obs.html");

fn load_overlay_html() -> std::borrow::Cow<'static, str> {
    // The working directory is attacker-influenced in release builds, so CWD overrides are dev-only.
    if cfg!(debug_assertions) {
        let candidates = [
            PathBuf::from("assets/subtitulos_obs.html"),
            PathBuf::from("../assets/subtitulos_obs.html"),
            PathBuf::from("../../assets/subtitulos_obs.html"),
        ];
        for path in &candidates {
            if let Ok(content) = std::fs::read_to_string(path) {
                return std::borrow::Cow::Owned(content);
            }
        }
    }
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let exe_asset = exe_dir.join("assets").join("subtitulos_obs.html");
            if let Ok(content) = std::fs::read_to_string(exe_asset) {
                return std::borrow::Cow::Owned(content);
            }
        }
    }
    std::borrow::Cow::Borrowed(EMBEDDED_OBS_OVERLAY_HTML)
}

struct WsServerInner {
    config: WsServerConfig,
    bound_port: RwLock<Option<u16>>,
    clients: RwLock<HashMap<u64, mpsc::Sender<String>>>,
    /// Last theme frame (serialized) sent to every new client after its hello.
    theme: Mutex<Option<String>>,
    replay_buffer: Mutex<WireReplayBuffer>,
    pending_replay: Mutex<WireReplayBuffer>,
    replay_notify: Notify,
    replay_delivery_lock: tokio::sync::Mutex<()>,
    retry: Mutex<RetryState>,
    metrics: Arc<NetworkMetrics>,
    next_client_id: AtomicU64,
    start_instant: Instant,
    shutdown_token: CancellationToken,
    accept_task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    replay_task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    health_writer: Option<HealthFileWriter>,
    allowed_loopback_ports: Arc<RwLock<Vec<u16>>>,
    rejected_origins: Arc<Mutex<crate::security::RejectedOriginTracker>>,
}

/// Production WebSocket server delivering real-time subtitles and theme updates to OBS Studio.
#[derive(Clone)]
pub struct WsServer {
    inner: Arc<WsServerInner>,
}

impl Default for WsServer {
    fn default() -> Self {
        Self::new(WsServerConfig::default())
    }
}

impl WsServer {
    pub fn new(config: WsServerConfig) -> Self {
        let health_writer = config.health_file_path.as_ref().map(HealthFileWriter::new);
        let replay_cap = config.replay_buffer_capacity;
        let allowed_loopback_ports = Arc::new(RwLock::new(config.allowed_loopback_ports.clone()));
        let rejected_origins = config
            .rejected_origins
            .clone()
            .unwrap_or_else(|| Arc::new(Mutex::new(crate::security::RejectedOriginTracker::new())));

        let inner = WsServerInner {
            config,
            bound_port: RwLock::new(None),
            clients: RwLock::new(HashMap::new()),
            theme: Mutex::new(None),
            replay_buffer: Mutex::new(WireReplayBuffer::new(replay_cap)),
            pending_replay: Mutex::new(WireReplayBuffer::new(replay_cap)),
            replay_notify: Notify::new(),
            replay_delivery_lock: tokio::sync::Mutex::new(()),
            retry: Mutex::new(RetryState::default()),
            metrics: Arc::new(NetworkMetrics::new()),
            next_client_id: AtomicU64::new(1),
            start_instant: Instant::now(),
            shutdown_token: CancellationToken::new(),
            accept_task: tokio::sync::Mutex::new(None),
            replay_task: tokio::sync::Mutex::new(None),
            health_writer,
            allowed_loopback_ports,
            rejected_origins,
        };

        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn config(&self) -> &WsServerConfig {
        &self.inner.config
    }

    pub fn update_allowed_loopback_ports(&self, ports: Vec<u16>) {
        let mut guard = self
            .inner
            .allowed_loopback_ports
            .write()
            .unwrap_or_else(|p| p.into_inner());
        *guard = ports;
    }

    pub fn allowed_loopback_ports(&self) -> Vec<u16> {
        self.inner
            .allowed_loopback_ports
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    pub fn rejected_origins(&self) -> Arc<Mutex<crate::security::RejectedOriginTracker>> {
        Arc::clone(&self.inner.rejected_origins)
    }

    pub fn metrics(&self) -> &Arc<NetworkMetrics> {
        &self.inner.metrics
    }

    pub fn health_report(&self) -> NetworkHealthReport {
        let port = self.bound_port();
        self.inner.metrics.snapshot(port, self.inner.start_instant)
    }

    pub fn write_health_file(&self) -> std::io::Result<()> {
        if let Some(ref writer) = self.inner.health_writer {
            let report = self.health_report();
            writer.write_atomic(&report)?;
        }
        Ok(())
    }

    /// Number of delayed cues awaiting paced delivery to current clients.
    pub fn pending_replay_len(&self) -> usize {
        self.inner
            .pending_replay
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }

    /// Broadcast a pre-built subtitle wire message.
    pub fn broadcast_subtitle(&self, msg: &SubtitleWireMessage) -> Result<(), NetworkError> {
        if msg.is_replay && msg.catchup_interval_sec > 0.0 {
            let mut pending = self
                .inner
                .pending_replay
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            pending.push(msg.clone());
            self.inner
                .metrics
                .replay_buffer_size
                .store(pending.len(), Ordering::SeqCst);
            self.inner
                .metrics
                .replay_drops
                .store(pending.dropped_count() as u64, Ordering::SeqCst);
            drop(pending);
            self.inner.replay_notify.notify_one();
            return Ok(());
        }

        self.remember_replay(msg);
        let json = serde_json::to_string(msg)?;
        self.broadcast_raw(&json)
    }

    fn remember_replay(&self, msg: &SubtitleWireMessage) {
        let mut replay = self
            .inner
            .replay_buffer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        replay.push(msg.clone());
        self.inner
            .metrics
            .replay_buffer_size
            .store(replay.len(), Ordering::SeqCst);
        self.inner
            .metrics
            .replay_drops
            .store(replay.dropped_count() as u64, Ordering::SeqCst);
    }

    /// Store and broadcast a dynamic CSS token theme update. The stored theme is
    /// sent to clients that connect later, right after their hello frame.
    pub fn broadcast_theme(&self, tokens: HashMap<String, String>) -> Result<(), NetworkError> {
        let msg = ThemeWireMessage::new(tokens);
        let json = serde_json::to_string(&msg)?;
        // Stored under the clients lock that late joiners hold while they snapshot the theme
        // and register, so a joiner either sees this theme in its snapshot or receives it live.
        {
            let _clients = self
                .inner
                .clients
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *self
                .inner
                .theme
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(json.clone());
        }
        self.broadcast_raw(&json)
    }

    /// Publish one transient acoustic event to clients connected at this boundary.
    /// Unlike subtitles, events are never replayed or retried for later clients.
    pub fn broadcast_acoustic_event(
        &self,
        event: &AcousticSpeechEvent,
    ) -> Result<(), NetworkError> {
        self.broadcast_live_only(serde_json::to_string(event)?)
    }

    /// Publish one provisional subtitle for an utterance still being spoken. The frame has no
    /// `text` key, so overlays that predate it ignore it. Live only: never replayed or retried.
    pub fn broadcast_subtitle_partial(
        &self,
        utterance_id: &str,
        partial_text: &str,
    ) -> Result<(), NetworkError> {
        self.broadcast_live_only(serde_json::to_string(&serde_json::json!({
            "type": "subtitle_partial",
            "utterance_id": utterance_id,
            "partial_text": partial_text,
        }))?)
    }

    fn broadcast_live_only(&self, payload: String) -> Result<(), NetworkError> {
        let clients = self
            .inner
            .clients
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for tx in clients.values() {
            if matches!(
                tx.try_send(payload.clone()),
                Err(mpsc::error::TrySendError::Full(_))
            ) {
                self.inner
                    .metrics
                    .backpressure_events
                    .fetch_add(1, Ordering::SeqCst);
            }
        }
        self.inner
            .metrics
            .broadcast_messages_total
            .fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    /// Broadcast raw JSON text to all connected clients, applying retry buffering on backpressure.
    ///
    /// Each client has its own retry queue. A client that cannot take a payload keeps it there
    /// and receives it, in order, before anything newer. Healthy clients are never held back by
    /// a slow one, and no client receives the same payload twice.
    pub fn broadcast_raw(&self, payload: &str) -> Result<(), NetworkError> {
        let clients = self
            .inner
            .clients
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut state = self
            .inner
            .retry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let max_queue = self.inner.config.max_retry_buffer;
        let mut backpressure = 0u64;

        for (id, tx) in clients.iter() {
            let queue = state
                .queues
                .entry(*id)
                .or_insert_with(|| RetryBuffer::new(max_queue));

            while let Some(next) = queue.front().cloned() {
                match tx.try_send(next) {
                    Ok(()) => {
                        queue.pop_front();
                    }
                    Err(_) => break,
                }
            }

            let delivered = queue.is_empty() && tx.try_send(payload.to_string()).is_ok();
            if !delivered {
                queue.push(payload.to_string());
                backpressure += 1;
            }
        }

        if backpressure > 0 {
            self.inner
                .metrics
                .backpressure_events
                .fetch_add(backpressure, Ordering::SeqCst);
        }
        state.publish(&self.inner.metrics);
        drop(state);
        drop(clients);

        self.inner
            .metrics
            .broadcast_messages_total
            .fetch_add(1, Ordering::SeqCst);

        let _ = self.write_health_file();
        Ok(())
    }
}

/// Binds `server` only when OBS is enabled; a disabled server stays unbound and returns `None`.
/// The unbound server still accepts broadcasts (into its replay buffer) so transcription keeps working.
pub async fn start_if_enabled(
    server: &mut WsServer,
    obs_enabled: bool,
    base_port: u16,
) -> Result<Option<u16>, NetworkError> {
    if !obs_enabled {
        return Ok(None);
    }
    NetworkServer::start(server, base_port).await.map(Some)
}

impl NetworkServer for WsServer {
    fn start(&mut self, base_port: u16) -> NetworkBoxFuture<'_, Result<u16, NetworkError>> {
        Box::pin(async move {
            if let Some(port) = self.bound_port() {
                return Err(NetworkError::AlreadyRunning(port));
            }

            let candidates = candidate_ports(base_port, Some(self.inner.config.fallback_range));
            let mut bound_listener: Option<(TcpListener, u16)> = None;

            for candidate in candidates {
                match TcpListener::bind(("127.0.0.1", candidate)).await {
                    Ok(listener) => {
                        let actual_port = listener
                            .local_addr()
                            .map(|a| a.port())
                            .unwrap_or(candidate);
                        bound_listener = Some((listener, actual_port));
                        break;
                    }
                    Err(e) if is_addr_in_use(&e) => {
                        tracing::warn!(
                            "[WebSocket] Puerto {} ocupado; intentando siguiente puerto de respaldo",
                            candidate
                        );
                        continue;
                    }
                    Err(e) => {
                        return Err(NetworkError::BindError {
                            port: candidate,
                            reason: e.to_string(),
                        });
                    }
                }
            }

            let (listener, effective_port) = match bound_listener {
                Some(pair) => pair,
                None => {
                    let top = (base_port as u32 + self.inner.config.fallback_range as u32 - 1)
                        .min(65535) as u16;
                    return Err(NetworkError::PortExhaustion {
                        base: base_port,
                        top,
                    });
                }
            };

            {
                let mut p = self
                    .inner
                    .bound_port
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                *p = Some(effective_port);
            }

            if effective_port != base_port {
                tracing::info!(
                    "[WebSocket] Puerto {} ocupado; usando puerto de respaldo {}",
                    base_port,
                    effective_port
                );
            }
            tracing::info!(
                "[WebSocket] Iniciando servidor en ws://127.0.0.1:{}",
                effective_port
            );

            // Announce ws_port discovery event
            let ws_port_event = WsPortWireEvent::new(effective_port, base_port);
            let _ = serde_json::to_string(&ws_port_event);

            let inner_clone = Arc::clone(&self.inner);
            let accept_task = tokio::spawn(async move {
                run_accept_loop(listener, effective_port, inner_clone).await;
            });
            *self.inner.accept_task.lock().await = Some(accept_task);
            let replay_inner = Arc::clone(&self.inner);
            let replay_task = tokio::spawn(async move {
                run_replay_loop(replay_inner).await;
            });
            *self.inner.replay_task.lock().await = Some(replay_task);

            let _ = self.write_health_file();
            Ok(effective_port)
        })
    }

    fn stop(&mut self) -> NetworkBoxFuture<'_, Result<(), NetworkError>> {
        Box::pin(async move {
            if self.bound_port().is_none() {
                return Err(NetworkError::NotRunning);
            }

            self.inner.shutdown_token.cancel();
            self.inner.replay_notify.notify_waiters();
            self.inner
                .pending_replay
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clear();
            self.inner
                .metrics
                .replay_buffer_size
                .store(0, Ordering::SeqCst);

            // Clear bound port
            {
                let mut p = self
                    .inner
                    .bound_port
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                *p = None;
            }

            // Close all clients
            {
                let mut clients = self
                    .inner
                    .clients
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                clients.clear();
            }

            self.inner.metrics.client_count.store(0, Ordering::SeqCst);
            {
                let mut state = self
                    .inner
                    .retry
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state.queues.clear();
                state.publish(&self.inner.metrics);
            }
            if let Some(task) = self.inner.accept_task.lock().await.take() {
                task.await.map_err(|error| {
                    NetworkError::IoError(std::io::Error::other(error.to_string()))
                })?;
            }
            if let Some(task) = self.inner.replay_task.lock().await.take() {
                task.await.map_err(|error| {
                    NetworkError::IoError(std::io::Error::other(error.to_string()))
                })?;
            }
            let _ = self.write_health_file();
            tracing::info!("[WebSocket] Servidor detenido");
            Ok(())
        })
    }

    fn broadcast_cue(&self, cue: &TranscriptionCue) -> Result<(), NetworkError> {
        let wire = SubtitleWireMessage::from_cue(cue, None, false, 0.0);
        self.broadcast_subtitle(&wire)
    }

    fn client_count(&self) -> usize {
        self.inner.metrics.client_count.load(Ordering::SeqCst)
    }

    fn bound_port(&self) -> Option<u16> {
        *self
            .inner
            .bound_port
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

async fn run_replay_loop(inner: Arc<WsServerInner>) {
    let mut next_replay_at = tokio::time::Instant::now();
    loop {
        if inner
            .pending_replay
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_empty()
        {
            tokio::select! {
                _ = inner.shutdown_token.cancelled() => break,
                _ = inner.replay_notify.notified() => continue,
            }
        }

        tokio::select! {
            _ = inner.shutdown_token.cancelled() => break,
            _ = tokio::time::sleep_until(next_replay_at) => {}
        }

        let _delivery = inner.replay_delivery_lock.lock().await;
        let msg = inner
            .pending_replay
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop_front();
        let Some(msg) = msg else { continue };
        if let Ok(payload) = serde_json::to_string(&msg) {
            let mut replay = inner
                .replay_buffer
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            replay.push(msg.clone());
            inner
                .metrics
                .replay_buffer_size
                .store(replay.len(), Ordering::SeqCst);
            inner
                .metrics
                .replay_drops
                .store(replay.dropped_count() as u64, Ordering::SeqCst);
            drop(replay);
            let server = WsServer {
                inner: Arc::clone(&inner),
            };
            let _ = server.broadcast_raw(&payload);
        }
        next_replay_at = tokio::time::Instant::now()
            + Duration::from_secs_f64(msg.catchup_interval_sec.max(0.0));
    }
}

async fn run_accept_loop(listener: TcpListener, effective_port: u16, inner: Arc<WsServerInner>) {
    let permits = Arc::new(tokio::sync::Semaphore::new(inner.config.max_connections));
    loop {
        tokio::select! {
            _ = inner.shutdown_token.cancelled() => {
                break;
            }
            res = listener.accept() => {
                match res {
                    Ok((stream, peer_addr)) => {
                        let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
                            inner.metrics.rejected_clients.fetch_add(1, Ordering::SeqCst);
                            tracing::warn!("[WebSocket] Connection cap reached; dropping {}", peer_addr.ip());
                            continue;
                        };
                        let inner_client = Arc::clone(&inner);
                        tokio::spawn(async move {
                            let _permit = permit;
                            handle_connection(stream, peer_addr, effective_port, inner_client).await;
                        });
                    }
                    Err(err) => {
                        tracing::error!("[WebSocket] Error en listener.accept: {}", err);
                        tokio::select! {
                            _ = inner.shutdown_token.cancelled() => break,
                            _ = tokio::time::sleep(ACCEPT_ERROR_BACKOFF) => {}
                        }
                    }
                }
            }
        }
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    effective_port: u16,
    inner: Arc<WsServerInner>,
) {
    // 1. Remote peer IP loopback check
    if !is_remote_addr_allowed(&peer_addr) {
        inner
            .metrics
            .rejected_clients
            .fetch_add(1, Ordering::SeqCst);
        tracing::warn!("[WebSocket] Conexion rechazada de {}", peer_addr.ip());
        return;
    }

    // 1.5. Peek incoming bytes for loopback HTTP overlay requests
    let mut peek_buf = [0u8; 1024];
    let peek_timeout = inner.config.handshake_timeout.min(Duration::from_secs(2));
    let peek_res = tokio::time::timeout(peek_timeout, stream.peek(&mut peek_buf)).await;
    if let Ok(Ok(n)) = peek_res {
        if n > 0 {
            let peek_text = String::from_utf8_lossy(&peek_buf[..n]);
            let lower = peek_text.to_ascii_lowercase();
            let is_websocket_upgrade = lower.lines().any(|line| {
                let trimmed = line.trim();
                if let Some(val) = trimmed.strip_prefix("upgrade:") {
                    val.contains("websocket")
                } else {
                    false
                }
            });

            if !is_websocket_upgrade {
                let first_line = peek_text.lines().next().unwrap_or("").trim();
                let mut parts = first_line.split_whitespace();
                let method = parts.next().unwrap_or("");
                let target = parts.next().unwrap_or("");
                let path = target.split('?').next().unwrap_or(target);

                if (method == "GET" || method == "HEAD")
                    && matches!(
                        path,
                        "/overlay"
                            | "/overlay/"
                            | "/subtitulos_obs.html"
                            | "/assets/subtitulos_obs.html"
                    )
                {
                    // Read/drain the HTTP request from stream until \r\n\r\n (bounded to max 8192 bytes)
                    let mut drain_buf = [0u8; 1024];
                    let mut total_drained = 0;
                    let mut header_end = false;
                    let mut window = [0u8; 4];

                    while total_drained < 8192 {
                        let to_read = (8192 - total_drained).min(drain_buf.len());
                        let read_res = tokio::time::timeout(
                            Duration::from_secs(2),
                            stream.read(&mut drain_buf[..to_read]),
                        )
                        .await;
                        match read_res {
                            Ok(Ok(0)) | Ok(Err(_)) | Err(_) => break,
                            Ok(Ok(read_bytes)) => {
                                total_drained += read_bytes;
                                for &b in &drain_buf[..read_bytes] {
                                    window[0] = window[1];
                                    window[1] = window[2];
                                    window[2] = window[3];
                                    window[3] = b;
                                    if &window == b"\r\n\r\n"
                                        || (window[2] == b'\n' && window[3] == b'\n')
                                    {
                                        header_end = true;
                                        break;
                                    }
                                }
                                if header_end {
                                    break;
                                }
                            }
                        }
                    }

                    let body = load_overlay_html();
                    let body_bytes = body.as_bytes();
                    let content_length = body_bytes.len();
                    let response_header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
                        content_length
                    );

                    if stream.write_all(response_header.as_bytes()).await.is_ok() {
                        if method == "GET" {
                            let _ = stream.write_all(body_bytes).await;
                        }
                        let _ = stream.flush().await;
                    }
                    return;
                }
            }
        }
    }

    // 2. Handshake with strict Origin filter
    #[allow(clippy::result_large_err)]
    let callback = |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
        let origin_values = req.headers().get_all("Origin");
        let origins: Vec<&str> = origin_values.iter().filter_map(|v| v.to_str().ok()).collect();
        let allowed_ports = inner
            .allowed_loopback_ports
            .read()
            .unwrap_or_else(|p| p.into_inner());
        // A value that is not visible ASCII cannot be checked: reject it instead of dropping it.
        let verdict = if origins.len() == origin_values.iter().count() {
            validate_origin_headers(&origins, effective_port, &allowed_ports)
        } else {
            Err(NetworkError::ForbiddenOrigin("non-ASCII Origin header".to_string()))
        };

        if let Err(err) = verdict {
            let received = origins.join(", ");
            tracing::warn!(
                "[WebSocket] Conexion rechazada por Origin no permitido: {}",
                received
            );
            if let Ok(mut tracker) = inner.rejected_origins.lock() {
                tracker.record(&received);
            }
            let err_body = Some(format!("Origen no permitido: {}\n", err));
            let err_resp = Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(err_body.clone())
                .unwrap_or_else(|_| {
                    let mut fallback =
                        tokio_tungstenite::tungstenite::http::Response::new(err_body);
                    *fallback.status_mut() = StatusCode::FORBIDDEN;
                    fallback
                });
            return Err(err_resp);
        }
        Ok(resp)
    };

    let limit = Some(inner.config.max_inbound_message_bytes);
    let ws_config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(limit)
        .max_frame_size(limit);
    let handshake = tokio::time::timeout(
        inner.config.handshake_timeout,
        tokio_tungstenite::accept_hdr_async_with_config(stream, callback, Some(ws_config)),
    )
    .await;
    let ws_stream = match handshake {
        Ok(Ok(ws)) => ws,
        Ok(Err(e)) => {
            inner
                .metrics
                .rejected_clients
                .fetch_add(1, Ordering::SeqCst);
            tracing::debug!("[WebSocket] Handshake rechazado o fallido: {}", e);
            return;
        }
        Err(_) => {
            inner
                .metrics
                .rejected_clients
                .fetch_add(1, Ordering::SeqCst);
            tracing::debug!("[WebSocket] Handshake sin completar; cerrando conexion");
            return;
        }
    };

    // 3. Accepted client setup
    let client_id = inner.next_client_id.fetch_add(1, Ordering::SeqCst);
    inner
        .metrics
        .total_accepted_clients
        .fetch_add(1, Ordering::SeqCst);
    inner.metrics.client_count.fetch_add(1, Ordering::SeqCst);
    let count = inner.metrics.client_count.load(Ordering::SeqCst);

    tracing::info!(
        "[WebSocket] Cliente conectado: {} (total: {})",
        peer_addr,
        count
    );

    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    // 4. Send handshake hello frame immediately
    let hello = HelloMessage::new(effective_port);
    if let Ok(hello_json) = serde_json::to_string(&hello) {
        if let Err(e) = ws_sink.send(Message::text(hello_json)).await {
            tracing::warn!("[WebSocket] Error enviando hello frame: {}", e);
            decrement_client_count(&inner.metrics);
            return;
        }
    }

    // 5. Late-joining client replay burst
    let (replayed_cues, mut rx, theme) = {
        let _delivery = inner.replay_delivery_lock.lock().await;
        let replay = inner
            .replay_buffer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let cues =
            replay.select_for_client(inner.config.backlog_policy, inner.config.max_live_delay_sec);
        let (tx, rx) = mpsc::channel::<String>(64);
        let mut clients = inner
            .clients
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let theme = inner
            .theme
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        clients.insert(client_id, tx);
        drop(clients);
        (cues, rx, theme)
    };

    // The theme goes before any replayed cue; live frames queue in `rx` behind it.
    if let Some(theme_json) = theme {
        if let Err(e) = ws_sink.send(Message::text(theme_json)).await {
            tracing::warn!("[WebSocket] Error enviando theme inicial: {}", e);
        }
    }

    let mut next_replay_at = tokio::time::Instant::now();
    for mut cue_msg in replayed_cues {
        // History re-sent to a new connection is a replay even if it first went out live, so
        // consumers that open a socket per session (OpenCohost PTT) can drop it.
        cue_msg.is_replay = true;
        tokio::select! {
            _ = inner.shutdown_token.cancelled() => break,
            _ = tokio::time::sleep_until(next_replay_at) => {}
        }
        if let Ok(cue_json) = serde_json::to_string(&cue_msg) {
            if ws_sink.send(Message::text(cue_json)).await.is_err() {
                break;
            }
        }
        next_replay_at = tokio::time::Instant::now()
            + Duration::from_secs_f64(cue_msg.catchup_interval_sec.max(0.0));
    }

    if inner.shutdown_token.is_cancelled() {
        let _ = ws_sink
            .send(Message::Close(Some(CloseFrame {
                code: CloseCode::Normal,
                reason: "Server shutdown".into(),
            })))
            .await;
        inner
            .clients
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&client_id);
        inner
            .retry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .forget(client_id);
        decrement_client_count(&inner.metrics);
        return;
    }

    let _ = inner.health_writer.as_ref().map(|w| {
        let rep = inner
            .metrics
            .snapshot(Some(effective_port), inner.start_instant);
        let _ = w.write_atomic(&rep);
    });

    // 7. Message pump and connection maintenance loop
    let shutdown = inner.shutdown_token.clone();
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => {
                let _ = ws_sink.send(Message::Close(Some(CloseFrame {
                    code: CloseCode::Normal,
                    reason: "Server shutdown".into(),
                }))).await;
                break;
            }
            Some(outgoing) = rx.recv() => {
                if let Err(e) = ws_sink.send(Message::text(outgoing)).await {
                    tracing::debug!("[WebSocket] Error enviando a cliente {}: {}", client_id, e);
                    break;
                }
            }
            incoming = ws_stream.next() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => {
                        break;
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if ws_sink.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(_)) => {
                        // OBS browser source is receive-only; client inbound data safely ignored
                    }
                    Some(Err(_)) => {
                        break;
                    }
                }
            }
        }
    }

    // 8. Cleanup upon disconnect
    {
        let mut clients = inner
            .clients
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        clients.remove(&client_id);
    }
    {
        let mut state = inner
            .retry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.forget(client_id);
        state.publish(&inner.metrics);
    }
    decrement_client_count(&inner.metrics);
    let remaining = inner.metrics.client_count.load(Ordering::SeqCst);
    tracing::info!(
        "[WebSocket] Cliente desconectado: {} (restantes: {})",
        peer_addr,
        remaining
    );

    let _ = inner.health_writer.as_ref().map(|w| {
        let rep = inner
            .metrics
            .snapshot(Some(effective_port), inner.start_instant);
        let _ = w.write_atomic(&rep);
    });
}

#[cfg(test)]
mod broadcast_order_tests {
    use super::*;

    fn drain(rx: &mut mpsc::Receiver<String>) -> Vec<String> {
        let mut out = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            out.push(msg);
        }
        out
    }

    #[test]
    fn a_backpressured_client_does_not_make_healthy_clients_receive_duplicates_or_reordered_cues() {
        let server = WsServer::default();
        let (slow_tx, mut slow_rx) = mpsc::channel(2);
        slow_tx.try_send("filler".to_string()).unwrap();
        slow_tx.try_send("filler".to_string()).unwrap();
        let (healthy_tx, mut healthy_rx) = mpsc::channel(64);
        {
            let mut clients = server.inner.clients.write().unwrap();
            clients.insert(1, slow_tx);
            clients.insert(2, healthy_tx);
        }

        server.broadcast_raw("p1").unwrap();
        assert_eq!(drain(&mut healthy_rx), vec!["p1"]);
        assert_eq!(
            drain(&mut slow_rx),
            vec!["filler", "filler"],
            "the full queue got nothing new"
        );
        assert_eq!(
            server
                .inner
                .metrics
                .retry_buffer_size
                .load(Ordering::SeqCst),
            1
        );

        server.broadcast_raw("p2").unwrap();

        // The slow client gets its missed payload first, then the live one, each exactly once.
        assert_eq!(drain(&mut slow_rx), vec!["p1", "p2"]);
        // The healthy client only gets the new payload: it already had p1.
        assert_eq!(drain(&mut healthy_rx), vec!["p2"]);
        assert_eq!(
            server
                .inner
                .metrics
                .retry_buffer_size
                .load(Ordering::SeqCst),
            0
        );
    }
}

#[cfg(test)]
mod acoustic_event_tests {
    use super::*;

    #[test]
    fn a_full_client_queue_drops_acoustic_events_without_retry_or_replay() {
        let server = WsServer::default();
        let (tx, mut rx) = mpsc::channel(1);
        tx.try_send("already queued".to_string()).unwrap();
        server.inner.clients.write().unwrap().insert(1, tx);
        let event = AcousticSpeechEvent::SpeechActivityStart {
            event_version: 1,
            run_id: "run".into(),
            attempt_id: 1,
            utterance_id: "utterance".into(),
            seq: 2,
            monotonic_ms: 0,
        };

        server.broadcast_acoustic_event(&event).unwrap();
        assert_eq!(rx.try_recv().unwrap(), "already queued");

        let next_event = AcousticSpeechEvent::SpeechActivityStart {
            event_version: 1,
            run_id: "run".into(),
            attempt_id: 1,
            utterance_id: "utterance".into(),
            seq: 3,
            monotonic_ms: 0,
        };
        server.broadcast_acoustic_event(&next_event).unwrap();
        let delivered: serde_json::Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(delivered["seq"], 3);

        assert_eq!(
            server
                .inner
                .metrics
                .backpressure_events
                .load(Ordering::SeqCst),
            1
        );
        assert_eq!(
            server
                .inner
                .metrics
                .retry_buffer_size
                .load(Ordering::SeqCst),
            0
        );
        assert_eq!(server.pending_replay_len(), 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_recovers_cleanly_from_poisoned_locks() {
        let server = WsServer::default();

        let inner_replay = Arc::clone(&server.inner);
        let _ = std::thread::spawn(move || {
            let _guard = inner_replay.replay_buffer.lock().unwrap();
            panic!("intentional panic to poison replay_buffer");
        })
        .join();
        assert!(server.inner.replay_buffer.is_poisoned());

        let inner_clients = Arc::clone(&server.inner);
        let _ = std::thread::spawn(move || {
            let _guard = inner_clients.clients.write().unwrap();
            panic!("intentional panic to poison clients");
        })
        .join();
        assert!(server.inner.clients.is_poisoned());

        let inner_pending = Arc::clone(&server.inner);
        let _ = std::thread::spawn(move || {
            let _guard = inner_pending.pending_replay.lock().unwrap();
            panic!("intentional panic to poison pending_replay");
        })
        .join();
        assert!(server.inner.pending_replay.is_poisoned());

        let inner_retry = Arc::clone(&server.inner);
        let _ = std::thread::spawn(move || {
            let _guard = inner_retry.retry.lock().unwrap();
            panic!("intentional panic to poison retry");
        })
        .join();
        assert!(server.inner.retry.is_poisoned());

        let inner_port = Arc::clone(&server.inner);
        let _ = std::thread::spawn(move || {
            let _guard = inner_port.bound_port.write().unwrap();
            panic!("intentional panic to poison bound_port");
        })
        .join();
        assert!(server.inner.bound_port.is_poisoned());

        assert_eq!(server.pending_replay_len(), 0);
        assert_eq!(server.bound_port(), None);
        assert!(server.broadcast_raw("test payload").is_ok());

        let dummy_asr = liveaudio_core::AsrTranscriptionResponse {
            request_id: "req-1".to_string(),
            text: "test text".to_string(),
            language: "es".to_string(),
            duration_sec: 1.0,
            latency_sec: 0.1,
            segments: vec![],
        };
        let cue = TranscriptionCue::new(
            0,
            0.0,
            1.0,
            "test text".to_string(),
            dummy_asr,
        );
        let msg = SubtitleWireMessage::from_cue(&cue, None, false, 0.0);
        assert!(server.broadcast_subtitle(&msg).is_ok());

        let event = AcousticSpeechEvent::SpeechActivityStart {
            event_version: 1,
            run_id: "run".into(),
            attempt_id: 1,
            utterance_id: "utterance".into(),
            seq: 1,
            monotonic_ms: 0,
        };
        assert!(server.broadcast_acoustic_event(&event).is_ok());
    }
}

#[cfg(test)]
mod http_overlay_tests {
    use super::*;

    #[tokio::test]
    async fn loopback_http_overlay_serving_and_ws_origin_security() {
        let mut server = WsServer::default();
        let effective_port = server
            .start(0)
            .await
            .expect("Server should bind to ephemeral port 0");
        assert!(effective_port > 0);
        assert_eq!(server.bound_port(), Some(effective_port));

        // 1. Raw TCP GET /overlay?port=8765
        let mut http_client = TcpStream::connect(("127.0.0.1", effective_port))
            .await
            .expect("Failed to connect HTTP client to server");
        let http_request =
            "GET /overlay?port=8765 HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
        http_client
            .write_all(http_request.as_bytes())
            .await
            .expect("Failed to write HTTP request");

        let mut http_response = Vec::new();
        http_client
            .read_to_end(&mut http_response)
            .await
            .expect("Failed to read HTTP response");
        let http_resp_str = String::from_utf8_lossy(&http_response);

        assert!(
            http_resp_str.starts_with("HTTP/1.1 200 OK"),
            "Expected HTTP 200 OK, got: {}",
            http_resp_str
        );
        assert!(
            http_resp_str.contains("Content-Type: text/html; charset=utf-8"),
            "Missing HTML content type header"
        );
        assert!(
            http_resp_str.contains("WebSocket"),
            "HTML body should contain 'WebSocket'"
        );

        // 2. WebSocket client with allowed Origin: http://127.0.0.1:<port>
        let mut ws_client = TcpStream::connect(("127.0.0.1", effective_port))
            .await
            .expect("Failed to connect WS client to server");
        let ws_handshake = format!(
            "GET / HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nOrigin: http://127.0.0.1:{}\r\n\r\n",
            effective_port, effective_port
        );
        ws_client
            .write_all(ws_handshake.as_bytes())
            .await
            .expect("Failed to write WS handshake");

        let mut ws_buf = [0u8; 1024];
        let n = ws_client
            .read(&mut ws_buf)
            .await
            .expect("Failed to read WS handshake response");
        let ws_resp_str = String::from_utf8_lossy(&ws_buf[..n]);
        assert!(
            ws_resp_str.starts_with("HTTP/1.1 101 Switching Protocols"),
            "Expected 101 Switching Protocols for loopback Origin, got: {}",
            ws_resp_str
        );

        // 3. WebSocket client with forbidden Origin: null
        let mut null_ws_client = TcpStream::connect(("127.0.0.1", effective_port))
            .await
            .expect("Failed to connect null Origin WS client");
        let null_handshake = format!(
            "GET / HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nOrigin: null\r\n\r\n",
            effective_port
        );
        null_ws_client
            .write_all(null_handshake.as_bytes())
            .await
            .expect("Failed to write null Origin WS handshake");

        let mut null_buf = [0u8; 1024];
        let n = null_ws_client
            .read(&mut null_buf)
            .await
            .expect("Failed to read null Origin response");
        let null_resp_str = String::from_utf8_lossy(&null_buf[..n]);
        assert!(
            null_resp_str.starts_with("HTTP/1.1 403 Forbidden"),
            "Expected 403 Forbidden for Origin: null, got: {}",
            null_resp_str
        );

        server.stop().await.expect("Server should stop cleanly");
    }

    /// Sends a raw WebSocket handshake whose Origin header is `origin` and returns the status line.
    async fn handshake_status_line(port: u16, origin: &[u8]) -> String {
        let mut client = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("Failed to connect WS client to server");
        let mut request = format!(
            "GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nOrigin: "
        )
        .into_bytes();
        request.extend_from_slice(origin);
        request.extend_from_slice(b"\r\n\r\n");
        client
            .write_all(&request)
            .await
            .expect("Failed to write WS handshake");
        let mut buf = [0u8; 1024];
        let n = client
            .read(&mut buf)
            .await
            .expect("Failed to read WS handshake response");
        String::from_utf8_lossy(&buf[..n])
            .lines()
            .next()
            .unwrap_or_default()
            .to_string()
    }

    #[tokio::test]
    async fn loopback_origin_on_another_port_is_forbidden() {
        let mut server = WsServer::default();
        let port = server
            .start(0)
            .await
            .expect("Server should bind to ephemeral port 0");
        let other_port = if port == u16::MAX { port - 1 } else { port + 1 };

        let other = handshake_status_line(port, format!("http://127.0.0.1:{other_port}").as_bytes()).await;
        assert!(
            other.starts_with("HTTP/1.1 403 Forbidden"),
            "Loopback Origin on another port must be forbidden, got: {other}"
        );

        let bound = handshake_status_line(port, format!("http://127.0.0.1:{port}").as_bytes()).await;
        assert!(
            bound.starts_with("HTTP/1.1 101"),
            "Loopback Origin on the bound port must upgrade, got: {bound}"
        );

        server.stop().await.expect("Server should stop cleanly");
    }

    #[tokio::test]
    async fn non_ascii_origin_header_fails_closed() {
        let mut server = WsServer::default();
        let port = server
            .start(0)
            .await
            .expect("Server should bind to ephemeral port 0");

        let mut origin = format!("http://127.0.0.1:{port}").into_bytes();
        origin.extend_from_slice("\u{e9}".as_bytes());
        let status = handshake_status_line(port, &origin).await;
        assert!(
            status.starts_with("HTTP/1.1 403 Forbidden"),
            "Non-ASCII Origin header must be rejected, got: {status}"
        );

        server.stop().await.expect("Server should stop cleanly");
    }
}
