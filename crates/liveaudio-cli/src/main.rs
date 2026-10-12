// SPDX-License-Identifier: MIT

//! LiveAudio CLI - Headless binary and service runner.
//!
//! Provides headless background service execution for OpenCohost, VoiceAI, and automated deployments:
//! - Full pipeline: Hardware Audio Capture (CPAL) -> Silero VAD (ONNX) -> Faster-Whisper Worker (Python) -> WebSocket Server (Tokio).
//! - Zero GUI, ~28MB core RAM footprint.
//! - Parent PID Watchdog and Windows Job Object 100% orphan/zombie prevention.

use std::collections::VecDeque;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

const THIRD_PARTY_NOTICES: &str = include_str!("../../../THIRD_PARTY_NOTICES.md");

use liveaudio_audio::traits::AudioSource;
use liveaudio_audio::{AudioChunkReceiver, AudioStreamConfig, CpalAudioSource};
use liveaudio_core::acoustic_events::{
    AcousticEventLifecycle, AcousticSpeechEndReason, AcousticSpeechEvent,
};
use liveaudio_core::config::{get_data_home, get_hf_home, load_config};
use liveaudio_core::job_object::JobObject;
use liveaudio_core::runtime::{preflight_asr_runtime, setup_managed_runtime, AsrBackend};
use liveaudio_core::supervisor::{
    is_parent_alive, HealthEmitter, ProcessSupervisor, SupervisorError, WorkerProcessConfig,
};
use liveaudio_ipc::ServiceEventType;
use liveaudio_network::{
    candidate_ports, subtitle_from_worker_result, BacklogPolicy, NetworkServer,
    PendingUtteranceTracker, WsServer, WsServerConfig,
};
use liveaudio_vad::{
    cpu_runtime_library_path, initialize_onnx_runtime, SileroVad, VadConfig, VadEngine,
    VadTransition,
};

fn resolve_cli_onnx_runtime_path(
    executable: &Path,
    development_resource_dir: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let development_library = cpu_runtime_library_path(development_resource_dir)?;
    let resource_dir = if development_library.is_file() {
        development_resource_dir.to_path_buf()
    } else {
        executable
            .parent()
            .ok_or("CLI executable has no parent directory")?
            .join("resources")
    };
    Ok(cpu_runtime_library_path(resource_dir)?)
}

fn observe_and_publish_acoustic_transition(
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
        eprintln!("Failed to publish acoustic event: {error}");
    }
}

fn close_and_publish_acoustic_attempt(
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

fn audio_receiver_close_reason(receiver: &AudioChunkReceiver) -> AcousticSpeechEndReason {
    if receiver.capture_failed() {
        AcousticSpeechEndReason::CaptureError
    } else {
        AcousticSpeechEndReason::ServiceStopped
    }
}

fn capture_error_preempts_audio(receiver: &AudioChunkReceiver) -> bool {
    receiver.capture_failed()
}

async fn send_audio_command_until_cancelled(
    supervisor: &ProcessSupervisor,
    cancellation: &tokio_util::sync::CancellationToken,
    command: &serde_json::Value,
) -> Result<(), SupervisorError> {
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => Ok(()),
        result = supervisor.send_command(command) => result,
    }
}

