// SPDX-License-Identifier: MIT

//! Unit tests for LiveAudio Tauri Desktop backend.

use crate::commands::{
    get_service_status_internal, persist_config_and_apply, profile_presets, stop_active_websocket,
};
use crate::dto::*;
use crate::state::AppState;
use liveaudio_core::config::LiveAudioConfig;
use liveaudio_core::sink::{SinkError, SinkStats, TranscriptionCue, TranscriptionSink};
use liveaudio_core::supervisor::traits::ServiceSupervisor;
use liveaudio_core::supervisor::{is_parent_alive, ProcessSupervisor, WorkerProcessConfig};
use liveaudio_core::AsrTranscriptionResponse;
use liveaudio_network::{NetworkServer, WsServer, WsServerConfig};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn author_profile_opener_uses_the_fixed_github_destination() {
    let (program, args) = crate::commands::author_profile_open_command();

    assert_eq!(args.last(), Some(&"https://github.com/franguh"));
    #[cfg(target_os = "windows")]
    {
        assert_eq!(program, "rundll32.exe");
        assert_eq!(
            args,
            &["url.dll,FileProtocolHandler", "https://github.com/franguh"][..]
        );
    }
    #[cfg(target_os = "macos")]
    {
        assert_eq!(program, "open");
        assert_eq!(args, &["https://github.com/franguh"][..]);
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        assert_eq!(program, "xdg-open");
        assert_eq!(args, &["https://github.com/franguh"][..]);
    }
}

#[tokio::test]
async fn desktop_audio_dispatch_cancels_while_worker_command_queue_is_saturated() {
    let mut config = WorkerProcessConfig::default();
    config.name = "desktop-nonreading-worker".to_string();
    config.command = "python".to_string();
    config.args = vec![
        "-u".to_string(),
        "-c".to_string(),
        "import json,time; print(json.dumps({'event':'ready'}), flush=True); time.sleep(60)"
            .to_string(),
    ];
    config.ready_timeout = std::time::Duration::from_secs(5);
    config.auto_respawn = false;
    let mut supervisor =
        ProcessSupervisor::new(config, std::process::id(), 0, None).expect("supervisor creation");
    supervisor
        .start()
        .await
        .expect("mock worker ready handshake");
    let supervisor = Arc::new(supervisor);
    let payload = serde_json::json!({"cmd":"transcribe", "payload":"x".repeat(4096)});

    let mut producers = Vec::new();
    for _ in 0..256 {
        let supervisor = supervisor.clone();
        let payload = payload.clone();
        producers.push(tokio::spawn(async move {
            supervisor.send_command(&payload).await
        }));
    }
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while producers.iter().all(|producer| producer.is_finished()) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("worker queue producers should be active");
    assert!(
        producers.iter().any(|producer| !producer.is_finished()),
        "nonreading worker must leave commands pending"
    );

    let cancellation = tokio_util::sync::CancellationToken::new();
    let task: tokio::task::JoinHandle<()> = {
        let supervisor = supervisor.clone();
        let cancellation = cancellation.clone();
        let payload = payload.clone();
        tokio::spawn(async move {
            crate::commands::send_audio_command_until_cancelled(
                &supervisor,
                &cancellation,
                &payload,
            )
            .await
            .expect("cancellation is a clean exit");
        })
    };
    let task_abort = task.abort_handle();
    tokio::task::yield_now().await;
    cancellation.cancel();
    let completed = match tokio::time::timeout(std::time::Duration::from_secs(1), task).await {
        Ok(result) => {
            result.expect("audio task join");
            true
        }
        Err(_) => {
            task_abort.abort();
            false
        }
    };

    for producer in producers {
        producer.abort();
        let _ = producer.await;
    }
    let mut supervisor =
        Arc::try_unwrap(supervisor).unwrap_or_else(|_| panic!("all producer tasks aborted"));
    supervisor
        .stop(std::time::Duration::from_secs(1))
        .await
        .expect("desktop lifecycle must still reap the real worker");
    assert!(
        completed,
        "desktop audio submission must observe cancellation instead of waiting on the queue"
    );
}

