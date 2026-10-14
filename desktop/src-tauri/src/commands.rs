use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

use liveaudio_audio::{AudioChunkReceiver, AudioStreamConfig, CpalAudioSource, ResilientCapture};
use liveaudio_core::acoustic_events::{
    AcousticEventLifecycle, AcousticSpeechEndReason, AcousticSpeechEvent,
};
use liveaudio_core::asr::worker::{
    presentation_text, transcript_text_from_result, worker_init_payload,
};
use liveaudio_core::asr::AsrTranscriptionResponse;
use liveaudio_core::config::{get_data_home, save_config as save_config_core, LiveAudioConfig};
use liveaudio_core::job_object::JobObject;
use liveaudio_core::runtime::{
    discover_managed_python, managed_runtime_root, preflight_asr_runtime,
    provision_runtime_with_uv_cancellable, AsrBackend, RuntimeError, RuntimeSetupProgress,
};
use liveaudio_core::service_lock::{service_lock_holder, ServiceLock, ServiceLockHolder};
use liveaudio_core::sink::{
    last_cue_counter, CompositeSink, JsonlSink, SinkError, SinkStats, TranscriptionCue,
    TranscriptionSink, WebVttSink,
};
use liveaudio_core::supervisor::{ProcessSupervisor, WorkerProcessConfig};
use liveaudio_network::{
    candidate_ports, subtitle_from_worker_result, BacklogPolicy, NetworkServer,
    PendingUtteranceTracker, WsServer, WsServerConfig,
};
use liveaudio_vad::{
    cpu_runtime_library_path, initialize_onnx_runtime, SileroVad, VadConfig, VadEngine,
    VadTransition,
};

fn initialize_bundled_cpu_runtime(app: &AppHandle) -> Result<(), String> {
    // 1. Try packaged resource directory first
    let packaged_path = app.path().resource_dir().ok().and_then(|resource_root| {
        cpu_runtime_library_path(&resource_root).ok()
    }).filter(|p| p.is_file());

    // 2. Try development and local executable fallbacks
    let runtime_path = if let Some(path) = packaged_path {
        path
    } else {
        let current_dir = std::env::current_dir().ok();
        let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf()));

        let candidates = [
            // packaging in current working dir (e.g. E:\LiveAudio\packaging)
            current_dir.as_ref().map(|d| d.join("packaging")),
            // packaging in workspace root if running from desktop directory
            current_dir.as_ref().map(|d| d.join("../packaging")),
            // packaging next to executable
            exe_dir.as_ref().map(|d| d.join("packaging")),
            // directly next to executable or target/debug
            exe_dir.as_ref().cloned(),
        ];

        let mut found = None;
        for candidate in candidates.into_iter().flatten() {
            if let Ok(lib) = cpu_runtime_library_path(&candidate) {
                if lib.is_file() {
                    found = Some(lib);
                    break;
                }
            }
        }

        found.ok_or_else(|| {
            "LiveAudio's bundled CPU audio runtime is unavailable. Repair or reinstall the application.".to_string()
        })?
    };

    tracing::info!("Cargando ONNX Runtime desde: {}", runtime_path.display());
    initialize_onnx_runtime(&runtime_path).map_err(|error| {
        tracing::error!("Error inicializando ONNX Runtime: {}", error);
        format!("LiveAudio's bundled CPU audio runtime could not be loaded: {error}")
    })
}

pub(crate) fn observe_and_publish_acoustic_transition(
    lifecycle: &mut AcousticEventLifecycle,
    server: &WsServer,
    enabled: bool,
    transition: VadTransition,
) {
    if enabled {
        if let Some(event) = lifecycle.observe_vad_transition(transition, Instant::now()) {
            publish_acoustic_event(server, &event);
        }
    }
}

fn publish_acoustic_event(server: &WsServer, event: &AcousticSpeechEvent) {
    if let Err(error) = server.broadcast_acoustic_event(event) {
        tracing::warn!("Failed to publish acoustic event: {}", error);
    }
}

pub(crate) fn close_and_publish_acoustic_attempt(
    lifecycle: &mut AcousticEventLifecycle,
    server: &WsServer,
    enabled: bool,
    reason: AcousticSpeechEndReason,
) {
    if enabled {
        if let Some(event) = lifecycle.close_attempt(reason, Instant::now()) {
            publish_acoustic_event(server, &event);
        }
    }
}

pub(crate) fn audio_receiver_close_reason(
    receiver: &AudioChunkReceiver,
) -> AcousticSpeechEndReason {
    if receiver.capture_failed() {
        AcousticSpeechEndReason::CaptureError
    } else {
        AcousticSpeechEndReason::ServiceStopped
    }
}

pub(crate) fn capture_error_preempts_audio(receiver: &AudioChunkReceiver) -> bool {
    receiver.capture_failed()
}

pub(crate) fn asr_progress_from_worker_event(value: &serde_json::Value) -> Option<AsrProgressDto> {
    if value.get("event")?.as_str()? != "download_progress" {
        return None;
    }
    let payload = value.get("payload")?;
    let bytes_total = payload
        .get("bytes_total")
        .and_then(serde_json::Value::as_u64)
        .filter(|total| *total > 0);
    let percent = payload
        .get("percent")
        .and_then(serde_json::Value::as_f64)
        .filter(|percent| {
            bytes_total.is_some() && percent.is_finite() && (0.0..=100.0).contains(percent)
        });
    Some(AsrProgressDto {
        phase: payload
            .get("phase")
            .and_then(|v| v.as_str())
            .unwrap_or("downloading")
            .to_string(),
        text: payload
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("Preparing the ASR model…")
            .to_string(),
        percent,
        bytes_downloaded: payload
            .get("bytes_downloaded")
            .and_then(serde_json::Value::as_u64),
        bytes_total,
        speed_bps: payload.get("speed_bps").and_then(serde_json::Value::as_u64),
        eta_sec: payload
            .get("eta_sec")
            .and_then(serde_json::Value::as_f64)
            .filter(|eta| eta.is_finite() && *eta >= 0.0),
        attempt: payload
            .get("attempt")
            .and_then(serde_json::Value::as_u64)
            .or_else(|| value.get("attempt_id").and_then(serde_json::Value::as_u64)),
    })
}