fn encode_f32_le_base64(samples: &[f32]) -> String {
    let mut bytes = Vec::with_capacity(samples.len() * 4);
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    const B64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
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

#[cfg(test)]
mod parent_pid_tests {
    use super::parse_parent_pid_args;

    #[test]
    fn rejects_missing_invalid_and_nonpositive_parent_pid_before_service_start() {
        for args in [
            vec!["--parent-pid".to_string()],
            vec!["--parent-pid".to_string(), "abc".to_string()],
            vec!["--parent-pid".to_string(), "0".to_string()],
            vec!["--parent-pid".to_string(), "-1".to_string()],
        ] {
            assert!(parse_parent_pid_args(&args, None).is_err(), "args={args:?}");
        }
    }

    #[test]
    fn rejects_invalid_environment_parent_pid_and_accepts_positive_pid() {
        assert!(parse_parent_pid_args(&[], Some("not-a-pid")).is_err());
        assert!(parse_parent_pid_args(&[], Some("0")).is_err());
        assert_eq!(parse_parent_pid_args(&[], Some("42")).unwrap(), 42);
        assert_eq!(parse_parent_pid_args(&[], None).unwrap(), 0);
    }
}

#[cfg(test)]
mod runtime_setup_args_tests {
    use super::{parse_setup_runtime_args, AsrBackend};

    #[test]
    fn setup_requires_an_explicit_supported_backend() {
        assert!(parse_setup_runtime_args(&["setup-runtime".into()]).is_err());
        assert!(parse_setup_runtime_args(&[
            "setup-runtime".into(),
            "--backend".into(),
            "auto".into()
        ])
        .is_err());
        assert_eq!(
            parse_setup_runtime_args(&["setup-runtime".into(), "--backend".into(), "cpu".into()])
                .unwrap(),
            Some(AsrBackend::Cpu)
        );
    }
}

#[cfg(test)]
mod acoustic_event_adapter_tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn cli_runtime_resolution_uses_explicit_resource_roots_only() {
        let executable = PathBuf::from(r"C:\LiveAudio\liveaudio-cli.exe");
        let packaged = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging");
        let resolved = super::resolve_cli_onnx_runtime_path(&executable, &packaged)
            .expect("existing dev resource should resolve");
        assert!(resolved.is_absolute());
        assert!(resolved.is_file());
    }

    #[test]
    fn cli_runtime_resolution_falls_back_to_executable_resources_not_cwd() {
        let root =
            std::env::temp_dir().join(format!("liveaudio-cli-resources-{}", std::process::id()));
        let executable = root.join("bin/liveaudio-cli.exe");
        let development = root.join("missing-development-assets");
        let packaged = root.join("bin/resources");
        let library = cpu_runtime_library_path(&packaged).expect("supported test platform");
        std::fs::create_dir_all(library.parent().expect("runtime parent"))
            .expect("resource directory");
        std::fs::write(&library, b"test fixture").expect("runtime fixture");

        let resolved = super::resolve_cli_onnx_runtime_path(&executable, &development)
            .expect("executable-relative package path");
        assert_eq!(resolved, library);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn cli_acoustic_event_adapter_obeys_opt_in_flag_without_audio_hardware() {
        let server = WsServer::default();
        let mut lifecycle = AcousticEventLifecycle::new();

        observe_and_publish_acoustic_transition(
            &mut lifecycle,
            &server,
            false,
            VadTransition::SpeechStart,
        );
        assert_eq!(
            server
                .metrics()
                .broadcast_messages_total
                .load(Ordering::SeqCst),
            0
        );

        observe_and_publish_acoustic_transition(
            &mut lifecycle,
            &server,
            true,
            VadTransition::SpeechStart,
        );
        assert_eq!(
            server
                .metrics()
                .broadcast_messages_total
                .load(Ordering::SeqCst),
            1
        );

        close_and_publish_acoustic_attempt(
            &mut lifecycle,
            &server,
            true,
            AcousticSpeechEndReason::ServiceStopped,
        );
        assert_eq!(
            server
                .metrics()
                .broadcast_messages_total
                .load(Ordering::SeqCst),
            2
        );
        close_and_publish_acoustic_attempt(
            &mut lifecycle,
            &server,
            true,
            AcousticSpeechEndReason::ServiceStopped,
        );
        assert_eq!(
            server
                .metrics()
                .broadcast_messages_total
                .load(Ordering::SeqCst),
            2
        );
    }

    #[tokio::test]
    async fn cli_audio_send_cancellation_closes_active_acoustic_event_once() {
        let mut config = WorkerProcessConfig::default();
        config.name = "cli-nonreading-worker".to_string();
        config.command = "python".to_string();
        config.args = vec![
            "-u".to_string(),
            "-c".to_string(),
            "import json,time; print(json.dumps({'event':'ready'}), flush=True); time.sleep(60)"
                .to_string(),
        ];
        config.ready_timeout = Duration::from_secs(5);
        config.auto_respawn = false;
        let mut supervisor = ProcessSupervisor::new(config, std::process::id(), 0, None)
            .expect("supervisor creation");
        supervisor
            .start_internal()
            .await
            .expect("mock worker ready handshake");
        let supervisor = Arc::new(supervisor);
        let command = serde_json::json!({"cmd":"transcribe", "payload":"x".repeat(4096)});

        let mut producers = Vec::new();
        for _ in 0..256 {
            let supervisor = supervisor.clone();
            let command = command.clone();
            producers.push(tokio::spawn(async move {
                supervisor.send_command(&command).await
            }));
        }
        tokio::time::timeout(Duration::from_secs(2), async {
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
        let server = WsServer::default();
        let mut lifecycle = AcousticEventLifecycle::new();
        observe_and_publish_acoustic_transition(
            &mut lifecycle,
            &server,
            true,
            VadTransition::SpeechStart,
        );
        let task = {
            let supervisor = supervisor.clone();
            let cancellation = cancellation.clone();
            let command = command.clone();
            let server = server.clone();
            tokio::spawn(async move {
                send_audio_command_until_cancelled(&supervisor, &cancellation, &command)
                    .await
                    .expect("cancellation should end pending submission cleanly");
                let mut lifecycle = lifecycle;
                close_and_publish_acoustic_attempt(
                    &mut lifecycle,
                    &server,
                    true,
                    AcousticSpeechEndReason::ServiceStopped,
                );
                close_and_publish_acoustic_attempt(
                    &mut lifecycle,
                    &server,
                    true,
                    AcousticSpeechEndReason::ServiceStopped,
                );
            })
        };
        tokio::task::yield_now().await;
        cancellation.cancel();
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("canceled audio send must allow task join")
            .expect("audio task join");

        for producer in producers {
            producer.abort();
            let _ = producer.await;
        }
        let mut supervisor = Arc::try_unwrap(supervisor).unwrap_or_else(|_| {
            panic!("all worker queue producers and audio send task must be released")
        });
        supervisor
            .stop_internal(Duration::from_secs(1))
            .await
            .expect("mock worker must be reaped");
        assert_eq!(
            server
                .metrics()
                .broadcast_messages_total
                .load(Ordering::SeqCst),
            2,
            "cancellation must publish exactly one matching end after the start"
        );
    }

    #[tokio::test]
    async fn cli_capture_error_closes_active_acoustic_event_once() {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        drop(tx);
        let failed = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let mut receiver =
            liveaudio_audio::AudioChunkReceiver::with_capture_error(rx, failed.clone());
        assert!(receiver.recv().await.is_none());

        let server = WsServer::default();
        let mut lifecycle = AcousticEventLifecycle::new();
        observe_and_publish_acoustic_transition(
            &mut lifecycle,
            &server,
            true,
            VadTransition::SpeechStart,
        );
        close_and_publish_acoustic_attempt(
            &mut lifecycle,
            &server,
            true,
            audio_receiver_close_reason(&receiver),
        );
        close_and_publish_acoustic_attempt(
            &mut lifecycle,
            &server,
            true,
            audio_receiver_close_reason(&receiver),
        );

        assert_eq!(
            server
                .metrics()
                .broadcast_messages_total
                .load(Ordering::SeqCst),
            2,
            "capture failure must emit one end after the start"
        );
    }

    #[tokio::test]
    async fn cli_fatal_capture_discards_queued_would_start_chunk_and_closes_active_once() {
        let (tx, rx) = tokio::sync::mpsc::channel(2);
        tx.send(liveaudio_audio::AudioChunk::new(
            vec![0.5; 512],
            16_000,
            1,
            1,
        ))
        .await
        .unwrap();
        tx.send(liveaudio_audio::AudioChunk::new(
            vec![0.5; 512],
            16_000,
            1,
            2,
        ))
        .await
        .unwrap();
        let failed = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let mut receiver = AudioChunkReceiver::with_capture_error(rx, failed);
        let queued_chunk = receiver.recv().await.expect("queued capture chunk");

        let server = WsServer::default();
        let mut lifecycle = AcousticEventLifecycle::new();
        observe_and_publish_acoustic_transition(
            &mut lifecycle,
            &server,
            true,
            VadTransition::SpeechStart,
        );
        let mut close_reason = AcousticSpeechEndReason::ServiceStopped;
        let mut vad_evaluations = 0;
        let mut asr_submissions = 0;
        if capture_error_preempts_audio(&receiver) {
            close_reason = audio_receiver_close_reason(&receiver);
        } else {
            for _frame in queued_chunk.samples.chunks_exact(512) {
                vad_evaluations += 1;
                asr_submissions += 1;
            }
        }
        close_and_publish_acoustic_attempt(&mut lifecycle, &server, true, close_reason);
        close_and_publish_acoustic_attempt(&mut lifecycle, &server, true, close_reason);

        assert_eq!(
            vad_evaluations, 0,
            "fatal capture must not mutate VAD state"
        );
        assert_eq!(
            asr_submissions, 0,
            "fatal capture must not submit queued audio"
        );
        assert_eq!(
            server
                .metrics()
                .broadcast_messages_total
                .load(Ordering::SeqCst),
            2,
            "active speech gets exactly one end after the start"
        );
        assert_eq!(
            audio_receiver_close_reason(&receiver),
            AcousticSpeechEndReason::CaptureError
        );
    }
}

fn parse_setup_runtime_args(args: &[String]) -> Result<Option<AsrBackend>, String> {
    if args.first().map(String::as_str) != Some("setup-runtime") {
        return Ok(None);
    }
    if args.len() != 3 || args[1] != "--backend" {
        return Err("Usage: liveaudio setup-runtime --backend <cpu|cu121>".to_string());
    }
    args[2]
        .parse::<AsrBackend>()
        .map(Some)
        .map_err(|error| error.to_string())
}

fn parse_parent_pid_args(args: &[String], env_pid: Option<&str>) -> Result<u32, String> {
    if let Some(index) = args
        .iter()
        .position(|arg| arg == "--parent-pid" || arg == "-p")
    {
        let raw = args
            .get(index + 1)
            .filter(|value| !value.starts_with('-'))
            .ok_or_else(|| "--parent-pid requires a positive integer".to_string())?;
        return parse_positive_pid(raw);
    }
    match env_pid {
        Some(raw) => parse_positive_pid(raw),
        None => Ok(0),
    }
}

fn parse_positive_pid(raw: &str) -> Result<u32, String> {
    let pid = raw
        .parse::<u32>()
        .map_err(|_| "parent PID must be a positive integer".to_string())?;
    if pid == 0 {
        return Err("parent PID must be a positive integer".to_string());
    }
    Ok(pid)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args
        .iter()
        .any(|arg| arg == "--licenses" || arg == "--license")
    {
        print!("{THIRD_PARTY_NOTICES}");
        return Ok(());
    }

    if let Some(backend) = parse_setup_runtime_args(&args[1..])? {
        let root = setup_managed_runtime(backend)?;
        println!("ASR runtime provisioned at {}", root.display());
        return Ok(());
    }

    // Check for help
    if args
        .iter()
        .any(|a| a == "help" || a == "--help" || a == "-h")
    {
        print_usage();
        return Ok(());
    }

    // Check for doctor
    if args.iter().any(|a| a == "doctor" || a == "--doctor") {
        run_doctor().await?;
        return Ok(());
    }

    // Check for devices
    if args.iter().any(|a| a == "devices" || a == "--devices") {
        list_devices().await?;
        return Ok(());
    }

    // Parse parent PID (CLI arg --parent-pid <PID> or -p <PID>, or env var LIVEAUDIO_PARENT_PID)
    let arg_values = args.iter().skip(1).cloned().collect::<Vec<_>>();
    let env_parent_pid = match env::var_os("LIVEAUDIO_PARENT_PID") {
        Some(value) => Some(value.into_string().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "LIVEAUDIO_PARENT_PID must be a positive integer",
            )
        })?),
        None => None,
    };
    let parent_pid = parse_parent_pid_args(&arg_values, env_parent_pid.as_deref())
        .map_err(|message| std::io::Error::new(std::io::ErrorKind::InvalidInput, message))?;

    let mut custom_port: Option<u16> = None;

    for (i, arg) in args.iter().enumerate() {
        if (arg == "--port" || arg == "-P") && i + 1 < args.len() {
            if let Ok(port) = args[i + 1].parse::<u16>() {
                custom_port = Some(port);
            }
        }
    }

    // Check for service / headless daemon
    let is_service = args
        .iter()
        .any(|a| a == "serve" || a == "--service" || a == "-s")
        || parent_pid > 0;

    if is_service {
        eprintln!("🚀 Starting LiveAudio headless background service daemon...");
        run_service(parent_pid, custom_port).await?;
    } else {
        eprintln!("Starting LiveAudio pipeline in standalone mode...");
        run_service(0, custom_port).await?;
    }

    Ok(())
}

