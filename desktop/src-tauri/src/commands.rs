use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};

use liveaudio_audio::traits::AudioSource;
use liveaudio_audio::{AudioStreamConfig, CpalAudioSource};
use liveaudio_core::asr::AsrTranscriptionResponse;
use liveaudio_core::config::{get_data_home, save_config as save_config_core, LiveAudioConfig};
use liveaudio_core::job_object::JobObject;
use liveaudio_core::runtime::preflight_asr_runtime;
use liveaudio_core::sink::{
    CompositeSink, JsonlSink, SinkError, SinkStats, TranscriptionCue, TranscriptionSink, WebVttSink,
};
use liveaudio_core::supervisor::{ProcessSupervisor, WorkerProcessConfig};
use liveaudio_network::{
    candidate_ports, subtitle_from_worker_result, BacklogPolicy, NetworkServer,
    PendingUtteranceTracker, WsServer, WsServerConfig,
};
use liveaudio_vad::{SileroVad, VadConfig, VadEngine, VadTransition};

use crate::dto::{
    AudioDeviceDto, DiagnosticsExportDto, ObsOverlayDto, ProfilePresetDto, ServiceStatusDto,
    SubtitleCueDto,
};
use crate::state::AppState;

/// Retrieve current LiveAudio configuration.
#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<LiveAudioConfig, String> {
    let guard = state.config.read().await;
    Ok(guard.clone())
}

/// Validate, normalize, and atomically save LiveAudio configuration.
#[tauri::command]
pub async fn save_config(
    config: LiveAudioConfig,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let mut normalized_config = config;
    normalized_config.normalize();

    persist_config_and_apply(state.inner(), normalized_config, |config| {
        save_config_core(config).map_err(|e| format!("Failed to save config: {}", e))
    })
    .await?;

    Ok(true)
}

/// Reset configuration to system defaults.
#[tauri::command]
pub async fn reset_config_defaults(state: State<'_, AppState>) -> Result<LiveAudioConfig, String> {
    let mut defaults = LiveAudioConfig::default();
    defaults.normalize();

    persist_config_and_apply(state.inner(), defaults.clone(), |config| {
        save_config_core(config).map_err(|e| format!("Failed to save config: {}", e))
    })
    .await?;

    Ok(defaults)
}

pub(crate) async fn persist_config_and_apply<F>(
    state: &AppState,
    config: LiveAudioConfig,
    persist: F,
) -> Result<(), String>
where
    F: FnOnce(&LiveAudioConfig) -> Result<bool, String>,
{
    if !persist(&config)? {
        return Err("Failed to save config: config lock unavailable".to_string());
    }

    let mut guard = state.config.write().await;
    *guard = config.clone();
    Ok(())
}

#[tauri::command]
pub async fn get_profile_presets() -> Vec<ProfilePresetDto> {
    profile_presets()
}

pub(crate) fn profile_presets() -> Vec<ProfilePresetDto> {
    vec![
        ProfilePresetDto {
            id: "fast".into(),
            label: "Rápido".into(),
            description: "Menos demora y frases cortas; baja un poco la precisión.".into(),
            device: "cpu".into(),
            model_size: "base (Rápido)".into(),
            silence_timeout: 0.4,
            subtitle_backlog_policy: "live_only".into(),
            subtitle_max_live_delay_sec: 5.0,
            subtitle_catchup_interval_sec: 0.8,
        },
        ProfilePresetDto {
            id: "balanced".into(),
            label: "Balanceado".into(),
            description: "Recomendado para la mayoría de sesiones.".into(),
            device: "cuda".into(),
            model_size: "small (Balance CPU)".into(),
            silence_timeout: 0.8,
            subtitle_backlog_policy: "auto".into(),
            subtitle_max_live_delay_sec: 10.0,
            subtitle_catchup_interval_sec: 1.5,
        },
        ProfilePresetDto {
            id: "quality".into(),
            label: "Calidad".into(),
            description: "Más precisión; puede usar más VRAM y tardar más.".into(),
            device: "cuda".into(),
            model_size: "turbo (Máxima precisión GPU)".into(),
            silence_timeout: 1.0,
            subtitle_backlog_policy: "auto".into(),
            subtitle_max_live_delay_sec: 15.0,
            subtitle_catchup_interval_sec: 2.0,
        },
        ProfilePresetDto {
            id: "stable_streaming".into(),
            label: "Streaming estable".into(),
            description: "Reduce carga de GPU para jugar o transmitir en PC ocupada.".into(),
            device: "cpu".into(),
            model_size: "small (Balance CPU)".into(),
            silence_timeout: 0.6,
            subtitle_backlog_policy: "live_only".into(),
            subtitle_max_live_delay_sec: 6.0,
            subtitle_catchup_interval_sec: 1.0,
        },
    ]
}

pub(crate) fn log_transcription_metadata(payload: &serde_json::Value) {
    let duration_sec = payload
        .get("duration_sec")
        .and_then(|value| value.as_f64())
        .unwrap_or(3.0);
    let utterance_id = payload
        .get("utterance_id")
        .and_then(|value| value.as_str())
        .unwrap_or("utt");

    tracing::info!(
        "📝 [ASR Transcripción] [{}] ({:.2}s)",
        utterance_id,
        duration_sec
    );
}

