// SPDX-License-Identifier: MIT

//! Generic worker main loop shared by every ASR worker binary.

use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::protocol::{
    decode_audio_base64, emit_download_progress, emit_error, emit_pong, emit_ready,
    emit_shutdown_ack, emit_transcription_result, get_process_memory_mb, InitPayload,
    RustCommand, TranscribePayload,
};

/// Download/preparation progress: phase, text, percent, bytes downloaded, bytes total, speed, eta.
pub type ProgressFn<'a> =
    dyn FnMut(&str, &str, Option<f64>, Option<u64>, Option<u64>, Option<u64>, Option<f64>) + 'a;

pub struct ReadyInfo {
    pub device: String,
    pub compute_type: String,
    pub fallback_occurred: bool,
    pub fallback_reason: Option<String>,
}

pub struct Transcript {
    pub text: String,
    pub duration_sec: f64,
    pub inference_sec: f64,
    pub no_speech_prob: f64,
}

/// A load failure reported to the host as an `error` event.
pub struct LoadError {
    pub code: &'static str,
    pub message: String,
    pub recoverable: bool,
}

/// What a worker binary provides: model preparation + load, and decoding.
pub trait Engine: Sized {
    /// Prepare (download/verify) and load the model. Called with the previous engine already dropped.
    fn load(init: &InitPayload, progress: &mut ProgressFn) -> Result<(Self, ReadyInfo), LoadError>;

    fn transcribe(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
        language: Option<&str>,
        prompt: Option<&str>,
    ) -> Result<Transcript, String>;
}

