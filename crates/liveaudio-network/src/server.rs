// SPDX-License-Identifier: MIT

use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Notify};
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

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
        }
    }
}

struct WsServerInner {
    config: WsServerConfig,
    bound_port: RwLock<Option<u16>>,
    clients: RwLock<HashMap<u64, mpsc::Sender<String>>>,
    replay_buffer: Mutex<WireReplayBuffer>,
    pending_replay: Mutex<WireReplayBuffer>,
    replay_notify: Notify,
    replay_delivery_lock: tokio::sync::Mutex<()>,
    retry_buffer: Mutex<RetryBuffer>,
    metrics: Arc<NetworkMetrics>,
    next_client_id: AtomicU64,
    start_instant: Instant,
    shutdown_token: CancellationToken,
    accept_task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    replay_task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    health_writer: Option<HealthFileWriter>,
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
        let retry_cap = config.max_retry_buffer;

        let inner = WsServerInner {
            config,
            bound_port: RwLock::new(None),
            clients: RwLock::new(HashMap::new()),
            replay_buffer: Mutex::new(WireReplayBuffer::new(replay_cap)),
            pending_replay: Mutex::new(WireReplayBuffer::new(replay_cap)),
            replay_notify: Notify::new(),
            replay_delivery_lock: tokio::sync::Mutex::new(()),
            retry_buffer: Mutex::new(RetryBuffer::new(retry_cap)),
            metrics: Arc::new(NetworkMetrics::new()),
            next_client_id: AtomicU64::new(1),
            start_instant: Instant::now(),
            shutdown_token: CancellationToken::new(),
            accept_task: tokio::sync::Mutex::new(None),
            replay_task: tokio::sync::Mutex::new(None),
            health_writer,
        };

        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn config(&self) -> &WsServerConfig {
        &self.inner.config
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
        self.inner.pending_replay.lock().unwrap().len()
    }