pub(crate) async fn rollback_failed_audio_start(
    state: &AppState,
    event_task: tokio::task::JoinHandle<()>,
    cancel_token: tokio_util::sync::CancellationToken,
    ws_created_here: bool,
    error: String,
) -> String {
    cancel_token.cancel();

    let _ = event_task.await;

    if let Some(token) = state.cancel_token.lock().await.take() {
        token.cancel();
    }
    if let Some(supervisor) = state.supervisor.lock().await.take() {
        match Arc::try_unwrap(supervisor) {
            Ok(mut supervisor) => {
                if let Err(error) = supervisor
                    .stop_internal(std::time::Duration::from_secs(2))
                    .await
                {
                    tracing::warn!(
                        "Failed to stop ASR worker after audio startup failure: {}",
                        error
                    );
                }
            }
            Err(supervisor) => {
                supervisor.cancellation_token().cancel();
                tracing::error!("ASR supervisor remained shared after audio startup failure");
            }
        }
    }
    if ws_created_here {
        if let Some(mut ws_server) = state.ws_server.lock().await.take() {
            if let Err(error) = NetworkServer::stop(&mut ws_server).await {
                tracing::debug!(
                    "New WebSocket server was not running during startup rollback: {}",
                    error
                );
            }
        }
    }
    if let Some(sink) = state.session_sink.lock().await.take() {
        if let Err(error) = sink.stop(std::time::Duration::from_secs(2)).await {
            tracing::warn!(
                "Failed to flush session files after startup failure: {}",
                error
            );
        }
    }
    *state.session_path.write().await = None;
    *state.active_config.write().await = None;

    state.is_running.store(false, Ordering::SeqCst);
    state.vad_onset.store(false, Ordering::SeqCst);
    *state.asr_state.write().await = "stopped".to_string();
    *state.start_time.write().await = None;
    state.active_port.store(0, Ordering::SeqCst);

    error
}

async fn rollback_failed_start_before_audio(
    state: &AppState,
    ws_created_here: bool,
    error: String,
) -> String {
    if ws_created_here {
        if let Some(mut server) = state.ws_server.lock().await.take() {
            if let Err(stop_error) = NetworkServer::stop(&mut server).await {
                tracing::warn!(
                    "Failed to stop WebSocket after startup failure: {}",
                    stop_error
                );
            }
        }
    }
    state.active_port.store(0, Ordering::SeqCst);
    *state.active_config.write().await = None;
    state.is_running.store(false, Ordering::SeqCst);
    *state.asr_state.write().await = "stopped".to_string();
    error
}

pub(crate) async fn stop_active_websocket(state: &AppState) -> Result<(), String> {
    let server = state.ws_server.lock().await.take();
    state.active_port.store(0, Ordering::SeqCst);
    if let Some(mut server) = server {
        NetworkServer::stop(&mut server)
            .await
            .map_err(|error| format!("Failed to stop OBS WebSocket: {}", error))?;
    }
    Ok(())
}

pub(crate) fn log_unknown_worker_event(event_type: &str, _payload: &serde_json::Value) {
    tracing::debug!("🔍 [ASR Worker Event] {}", event_type);
}

pub(crate) fn create_session_sinks(
    config: &LiveAudioConfig,
    previous_path: Option<&Path>,
) -> std::io::Result<(Option<Arc<CompositeSink>>, Option<PathBuf>)> {
    if !(config.save_transcript_enabled || config.save_vtt_enabled) {
        return Ok((None, None));
    }

    std::fs::create_dir_all(&config.output_dir)?;
    let root = std::fs::canonicalize(&config.output_dir)?;
    let reusable = config
        .continuous_session
        .then_some(previous_path)
        .flatten()
        .and_then(|path| std::fs::canonicalize(path).ok())
        .filter(|path| path.starts_with(&root) && path.is_dir());
    let canonical = if let Some(path) = reusable {
        path
    } else {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let path = root.join(format!("session-{}-{}", std::process::id(), stamp));
        std::fs::create_dir(&path)?;
        std::fs::canonicalize(&path)?
    };
    if !canonical.starts_with(&root) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "session directory escaped configured output directory",
        ));
    }

    let mut sinks: Vec<Arc<dyn TranscriptionSink>> = Vec::new();
    if config.save_transcript_enabled {
        sinks.push(Arc::new(JsonlSink::new(
            canonical.join("transcript.jsonl"),
            32,
        )));
    }
    if config.save_vtt_enabled {
        sinks.push(Arc::new(WebVttSink::new(
            canonical.join("subtitles.vtt"),
            32,
        )));
    }
    Ok((Some(Arc::new(CompositeSink::new(sinks))), Some(canonical)))
}

pub(super) fn session_reuse_path<'a>(
    config: &LiveAudioConfig,
    previous_path: Option<&'a Path>,
    is_restart: bool,
) -> Option<&'a Path> {
    (is_restart && config.continuous_session)
        .then_some(previous_path)
        .flatten()
}

pub(super) async fn record_session_stop_failure(
    state: &AppState,
    error: &SinkError,
    stats: &SinkStats,
) -> String {
    let message = session_sink_failure(stats).unwrap_or_else(|| match error {
        SinkError::DrainTimeout => {
            format!(
                "Session output drain timed out with {} pending record(s)",
                stats.pending
            )
        }
        _ => format!("Session output stop failed: {error}"),
    });
    *state.session_error.write().await = Some(message.clone());
    message
}

fn session_sink_failure(stats: &SinkStats) -> Option<String> {
    (stats.failed > 0 || stats.rejected > 0).then(|| {
        format!(
            "Session output failed: {} write(s), {} rejected",
            stats.failed, stats.rejected
        )
    })
}