/// Handle `--self-test`: run `test`, print the engine status line and exit. Returns if the flag is absent.
pub fn run_self_test_cli(engine_name: &str, test: impl FnOnce() -> Result<(), String>) {
    if !std::env::args().any(|arg| arg == "--self-test") {
        return;
    }
    match test() {
        Ok(()) => {
            println!(r#"{{"status":"ok","engine":"{engine_name}"}}"#);
            let _ = std::io::stdout().flush();
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Self-test failed: {e}");
            std::process::exit(1);
        }
    }
}

fn locked<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Run the worker protocol until `shutdown` or end of input. Blocking; must not be called
/// from inside an async runtime.
pub fn run_worker<E, R, W>(input: R, output: W)
where
    E: Engine,
    R: BufRead + Send + 'static,
    W: Write + Send + 'static,
{
    let stdout = Arc::new(Mutex::new(output));
    let worker_state = Arc::new(Mutex::new("idle".to_string()));
    let active_utterance = Arc::new(Mutex::new(None::<String>));
    let attempt_counter = Arc::new(AtomicU64::new(1));

    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::channel::<RustCommand>(128);

    // Dedicated reader thread: answers 'ping' immediately instead of queuing it behind a decode.
    {
        let stdout = stdout.clone();
        let state = worker_state.clone();
        let utterance = active_utterance.clone();
        let attempt = attempt_counter.clone();
        std::thread::spawn(move || {
            let mut input = input;
            let mut line = String::new();
            while input.read_line(&mut line).unwrap_or(0) > 0 {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    if let Ok(cmd) = serde_json::from_str::<RustCommand>(trimmed) {
                        if cmd.cmd == "ping" {
                            let state = locked(&state).clone();
                            let active_id = locked(&utterance).clone();
                            let attempt = attempt.load(Ordering::SeqCst);
                            let mem = get_process_memory_mb();
                            let mut out = locked(&stdout);
                            let _ = emit_pong(
                                &mut *out,
                                attempt,
                                cmd.request_id,
                                &state,
                                active_id.as_deref(),
                                mem,
                            );
                        } else if cmd_tx.blocking_send(cmd).is_err() {
                            break;
                        }
                    } else {
                        eprintln!("Unrecognized input JSON line: {trimmed}");
                    }
                }
                line.clear();
            }
        });
    }

    let set_state = |s: &str| *locked(&worker_state) = s.to_string();
    let set_active = |id: Option<String>| *locked(&active_utterance) = id;
    let send_error = |attempt_id: u64, code: &str, message: &str, recoverable: bool| {
        let _ = emit_error(&mut *locked(&stdout), attempt_id, code, message, recoverable);
    };

    let mut current_engine: Option<E> = None;

    while let Some(command) = cmd_rx.blocking_recv() {
        attempt_counter.store(command.attempt_id, Ordering::SeqCst);
        let attempt_id = command.attempt_id;

        match command.cmd.as_str() {
            "init" | "reload_model" => {
                set_state("loading");
                // Drop existing engine first so RAM and VRAM never double during load.
                drop(current_engine.take());

                let init_payload: InitPayload = match serde_json::from_value(command.payload) {
                    Ok(p) => p,
                    Err(e) => {
                        send_error(attempt_id, "INIT_PAYLOAD_INVALID", &format!("Invalid init payload: {e}"), false);
                        set_state("idle");
                        continue;
                    }
                };

                let progress_out = stdout.clone();
                let mut progress = move |phase: &str,
                                         text: &str,
                                         percent: Option<f64>,
                                         downloaded: Option<u64>,
                                         total: Option<u64>,
                                         speed: Option<u64>,
                                         eta: Option<f64>| {
                    let _ = emit_download_progress(
                        &mut *locked(&progress_out),
                        attempt_id,
                        phase,
                        text,
                        percent,
                        downloaded,
                        total,
                        speed,
                        eta,
                    );
                };

                match E::load(&init_payload, &mut progress) {
                    Ok((engine, info)) => {
                        let mem = get_process_memory_mb();
                        current_engine = Some(engine);
                        set_state("idle");
                        let _ = emit_ready(
                            &mut *locked(&stdout),
                            attempt_id,
                            &init_payload.model_name,
                            &info.device,
                            &info.compute_type,
                            info.fallback_occurred,
                            info.fallback_reason.as_deref(),
                            mem,
                        );
                    }
                    Err(e) => {
                        set_state("idle");
                        send_error(attempt_id, e.code, &e.message, e.recoverable);
                    }
                }
            }

            "transcribe" => {
                let payload: TranscribePayload = match serde_json::from_value(command.payload) {
                    Ok(p) => p,
                    Err(e) => {
                        send_error(attempt_id, "TRANSCRIBE_PAYLOAD_INVALID", &format!("Invalid transcribe payload: {e}"), true);
                        continue;
                    }
                };
                let seq = payload.sequence.unwrap_or(0);
                let empty_partial = |language: &str, latency_sec: f64| {
                    let _ = emit_transcription_result(
                        &mut *locked(&stdout),
                        attempt_id,
                        &payload.utterance_id,
                        seq,
                        "",
                        language,
                        0.0,
                        0.0,
                        latency_sec,
                        get_process_memory_mb(),
                        0.0,
                        true,
                    );
                };

                // A partial is optional work: if anything is queued behind it, skip the decode
                // and report an empty partial so the host can free its in-flight slot.
                if payload.partial && !cmd_rx.is_empty() {
                    empty_partial(payload.language.as_deref().unwrap_or("es"), 0.0);
                    continue;
                }

                let Some(engine) = current_engine.as_mut() else {
                    send_error(attempt_id, "ENGINE_NOT_INITIALIZED", "Received transcribe command before model initialization", true);
                    continue;
                };

                set_state("transcribing");
                // Distinct id for partials so the host decode watchdog restarts its clock
                // when the final decode of the same utterance begins.
                set_active(Some(if payload.partial {
                    format!("{}:partial", payload.utterance_id)
                } else {
                    payload.utterance_id.clone()
                }));

                let samples = match decode_audio_base64(&payload.audio_data) {
                    Ok(s) => s,
                    Err(e) => {
                        set_state("idle");
                        set_active(None);
                        send_error(attempt_id, "AUDIO_DECODE_FAILED", &e, true);
                        continue;
                    }
                };

                let language = payload.language.as_deref();
                let prompt = payload.context_prompt.as_deref().or(payload.initial_prompt.as_deref());
                let t_start = Instant::now();
                let result = engine.transcribe(&samples, payload.sample_rate.unwrap_or(16000), language, prompt);
                let latency_sec = t_start.elapsed().as_secs_f64();

                set_state("idle");
                set_active(None);

                match result {
                    Ok(t) => {
                        let _ = emit_transcription_result(
                            &mut *locked(&stdout),
                            attempt_id,
                            &payload.utterance_id,
                            seq,
                            &t.text,
                            language.unwrap_or("es"),
                            t.duration_sec,
                            t.inference_sec,
                            latency_sec,
                            get_process_memory_mb(),
                            t.no_speech_prob,
                            payload.partial,
                        );
                    }
                    Err(e) if payload.partial => {
                        // A failed provisional decode must not flip the worker into an error state.
                        eprintln!("Partial decode failed: {e}");
                        empty_partial(language.unwrap_or("es"), latency_sec);
                    }
                    Err(e) => send_error(attempt_id, "INFERENCE_FAILED", &e, true),
                }
            }

            "shutdown" => {
                set_state("idle");
                set_active(None);
                let _ = emit_shutdown_ack(&mut *locked(&stdout), attempt_id);
                drop(current_engine.take());
                return;
            }

            unknown => eprintln!("Unknown command ignored: {unknown}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::InitPayload;
    use base64::Engine as _;
    use std::io::{Cursor, Write};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct SharedOut(Arc<Mutex<Vec<u8>>>);
    impl Write for SharedOut {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    struct MockEngine;
    impl Engine for MockEngine {
        fn load(
            init: &InitPayload,
            progress: &mut ProgressFn,
        ) -> Result<(Self, ReadyInfo), LoadError> {
            match init.model_name.as_str() {
                "slow" => std::thread::sleep(std::time::Duration::from_millis(300)),
                "broken" => {
                    return Err(LoadError {
                        code: "ENGINE_LOAD_FAILED",
                        message: "boom".into(),
                        recoverable: true,
                    })
                }
                _ => {}
            }
            progress("download", "fetching", Some(50.0), Some(1), Some(2), None, None);
            Ok((
                MockEngine,
                ReadyInfo {
                    device: "cpu".into(),
                    compute_type: "int8".into(),
                    fallback_occurred: false,
                    fallback_reason: None,
                },
            ))
        }

        fn transcribe(
            &mut self,
            samples: &[f32],
            _sample_rate: u32,
            _language: Option<&str>,
            _prompt: Option<&str>,
        ) -> Result<Transcript, String> {
            if samples.len() == 1 {
                return Err("decode failed".into());
            }
            Ok(Transcript {
                text: "hello".into(),
                duration_sec: 1.0,
                inference_sec: 0.1,
                no_speech_prob: 0.0,
            })
        }
    }

    fn audio(n: usize) -> String {
        let bytes: Vec<u8> = (0..n).flat_map(|_| 0.5f32.to_le_bytes()).collect();
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    fn init(model: &str) -> String {
        format!(r#"{{"cmd":"init","attempt_id":7,"payload":{{"model_name":"{model}"}}}}"#)
    }

    fn transcribe(id: &str, samples: usize, partial: bool) -> String {
        format!(
            r#"{{"cmd":"transcribe","attempt_id":7,"payload":{{"utterance_id":"{id}","sequence":3,"audio_data":"{}","partial":{partial}}}}}"#,
            audio(samples)
        )
    }

    fn run(lines: &[String]) -> Vec<serde_json::Value> {
        let out = SharedOut::default();
        let input = Cursor::new(format!("{}\n", lines.join("\n")));
        run_worker::<MockEngine, _, _>(input, out.clone());
        let text = String::from_utf8(out.0.lock().unwrap().clone()).unwrap();
        text.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    fn events<'a>(v: &'a [serde_json::Value], name: &str) -> Vec<&'a serde_json::Value> {
        v.iter().filter(|e| e["event"] == name).collect()
    }

    #[test]
    fn ping_is_answered_with_pong_and_request_id() {
        let out = run(&[r#"{"cmd":"ping","request_id":"r1"}"#.into()]);
        let pongs = events(&out, "pong");
        assert_eq!(pongs.len(), 1);
        assert_eq!(pongs[0]["request_id"], "r1");
        assert_eq!(pongs[0]["payload"]["state"], "idle");
    }

    #[test]
    fn init_emits_progress_then_ready_with_engine_info() {
        let out = run(&[init("tiny")]);
        assert_eq!(events(&out, "download_progress").len(), 1);
        let ready = events(&out, "ready");
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0]["payload"]["model_name"], "tiny");
        assert_eq!(ready[0]["payload"]["device"], "cpu");
        assert_eq!(ready[0]["payload"]["compute_type"], "int8");
        assert_eq!(ready[0]["attempt_id"], 7);
    }

    #[test]
    fn load_failure_emits_error_with_engine_code() {
        let out = run(&[init("broken")]);
        let err = events(&out, "error");
        assert_eq!(err[0]["payload"]["code"], "ENGINE_LOAD_FAILED");
        assert_eq!(err[0]["payload"]["recoverable"], true);
    }

    #[test]
    fn transcribe_before_init_is_an_error() {
        let out = run(&[transcribe("u1", 4, false)]);
        assert_eq!(events(&out, "error")[0]["payload"]["code"], "ENGINE_NOT_INITIALIZED");
    }

    #[test]
    fn transcribe_emits_final_result() {
        let out = run(&[init("tiny"), transcribe("u1", 4, false)]);
        let res = events(&out, "transcription_result");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0]["payload"]["utterance_id"], "u1");
        assert_eq!(res[0]["payload"]["text"], "hello");
        assert_eq!(res[0]["payload"]["sequence"], 3);
        assert!(res[0]["payload"].get("partial").is_none());
    }

    #[test]
    fn partial_is_skipped_with_empty_result_when_commands_are_queued() {
        let out = run(&[init("slow"), transcribe("u1", 4, true), transcribe("u1", 4, false)]);
        let res = events(&out, "transcription_result");
        assert_eq!(res.len(), 2);
        assert_eq!(res[0]["payload"]["text"], "");
        assert_eq!(res[0]["payload"]["partial"], true);
        assert_eq!(res[1]["payload"]["text"], "hello");
    }

    #[test]
    fn failed_partial_decode_yields_empty_partial_not_error() {
        let out = run(&[init("tiny"), transcribe("u1", 1, true)]);
        assert!(events(&out, "error").is_empty());
        let res = events(&out, "transcription_result");
        assert_eq!(res[0]["payload"]["text"], "");
        assert_eq!(res[0]["payload"]["partial"], true);
    }

    #[test]
    fn failed_final_decode_is_inference_error() {
        let out = run(&[init("tiny"), transcribe("u1", 1, false)]);
        assert_eq!(events(&out, "error")[0]["payload"]["code"], "INFERENCE_FAILED");
    }

    #[test]
    fn shutdown_acks_and_stops_processing() {
        let out = run(&[init("tiny"), r#"{"cmd":"shutdown","attempt_id":7}"#.into(), transcribe("u1", 4, false)]);
        assert_eq!(events(&out, "shutdown_ack").len(), 1);
        assert!(events(&out, "transcription_result").is_empty());
    }
}