#[derive(Clone)]
struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

impl Write for CapturedLogs {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturedLogs {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[test]
fn test_app_state_initialization() {
    let mut config = LiveAudioConfig::default();
    config.ws_port = 8765;
    config.device = "cuda".to_string();
    config.model_size = "small (Balance CPU)".to_string();

    let state = AppState::new(config);
    assert!(!state.is_running());
    assert!(!state.vad_onset());
    assert_eq!(state.ws_port(), 8765);
    assert_eq!(state.ws_clients(), 0);
}

#[tokio::test]
async fn test_app_state_toggle_running() {
    let state = AppState::default();
    assert!(!state.is_running());

    state.is_running.store(true, Ordering::SeqCst);
    {
        let mut asr = state.asr_state.write().await;
        *asr = "listening".to_string();
    }

    assert!(state.is_running());
    assert_eq!(*state.asr_state.read().await, "listening");

    state.is_running.store(false, Ordering::SeqCst);
    {
        let mut asr = state.asr_state.write().await;
        *asr = "stopped".to_string();
    }
    assert!(!state.is_running());
    assert_eq!(*state.asr_state.read().await, "stopped");
}

#[tokio::test]
async fn test_config_normalization_preserves_valid_fields() {
    let mut config = LiveAudioConfig::default();
    config.ws_port = 9000;
    config.device = "cpu".to_string();
    config.model_size = "tiny (Más rápido, baja precisión)".to_string();
    config.subtitle_style = "neon".to_string();

    let updated = config.normalize();
    assert!(!updated);
    assert_eq!(config.ws_port, 9000);
    assert_eq!(config.device, "cpu");
    assert_eq!(config.subtitle_style, "neon");
}

#[tokio::test]
async fn test_config_clamping_on_invalid_bounds() {
    let mut config = LiveAudioConfig::default();
    config.silence_timeout = 99.0;
    config.subtitle_ribbon_max_lines = 100;
    config.vad_speech_pad_ms = 10000;

    let updated = config.normalize();
    assert!(updated);
    assert_eq!(config.silence_timeout, 2.0);
    assert_eq!(config.subtitle_ribbon_max_lines, 8);
    assert_eq!(config.vad_speech_pad_ms, 500);
}

#[test]
fn test_audio_device_dto_serialization() {
    let dev = AudioDeviceDto {
        id: "default_mic".to_string(),
        name: "Micrófono USB".to_string(),
        is_default: true,
        kind: "input".to_string(),
    };

    let json = serde_json::to_string(&dev).expect("Serialization failed");
    assert!(json.contains("default_mic"));
    assert!(json.contains("Micrófono USB"));
    assert!(json.contains("\"is_default\":true"));
}

#[tokio::test]
async fn enumerated_audio_device_ids_match_cpal_resolver_ids() {
    let enumerated = liveaudio_audio::enumerate_cpal_devices().expect("enumerate CPAL devices");
    let desktop_devices = crate::commands::get_audio_devices()
        .await
        .expect("desktop device enumeration");

    for device in enumerated {
        assert!(
            desktop_devices
                .iter()
                .any(|candidate| candidate.id == device.id),
            "desktop selection id does not match CPAL id for {}",
            device.name
        );
    }
    #[cfg(target_os = "windows")]
    assert!(desktop_devices
        .iter()
        .any(|device| device.id == "default_loopback"));
}

#[tokio::test]
async fn config_is_persisted_before_it_is_applied_to_runtime_state() {
    let mut old_config = LiveAudioConfig::default();
    old_config.subtitle_style = "neon".to_string();
    let state = AppState::new(old_config);
    let mut reset_config = LiveAudioConfig::default();
    reset_config.subtitle_style = "default".to_string();
    let home = isolated_test_home();
    let path = home.join("config.json");

    crate::commands::persist_config_and_apply(&state, reset_config.clone(), |config| {
        assert_eq!(
            state
                .config
                .try_read()
                .expect("config lock available")
                .subtitle_style,
            "neon",
            "runtime state must remain unchanged until persistence completes"
        );
        liveaudio_core::config::save_to_path(config, &path)
            .map(|_| true)
            .map_err(|error| error.to_string())
    })
    .await
    .expect("persist and apply reset config");

    assert_eq!(*state.config.read().await, reset_config);
    assert_eq!(
        liveaudio_core::config::load_from_path(&path).expect("load persisted config"),
        reset_config
    );
    let _ = std::fs::remove_dir_all(home);
}

#[tokio::test]
async fn saving_settings_does_not_relabel_the_active_runtime() {
    let mut active = LiveAudioConfig::default();
    active.ws_port = 18765;
    active.model_size = "base (Rápido)".to_string();
    active.device = "cpu".to_string();
    let state = AppState::new(active.clone());
    state.is_running.store(true, Ordering::SeqCst);
    *state.active_config.write().await = Some(active.clone());

    let mut saved = active;
    saved.ws_port = 18766;
    saved.model_size = "turbo (Máxima precisión GPU)".to_string();
    saved.device = "cuda".to_string();
    persist_config_and_apply(&state, saved.clone(), |_| Ok(true))
        .await
        .expect("save draft");

    let status = get_service_status_internal(&state)
        .await
        .expect("runtime status");
    assert_eq!(status.ws_port, 18765);
    assert_eq!(status.model_size, "base (Rápido)");
    assert_eq!(status.device, "cpu");
    assert_eq!(state.config.read().await.ws_port, 18766);
}

#[tokio::test]
async fn stopped_status_uses_saved_websocket_port_not_a_stale_runtime_port() {
    let state = AppState::new(LiveAudioConfig::default());
    let mut saved = LiveAudioConfig::default();
    saved.ws_port = 18767;
    persist_config_and_apply(&state, saved, |_| Ok(true))
        .await
        .expect("save settings while stopped");
    let status = get_service_status_internal(&state)
        .await
        .expect("stopped service status");
    assert!(!status.is_running);
    assert_eq!(status.ws_port, 18767);
}

#[tokio::test]
async fn stopping_runtime_websocket_releases_port_for_rebind() {
    let state = AppState::default();
    let base = 45_000
        + (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_millis()
            % 1000) as u16;
    let mut original = WsServer::new(WsServerConfig {
        base_port: base,
        ..Default::default()
    });
    let bound = NetworkServer::start(&mut original, base)
        .await
        .expect("bind original listener");
    state.active_port.store(bound, Ordering::SeqCst);
    *state.ws_server.lock().await = Some(original);

    stop_active_websocket(&state)
        .await
        .expect("stop runtime listener");
    assert_eq!(state.active_port.load(Ordering::SeqCst), 0);

    let mut replacement = WsServer::new(WsServerConfig {
        base_port: bound,
        ..Default::default()
    });
    let rebound = NetworkServer::start(&mut replacement, bound)
        .await
        .expect("rebind released port");
    assert_eq!(rebound, bound);
    NetworkServer::stop(&mut replacement)
        .await
        .expect("stop replacement");
}

#[tokio::test]
async fn session_writers_follow_opt_in_flags_and_stay_under_output_root() {
    let home = isolated_test_home();
    let mut config = LiveAudioConfig::default();
    config.output_dir = home.join("sessions");
    config.continuous_session = false;
    config.save_transcript_enabled = true;
    config.save_vtt_enabled = false;
    assert!(
        crate::commands::create_session_sinks(&config, None)
            .expect("create non-continuous session output")
            .0
            .is_some(),
        "recording opt-in must not depend on continuous-session mode"
    );
    assert!(config.output_dir.exists());

    let (sink, path) =
        crate::commands::create_session_sinks(&config, None).expect("create session sink");
    let sink = sink.expect("JSONL writer enabled");
    let path = path.expect("backend session path");
    assert!(path.starts_with(std::fs::canonicalize(&config.output_dir).unwrap()));

    let response = AsrTranscriptionResponse {
        request_id: "synthetic-1".to_string(),
        text: "synthetic-session-fixture".to_string(),
        language: "en".to_string(),
        duration_sec: 1.0,
        latency_sec: 0.1,
        segments: Vec::new(),
    };
    let cue = TranscriptionCue::new(1, 0.0, 1.0, response.text.clone(), response);
    sink.emit(&cue).await.expect("queue JSONL cue");
    sink.stop(std::time::Duration::from_secs(2))
        .await
        .expect("flush JSONL cue");
    assert!(path.join("transcript.jsonl").exists());
    assert!(!path.join("subtitles.vtt").exists());
    let _ = std::fs::remove_dir_all(home);
}

#[tokio::test]
async fn session_stop_reports_real_sink_write_failures() {
    let home = isolated_test_home();
    let mut config = LiveAudioConfig::default();
    config.output_dir = home.join("sessions");
    config.save_transcript_enabled = true;
    config.save_vtt_enabled = false;
    let (sink, path) =
        crate::commands::create_session_sinks(&config, None).expect("create session");
    let sink = sink.expect("JSONL sink");
    let path = path.expect("session path");
    let state = AppState::default();
    *state.session_sink.lock().await = Some(sink.clone());
    *state.session_path.write().await = Some(path.clone());
    std::fs::create_dir(path.join("transcript.jsonl")).expect("make target unwritable");

    let response = AsrTranscriptionResponse {
        request_id: "synthetic-write-failure".to_string(),
        text: "synthetic-write-failure-fixture".to_string(),
        language: "en".to_string(),
        duration_sec: 1.0,
        latency_sec: 0.1,
        segments: Vec::new(),
    };
    let cue = TranscriptionCue::new(1, 0.0, 1.0, response.text.clone(), response);
    sink.emit(&cue).await.expect("enqueue synthetic cue");
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    let status = get_service_status_internal(&state)
        .await
        .expect("session status");
    assert!(
        status
            .session_error
            .as_deref()
            .unwrap_or_default()
            .contains("1 write"),
        "service status must expose asynchronous output failures"
    );
    assert!(
        sink.stop(std::time::Duration::from_secs(2)).await.is_err(),
        "stop must report a failed disk write after draining"
    );
    let _ = std::fs::remove_dir_all(home);
}

#[tokio::test]
async fn session_drain_timeout_is_retained_in_status_after_sink_removal() {
    let state = AppState::default();
    *state.session_sink.lock().await = None;
    let stats = SinkStats {
        pending: 1,
        ..SinkStats::default()
    };
    crate::commands::record_session_stop_failure(&state, &SinkError::DrainTimeout, &stats).await;
    let status = get_service_status_internal(&state)
        .await
        .expect("stopped status");
    assert!(
        status
            .session_error
            .as_deref()
            .unwrap_or_default()
            .contains("drain"),
        "a drain timeout must survive after the sink is removed"
    );
}

#[tokio::test]
async fn continuous_session_reuses_session_directory_on_restart() {
    let home = isolated_test_home();
    let mut config = LiveAudioConfig::default();
    config.output_dir = home.join("sessions");
    config.continuous_session = true;
    config.save_transcript_enabled = true;
    config.save_vtt_enabled = false;
    let first_path = crate::commands::create_session_sinks(&config, None)
        .expect("first start")
        .1
        .expect("first session path");
    let manual_reuse = crate::commands::session_reuse_path(&config, Some(&first_path), false);
    let manual_start_path = crate::commands::create_session_sinks(&config, manual_reuse)
        .expect("manual stop/start")
        .1
        .expect("manual session path");
    assert_ne!(
        first_path, manual_start_path,
        "manual stop/start creates a new session even when continuity is enabled"
    );
    let restart_reuse = crate::commands::session_reuse_path(&config, Some(&first_path), true);
    let restart_path = crate::commands::create_session_sinks(&config, restart_reuse)
        .expect("restart")
        .1
        .expect("restart session path");
    assert_eq!(
        first_path, restart_path,
        "continuous sessions keep one directory across restarts"
    );
    config.continuous_session = false;
    let rotated = crate::commands::create_session_sinks(&config, Some(&restart_path))
        .expect("non-continuous restart")
        .1
        .expect("rotated session path");
    assert_ne!(
        restart_path, rotated,
        "non-continuous sessions rotate directories on restart"
    );
    let _ = std::fs::remove_dir_all(home);
}

#[tokio::test]
async fn transcript_and_vtt_flags_independently_control_session_sinks() {
    let home = isolated_test_home();
    let response = AsrTranscriptionResponse {
        request_id: "synthetic-flags".to_string(),
        text: "synthetic-session-flags-fixture".to_string(),
        language: "en".to_string(),
        duration_sec: 1.0,
        latency_sec: 0.1,
        segments: Vec::new(),
    };
    for (jsonl, vtt) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut config = LiveAudioConfig::default();
        config.output_dir = home.join("sessions");
        config.continuous_session = false;
        config.save_transcript_enabled = jsonl;
        config.save_vtt_enabled = vtt;
        let (sink, path) = crate::commands::create_session_sinks(&config, None)
            .expect("construct session writers");
        assert_eq!(sink.is_some(), jsonl || vtt);
        assert_eq!(path.is_some(), jsonl || vtt);
        if let Some(sink) = sink {
            let path = path.unwrap();
            let cue = TranscriptionCue::new(1, 0.0, 1.0, response.text.clone(), response.clone());
            sink.emit(&cue).await.expect("queue cue");
            sink.stop(std::time::Duration::from_secs(2))
                .await
                .expect("write selected outputs");
            assert_eq!(path.join("transcript.jsonl").exists(), jsonl);
            assert_eq!(path.join("subtitles.vtt").exists(), vtt);
        }
    }
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn profile_presets_match_python_runtime_values() {
    let presets = profile_presets();
    assert_eq!(
        presets
            .iter()
            .map(|profile| profile.id.as_str())
            .collect::<Vec<_>>(),
        vec!["fast", "balanced", "quality", "stable_streaming"]
    );
    let fast = &presets[0];
    assert_eq!(fast.device, "cpu");
    assert_eq!(fast.model_size, "base (Rápido)");
    assert_eq!(fast.subtitle_backlog_policy, "live_only");
    assert_eq!(fast.subtitle_catchup_interval_sec, 0.8);
    let balanced = &presets[1];
    assert_eq!(balanced.subtitle_backlog_policy, "auto");
    assert_eq!(balanced.subtitle_max_live_delay_sec, 10.0);
    let quality = &presets[2];
    assert_eq!(quality.model_size, "turbo (Máxima precisión GPU)");
    assert_eq!(quality.subtitle_catchup_interval_sec, 2.0);
    let stable = &presets[3];
    assert_eq!(stable.device, "cpu");
    assert_eq!(stable.subtitle_max_live_delay_sec, 6.0);
}

#[tokio::test]
async fn config_persistence_failure_leaves_runtime_state_unchanged() {
    let mut old_config = LiveAudioConfig::default();
    old_config.subtitle_style = "neon".to_string();
    let state = AppState::new(old_config.clone());
    let reset_config = LiveAudioConfig::default();

    let result =
        crate::commands::persist_config_and_apply(&state, reset_config, |_| Ok(false)).await;

    assert!(
        result.is_err(),
        "a failed persistence result must not report success"
    );
    assert_eq!(*state.config.read().await, old_config);
}

#[tokio::test]
async fn failed_audio_start_releases_started_resources_and_resets_state() {
    struct DropMarker(Arc<Mutex<bool>>);
    impl Drop for DropMarker {
        fn drop(&mut self) {
            *self.0.lock().unwrap() = true;
        }
    }

    let state = AppState::default();
    let test_home = isolated_test_home();
    std::fs::create_dir_all(&test_home).expect("create isolated test home");
    let worker_script = test_home.join("mock_asr_worker.py");
    std::fs::write(
        &worker_script,
        r#"
import json
import sys

for raw in sys.stdin:
    try:
        command = json.loads(raw).get("cmd")
    except Exception:
        continue
    if command == "init":
        print(json.dumps({"event": "ready", "payload": {"device": "cpu"}}), flush=True)
    elif command == "ping":
        print(json.dumps({"event": "pong", "payload": {"state": "idle"}}), flush=True)
    elif command == "shutdown":
        break
"#,
    )
    .expect("write deterministic mock worker");

    state.is_running.store(true, Ordering::SeqCst);
    *state.asr_state.write().await = "listening".to_string();
    *state.start_time.write().await = Some(std::time::Instant::now());
    let cancel_token = tokio_util::sync::CancellationToken::new();
    *state.cancel_token.lock().await = Some(cancel_token.clone());
    let port = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("reserve loopback port")
        .local_addr()
        .expect("read reserved loopback port")
        .port();
    let mut ws_server = WsServer::new(WsServerConfig {
        base_port: port,
        fallback_range: 1,
        ..WsServerConfig::default()
    });
    let bound_port = NetworkServer::start(&mut ws_server, port)
        .await
        .expect("start local WebSocket server");
    let ws_server = ws_server.clone();
    state.active_port.store(bound_port, Ordering::SeqCst);
    *state.ws_server.lock().await = Some(ws_server.clone());

    let mut worker_config = WorkerProcessConfig::default();
    worker_config.name = "startup-rollback-test-worker".to_string();
    worker_config.command = "python".to_string();
    worker_config.args = vec![
        "-u".to_string(),
        worker_script.to_string_lossy().to_string(),
    ];
    worker_config.ready_timeout = std::time::Duration::from_secs(5);
    worker_config.watchdog_interval = std::time::Duration::from_millis(100);
    worker_config.watchdog_timeout = std::time::Duration::from_secs(2);
    worker_config.grace_shutdown_timeout = std::time::Duration::from_secs(2);
    worker_config.auto_respawn = false;
    let mut supervisor = ProcessSupervisor::new(worker_config, std::process::id(), 0, None)
        .expect("create mock-worker supervisor");
    supervisor
        .start_internal()
        .await
        .expect("start deterministic mock worker");
    let worker_pid = supervisor.child_pid().await.expect("mock worker PID");
    assert!(is_parent_alive(worker_pid));
    *state.supervisor.lock().await = Some(Arc::new(supervisor));

    let task_stopped = Arc::new(Mutex::new(false));
    let task_marker = task_stopped.clone();
    let event_cancel = cancel_token.clone();
    let event_task = tokio::spawn(async move {
        let _marker = DropMarker(task_marker);
        event_cancel.cancelled().await;
    });

    let injected_startup_error = async { Err::<(), _>("injected audio startup failure") }
        .await
        .expect_err("injected failure is deterministic and requires no microphone");
    assert_eq!(injected_startup_error, "injected audio startup failure");

    let returned_error = crate::commands::rollback_failed_audio_start(
        &state,
        event_task,
        cancel_token.clone(),
        true,
        injected_startup_error.to_string(),
    )
    .await;

    assert_eq!(returned_error, injected_startup_error);
    assert!(cancel_token.is_cancelled());
    assert!(!state.is_running.load(Ordering::SeqCst));
    assert_eq!(*state.asr_state.read().await, "stopped");
    assert!(state.start_time.read().await.is_none());
    assert!(state.cancel_token.lock().await.is_none());
    assert!(state.supervisor.lock().await.is_none());
    assert!(state.ws_server.lock().await.is_none());
    assert!(*task_stopped.lock().unwrap());
    assert!(
        ws_server.bound_port().is_none(),
        "failed startup must stop its newly-created WebSocket listener"
    );
    assert!(
        !is_parent_alive(worker_pid),
        "failed startup must terminate the newly-started worker process"
    );
    let _ = std::fs::remove_dir_all(test_home);
}

#[test]
fn transcription_diagnostics_never_include_transcript_text() {
    let sentinel = "SYNTHETIC_PRIVATE_TRANSCRIPT_SENTINEL";
    let output = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .with_writer(CapturedLogs(output.clone()))
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        crate::commands::log_transcription_metadata(&serde_json::json!({
            "utterance_id": "utt-test",
            "duration_sec": 1.25,
            "text": sentinel,
        }));
    });