/// Enumerate audio capture devices (Microphones and WASAPI Loopback).
#[tauri::command]
pub async fn get_audio_devices() -> Result<Vec<AudioDeviceDto>, String> {
    let mut devices = vec![AudioDeviceDto {
        id: "default_microphone".to_string(),
        name: "Micrófono predeterminado del sistema".to_string(),
        is_default: true,
        kind: "input".to_string(),
    }];
    #[cfg(target_os = "windows")]
    devices.push(AudioDeviceDto {
        id: "default_loopback".to_string(),
        name: "Audio del sistema / Altavoces (WASAPI Loopback)".to_string(),
        is_default: false,
        kind: "loopback".to_string(),
    });

    let enumerated_devices = liveaudio_audio::enumerate_cpal_devices()
        .map_err(|error| format!("Failed to enumerate audio devices: {}", error))?;
    for device in enumerated_devices {
        devices.push(AudioDeviceDto {
            id: device.id,
            name: device.name,
            is_default: device.is_default,
            kind: if device.is_loopback {
                "loopback".to_string()
            } else {
                "input".to_string()
            },
        });
    }

    Ok(devices)
}

/// Retrieve current service status and telemetry.
const B64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Fast IEEE-754 32-bit float little-endian Base64 encoder (zero-dependency).
pub fn encode_f32_le_base64(samples: &[f32]) -> String {
    let mut bytes = Vec::with_capacity(samples.len() * 4);
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        let b1 = if i + 1 < bytes.len() { bytes[i + 1] } else { 0 };
        let b2 = if i + 2 < bytes.len() { bytes[i + 2] } else { 0 };
        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        out.push(B64_CHARS[((n >> 18) & 63) as usize] as char);
        out.push(B64_CHARS[((n >> 12) & 63) as usize] as char);
        if i + 1 < bytes.len() {
            out.push(B64_CHARS[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if i + 2 < bytes.len() {
            out.push(B64_CHARS[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
        i += 3;
    }
    out
}

/// Helper to extract internal service status telemetry.
pub async fn get_service_status_internal(state: &AppState) -> Result<ServiceStatusDto, String> {
    let is_running = state.is_running.load(Ordering::SeqCst);
    let asr_state = state.asr_state.read().await.clone();
    let vad_onset = state.vad_onset.load(Ordering::SeqCst);
    let ws_port = if is_running {
        state.active_port.load(Ordering::SeqCst)
    } else {
        state.config.read().await.ws_port
    };
    let ws_clients = {
        let ws_guard = state.ws_server.lock().await;
        if let Some(ref ws) = *ws_guard {
            ws.client_count()
        } else {
            0
        }
    };

    let config_snapshot = if is_running {
        state.active_config.read().await.clone()
    } else {
        None
    };
    let config_fallback = state.config.read().await.clone();
    let config = config_snapshot.as_ref().unwrap_or(&config_fallback);
    let model_size = config.model_size.clone();
    let device = config.device.clone();
    let active_device = config
        .audio_device
        .as_ref()
        .and_then(|v| v.get("name").or_else(|| v.get("id")))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let uptime_sec = {
        let start_guard = state.start_time.read().await;
        start_guard
            .map(|i| i.elapsed().as_secs_f64())
            .unwrap_or(0.0)
    };
    let sink_failure = state
        .session_sink
        .lock()
        .await
        .as_ref()
        .and_then(|sink| session_sink_failure(&sink.stats()));
    if let Some(failure) = &sink_failure {
        *state.session_error.write().await = Some(failure.clone());
    }
    let session_error = match sink_failure {
        Some(failure) => Some(failure),
        None => state.session_error.read().await.clone(),
    };

    Ok(ServiceStatusDto {
        is_running,
        asr_state,
        vad_onset,
        ws_port,
        ws_clients,
        active_device,
        model_size,
        device,
        uptime_sec,
        restart_count: 0,
        session_path: {
            let active = state
                .session_path
                .read()
                .await
                .as_ref()
                .map(|p| p.to_string_lossy().to_string());
            if active.is_some() {
                active
            } else {
                let dir = state.config.read().await.output_dir.clone();
                let _ = std::fs::create_dir_all(&dir);
                Some(dir.to_string_lossy().to_string())
            }
        },
        session_error,
    })
}

/// Retrieve current service status and telemetry.
#[tauri::command]
pub async fn get_service_status(state: State<'_, AppState>) -> Result<ServiceStatusDto, String> {
    get_service_status_internal(state.inner()).await
}

/// Start the LiveAudio transcription service, binding audio capture, Silero VAD, Faster-Whisper worker, and OBS WebSocket.
#[tauri::command]
pub async fn start_service(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ServiceStatusDto, String> {
    let _transition_guard = state.transition_lock.lock().await;
    start_service_locked(&app, state.inner(), false).await
}

async fn start_service_locked(
    app: &AppHandle,
    state: &AppState,
    is_restart: bool,
) -> Result<ServiceStatusDto, String> {
    if state.is_running.load(Ordering::SeqCst) {
        return get_service_status_internal(state).await;
    }

    *state.asr_state.write().await = "starting".to_string();
    if let Ok(starting_status) = get_service_status_internal(state).await {
        let _ = app.emit("service_status_changed", &starting_status);
    }

    let config = {
        let guard = state.config.read().await;
        guard.clone()
    };

    let clean_model_name = config
        .model_size
        .split_whitespace()
        .next()
        .unwrap_or("small")
        .to_string();
    let clean_lang = if !config.asr_language.is_empty() {
        config.asr_language.clone()
    } else {
        config
            .language
            .as_deref()
            .filter(|language| !language.is_empty() && *language != "auto")
            .unwrap_or("es")
            .to_string()
    };
    let prompt = if clean_lang == "en" {
        &config.whisper_context_prompt_en
    } else {
        &config.whisper_context_prompt_es
    };
    let mut worker_config = WorkerProcessConfig::discover_asr_worker();
    worker_config.init_payload = Some(serde_json::json!({
        "model_name": clean_model_name,
        "device": config.device,
        "language": clean_lang,
        "initial_prompt": if prompt.is_empty() { None } else { Some(prompt) },
        "auto_cpu_fallback": true,
    }));
    if let Err(error) = preflight_asr_runtime(
        std::path::Path::new(&worker_config.command),
        worker_config.working_dir.as_deref(),
    )
    .await
    {
        *state.asr_state.write().await = "stopped".to_string();
        if let Ok(status) = get_service_status_internal(state).await {
            let _ = app.emit("service_status_changed", &status);
        }
        return Err(error.to_string());
    }

    *state.session_error.write().await = None;

    // 1. WebSocket Server: ensure started on configured port
    let (ws_server, ws_created_here) = {
        let mut ws_guard = state.ws_server.lock().await;
        if let Some(ref existing) = *ws_guard {
            (existing.clone(), false)
        } else {
            let mut ws = WsServer::new(WsServerConfig {
                base_port: config.ws_port,
                backlog_policy: BacklogPolicy::from_config(&config.subtitle_backlog_policy),
                max_live_delay_sec: config.subtitle_max_live_delay_sec as f32,
                ..Default::default()
            });
            let effective_port = NetworkServer::start(&mut ws, config.ws_port)
                .await
                .map_err(|error| format!("Failed to bind OBS WebSocket: {}", error))?;
            state.active_port.store(effective_port, Ordering::SeqCst);
            tracing::info!("Servidor WebSocket iniciado en puerto {}", effective_port);
            *ws_guard = Some(ws.clone());
            (ws, true)
        }
    };

    tracing::info!("🚀 Iniciando servicio de transcripción LiveAudio...");
    tracing::info!(
        "⚙️ Configuración: modelo='{}', dispositivo='{}', idioma='{}', puerto_ws={}",
        clean_model_name,
        config.device,
        clean_lang,
        config.ws_port
    );

    tracing::info!(
        "🐍 Arrancando proceso supervisado Faster-Whisper (timeout handshake: {}s)...",
        worker_config.ready_timeout.as_secs()
    );

    let mut supervisor = match ProcessSupervisor::new(worker_config, std::process::id(), 0, None) {
        Ok(supervisor) => supervisor,
        Err(error) => {
            let err_msg = rollback_failed_start_before_audio(
                state,
                ws_created_here,
                format!("Error inicializando supervisor ASR: {}", error),
            )
            .await;
            if let Ok(status) = get_service_status_internal(state).await {
                let _ = app.emit("service_status_changed", &status);
            }
            return Err(err_msg);
        }
    };

    let mut worker_events = supervisor.subscribe_worker_events();

    if let Err(error) = supervisor.start_internal().await {
        if let Err(stop_error) = supervisor
            .stop_internal(std::time::Duration::from_secs(2))
            .await
        {
            tracing::warn!("Failed to reap worker after startup error: {}", stop_error);
        }
        let err_msg = rollback_failed_start_before_audio(
            state,
            ws_created_here,
            format!("Error al arrancar proceso Faster-Whisper: {}", error),
        )
        .await;
        if let Ok(status) = get_service_status_internal(state).await {
            let _ = app.emit("service_status_changed", &status);
        }
        return Err(err_msg);
    }
    let supervisor_arc = Arc::new(supervisor);
    *state.supervisor.lock().await = Some(supervisor_arc.clone());

    // 3. Setup Cancellation Token
    let cancel_token = tokio_util::sync::CancellationToken::new();
    *state.cancel_token.lock().await = Some(cancel_token.clone());

    state.is_running.store(true, Ordering::SeqCst);
    {
        let mut asr = state.asr_state.write().await;
        *asr = "listening".to_string();
    }
    {
        let mut start_guard = state.start_time.write().await;
        *start_guard = Some(Instant::now());
    }

    // 4. Background Task: Worker events dispatcher (subtitles, model status)
    let app_events = app.clone();
    let state_events = state.clone();
    let ws_events = ws_server.clone();
    let cancel_events = cancel_token.clone();
    let pending_utterances = Arc::new(std::sync::Mutex::new(PendingUtteranceTracker::default()));
    let pending_events = pending_utterances.clone();
    let backlog_policy = BacklogPolicy::from_config(&config.subtitle_backlog_policy);
    let max_live_delay_sec = config.subtitle_max_live_delay_sec;
    let catchup_interval_sec = config.subtitle_catchup_interval_sec;
    let session_language = config
        .language
        .clone()
        .unwrap_or_else(|| "auto".to_string());
    let session_started = Instant::now();

    let event_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_events.cancelled() => break,
                msg_res = worker_events.recv() => {
                    match msg_res {
                        Ok(val) => {
                            if let Some(event_type) = val.get("event").and_then(|v| v.as_str()) {
                                match event_type {
                                    "status" => {
                                        if let Some(st) = val.get("payload").and_then(|p| p.get("state")).and_then(|s| s.as_str()) {
                                            let text = val.get("payload").and_then(|p| p.get("text")).and_then(|t| t.as_str()).unwrap_or("");
                                            tracing::info!("📡 [ASR Worker Status] {} - {}", st, text);
                                            *state_events.asr_state.write().await = st.to_string();
                                            if let Ok(current_status) = get_service_status_internal(&state_events).await {
                                                let _ = app_events.emit("service_status_changed", &current_status);
                                            }
                                        }
                                    }
                                    "download_progress" => {
                                        if let Some(payload) = val.get("payload") {
                                            let text = payload.get("text").and_then(|t| t.as_str()).unwrap_or("Descargando...");
                                            let pct = payload.get("percent").and_then(|p| p.as_f64());
                                            let progress_text = match pct {
                                                Some(p) => format!("Descargando ({:.0}%)", p),
                                                None => text.to_string(),
                                            };
                                            tracing::info!("📥 [ASR Download] {}", progress_text);
                                            *state_events.asr_state.write().await = progress_text;
                                            if let Ok(current_status) = get_service_status_internal(&state_events).await {
                                                let _ = app_events.emit("service_status_changed", &current_status);
                                            }
                                        }
                                    }
                                    "ready" => {
                                        tracing::info!("✅ [ASR Worker Ready] Faster-Whisper inicializado y listo para inferencia");
                                        *state_events.asr_state.write().await = "ready".to_string();
                                        if let Ok(current_status) = get_service_status_internal(&state_events).await {
                                            let _ = app_events.emit("service_status_changed", &current_status);
                                        }
                                    }
                                    "error" => {
                                        if let Some(payload) = val.get("payload") {
                                            let code = payload.get("code").and_then(|c| c.as_str()).unwrap_or("unknown");
                                            let msg = payload.get("message").and_then(|m| m.as_str()).unwrap_or("Unknown error");
                                            tracing::error!("❌ [ASR Worker Error] {}: {}", code, msg);
                                            *state_events.asr_state.write().await = format!("error: {}", code);
                                            if let Ok(current_status) = get_service_status_internal(&state_events).await {
                                                let _ = app_events.emit("service_status_changed", &current_status);
                                            }
                                        }
                                    }
                                    "transcription_result" => {
                                        if let Some(payload) = val.get("payload") {
                                            let text = payload.get("text").and_then(|t| t.as_str()).unwrap_or("").trim();
                                            let dur_sec = payload.get("duration_sec").and_then(|d| d.as_f64()).unwrap_or(3.0);
                                            let utt_id = payload.get("utterance_id").and_then(|u| u.as_str()).unwrap_or("utt");
                                            let inference_sec = payload.get("inference_sec").and_then(|d| d.as_f64())
                                                .or_else(|| payload.get("latency_sec").and_then(|d| d.as_f64())).unwrap_or(0.0);
                                            let timing = pending_events.lock().unwrap().complete(utt_id, inference_sec);

                                            if !text.is_empty() {
                                                if let Some(sink) = state_events.session_sink.lock().await.clone() {
                                                    let raw_response = AsrTranscriptionResponse {
                                                        request_id: utt_id.to_string(),
                                                        text: text.to_string(),
                                                        language: session_language.clone(),
                                                        duration_sec: dur_sec as f32,
                                                        latency_sec: inference_sec as f32,
                                                        segments: Vec::new(),
                                                    };
                                                    let cue = TranscriptionCue::new(
                                                        session_started.elapsed().as_millis().max(1) as u64,
                                                        session_started.elapsed().as_secs_f64(),
                                                        session_started.elapsed().as_secs_f64() + dur_sec,
                                                        text.to_string(),
                                                        raw_response,
                                                    );
                                                    if let Err(error) = sink.emit(&cue).await {
                                                        tracing::warn!("Session sink rejected a transcript cue: {}", error);
                                                    }
                                                    *state_events.session_error.write().await = session_sink_failure(&sink.stats());
                                                }
                                                log_transcription_metadata(payload);
                                                let now_ms = SystemTime::now()
                                                    .duration_since(UNIX_EPOCH)
                                                    .map(|d| d.as_millis() as u64)
                                                    .unwrap_or(0);
                                                let active_style = state_events.config.read().await.subtitle_style.clone();

                                                let cue = SubtitleCueDto {
                                                    id: now_ms,
                                                    start: 0.0,
                                                    end: dur_sec,
                                                    text: text.to_string(),
                                                    style: active_style.clone(),
                                                    timestamp_ms: now_ms,
                                                };

                                                // Emit to Tauri webview UI preview
                                                let _ = app_events.emit("subtitle_cue", &cue);

                                                // Broadcast to OBS Studio Browser Source clients
                                                let wire_msg = subtitle_from_worker_result(
                                                    utt_id.to_string(),
                                                    cue.text.clone(),
                                                    cue.style.clone(),
                                                    inference_sec,
                                                    timing,
                                                    backlog_policy,
                                                    max_live_delay_sec,
                                                    catchup_interval_sec,
                                                );
                                                if let Some(wire_msg) = wire_msg {
                                                    let _ = ws_events.broadcast_subtitle(&wire_msg);
                                                }
                                            } else {
                                                tracing::debug!("🔇 [ASR Transcripción vacía] [{}] (silencio/ruido descartado)", utt_id);
                                            }
                                        }
                                    }
                                    _ => {
                                        log_unknown_worker_event(event_type, &val);
                                    }
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    });

    // 5. Initialize Hardware Audio Capture (CPAL)
    let audio_device_id = config
        .audio_device
        .as_ref()
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .filter(|id| *id != "default_microphone")
        .map(|id| {
            if id == "wasapi_loopback" {
                "default_loopback".to_string()
            } else {
                id.to_string()
            }
        });

    let stream_config = AudioStreamConfig {
        device_id: audio_device_id.clone(),
        sample_rate: 16000,
        channels: 1,
        chunk_size: 512,
        ring_buffer_capacity: 500,
    };

    let mut audio_source = CpalAudioSource::new();
    let mut audio_rx = match audio_source.start(&stream_config).await {
        Ok(receiver) => receiver,
        Err(error) => {
            drop(supervisor_arc);
            let err_msg = rollback_failed_audio_start(
                state,
                event_task,
                cancel_token.clone(),
                ws_created_here,
                format!(
                    "Error al inicializar el dispositivo de audio seleccionado: {}",
                    error
                ),
            )
            .await;
            if let Ok(status) = get_service_status_internal(state).await {
                let _ = app.emit("service_status_changed", &status);
            }
            return Err(err_msg);
        }
    };

    // 6. Initialize Voice Activity Detection (Silero VAD ONNX)
    let vad_cfg = VadConfig {
        sample_rate: 16000,
        chunk_size: 512,
        threshold: config.vad_threshold,
        speech_pad_ms: config.vad_speech_pad_ms,
        silence_timeout_ms: (config.silence_timeout * 1000.0).round() as u32,
        max_chunk_duration_sec: config.max_chunk_duration as f32,
    };
    let mut vad = match SileroVad::new(vad_cfg) {
        Ok(vad) => vad,
        Err(error) => {
            let _ = audio_source.stop().await;
            drop(supervisor_arc);
            let err_msg = rollback_failed_audio_start(
                state,
                event_task,
                cancel_token.clone(),
                ws_created_here,
                format!("Error al inicializar Silero VAD: {}", error),
            )
            .await;
            if let Ok(status) = get_service_status_internal(state).await {
                let _ = app.emit("service_status_changed", &status);
            }
            return Err(err_msg);
        }
    };

    let previous_session_path = state.session_path.read().await.clone();
    let previous_session_path =
        session_reuse_path(&config, previous_session_path.as_deref(), is_restart);
    let (session_sink, session_path) = match create_session_sinks(&config, previous_session_path) {
        Ok(result) => result,
        Err(error) => {
            let _ = audio_source.stop().await;
            drop(supervisor_arc);
            let err_msg = rollback_failed_audio_start(
                state,
                event_task,
                cancel_token.clone(),
                ws_created_here,
                format!("Failed to initialize session files: {}", error),
            )
            .await;
            if let Ok(status) = get_service_status_internal(state).await {
                let _ = app.emit("service_status_changed", &status);
            }
            return Err(err_msg);
        }
    };
    *state.session_sink.lock().await = session_sink;
    *state.session_path.write().await = session_path;

    // 7. Background Task: Real-time Audio VAD + Speech Chunking -> Faster-Whisper Worker
    let app_audio = app.clone();
    let state_audio = state.clone();
    let cancel_audio = cancel_token.clone();
    let supervisor_audio = supervisor_arc.clone();

    let pre_buffer_capacity = vad.config().pre_buffer_chunks().max(1);
    let max_samples = (16000.0 * config.max_chunk_duration.ceil()) as usize;

    let audio_task = tokio::spawn(async move {
        let mut utterance_samples: Vec<f32> = Vec::with_capacity(max_samples.max(16000 * 5));
        let mut pre_roll_chunks: VecDeque<Vec<f32>> =
            VecDeque::with_capacity(pre_buffer_capacity + 1);
        let mut is_in_utterance = false;
        let mut utterance_seq = 0u64;

        loop {
            tokio::select! {
                _ = cancel_audio.cancelled() => break,
                chunk_opt = audio_rx.recv() => {
                    let chunk = match chunk_opt {
                        Some(c) => c,
                        None => break,
                    };

                    for frame in chunk.samples.chunks_exact(512) {
                        match vad.evaluate_chunk(frame) {
                            Ok(decision) => {
                                let prev_onset = state_audio.vad_onset.swap(decision.is_speech, Ordering::SeqCst);
                                if prev_onset != decision.is_speech {
                                    if let Ok(current_status) = get_service_status_internal(&state_audio).await {
                                        let _ = app_audio.emit("service_status_changed", &current_status);
                                    }
                                }

                                match decision.transition {
                                    VadTransition::SpeechStart => {
                                        tracing::debug!("🎙️ [VAD] Voz detectada (SpeechStart)");
                                        is_in_utterance = true;
                                        utterance_samples.clear();
                                        for pre in pre_roll_chunks.drain(..) {
                                            utterance_samples.extend_from_slice(&pre);
                                        }
                                        utterance_samples.extend_from_slice(frame);
                                    }
                                    VadTransition::SpeechEnd { duration_ms, .. } => {
                                        if is_in_utterance {
                                            utterance_samples.extend_from_slice(frame);
                                            is_in_utterance = false;

                                            // Only dispatch utterances with >= 300ms of audio (4800 samples)
                                            if utterance_samples.len() >= 4800 {
                                                utterance_seq += 1;
                                                tracing::info!("🎙️ [VAD] Voz finalizada: {}ms ({} muestras) -> enviando utt_{} a Faster-Whisper", duration_ms, utterance_samples.len(), utterance_seq);
                                                let b64_audio = encode_f32_le_base64(&utterance_samples);
                                                let lang = state_audio.config.read().await.language.clone();

                                                let cmd = serde_json::json!({
                                                    "version": 1,
                                                    "cmd": "transcribe",
                                                    "timestamp_ms": SystemTime::now()
                                                        .duration_since(UNIX_EPOCH)
                                                        .map(|d| d.as_millis() as u64)
                                                        .unwrap_or(0),
                                                    "payload": {
                                                        "utterance_id": format!("utt_{}", utterance_seq),
                                                        "sequence": utterance_seq,
                                                        "audio_data": b64_audio,
                                                        "sample_rate": 16000,
                                                        "channels": 1,
                                                        "duration_ms": duration_ms,
                                                        "language": lang,
                                                    }
                                                });

                                                let utterance_id = format!("utt_{}", utterance_seq);
                                                let created_at_sec = SystemTime::now().duration_since(UNIX_EPOCH)
                                                    .map(|d| d.as_secs_f64()).unwrap_or(0.0);
                                                pending_utterances.lock().unwrap().track(utterance_id.clone(), created_at_sec);

                                                if let Err(e) = send_audio_command_until_cancelled(
                                                    &supervisor_audio,
                                                    &cancel_audio,
                                                    &cmd,
                                                ).await {
                                                    pending_utterances.lock().unwrap().remove(&utterance_id);
                                                    tracing::error!("Error enviando audio a Faster-Whisper: {}", e);
                                                }
                                            } else {
                                                tracing::debug!("🎙️ [VAD] Segmento ignorado por ser menor a 300ms ({} muestras)", utterance_samples.len());
                                            }
                                            utterance_samples.clear();
                                        }
                                    }
                                    VadTransition::None => {
                                        if is_in_utterance {
                                            utterance_samples.extend_from_slice(frame);
                                        } else {
                                            if pre_roll_chunks.len() >= pre_buffer_capacity {
                                                pre_roll_chunks.pop_front();
                                            }
                                            pre_roll_chunks.push_back(frame.to_vec());
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Silero VAD evaluation error: {}", e);
                            }
                        }
                    }
                }
            }
        }

        let _ = audio_source.stop().await;
    });

    state.background_tasks.lock().await.push(event_task);
    state.background_tasks.lock().await.push(audio_task);
    state.active_port.store(
        ws_server.bound_port().unwrap_or(config.ws_port),
        Ordering::SeqCst,
    );
    *state.active_config.write().await = Some(config.clone());
    let status = get_service_status_internal(state).await?;
    let _ = app.emit("service_status_changed", &status);

    tracing::info!("Pipeline de transcripción iniciado correctamente");
    Ok(status)
}

pub(crate) async fn send_audio_command_until_cancelled(
    supervisor: &ProcessSupervisor,
    cancellation: &tokio_util::sync::CancellationToken,
    command: &serde_json::Value,
) -> Result<(), liveaudio_core::supervisor::SupervisorError> {
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => Ok(()),
        result = supervisor.send_command(command) => result,
    }
}

/// Stop the LiveAudio transcription service cleanly.
#[tauri::command]
pub async fn stop_service(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ServiceStatusDto, String> {
    let _transition_guard = state.transition_lock.lock().await;
    stop_service_locked(&app, state.inner()).await
}

async fn stop_service_locked(
    app: &AppHandle,
    state: &AppState,
) -> Result<ServiceStatusDto, String> {
    if !state.is_running.load(Ordering::SeqCst) {
        return get_service_status_internal(state).await;
    }

    *state.asr_state.write().await = "stopping".to_string();
    if let Ok(stopping_status) = get_service_status_internal(state).await {
        let _ = app.emit("service_status_changed", &stopping_status);
    }

    state.is_running.store(false, Ordering::SeqCst);
    state.vad_onset.store(false, Ordering::SeqCst);
    {
        let mut asr = state.asr_state.write().await;
        *asr = "stopped".to_string();
    }
    {
        let mut start_guard = state.start_time.write().await;
        *start_guard = None;
    }

    // 1. Cancel audio capture and background tasks
    if let Some(token) = state.cancel_token.lock().await.take() {
        token.cancel();
    }

    // 2. Join background capture/event tasks before stopping their shared resources.
    let tasks = std::mem::take(&mut *state.background_tasks.lock().await);
    for task in tasks {
        if let Err(error) = task.await {
            tracing::warn!("Background service task ended with an error: {}", error);
        }
    }

    let mut shutdown_error = None;
    // 3. Flush session output before dropping its sinks.
    if let Some(sink) = state.session_sink.lock().await.take() {
        let stop_result = sink.stop(std::time::Duration::from_secs(5)).await;
        let failure = session_sink_failure(&sink.stats());
        if failure.is_some() {
            *state.session_error.write().await = failure.clone();
        }
        if let Err(error) = stop_result {
            let message = record_session_stop_failure(state, &error, &sink.stats()).await;
            shutdown_error = Some(failure.unwrap_or(message));
        }
    }

    // 4. Terminate ASR worker supervisor cleanly.
    if let Some(supervisor) = state.supervisor.lock().await.take() {
        match Arc::try_unwrap(supervisor) {
            Ok(mut supervisor) => {
                if let Err(error) = supervisor
                    .stop_internal(std::time::Duration::from_secs(3))
                    .await
                {
                    shutdown_error
                        .get_or_insert_with(|| format!("Failed to stop ASR worker: {}", error));
                }
            }
            Err(supervisor) => {
                supervisor.cancellation_token().cancel();
                shutdown_error.get_or_insert_with(|| {
                    "ASR worker still has active owners during shutdown".to_string()
                });
            }
        }
    }

    // 5. Release the WebSocket listener so a changed port can be rebound on restart.
    if let Err(error) = stop_active_websocket(state).await {
        shutdown_error.get_or_insert(error);
    }

    if let Some(error) = shutdown_error {
        return Err(error);
    }
    let status = get_service_status_internal(state).await?;
    let _ = app.emit("service_status_changed", &status);

    tracing::info!("Transcription service stopped cleanly via Tauri IPC command");
    Ok(status)
}

/// Restart the LiveAudio transcription service.
#[tauri::command]
pub async fn restart_service(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ServiceStatusDto, String> {
    let _transition_guard = state.transition_lock.lock().await;
    if state.is_running.load(Ordering::SeqCst) {
        stop_service_locked(&app, state.inner()).await?;
    }
    start_service_locked(&app, state.inner(), true).await
}

#[tauri::command]
pub async fn open_session_folder(state: State<'_, AppState>) -> Result<String, String> {
    let path = if let Some(p) = state.session_path.read().await.clone() {
        p
    } else {
        let dir = state.config.read().await.output_dir.clone();
        let _ = std::fs::create_dir_all(&dir);
        dir
    };

    if !path.exists() {
        let _ = std::fs::create_dir_all(&path);
    }

    let session = std::fs::canonicalize(&path)
        .map_err(|error| format!("Session folder unavailable: {}", error))?;

    if !session.is_dir() {
        return Err("Target path is not a directory".to_string());
    }

    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer.exe")
        .arg(&session)
        .spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(&session).spawn();
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(&session).spawn();
    result.map_err(|error| format!("Failed to open session folder: {}", error))?;
    Ok(session.to_string_lossy().to_string())
}

const AUTHOR_PROFILE_URL: &str = "https://github.com/franguh";

pub(crate) fn author_profile_open_command() -> (&'static str, &'static [&'static str]) {
    #[cfg(target_os = "windows")]
    {
        (
            "rundll32.exe",
            &["url.dll,FileProtocolHandler", AUTHOR_PROFILE_URL],
        )
    }
    #[cfg(target_os = "macos")]
    {
        ("open", &[AUTHOR_PROFILE_URL])
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        ("xdg-open", &[AUTHOR_PROFILE_URL])
    }
}

/// Open the fixed author profile in the system's default browser.
#[tauri::command]
pub async fn open_author_profile() -> Result<(), String> {
    let (program, args) = author_profile_open_command();
    std::process::Command::new(program)
        .args(args)
        .spawn()
        .map_err(|error| format!("Failed to open author profile: {error}"))?;
    Ok(())
}

/// Resolve OBS overlay URL and browser source recommendations.
#[tauri::command]
pub async fn get_obs_overlay_info(state: State<'_, AppState>) -> Result<ObsOverlayDto, String> {
    let port = if state.is_running() {
        state.active_port.load(Ordering::SeqCst)
    } else {
        state.config.read().await.ws_port
    };

    // Look for subtitulos_obs.html
    let candidates = [
        PathBuf::from("liveaudio/assets/subtitulos_obs.html"),
        PathBuf::from("../liveaudio/assets/subtitulos_obs.html"),
        PathBuf::from("../../liveaudio/assets/subtitulos_obs.html"),
    ];

    let mut resolved_path = None;
    for cand in &candidates {
        if let Ok(abs) = std::fs::canonicalize(cand) {
            resolved_path = Some(abs);
            break;
        }
    }

    let raw_path = resolved_path
        .unwrap_or_else(|| get_data_home().join("assets").join("subtitulos_obs.html"))
        .to_string_lossy()
        .replace('\\', "/");

    let clean_path = raw_path.strip_prefix("//?/").unwrap_or(&raw_path);
    let url = format!(
        "file:///{}?port={}",
        clean_path.trim_start_matches('/'),
        port
    );

    Ok(ObsOverlayDto {
        url,
        port,
        width: 1920,
        height: 1080,
        instructions: "En OBS Studio: Añadir fuente -> 'Navegador' (Browser Source). Marca 'Archivo local' desmarcado, pega la URL en el campo URL, y establece Ancho: 1920, Alto: 1080, FPS: 60.".to_string(),
    })
}

/// Export full system and pipeline diagnostics to a JSON file.
#[tauri::command]
pub async fn export_diagnostics(
    state: State<'_, AppState>,
    target_dir: Option<String>,
) -> Result<DiagnosticsExportDto, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let export_dir = if let Some(ref dir) = target_dir {
        PathBuf::from(dir)
    } else {
        let guard = state.config.read().await;
        guard
            .diagnostics_export_dir
            .clone()
            .unwrap_or_else(|| get_data_home().join("diagnostics"))
    };

    std::fs::create_dir_all(&export_dir)
        .map_err(|e| format!("Could not create diagnostics directory: {}", e))?;

    let filename = format!("LiveAudio_Diagnostic_{}.json", now);
    let filepath = export_dir.join(filename);

    let config = {
        let guard = state.config.read().await;
        guard.clone()
    };

    let status = get_service_status(state).await?;
    let job_object_support = JobObject::create().is_ok();
    let ports = candidate_ports(config.ws_port, Some(5));

    let report = serde_json::json!({
        "timestamp_epoch": now,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "cpu_parallelism": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
        "job_object_supported": job_object_support,
        "candidate_ports": ports,
        "config": config,
        "service_status": status,
    });

    let bytes =
        serde_json::to_vec_pretty(&report).map_err(|e| format!("Serialization error: {}", e))?;

    std::fs::write(&filepath, bytes)
        .map_err(|e| format!("Failed to write diagnostics file: {}", e))?;

    Ok(DiagnosticsExportDto {
        path: filepath.to_string_lossy().to_string(),
        timestamp: format!("{}", now),
        success: true,
    })
}

/// Send a mock/test subtitle cue to verify preview styling and WebSocket propagation.
#[tauri::command]
pub async fn send_test_subtitle(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
    style: Option<String>,
) -> Result<SubtitleCueDto, String> {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let active_style = if let Some(s) = style {
        s
    } else {
        let guard = state.config.read().await;
        guard.subtitle_style.clone()
    };

    let cue = SubtitleCueDto {
        id: now_ms,
        start: 0.0,
        end: 3.0,
        text,
        style: active_style,
        timestamp_ms: now_ms,
    };

    let _ = app.emit("subtitle_cue", &cue);
    Ok(cue)
}