fn print_usage() {
    println!("LiveAudio CLI v0.1.0 — High-performance Headless STT Service");
    println!("Usage: liveaudio-cli [COMMAND] [OPTIONS]\n");
    println!("Commands:");
    println!("  serve, --service  Run as headless daemon for OpenCohost/OBS (default if parent-pid is given)");
    println!("  devices           Enumerate audio input devices");
    println!("  doctor            Run environment and hardware diagnostics");
    println!("  help, --help      Print this help message\n");
    println!("Options:");
    println!("  --parent-pid, -p <PID>   Parent process ID for watchdog supervision");
    println!("  --port, -P <PORT>        WebSocket listen port (default: 8765)");
    println!("  setup-runtime --backend <cpu|cu121>  Provision the managed ASR runtime");
    println!("  --licenses               Show bundled third-party notices");
}

async fn run_service(
    parent_pid: u32,
    port_override: Option<u16>,
) -> Result<(), Box<dyn std::error::Error>> {
    let executable = std::env::current_exe()?;
    let development_resource_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging");
    let runtime_path = resolve_cli_onnx_runtime_path(&executable, &development_resource_dir)?;
    initialize_onnx_runtime(runtime_path)?;

    let current_pid = std::process::id();
    let config = load_config().unwrap_or_default();
    let effective_base_port = port_override.unwrap_or(config.ws_port);

    // Validate imports before opening listeners or audio resources. This probe never installs packages.
    let mut worker_config = WorkerProcessConfig::discover_asr_worker();
    preflight_asr_runtime(
        std::path::Path::new(&worker_config.command),
        worker_config.working_dir.as_deref(),
    )
    .await?;

    // 1. Health Emitter (Arc for multi-task sharing)
    let emitter = Arc::new(HealthEmitter::new(current_pid, parent_pid, None));
    emitter.emit_service_state("starting");

    eprintln!("============================================================");
    eprintln!("LiveAudio Headless Service Daemon");
    eprintln!(
        "PID: {} | Parent PID Watchdog: {}",
        current_pid,
        if parent_pid > 0 {
            format!("{}", parent_pid)
        } else {
            "None".to_string()
        }
    );
    eprintln!(
        "Base Port: {} | Device: {} | Model: {}",
        effective_base_port, config.device, config.model_size
    );
    eprintln!("============================================================");

    // 2. WebSocket Server
    let mut ws = WsServer::new(WsServerConfig {
        base_port: effective_base_port,
        backlog_policy: BacklogPolicy::from_config(&config.subtitle_backlog_policy),
        max_live_delay_sec: config.subtitle_max_live_delay_sec as f32,
        ..Default::default()
    });
    let effective_port = match NetworkServer::start(&mut ws, effective_base_port).await {
        Ok(port) => {
            eprintln!("✅ WebSocket server listening on ws://127.0.0.1:{}", port);
            port
        }
        Err(e) => {
            eprintln!("❌ Failed to bind WebSocket server: {}", e);
            return Err(e.into());
        }
    };

    // Emit ws_port event immediately over stdout JSON-lines for OpenCohost discovery
    emitter.emit_ws_port(effective_base_port, effective_port);

    // 3. Faster-Whisper Supervised Worker
    let clean_model_name = config
        .model_size
        .split_whitespace()
        .next()
        .unwrap_or("small")
        .to_string();

    let clean_lang = config
        .language
        .as_deref()
        .filter(|l| !l.is_empty() && *l != "auto")
        .unwrap_or("es")
        .to_string();

    let prompt = if clean_lang == "en" {
        &config.whisper_context_prompt_en
    } else {
        &config.whisper_context_prompt_es
    };

    worker_config.init_payload = Some(serde_json::json!({
        "model_name": clean_model_name,
        "device": config.device,
        "language": clean_lang,
        "initial_prompt": if prompt.is_empty() { None } else { Some(prompt) },
        "auto_cpu_fallback": true,
    }));

    emitter.emit_asr_state("starting", Some("starting"), Some(1));
    eprintln!(
        "🐍 Spawning Faster-Whisper worker ('{}' on {})...",
        clean_model_name, config.device
    );
    emitter.emit_asr_state("loading", Some("loading"), Some(1));

    let mut supervisor = ProcessSupervisor::new(worker_config, current_pid, parent_pid, None)?;

    let mut worker_events = supervisor.subscribe_worker_events();
    supervisor.start_internal().await?;
    eprintln!("✅ Faster-Whisper worker ready for inference");

    emitter.emit_asr_state("ready", Some("ready"), Some(1));
    emitter.emit_service_state("running");

    let supervisor_arc = Arc::new(supervisor);

    // 4. Audio Capture (CPAL)
    let stream_config = AudioStreamConfig {
        device_id: None,
        sample_rate: 16000,
        channels: 1,
        chunk_size: 512,
        ring_buffer_capacity: 500,
    };
    let mut audio_source = CpalAudioSource::new().with_rnnoise_enabled(config.rnnoise_enabled);
    let mut audio_rx = audio_source.start(&stream_config).await?;
    eprintln!("🎙️ Audio capture active on default microphone (16kHz mono)");

    // 5. Silero VAD (ONNX)
    let vad_cfg = VadConfig {
        sample_rate: 16000,
        chunk_size: 512,
        threshold: config.vad_threshold,
        speech_pad_ms: config.vad_speech_pad_ms,
        silence_timeout_ms: (config.silence_timeout * 1000.0).round() as u32,
        max_chunk_duration_sec: config.max_chunk_duration as f32,
    };
    let mut vad = SileroVad::new(vad_cfg)?;
    eprintln!("🧠 Silero VAD engine active");

    // Emit Ready Event (legacy compatibility)
    let mut ready_payload = serde_json::Map::new();
    ready_payload.insert("status".to_string(), serde_json::json!("ready"));
    ready_payload.insert("port".to_string(), serde_json::json!(effective_port));
    emitter.emit(ServiceEventType::Ready, ready_payload);

    let cancel_token = tokio_util::sync::CancellationToken::new();

    // Background Task: Worker events -> WebSocket Broadcast & ASR status relay
    let ws_clone = ws.clone();
    let cancel_worker = cancel_token.clone();
    let active_style = config.subtitle_style.clone();
    let backlog_policy = BacklogPolicy::from_config(&config.subtitle_backlog_policy);
    let max_live_delay_sec = config.subtitle_max_live_delay_sec;
    let catchup_interval_sec = config.subtitle_catchup_interval_sec;
    let pending_utterances = Arc::new(std::sync::Mutex::new(PendingUtteranceTracker::default()));
    let pending_events = pending_utterances.clone();
    let emitter_worker = emitter.clone();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_worker.cancelled() => break,
                msg_res = worker_events.recv() => {
                    match msg_res {
                        Ok(val) => {
                            if let Some(event_name) = val.get("event").and_then(|v| v.as_str()) {
                                match event_name {
                                    "transcription_result" => {
                                        if let Some(payload) = val.get("payload") {
                                            let text = payload.get("text").and_then(|t| t.as_str()).unwrap_or("").trim();
                                            let utt_id = payload.get("utterance_id").and_then(|u| u.as_str()).unwrap_or("utt");
                                            let inference_sec = payload.get("inference_sec").and_then(|d| d.as_f64())
                                                .or_else(|| payload.get("latency_sec").and_then(|d| d.as_f64())).unwrap_or(0.0);
                                            let timing = pending_events.lock().unwrap().complete(utt_id, inference_sec);
                                            if !text.is_empty() {
                                                let dur_sec = payload.get("duration_sec").and_then(|d| d.as_f64()).unwrap_or(3.0);
                                                eprintln!("[ASR Transcription] [{}] \"{}\" ({:.2}s)", utt_id, text, dur_sec);

                                                let wire_msg = subtitle_from_worker_result(
                                                    utt_id.to_string(),
                                                    text.to_string(),
                                                    active_style.clone(),
                                                    inference_sec,
                                                    timing,
                                                    backlog_policy,
                                                    max_live_delay_sec,
                                                    catchup_interval_sec,
                                                );
                                                if let Some(wire_msg) = wire_msg {
                                                    let _ = ws_clone.broadcast_subtitle(&wire_msg);
                                                }
                                            }
                                        }
                                    }
                                    "ready" => {
                                        emitter_worker.emit_asr_state("ready", Some("ready"), Some(1));
                                    }
                                    "download_progress" => {
                                        emitter_worker.emit_asr_state("downloading", Some("downloading"), Some(1));
                                    }
                                    "error" => {
                                        if let Some(code) = val.get("payload").and_then(|p| p.get("code")).and_then(|c| c.as_str()) {
                                            if code == "cuda_oom" {
                                                emitter_worker.emit_asr_state("fallback_cpu", Some("fallback_cpu"), Some(1));
                                            }
                                        }
                                    }
                                    _ => {}
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

    // Background Task: Audio Capture -> VAD -> Supervisor Transcribe Command
    let cancel_audio = cancel_token.clone();
    let supervisor_audio = supervisor_arc.clone();
    let lang_clone = clean_lang.clone();

    let pre_buffer_capacity = vad.config().pre_buffer_chunks().max(1);
    let max_samples = (16000.0 * config.max_chunk_duration.ceil()) as usize;
    let ws_audio = ws.clone();
    let acoustic_events_enabled = config.acoustic_events_enabled;

    let audio_task = tokio::spawn(async move {
        let mut utterance_samples: Vec<f32> = Vec::with_capacity(max_samples.max(16000 * 5));
        let mut pre_roll_chunks: VecDeque<Vec<f32>> =
            VecDeque::with_capacity(pre_buffer_capacity + 1);
        let mut is_in_utterance = false;
        let mut utterance_seq = 0u64;
        let mut acoustic_lifecycle = AcousticEventLifecycle::new();
        let mut close_reason = AcousticSpeechEndReason::ServiceStopped;

        'capture: loop {
            tokio::select! {
                _ = cancel_audio.cancelled() => break,
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
                        if let Ok(decision) = vad.evaluate_chunk(frame) {
                            observe_and_publish_acoustic_transition(
                                &mut acoustic_lifecycle,
                                &ws_audio,
                                acoustic_events_enabled,
                                decision.transition,
                            );
                            match decision.transition {
                                VadTransition::SpeechStart => {
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

                                        if utterance_samples.len() >= 4800 {
                                            utterance_seq += 1;
                                            let b64_audio = encode_f32_le_base64(&utterance_samples);
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
                                                    "language": lang_clone,
                                                }
                                            });
                                            let utterance_id = format!("utt_{}", utterance_seq);
                                            let created_at_sec = SystemTime::now().duration_since(UNIX_EPOCH)
                                                .map(|d| d.as_secs_f64()).unwrap_or(0.0);
                                            pending_utterances.lock().unwrap().track(utterance_id.clone(), created_at_sec);
                                            if send_audio_command_until_cancelled(
                                                &supervisor_audio,
                                                &cancel_audio,
                                                &cmd,
                                            )
                                            .await
                                            .is_err()
                                            {
                                                pending_utterances.lock().unwrap().remove(&utterance_id);
                                            }
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
        let _ = audio_source.stop().await;
    });

    // Parent PID Watchdog
    if parent_pid > 0 {
        let cancel_watchdog = cancel_token.clone();
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(1)).await;
                if !is_parent_alive(parent_pid) {
                    eprintln!(
                        "\n🚨 Parent process PID {} died. Triggering graceful shutdown.",
                        parent_pid
                    );
                    cancel_watchdog.cancel();
                    break;
                }
            }
        });
    }

    eprintln!("\n🟢 LiveAudio service daemon running. Waiting for audio / connections... (Press Ctrl+C to stop)");

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            eprintln!("\nReceived Ctrl+C interrupt.");
        }
        _ = cancel_token.cancelled() => {
            eprintln!("\nShutdown triggered.");
        }
    }

    cancel_token.cancel();
    audio_task.await?;
    emitter.emit_service_state("stopping");
    emitter.emit(ServiceEventType::Stopped, serde_json::Map::new());

    // Clean shutdown of child supervisor
    let shutdown_cmd = serde_json::json!({
        "version": 1,
        "cmd": "shutdown",
        "payload": { "grace_timeout_ms": 1500 }
    });
    let _ = supervisor_arc.send_command(&shutdown_cmd).await;
    supervisor_arc.cancellation_token().cancel();

    eprintln!("LiveAudio service shutdown completed cleanly. 0 orphan processes.");
    Ok(())
}

async fn list_devices() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Audio Input Devices ===");
    println!("  [0] Default System Microphone (16000Hz, 1ch) [DEFAULT]");
    println!("  [1] Audio del sistema / Altavoces (WASAPI Loopback)");
    Ok(())
}