    let logs = String::from_utf8(output.lock().unwrap().clone()).expect("UTF-8 log output");
    assert!(logs.contains("utt-test"));
    assert!(logs.contains("1.25s"));
    assert!(!logs.contains(sentinel));
}

#[test]
fn unknown_worker_event_diagnostics_never_include_payload_contents() {
    let sentinel = "SYNTHETIC_UNKNOWN_EVENT_PRIVATE_SENTINEL";
    let output = Arc::new(Mutex::new(Vec::new()));
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(CapturedLogs(output.clone()))
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        crate::commands::log_unknown_worker_event(
            "unknown_test_event",
            &serde_json::json!({
                "event": "unknown_test_event",
                "payload": { "text": sentinel }
            }),
        );
    });

    let logs = String::from_utf8(output.lock().unwrap().clone()).expect("UTF-8 log output");
    assert!(logs.contains("unknown_test_event"));
    assert!(!logs.contains(sentinel));
}

static NEXT_ISOLATED_HOME_ID: AtomicU64 = AtomicU64::new(0);

fn isolated_test_home() -> PathBuf {
    let temp = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    loop {
        let sequence = NEXT_ISOLATED_HOME_ID.fetch_add(1, Ordering::Relaxed);
        let path = temp.join(format!(
            "liveaudio-tauri-test-{}-{}-{}",
            std::process::id(),
            nanos,
            sequence
        ));
        match std::fs::create_dir(&path) {
            Ok(()) => return path,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("create isolated test home: {error}"),
        }
    }
}