pub(crate) fn worker_attempt_is_current(incoming: Option<u64>, latest: Option<u64>) -> bool {
    match (incoming, latest) {
        (_, None) => true,
        (Some(incoming), Some(latest)) => incoming >= latest,
        (None, Some(_)) => false,
    }
}

pub(crate) fn admit_worker_event_generation(
    value: &serde_json::Value,
    latest_attempt: &mut Option<u64>,
) -> bool {
    let event = value.get("event").and_then(serde_json::Value::as_str);
    let attempt = value
        .get("payload")
        .and_then(|payload| payload.get("attempt"))
        .and_then(serde_json::Value::as_u64)
        .or_else(|| value.get("attempt_id").and_then(serde_json::Value::as_u64));
    if event == Some("worker_attempt_started") {
        if let Some(attempt) = attempt {
            *latest_attempt = Some(latest_attempt.unwrap_or_default().max(attempt));
        }
        return false;
    }
    if !worker_attempt_is_current(attempt, *latest_attempt) {
        return false;
    }
    if let Some(attempt) = attempt {
        *latest_attempt = Some(latest_attempt.unwrap_or_default().max(attempt));
    }
    true
}

pub(crate) fn desktop_runtime_preflight_error(error: &RuntimeError) -> String {
    match error {
        RuntimeError::DependenciesUnavailable { .. } => {
            "The managed ASR Python environment is incomplete or invalid. Use Repair ASR Runtime in Settings to prepare it.".to_string()
        }
        _ => format!("ASR runtime check failed. Use Repair ASR Runtime in Settings. Details: {error}"),
    }
}

pub(crate) fn asr_progress_update_for_worker_event(
    value: &serde_json::Value,
) -> Option<Option<AsrProgressDto>> {
    match value.get("event")?.as_str()? {
        "download_progress" => asr_progress_from_worker_event(value).map(Some),
        "ready" | "error" => Some(None),
        _ => None,
    }
}

async fn forward_startup_worker_event(
    app: &AppHandle,
    state: &AppState,
    value: &serde_json::Value,
    latest_attempt: &mut Option<u64>,
) {
    if !admit_worker_event_generation(value, latest_attempt) {
        return;
    }
    let event_type = value.get("event").and_then(|v| v.as_str()).unwrap_or("");
    let payload = value.get("payload");
    let attempt = payload
        .and_then(|payload| payload.get("attempt"))
        .and_then(serde_json::Value::as_u64)
        .or_else(|| value.get("attempt_id").and_then(serde_json::Value::as_u64));
    if let Some(progress) = asr_progress_update_for_worker_event(value) {
        *state.asr_progress.write().await = progress;
    }

    match event_type {
        "download_progress" => {
            if let Some(progress) = asr_progress_from_worker_event(value) {
                *state.asr_state.write().await = progress.phase.clone();
                *state.asr_progress.write().await = Some(progress);
            }
        }
        "status" => {
            if let Some(payload) = payload {
                if let Some(worker_state) = payload.get("state").and_then(|v| v.as_str()) {
                    *state.asr_state.write().await = worker_state.to_string();
                    if worker_state == "downloading" {
                        *state.asr_progress.write().await = Some(AsrProgressDto {
                            phase: payload
                                .get("phase")
                                .and_then(|v| v.as_str())
                                .unwrap_or("downloading")
                                .to_string(),
                            text: payload
                                .get("text")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Preparing the ASR model…")
                                .to_string(),
                            percent: None,
                            bytes_downloaded: None,
                            bytes_total: None,
                            speed_bps: None,
                            eta_sec: None,
                            attempt,
                        });
                    }
                }
            }
        }
        "ready" => {
            *state.asr_state.write().await = "ready".to_string();
            *state.asr_progress.write().await = None;
        }
        "error" => {
            let code = payload
                .and_then(|payload| payload.get("code"))
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            *state.asr_state.write().await = format!("error: {code}");
            *state.asr_progress.write().await = None;
        }
        _ => return,
    }
    if let Ok(status) = get_service_status_internal(state).await {
        let _ = app.emit("service_status_changed", &status);
    }
}

use crate::dto::{
    AsrProgressDto, AudioDeviceDto, DiagnosticsExportDto, ObsOverlayDto, ProfilePresetDto,
    ServiceStatusDto, SubtitleCueDto,
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
    *state.previous_transcript.write().await = None;

    state.is_running.store(false, Ordering::SeqCst);
    state.vad_onset.store(false, Ordering::SeqCst);
    *state.asr_state.write().await = "stopped".to_string();
    *state.asr_progress.write().await = None;
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
    *state.asr_progress.write().await = None;
    error
}

/// Reuses the running OBS WebSocket or binds a new one on the configured port.
/// Returns the server and whether this call created it; a bind failure rolls back
/// the "starting" state like the other pre-audio startup failures.
pub(crate) async fn ensure_startup_websocket(
    state: &AppState,
    config: &LiveAudioConfig,
) -> Result<(WsServer, bool), String> {
    let mut ws_guard = state.ws_server.lock().await;
    if let Some(ref existing) = *ws_guard {
        return Ok((existing.clone(), false));
    }
    let mut ws = WsServer::new(WsServerConfig {
        base_port: config.ws_port,
        backlog_policy: BacklogPolicy::from_config(&config.subtitle_backlog_policy),
        max_live_delay_sec: config.subtitle_max_live_delay_sec as f32,
        ..Default::default()
    });
    let effective_port = match NetworkServer::start(&mut ws, config.ws_port).await {
        Ok(port) => port,
        Err(error) => {
            drop(ws_guard);
            return Err(rollback_failed_start_before_audio(
                state,
                false,
                format!("Failed to bind OBS WebSocket: {}", error),
            )
            .await);
        }
    };
    state.active_port.store(effective_port, Ordering::SeqCst);
    tracing::info!("Servidor WebSocket iniciado en puerto {}", effective_port);
    *ws_guard = Some(ws.clone());
    Ok((ws, true))
}