    /// Broadcast a pre-built subtitle wire message.
    pub fn broadcast_subtitle(&self, msg: &SubtitleWireMessage) -> Result<(), NetworkError> {
        if msg.is_replay && msg.catchup_interval_sec > 0.0 {
            let mut pending = self.inner.pending_replay.lock().unwrap();
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
        let mut replay = self.inner.replay_buffer.lock().unwrap();
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

    /// Broadcast a dynamic CSS token theme update.
    pub fn broadcast_theme(&self, tokens: HashMap<String, String>) -> Result<(), NetworkError> {
        let msg = ThemeWireMessage::new(tokens);
        let json = serde_json::to_string(&msg)?;
        self.broadcast_raw(&json)
    }

    /// Broadcast raw JSON text to all connected clients, applying retry buffering on backpressure.
    pub fn broadcast_raw(&self, payload: &str) -> Result<(), NetworkError> {
        let clients = self.inner.clients.read().unwrap();
        let mut had_backpressure = false;

        for (_id, tx) in clients.iter() {
            match tx.try_send(payload.to_string()) {
                Ok(_) => {}
                Err(mpsc::error::TrySendError::Full(_)) => {
                    had_backpressure = true;
                    self.inner
                        .metrics
                        .backpressure_events
                        .fetch_add(1, Ordering::SeqCst);
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {}
            }
        }

        if had_backpressure {
            let mut retry = self.inner.retry_buffer.lock().unwrap();
            retry.push(payload.to_string());
            self.inner
                .metrics
                .retry_buffer_size
                .store(retry.len(), Ordering::SeqCst);
            self.inner
                .metrics
                .retry_drops
                .store(retry.dropped_count() as u64, Ordering::SeqCst);
        } else {
            // Attempt to drain pending retry buffer if transport is clear
            let mut retry = self.inner.retry_buffer.lock().unwrap();
            while let Some(buffered) = retry.pop_front() {
                for (_id, tx) in clients.iter() {
                    let _ = tx.try_send(buffered.clone());
                }
            }
            self.inner
                .metrics
                .retry_buffer_size
                .store(0, Ordering::SeqCst);
        }

        self.inner
            .metrics
            .broadcast_messages_total
            .fetch_add(1, Ordering::SeqCst);

        let _ = self.write_health_file();
        Ok(())
    }
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
                        bound_listener = Some((listener, candidate));
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
                let mut p = self.inner.bound_port.write().unwrap();
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
            self.inner.pending_replay.lock().unwrap().clear();
            self.inner
                .metrics
                .replay_buffer_size
                .store(0, Ordering::SeqCst);

            // Clear bound port
            {
                let mut p = self.inner.bound_port.write().unwrap();
                *p = None;
            }

            // Close all clients
            {
                let mut clients = self.inner.clients.write().unwrap();
                clients.clear();
            }

            self.inner.metrics.client_count.store(0, Ordering::SeqCst);
            if let Some(task) = self.inner.accept_task.lock().await.take() {
                task.await.map_err(|error| {
                    NetworkError::IoError(std::io::Error::new(ErrorKind::Other, error.to_string()))
                })?;
            }
            if let Some(task) = self.inner.replay_task.lock().await.take() {
                task.await.map_err(|error| {
                    NetworkError::IoError(std::io::Error::new(ErrorKind::Other, error.to_string()))
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
        *self.inner.bound_port.read().unwrap()
    }
}

async fn run_replay_loop(inner: Arc<WsServerInner>) {
    let mut next_replay_at = tokio::time::Instant::now();
    loop {
        if inner.pending_replay.lock().unwrap().is_empty() {
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
        let msg = inner.pending_replay.lock().unwrap().pop_front();
        let Some(msg) = msg else { continue };
        if let Ok(payload) = serde_json::to_string(&msg) {
            let mut replay = inner.replay_buffer.lock().unwrap();
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
    loop {
        tokio::select! {
            _ = inner.shutdown_token.cancelled() => {
                break;
            }
            res = listener.accept() => {
                match res {
                    Ok((stream, peer_addr)) => {
                        let inner_client = Arc::clone(&inner);
                        tokio::spawn(async move {
                            handle_connection(stream, peer_addr, effective_port, inner_client).await;
                        });
                    }
                    Err(err) => {
                        tracing::error!("[WebSocket] Error en listener.accept: {}", err);
                    }
                }
            }
        }
    }
}

async fn handle_connection(
    stream: TcpStream,
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

    // 2. Handshake with strict Origin filter
    #[allow(clippy::result_large_err)]
    let callback = |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
        let origins: Vec<&str> = req
            .headers()
            .get_all("Origin")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect();

        if let Err(err) = validate_origin_headers(&origins) {
            let received = origins.join(", ");
            tracing::warn!(
                "[WebSocket] Conexion rechazada por Origin no permitido: {}",
                received
            );
            println!(
                "[WebSocket] Conexion rechazada por Origin no permitido: {}",
                received
            );
            let err_resp = Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Some(format!("Origen no permitido: {}\n", err)))
                .unwrap();
            return Err(err_resp);
        }
        Ok(resp)
    };

    let ws_stream = match tokio_tungstenite::accept_hdr_async(stream, callback).await {
        Ok(ws) => ws,
        Err(e) => {
            inner
                .metrics
                .rejected_clients
                .fetch_add(1, Ordering::SeqCst);
            tracing::debug!("[WebSocket] Handshake rechazado o fallido: {}", e);
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
    println!("[WebSocket] Cliente conectado: {}", peer_addr);

    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    // 4. Send handshake hello frame immediately
    let hello = HelloMessage::new(effective_port);
    if let Ok(hello_json) = serde_json::to_string(&hello) {
        if let Err(e) = ws_sink.send(Message::text(hello_json)).await {
            tracing::warn!("[WebSocket] Error enviando hello frame: {}", e);
            inner.metrics.client_count.fetch_sub(1, Ordering::SeqCst);
            return;
        }
    }

    // 5. Late-joining client replay burst
    let (replayed_cues, mut rx) = {
        let _delivery = inner.replay_delivery_lock.lock().await;
        let replay = inner.replay_buffer.lock().unwrap();
        let cues =
            replay.select_for_client(inner.config.backlog_policy, inner.config.max_live_delay_sec);
        let (tx, rx) = mpsc::channel::<String>(64);
        inner.clients.write().unwrap().insert(client_id, tx);
        (cues, rx)
    };

    let mut next_replay_at = tokio::time::Instant::now();
    for cue_msg in replayed_cues {
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
        inner.clients.write().unwrap().remove(&client_id);
        inner.metrics.client_count.fetch_sub(1, Ordering::SeqCst);
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
        let mut clients = inner.clients.write().unwrap();
        clients.remove(&client_id);
    }
    inner.metrics.client_count.fetch_sub(1, Ordering::SeqCst);
    let remaining = inner.metrics.client_count.load(Ordering::SeqCst);
    tracing::info!(
        "[WebSocket] Cliente desconectado: {} (restantes: {})",
        peer_addr,
        remaining
    );
    println!("[WebSocket] Cliente desconectado: {}", peer_addr);

    let _ = inner.health_writer.as_ref().map(|w| {
        let rep = inner
            .metrics
            .snapshot(Some(effective_port), inner.start_instant);
        let _ = w.write_atomic(&rep);
    });
}