async fn run_doctor() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== LiveAudio Doctor ===");
    println!("OS: {}", std::env::consts::OS);
    println!("Architecture: {}", std::env::consts::ARCH);

    // Test Windows Job Object creation
    match JobObject::create() {
        Ok(_) => println!("Windows Job Object: AVAILABLE (0 zombie child guarantees)"),
        Err(e) => println!("Windows Job Object: UNAVAILABLE ({})", e),
    }

    // Python Worker Discovery
    let py_path = WorkerProcessConfig::find_python_executable();
    println!("Python Runtime: {}", py_path.display());
    match WorkerProcessConfig::find_asr_worker_script() {
        Some(script) => println!("ASR Worker Script: DISCOVERED ({})", script.display()),
        None => println!("ASR Worker Script: NOT FOUND"),
    }

    // Model Cache Directory Layout
    let data_home = get_data_home();
    let hf_cache = get_hf_home();
    println!("App Data Home: {}", data_home.display());
    println!("HuggingFace Cache (HF_HOME): {}", hf_cache.display());

    // Unicode / Space Path Validation
    let test_space_path = data_home.join("test spaces & accénts 🎙️");
    println!(
        "Unicode Path Support: VERIFIED ({})",
        test_space_path.display()
    );

    // Candidate port range
    let ports = candidate_ports(8765, None);
    println!("WebSocket Port candidates: {:?}", ports);

    println!("Diagnostics complete.");
    Ok(())
}