/// Takes the service lock in `dir` for this desktop's pipeline. Another live owner (e.g. a
/// headless `liveaudio-cli serve`) fails the start with `service-already-running`.
pub(crate) async fn acquire_service_lock(state: &AppState, dir: &Path) -> Result<(), String> {
    match ServiceLock::try_acquire(dir) {
        Ok(Some(lock)) => {
            *state.service_lock.lock().await = Some(lock);
            Ok(())
        }
        Ok(None) => {
            // Detection before every Start: a refused start enters Monitor Mode.
            let holder = match refresh_foreign_service(state, dir).await {
                Some(ServiceLockHolder { pid: Some(pid), .. }) => format!(" (PID {pid})"),
                _ => String::new(),
            };
            Err(format!(
                "service-already-running: another LiveAudio process{holder} is already running the service"
            ))
        }
        Err(error) => {
            // Same policy as the former launch lock: an unopenable lock does not block the app.
            tracing::warn!("Service lock unavailable, starting without it: {error}");
            Ok(())
        }
    }
}

/// Monitor Mode poll interval. Short enough for a timely takeover (same 3 s cadence as audio
/// reconnect); sparse because the non-Windows probe briefly takes the lock (T7a finding L1).
const FOREIGN_SERVICE_POLL: Duration = Duration::from_secs(3);
const TAKEOVER_RETRY_DELAY: Duration = Duration::from_secs(1);

/// Outcome of one Monitor Mode poll.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ForeignServicePoll {
    NotMonitoring,
    Monitoring,
    Released,
}

/// Records whether another live process owns the service lock in `dir`. A lock this desktop
/// holds is never foreign; a failed probe counts as no owner (a later start re-checks).
pub(crate) async fn refresh_foreign_service(
    state: &AppState,
    dir: &Path,
) -> Option<ServiceLockHolder> {
    let holder = if state.service_lock.lock().await.is_some() {
        None
    } else {
        service_lock_holder(dir).ok().flatten()
    };
    *state.foreign_service.write().await = holder;
    holder
}

/// Re-probes only while monitoring; `Released` means the foreign owner exited.
pub(crate) async fn poll_foreign_service(state: &AppState, dir: &Path) -> ForeignServicePoll {
    if state.foreign_service.read().await.is_none() {
        return ForeignServicePoll::NotMonitoring;
    }
    match refresh_foreign_service(state, dir).await {
        Some(_) => ForeignServicePoll::Monitoring,
        None => ForeignServicePoll::Released,
    }
}

/// Retries a start once when the lock looked held, absorbing a momentary probe/acquire race.
pub(crate) async fn retry_once_if_already_running<T, F, Fut>(
    delay: Duration,
    mut attempt: F,
) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    match attempt().await {
        Err(error) if error.starts_with("service-already-running") => {
            tokio::time::sleep(delay).await;
            attempt().await
        }
        result => result,
    }
}