#[test]
fn isolated_test_homes_are_unique_under_parallel_creation() {
    let workers: Vec<_> = (0..32)
        .map(|_| std::thread::spawn(isolated_test_home))
        .collect();
    let homes: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().expect("create isolated home"))
        .collect();
    let unique: std::collections::HashSet<_> = homes.iter().collect();
    assert_eq!(unique.len(), homes.len());
    for home in homes {
        std::fs::remove_dir(home).expect("remove test-owned isolated home");
    }
}

#[test]
fn test_subtitle_cue_dto_serialization() {
    let cue = SubtitleCueDto {
        id: 12345,
        start: 0.5,
        end: 2.5,
        text: "Hola mundo".to_string(),
        style: "karaoke".to_string(),
        timestamp_ms: 1700000000000,
    };

    let json = serde_json::to_string(&cue).expect("Serialization failed");
    assert!(json.contains("Hola mundo"));
    assert!(json.contains("karaoke"));
    assert!(json.contains("12345"));
}

#[test]
fn test_obs_overlay_dto_structure() {
    let obs = ObsOverlayDto {
        url: "file:///E:/LiveAudio/liveaudio/assets/subtitulos_obs.html?port=8765".to_string(),
        port: 8765,
        width: 1920,
        height: 1080,
        instructions: "Copiar en fuente de navegador".to_string(),
    };

    assert_eq!(obs.port, 8765);
    assert_eq!(obs.width, 1920);
    assert_eq!(obs.height, 1080);
    assert!(obs.url.contains("port=8765"));
}

#[test]
fn test_encode_f32_le_base64_codec() {
    let samples = vec![0.0f32, 1.0f32, -1.0f32, 0.5f32];
    let b64 = crate::commands::encode_f32_le_base64(&samples);
    assert!(!b64.is_empty());
    assert_eq!(b64.len() % 4, 0);
}