/// Detects a foreign service at launch, then polls it while in Monitor Mode and starts this
/// desktop's own service when the foreign owner exits (takeover).
pub(crate) async fn watch_foreign_service(app: AppHandle) {
    let state = app.state::<AppState>().inner().clone();
    let dir = get_data_home();
    if refresh_foreign_service(&state, &dir).await.is_some() {
        if let Ok(status) = get_service_status_internal(&state).await {
            let _ = app.emit("service_status_changed", &status);
        }
    }
    loop {
        tokio::time::sleep(FOREIGN_SERVICE_POLL).await;
        if poll_foreign_service(&state, &dir).await != ForeignServicePoll::Released {
            continue;
        }
        tracing::info!("The service owner exited; starting this desktop's service");
        let start = || start_service_guarded(&app, &state);
        if let Err(error) = retry_once_if_already_running(TAKEOVER_RETRY_DELAY, start).await {
            tracing::warn!("Service takeover did not start: {error}");
        }
    }
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

/// Timeline origin and last written cue number for a session. A resumed continuous
/// session keeps both, so cue numbers and capture offsets continue (`engine.py`).
pub(crate) fn session_timeline(
    session_path: Option<&Path>,
    previous_origin: Option<Instant>,
    resumed: bool,
) -> (Instant, u64) {
    let origin = previous_origin
        .filter(|_| resumed)
        .unwrap_or_else(Instant::now);
    let last_cue = session_path.map_or(0, |path| last_cue_counter(&path.join("subtitles.vtt")));
    (origin, last_cue)
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
    let foreign_service = *state.foreign_service.read().await;
    let ws_port = if is_running {
        state.active_port.load(Ordering::SeqCst)
    } else {
        let configured = state.config.read().await.ws_port;
        foreign_service
            .and_then(|holder| holder.ws_port)
            .unwrap_or(configured)
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
        asr_progress: state.asr_progress.read().await.clone(),
        audio_state: if is_running {
            *state.audio_state.read().await
        } else {
            liveaudio_audio::AudioSourceStatus::Stopped
        },
        monitor_mode: foreign_service.is_some(),
        foreign_service_pid: foreign_service.and_then(|holder| holder.pid),
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
    start_service_guarded(&app, state.inner()).await
}

async fn start_service_guarded(
    app: &AppHandle,
    state: &AppState,
) -> Result<ServiceStatusDto, String> {
    let _transition_guard = state.transition_lock.lock().await;
    let startup_token = tokio_util::sync::CancellationToken::new();
    *state.startup_cancel_token.lock().await = Some(startup_token.clone());
    let result = start_service_locked(app, state, false, startup_token).await;
    *state.startup_cancel_token.lock().await = None;
    result
}

async fn start_service_locked(
    app: &AppHandle,
    state: &AppState,
    is_restart: bool,
    startup_token: tokio_util::sync::CancellationToken,
) -> Result<ServiceStatusDto, String> {
    if state.is_running.load(Ordering::SeqCst) {
        return get_service_status_internal(state).await;
    }
    if let Err(error) = acquire_service_lock(state, &get_data_home()).await {
        if let Ok(status) = get_service_status_internal(state).await {
            let _ = app.emit("service_status_changed", &status);
        }
        return Err(error);
    }
    let result = start_pipeline_locked(app, state, is_restart, startup_token).await;
    if result.is_err() {
        // Every failed start has rolled back its resources; give the lock back too.
        state.service_lock.lock().await.take();
    }
    result
}

async fn start_pipeline_locked(
    app: &AppHandle,
    state: &AppState,
    is_restart: bool,
    startup_token: tokio_util::sync::CancellationToken,
) -> Result<ServiceStatusDto, String> {
    initialize_bundled_cpu_runtime(app)?;

    *state.asr_state.write().await = "starting".to_string();
    *state.asr_progress.write().await = None;
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
    if discover_managed_python(&managed_runtime_root()).is_none() {
        let backend = if config.device.eq_ignore_ascii_case("cuda") {
            AsrBackend::Cu121
        } else {
            AsrBackend::Cpu
        };
        let runtime_token = startup_token.child_token();
        if let Err(error) = prepare_managed_runtime(app, state, backend, runtime_token).await {
            *state.asr_state.write().await = "stopped".to_string();
            if let Ok(status) = get_service_status_internal(state).await {
                let _ = app.emit("service_status_changed", &status);
            }
            return Err(format!("ASR runtime preparation failed: {error}"));
        }
    }

    let mut worker_config = WorkerProcessConfig::discover_asr_worker();
    worker_config.init_payload = Some(worker_init_payload(
        &config,
        &clean_model_name,
        &clean_lang,
        Some(prompt.as_str()).filter(|prompt| !prompt.is_empty()),
    ));
    worker_config.decode_timeout = Some(Duration::from_secs(config.asr_decode_timeout_sec.into()));
    if let Err(error) = preflight_asr_runtime(
        std::path::Path::new(&worker_config.command),
        worker_config.working_dir.as_deref(),
    )
    .await
    {
        *state.asr_state.write().await = "stopped".to_string();
        let _ = app.emit(
            "runtime_setup_progress",
            RuntimeSetupProgress {
                phase: "error".to_string(),
                text: "ASR runtime is unavailable. Choose Repair ASR Runtime and retry."
                    .to_string(),
                percent: None,
            },
        );
        if let Ok(status) = get_service_status_internal(state).await {
            let _ = app.emit("service_status_changed", &status);
        }
        return Err(desktop_runtime_preflight_error(&error));
    }

    *state.session_error.write().await = None;

    // 1. WebSocket Server: ensure started on configured port
    let (ws_server, ws_created_here) = match ensure_startup_websocket(state, &config).await {
        Ok(started) => started,
        Err(err_msg) => {
            if let Ok(status) = get_service_status_internal(state).await {
                let _ = app.emit("service_status_changed", &status);
            }
            return Err(err_msg);
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
    let _ = app.emit(
        "runtime_setup_progress",
        RuntimeSetupProgress {
            phase: "starting_worker".to_string(),
            text: "Starting the ASR worker and waiting for it to become ready…".to_string(),
            percent: None,
        },
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

    let mut startup_events = supervisor.subscribe_worker_events();
    let startup_events_cancel = tokio_util::sync::CancellationToken::new();
    let startup_events_stop = startup_events_cancel.clone();
    let app_startup_events = app.clone();
    let state_startup_events = state.clone();
    let startup_event_task = tokio::spawn(async move {
        let mut latest_attempt = None;
        loop {
            tokio::select! {
                _ = startup_events_stop.cancelled() => break,
                message = startup_events.recv() => match message {
                    Ok(value) => forward_startup_worker_event(
                        &app_startup_events,
                        &state_startup_events,
                        &value,
                        &mut latest_attempt,
                    ).await,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    });

    let start_result = supervisor
        .start_internal_cancellable(startup_token.clone())
        .await;
    startup_events_cancel.cancel();
    if let Err(error) = startup_event_task.await {
        tracing::warn!("Startup ASR event dispatcher failed: {error}");
    }

    if let Err(error) = start_result {
        *state.asr_progress.write().await = None;
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
    *state.asr_state.write().await = "ready".to_string();
    *state.asr_progress.write().await = None;
    let mut worker_events = supervisor.subscribe_worker_events();
    let initial_worker_attempt = supervisor.current_worker_attempt();
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
    *state.previous_transcript.write().await = None;

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
    let session_language = clean_lang.clone();
    let initial_worker_attempt = initial_worker_attempt.max(1);

    let event_task = tokio::spawn(async move {
        let mut latest_attempt = Some(initial_worker_attempt);
        loop {
            tokio::select! {
                _ = cancel_events.cancelled() => break,
                msg_res = worker_events.recv() => {
                    match msg_res {
                        Ok(val) => {
                            if !admit_worker_event_generation(&val, &mut latest_attempt) {
                                continue;
                            }
                            if let Some(event_type) = val.get("event").and_then(|v| v.as_str()) {
                                match event_type {
                                    "status" => {
                                        if let Some(st) = val.get("payload").and_then(|p| p.get("state")).and_then(|s| s.as_str()) {
                                            let text = val.get("payload").and_then(|p| p.get("text")).and_then(|t| t.as_str()).unwrap_or("");
                                            tracing::info!("📡 [ASR Worker Status] {} - {}", st, text);
                                            *state_events.asr_state.write().await = st.to_string();
                                            if st == "ready" {
                                                *state_events.asr_progress.write().await = None;
                                            } else if st == "downloading" {
                                                forward_startup_worker_event(&app_events, &state_events, &val, &mut latest_attempt).await;
                                            }
                                            if let Ok(current_status) = get_service_status_internal(&state_events).await {
                                                let _ = app_events.emit("service_status_changed", &current_status);
                                            }
                                        }
                                    }
                                    "download_progress" => {
                                        if let Some(progress) = asr_progress_from_worker_event(&val) {
                                            tracing::info!("📥 [ASR Download] {}", progress.text);
                                            *state_events.asr_state.write().await = progress.phase.clone();
                                            *state_events.asr_progress.write().await = Some(progress);
                                            if let Ok(current_status) = get_service_status_internal(&state_events).await {
                                                let _ = app_events.emit("service_status_changed", &current_status);
                                            }
                                        }
                                    }
                                    "ready" => {
                                        tracing::info!("✅ [ASR Worker Ready] Faster-Whisper inicializado y listo para inferencia");
                                        *state_events.asr_state.write().await = "ready".to_string();
                                        *state_events.asr_progress.write().await = None;
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
                                            *state_events.asr_progress.write().await = None;
                                            if let Ok(current_status) = get_service_status_internal(&state_events).await {
                                                let _ = app_events.emit("service_status_changed", &current_status);
                                            }
                                        }
                                    }
                                    "transcription_result" => {
                                        if let Some(payload) = val.get("payload") {
                                            let transcript_text = transcript_text_from_result(payload);
                                            let text = transcript_text.as_str();
                                            let dur_sec = payload.get("duration_sec").and_then(|d| d.as_f64()).unwrap_or(3.0);
                                            let utt_id = payload.get("utterance_id").and_then(|u| u.as_str()).unwrap_or("utt");
                                            let inference_sec = payload.get("inference_sec").and_then(|d| d.as_f64())
                                                .or_else(|| payload.get("latency_sec").and_then(|d| d.as_f64())).unwrap_or(0.0);
                                            let (capture_offset_sec, timing) = {
                                                let mut pending = pending_events.lock().unwrap();
                                                (pending.capture_offset_sec(utt_id), pending.complete(utt_id, inference_sec))
                                            };

                                            if !text.is_empty() {
                                                let blacklist = state_events.config.read().await.blacklist.clone();
                                                if is_text_blacklisted(text, &blacklist) {
                                                    tracing::info!("🚫 [Blacklist Filter] Descartando transcripción alucinada [{}] ({} caracteres)", utt_id, text.chars().count());
                                                } else {
                                                    let trimmed_text = {
                                                        let prev_guard = state_events.previous_transcript.read().await;
                                                        if let Some(ref prev) = *prev_guard {
                                                            trim_boundary_overlap(prev, text)
                                                        } else {
                                                            text.to_string()
                                                        }
                                                    };

                                                    if trimmed_text.trim().is_empty() {
                                                        tracing::debug!("🔇 [ASR Solapamiento descartado] [{}] (todo el texto era duplicado del chunk anterior)", utt_id);
                                                    } else {
                                                        *state_events.previous_transcript.write().await = Some(text.to_string());
                                                        let text_to_emit = trimmed_text;

                                                        if let Some(sink) = state_events.session_sink.lock().await.clone() {
                                                            let raw_response = AsrTranscriptionResponse {
                                                                request_id: utt_id.to_string(),
                                                                text: text_to_emit.clone(),
                                                                language: session_language.clone(),
                                                                duration_sec: dur_sec as f32,
                                                                latency_sec: inference_sec as f32,
                                                                segments: Vec::new(),
                                                            };
                                                            // Cues sit on the capture timeline; emit time is only a
                                                            // fallback when the utterance was not tracked.
                                                            let start_sec = match capture_offset_sec {
                                                                Some(offset) => offset,
                                                                None => state_events.session_origin.read().await
                                                                    .map_or(0.0, |origin| (origin.elapsed().as_secs_f64() - dur_sec).max(0.0)),
                                                            };
                                                            let cue = TranscriptionCue::new(
                                                                state_events.session_cue_counter.fetch_add(1, Ordering::SeqCst) + 1,
                                                                start_sec,
                                                                start_sec + dur_sec,
                                                                text_to_emit.clone(),
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
                                                            text: presentation_text(&text_to_emit),
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
                                                    }
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
    let audio_device_id = config.capture_device_id();

    let stream_config = AudioStreamConfig {
        device_id: audio_device_id.clone(),
        sample_rate: 16000,
        channels: 1,
        chunk_size: 512,
        ring_buffer_capacity: 500,
    };

    let audio_source = CpalAudioSource::new().with_rnnoise_enabled(config.rnnoise_enabled);
    let (mut audio_capture, mut audio_rx) = match ResilientCapture::start(
        audio_source,
        stream_config,
        liveaudio_audio::get_input_device_count,
    )
    .await
    {
        Ok(started) => started,
        Err(error) => {
            drop(supervisor_arc);
            let err_msg = rollback_failed_audio_start(
                state,
                event_task,
                cancel_token.clone(),
                ws_created_here,
                format!(
                    "Error al inicializar la captura de audio (dispositivo o RNNoise): {}",
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
            audio_capture.stop().await;
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
            audio_capture.stop().await;
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
    let resumed =
        previous_session_path.is_some() && previous_session_path == session_path.as_deref();
    let previous_origin = *state.session_origin.read().await;
    let (session_origin, last_cue) =
        session_timeline(session_path.as_deref(), previous_origin, resumed);
    *state.session_origin.write().await = Some(session_origin);
    state.session_cue_counter.store(last_cue, Ordering::SeqCst);
    *state.session_sink.lock().await = session_sink;
    *state.session_path.write().await = session_path;

    // 7. Background Task: Real-time Audio VAD + Speech Chunking -> Faster-Whisper Worker
    let app_audio = app.clone();
    let state_audio = state.clone();
    let cancel_audio = cancel_token.clone();
    let supervisor_audio = supervisor_arc.clone();
    let ws_audio = ws_server.clone();
    let acoustic_events_enabled = config.acoustic_events_enabled;
    let decode_timeout_ms = u64::from(config.asr_decode_timeout_sec) * 1000;

    let pre_buffer_capacity = vad.config().pre_buffer_chunks().max(1);
    let max_samples = (16000.0 * config.max_chunk_duration.ceil()) as usize;

    let mut audio_state_rx = audio_capture.state();
    let initial_audio_state = *audio_state_rx.borrow_and_update();
    *state.audio_state.write().await = initial_audio_state;

    let audio_task = tokio::spawn(async move {
        let mut utterance_samples: Vec<f32> = Vec::with_capacity(max_samples.max(16000 * 5));
        let mut pre_roll_chunks: VecDeque<Vec<f32>> =
            VecDeque::with_capacity(pre_buffer_capacity + 1);
        let mut is_in_utterance = false;
        let mut utterance_started_at = Instant::now();
        let mut utterance_seq = 0u64;
        let mut acoustic_lifecycle = AcousticEventLifecycle::new();
        let mut close_reason = AcousticSpeechEndReason::ServiceStopped;

        'capture: loop {
            tokio::select! {
                _ = cancel_audio.cancelled() => break,
                Ok(()) = audio_state_rx.changed() => {
                    let audio_state = *audio_state_rx.borrow_and_update();
                    *state_audio.audio_state.write().await = audio_state;
                    if let Ok(current_status) = get_service_status_internal(&state_audio).await {
                        let _ = app_audio.emit("service_status_changed", &current_status);
                    }
                }
                chunk_opt = audio_rx.recv() => {
                    let chunk = match chunk_opt {
                        Some(c) => {
                            if capture_error_preempts_audio(&audio_rx) {
                                close_reason = AcousticSpeechEndReason::CaptureError;
                                break 'capture;
                            }
                            c
                        },
                        None => {
                            close_reason = audio_receiver_close_reason(&audio_rx);
                            break;
                        },
                    };

                    for frame in chunk.samples.chunks_exact(512) {
                        if capture_error_preempts_audio(&audio_rx) {
                            close_reason = AcousticSpeechEndReason::CaptureError;
                            break 'capture;
                        }
                        match vad.evaluate_chunk(frame) {
                            Ok(decision) => {
                                observe_and_publish_acoustic_transition(
                                    &mut acoustic_lifecycle,
                                    &ws_audio,
                                    acoustic_events_enabled,
                                    decision.transition,
                                );
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
                                        // Pre-roll audio was captured before this frame was evaluated.
                                        let buffered = Duration::from_secs_f64(utterance_samples.len() as f64 / 16000.0);
                                        utterance_started_at = Instant::now().checked_sub(buffered).unwrap_or_else(Instant::now);
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
                                                let (clean_lang, context_prompt) = {
                                                    let cfg = state_audio.config.read().await;
                                                    let clean_lang = if !cfg.asr_language.is_empty() {
                                                        cfg.asr_language.clone()
                                                    } else {
                                                        cfg.language
                                                            .as_deref()
                                                            .filter(|language| !language.is_empty() && *language != "auto")
                                                            .unwrap_or("es")
                                                            .to_string()
                                                    };
                                                    let prompt = if clean_lang == "en" {
                                                        cfg.whisper_context_prompt_en.trim()
                                                    } else {
                                                        cfg.whisper_context_prompt_es.trim()
                                                    };
                                                    let context_prompt = if prompt.is_empty() {
                                                        None
                                                    } else {
                                                        Some(prompt.to_string())
                                                    };
                                                    (clean_lang, context_prompt)
                                                };

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
                                                        "language": clean_lang,
                                                        "context_prompt": context_prompt,
                                                        "timeout_ms": decode_timeout_ms,
                                                    }
                                                });

                                                let utterance_id = format!("utt_{}", utterance_seq);
                                                let created_at_sec = SystemTime::now().duration_since(UNIX_EPOCH)
                                                    .map(|d| d.as_secs_f64()).unwrap_or(0.0);
                                                let capture_offset_sec = utterance_started_at
                                                    .saturating_duration_since(session_origin)
                                                    .as_secs_f64();
                                                pending_utterances.lock().unwrap().track_with_capture_offset(
                                                    utterance_id.clone(),
                                                    created_at_sec,
                                                    capture_offset_sec,
                                                );

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

        if audio_rx.capture_failed() {
            close_reason = AcousticSpeechEndReason::CaptureError;
        }
        close_and_publish_acoustic_attempt(
            &mut acoustic_lifecycle,
            &ws_audio,
            acoustic_events_enabled,
            close_reason,
        );
        audio_capture.stop().await;
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
    let _ = app.emit(
        "runtime_setup_progress",
        RuntimeSetupProgress {
            phase: "ready".to_string(),
            text: "ASR runtime is ready.".to_string(),
            percent: Some(100.0),
        },
    );
    Ok(status)
}

/// Bundled uv resource path, relative to the app resource directory.
pub(crate) const BUNDLED_UV_RESOURCE: &str = if cfg!(windows) {
    "packaging/vendor/uv.exe"
} else {
    "packaging/vendor/uv"
};

/// Prefer the bundled uv when it exists on disk, otherwise fall back to discovery (bundle roots, PATH).
pub(crate) fn select_uv_executable(
    bundled: Option<PathBuf>,
    fallback: impl FnOnce() -> Option<PathBuf>,
) -> Result<PathBuf, String> {
    bundled
        .filter(|path| path.is_file())
        .or_else(fallback)
        .ok_or_else(|| {
            format!(
                "uv executable not found: expected the bundled {BUNDLED_UV_RESOURCE} or uv on PATH; reinstall LiveAudio or install uv"
            )
        })
}

async fn prepare_managed_runtime(
    app: &AppHandle,
    state: &AppState,
    backend: AsrBackend,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<(), String> {
    {
        let mut active = state.runtime_setup_token.lock().await;
        if active.is_some() {
            return Err("ASR runtime preparation is already running".to_string());
        }
        *active = Some(cancellation.clone());
    }

    let setup_result = async {
        let bundled = app
            .path()
            .resolve(BUNDLED_UV_RESOURCE, tauri::path::BaseDirectory::Resource)
            .ok();
        let uv = select_uv_executable(bundled, liveaudio_core::runtime::find_uv_executable)?;
        let worker = WorkerProcessConfig::find_asr_worker_source()
            .ok_or_else(|| "Bundled ASR worker package is unavailable".to_string())?;
        let package_root = WorkerProcessConfig::find_package_root(&Some(worker))
            .ok_or_else(|| "Bundled ASR worker package root is unavailable".to_string())?;
        let app_events = app.clone();
        provision_runtime_with_uv_cancellable(
            &managed_runtime_root(),
            &package_root,
            backend,
            &uv,
            cancellation,
            move |progress: RuntimeSetupProgress| {
                let _ = app_events.emit("runtime_setup_progress", &progress);
            },
        )
        .await
        .map_err(|error| error.to_string())
    }
    .await;
    *state.runtime_setup_token.lock().await = None;
    setup_result
}

/// Prepare or repair the managed ASR environment from the runtime bundled with the desktop app.
#[tauri::command]
pub async fn prepare_asr_runtime(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let _transition_guard = state.transition_lock.lock().await;
    if state.is_running.load(Ordering::SeqCst) {
        return Err("Stop the transcription service before repairing its ASR runtime".to_string());
    }
    let config = state.config.read().await.clone();
    let backend = if config.device.eq_ignore_ascii_case("cuda") {
        AsrBackend::Cu121
    } else {
        AsrBackend::Cpu
    };
    prepare_managed_runtime(
        &app,
        state.inner(),
        backend,
        tokio_util::sync::CancellationToken::new(),
    )
    .await
}

/// Cancel active runtime preparation or the startup readiness handshake without taking the transition lock.
#[tauri::command]
pub async fn cancel_runtime_setup(state: State<'_, AppState>) -> Result<bool, String> {
    let mut cancelled = false;
    if let Some(token) = state.runtime_setup_token.lock().await.as_ref() {
        token.cancel();
        cancelled = true;
    }
    if let Some(token) = state.startup_cancel_token.lock().await.as_ref() {
        token.cancel();
        cancelled = true;
    }
    Ok(cancelled)
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
    *state.asr_progress.write().await = None;
    if let Ok(stopping_status) = get_service_status_internal(state).await {
        let _ = app.emit("service_status_changed", &stopping_status);
    }

    let shutdown = shutdown_service_resources(state).await;
    let status = get_service_status_internal(state).await?;
    let _ = app.emit("service_status_changed", &status);
    shutdown?;

    tracing::info!("Transcription service stopped cleanly via Tauri IPC command");
    Ok(status)
}

/// Stops the service for application exit, aborting any in-flight startup first.
pub(crate) async fn stop_service_for_exit(app: &AppHandle, state: &AppState) {
    if let Some(token) = state.startup_cancel_token.lock().await.as_ref() {
        token.cancel();
    }
    let _transition_guard = state.transition_lock.lock().await;
    if let Err(error) = stop_service_locked(app, state).await {
        tracing::warn!("Service did not stop cleanly on exit: {}", error);
    }
}

/// Releases every service resource (capture tasks, session sinks, ASR worker, WebSocket).
/// Every cleanup step runs even when an earlier one fails; the first failure is returned.
pub(crate) async fn shutdown_service_resources(state: &AppState) -> Result<(), String> {
    state.is_running.store(false, Ordering::SeqCst);
    state.vad_onset.store(false, Ordering::SeqCst);
    {
        let mut start_guard = state.start_time.write().await;
        *start_guard = None;
    }
    *state.previous_transcript.write().await = None;

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

    // Report "stopped" only once every resource was actually released.
    let (asr_state, result) = match shutdown_error {
        Some(error) => ("error: stop_failed", Err(error)),
        None => ("stopped", Ok(())),
    };
    *state.asr_state.write().await = asr_state.to_string();
    state.service_lock.lock().await.take();
    result
}

/// Restart the LiveAudio transcription service.
#[tauri::command]
pub async fn restart_service(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ServiceStatusDto, String> {
    let _transition_guard = state.transition_lock.lock().await;
    let startup_token = tokio_util::sync::CancellationToken::new();
    *state.startup_cancel_token.lock().await = Some(startup_token.clone());
    if state.is_running.load(Ordering::SeqCst) {
        if let Err(error) = stop_service_locked(&app, state.inner()).await {
            *state.startup_cancel_token.lock().await = None;
            return Err(error);
        }
    }
    let result = start_service_locked(&app, state.inner(), true, startup_token).await;
    *state.startup_cancel_token.lock().await = None;
    result
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

const OBS_OVERLAY_RELATIVE: &str = "liveaudio/assets/subtitulos_obs.html";

/// Find the OBS overlay: bundled resource directory first, then dev checkout fallbacks.
pub(crate) fn resolve_obs_overlay_path(resource_dir: Option<&Path>) -> Option<PathBuf> {
    resource_dir
        .map(|dir| dir.join(OBS_OVERLAY_RELATIVE))
        .into_iter()
        .chain(
            ["", "../", "../../"]
                .iter()
                .map(|prefix| PathBuf::from(format!("{prefix}{OBS_OVERLAY_RELATIVE}"))),
        )
        .find(|path| path.is_file())
        .map(|path| std::path::absolute(&path).unwrap_or(path))
}

/// Build a percent-encoded `file:///` URL for the overlay with the `?port=` query.
pub(crate) fn obs_overlay_url(path: &Path, port: u16) -> Result<String, String> {
    let raw = path.to_string_lossy();
    let clean = raw.strip_prefix(r"\\?\").unwrap_or(&raw);
    let mut url = tauri::Url::from_file_path(clean)
        .map_err(|_| format!("OBS overlay path is not absolute: {}", path.display()))?;
    url.set_query(Some(&format!("port={port}")));
    Ok(url.to_string())
}

/// Resolve OBS overlay URL and browser source recommendations.
#[tauri::command]
pub async fn get_obs_overlay_info(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ObsOverlayDto, String> {
    let port = if state.is_running() {
        state.active_port.load(Ordering::SeqCst)
    } else {
        state.config.read().await.ws_port
    };

    let resource_dir = app.path().resource_dir().ok();
    let overlay = resolve_obs_overlay_path(resource_dir.as_deref()).ok_or_else(|| {
        "OBS overlay file subtitulos_obs.html was not found; reinstall LiveAudio".to_string()
    })?;
    let url = obs_overlay_url(&overlay, port)?;

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

/// Retrieve recent lines from the application log file.
#[tauri::command]
pub async fn get_recent_logs(max_lines: Option<usize>) -> Result<Vec<String>, String> {
    let limit = max_lines.unwrap_or(100);
    let data_home = liveaudio_core::config::get_data_home();
    let log_file_path = data_home.join("logs").join("liveaudio-desktop.log");

    if !log_file_path.is_file() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&log_file_path).map_err(|e| e.to_string())?;
    let lines: Vec<String> = content
        .lines()
        .rev()
        .take(limit)
        .map(|s| s.to_string())
        .collect();
    let mut reversed = lines;
    reversed.reverse();
    Ok(reversed)
}

fn fold_diacritics(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' | 'ã' | 'ā' => 'a',
            'é' | 'è' | 'ë' | 'ê' | 'ē' => 'e',
            'í' | 'ì' | 'ï' | 'î' | 'ī' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' | 'ō' => 'o',
            'ú' | 'ù' | 'ü' | 'û' | 'ū' => 'u',
            other => other,
        })
        .collect()
}

/// Checks whether a transcribed text matches any phrase in the blacklist.
/// Matches case-insensitively, folds diacritics/accents, splits on commas/semicolons/newlines,
/// trims whitespace, and strips standard and typographical quotes.
pub fn is_text_blacklisted(text: &str, blacklist_csv: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let text_folded = fold_diacritics(trimmed);
    blacklist_csv
        .split(|c| c == ',' || c == ';' || c == '\n' || c == '\r')
        .map(|entry| {
            entry
                .trim()
                .trim_matches(|c| {
                    c == '"' || c == '\'' || c == '“' || c == '”' || c == '‘' || c == '’' || c == '«' || c == '»'
                })
                .trim()
        })
        .filter(|entry| !entry.is_empty())
        .any(|phrase| {
            let phrase_folded = fold_diacritics(phrase);
            !phrase_folded.is_empty() && text_folded.contains(&phrase_folded)
        })
}

#[derive(Debug, Clone)]
struct WordToken {
    normalized: String,
    end_byte: usize,
}

fn normalize_word(token: &str) -> String {
    fold_diacritics(token)
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn extract_words(s: &str) -> Vec<WordToken> {
    let mut words = Vec::new();
    let mut start_idx = None;

    for (idx, ch) in s.char_indices() {
        if ch.is_whitespace() {
            if let Some(start) = start_idx {
                let token = &s[start..idx];
                let norm = normalize_word(token);
                if !norm.is_empty() {
                    words.push(WordToken {
                        normalized: norm,
                        end_byte: idx,
                    });
                }
                start_idx = None;
            }
        } else if start_idx.is_none() {
            start_idx = Some(idx);
        }
    }

    if let Some(start) = start_idx {
        let token = &s[start..];
        let norm = normalize_word(token);
        if !norm.is_empty() {
            words.push(WordToken {
                normalized: norm,
                end_byte: s.len(),
            });
        }
    }

    words
}

fn clean_trimmed_remainder(remainder: &str) -> &str {
    remainder
        .trim_start_matches(|c: char| {
            c.is_whitespace()
                || c == ','
                || c == '.'
                || c == '-'
                || c == '—'
                || c == '–'
                || c == '…'
                || c == ':'
                || c == ';'
        })
        .trim()
}

/// Trims boundary overlap words from `curr` that duplicate trailing words of `prev`.
///
/// Compares trailing 1..=max_words of `prev` with leading 1..=max_words of `curr`
/// (case-insensitive, normalized punctuation and diacritics).
/// If an overlap of length `k` is detected, the overlapping prefix in `curr` is stripped.
/// Returns the trimmed remainder of `curr` (which may be empty).
pub fn trim_boundary_overlap(prev: &str, curr: &str) -> String {
    trim_boundary_overlap_with_max(prev, curr, 5)
}

/// Trims boundary overlap words from `curr` that duplicate trailing words of `prev`,
/// testing overlaps up to `max_words` tokens (from largest down to 1).
pub fn trim_boundary_overlap_with_max(prev: &str, curr: &str, max_words: usize) -> String {
    let curr_trimmed = curr.trim();
    if curr_trimmed.is_empty() {
        return String::new();
    }
    let prev_trimmed = prev.trim();
    if prev_trimmed.is_empty() || max_words == 0 {
        return curr_trimmed.to_string();
    }

    let prev_words = extract_words(prev_trimmed);
    let curr_words = extract_words(curr_trimmed);

    if prev_words.is_empty() || curr_words.is_empty() {
        return curr_trimmed.to_string();
    }

    let max_k = max_words.min(prev_words.len()).min(curr_words.len());

    for k in (1..=max_k).rev() {
        let prev_slice = &prev_words[prev_words.len() - k..];
        let curr_slice = &curr_words[..k];

        let matches = prev_slice
            .iter()
            .zip(curr_slice.iter())
            .all(|(p, c)| p.normalized == c.normalized);

        if matches {
            let end_byte = curr_slice.last().map(|w| w.end_byte).unwrap_or(0);
            let remainder = if end_byte < curr_trimmed.len() {
                &curr_trimmed[end_byte..]
            } else {
                ""
            };
            return clean_trimmed_remainder(remainder).to_string();
        }
    }

    curr_trimmed.to_string()
}



