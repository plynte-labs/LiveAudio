// SPDX-License-Identifier: MIT

use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Deserialize)]
pub struct RustCommand {
    #[serde(default = "default_protocol_version")]
    pub version: u32,
    pub cmd: String,
    #[serde(default = "default_attempt_id")]
    pub attempt_id: u64,
    #[serde(default)]
    pub timestamp_ms: u64,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub payload: serde_json::Value,
}

fn default_protocol_version() -> u32 {
    PROTOCOL_VERSION
}

fn default_attempt_id() -> u64 {
    1
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InitPayload {
    #[serde(default = "default_model_name")]
    pub model_name: String,
    #[serde(default = "default_device")]
    pub device: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub initial_prompt: Option<String>,
    #[serde(default)]
    pub cpu_threads: Option<usize>,
    #[serde(default = "default_auto_cpu_fallback")]
    pub auto_cpu_fallback: Option<bool>,
    #[serde(default)]
    /// Accepted for protocol compatibility only: the native decoder is greedy (beam 1).
    pub beam_size: Option<usize>,
    #[serde(default = "default_compute_type")]
    pub compute_type: Option<String>,
    #[serde(default)]
    pub cache_dir: Option<String>,
    #[serde(default)]
    pub local_files_only: Option<bool>,
}

impl Default for InitPayload {
    fn default() -> Self {
        Self {
            model_name: default_model_name(),
            device: default_device(),
            language: None,
            initial_prompt: None,
            cpu_threads: None,
            auto_cpu_fallback: Some(true),
            beam_size: None,
            compute_type: default_compute_type(),
            cache_dir: None,
            local_files_only: None,
        }
    }
}

fn default_model_name() -> String {
    "small".to_string()
}

fn default_device() -> String {
    "auto".to_string()
}

fn default_compute_type() -> Option<String> {
    Some("default".to_string())
}

fn default_auto_cpu_fallback() -> Option<bool> {
    Some(true)
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PingPayload {
    #[serde(default)]
    pub client_monotonic_ms: Option<u64>,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TranscribePayload {
    pub utterance_id: String,
    #[serde(default)]
    pub sequence: Option<u64>,
    #[serde(alias = "audio_base64")]
    pub audio_data: String,
    #[serde(default)]
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub channels: Option<u16>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub context_prompt: Option<String>,
    #[serde(default)]
    pub initial_prompt: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Provisional decode of an in-progress phrase; the worker may skip it.
    #[serde(default)]
    pub partial: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutgoingEnvelope<T: Serialize> {
    pub version: u32,
    pub attempt_id: u64,
    pub timestamp_ms: u64,
    pub event: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub payload: T,
}

#[derive(Debug, Clone, Serialize)]
pub struct PongPayload<'a> {
    pub state: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_utterance_id: Option<&'a str>,
    pub memory_mb: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgressPayload<'a> {
    pub phase: &'a str,
    pub text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_downloaded: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_bps: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_sec: Option<f64>,
    pub attempt: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReadyPayload<'a> {
    pub model_name: &'a str,
    pub device: &'a str,
    pub compute_type: &'a str,
    pub fallback_occurred: bool,
    pub fallback_reason: Option<&'a str>,
    pub memory_mb: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TranscriptionSegment<'a> {
    pub id: usize,
    pub start: f64,
    pub end: f64,
    pub text: &'a str,
    pub no_speech_prob: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TranscriptionResultPayload<'a> {
    pub utterance_id: &'a str,
    pub sequence: u64,
    pub text: &'a str,
    pub language: &'a str,
    pub duration_sec: f64,
    pub inference_sec: f64,
    pub latency_sec: f64,
    pub memory_mb: u64,
    pub segments: Vec<TranscriptionSegment<'a>>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub partial: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorPayload<'a> {
    pub code: &'a str,
    pub message: &'a str,
    pub recoverable: bool,
}

pub fn emit_jsonl<T: Serialize>(writer: &mut impl Write, envelope: &OutgoingEnvelope<T>) -> std::io::Result<()> {
    serde_json::to_writer(&mut *writer, envelope)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

pub fn emit_pong(
    writer: &mut impl Write,
    attempt_id: u64,
    request_id: Option<String>,
    state: &str,
    active_utterance_id: Option<&str>,
    memory_mb: u64,
) -> std::io::Result<()> {
    let envelope = OutgoingEnvelope {
        version: PROTOCOL_VERSION,
        attempt_id,
        timestamp_ms: now_millis(),
        event: "pong",
        request_id,
        payload: PongPayload {
            state,
            active_utterance_id,
            memory_mb,
        },
    };
    emit_jsonl(writer, &envelope)
}

#[allow(clippy::too_many_arguments)]
pub fn emit_download_progress(
    writer: &mut impl Write,
    attempt_id: u64,
    phase: &str,
    text: &str,
    percent: Option<f64>,
    bytes_downloaded: Option<u64>,
    bytes_total: Option<u64>,
    speed_bps: Option<u64>,
    eta_sec: Option<f64>,
) -> std::io::Result<()> {
    let envelope = OutgoingEnvelope {
        version: PROTOCOL_VERSION,
        attempt_id,
        timestamp_ms: now_millis(),
        event: "download_progress",
        request_id: None,
        payload: DownloadProgressPayload {
            phase,
            text,
            percent,
            bytes_downloaded,
            bytes_total,
            speed_bps,
            eta_sec,
            attempt: attempt_id,
        },
    };
    emit_jsonl(writer, &envelope)
}

#[allow(clippy::too_many_arguments)]
pub fn emit_ready(
    writer: &mut impl Write,
    attempt_id: u64,
    model_name: &str,
    device: &str,
    compute_type: &str,
    fallback_occurred: bool,
    fallback_reason: Option<&str>,
    memory_mb: u64,
) -> std::io::Result<()> {
    let envelope = OutgoingEnvelope {
        version: PROTOCOL_VERSION,
        attempt_id,
        timestamp_ms: now_millis(),
        event: "ready",
        request_id: None,
        payload: ReadyPayload {
            model_name,
            device,
            compute_type,
            fallback_occurred,
            fallback_reason,
            memory_mb,
        },
    };
    emit_jsonl(writer, &envelope)
}

#[allow(clippy::too_many_arguments)]
pub fn emit_transcription_result(
    writer: &mut impl Write,
    attempt_id: u64,
    utterance_id: &str,
    sequence: u64,
    text: &str,
    language: &str,
    duration_sec: f64,
    inference_sec: f64,
    latency_sec: f64,
    memory_mb: u64,
    no_speech_prob: f64,
    partial: bool,
) -> std::io::Result<()> {
    let segment = TranscriptionSegment {
        id: 0,
        start: 0.0,
        end: duration_sec,
        text,
        no_speech_prob,
    };
    let envelope = OutgoingEnvelope {
        version: PROTOCOL_VERSION,
        attempt_id,
        timestamp_ms: now_millis(),
        event: "transcription_result",
        request_id: None,
        payload: TranscriptionResultPayload {
            utterance_id,
            sequence,
            text,
            language,
            duration_sec,
            inference_sec,
            latency_sec,
            memory_mb,
            segments: vec![segment],
            partial,
        },
    };
    emit_jsonl(writer, &envelope)
}

pub fn emit_error(
    writer: &mut impl Write,
    attempt_id: u64,
    code: &str,
    message: &str,
    recoverable: bool,
) -> std::io::Result<()> {
    let envelope = OutgoingEnvelope {
        version: PROTOCOL_VERSION,
        attempt_id,
        timestamp_ms: now_millis(),
        event: "error",
        request_id: None,
        payload: ErrorPayload {
            code,
            message,
            recoverable,
        },
    };
    emit_jsonl(writer, &envelope)
}

pub fn emit_shutdown_ack(writer: &mut impl Write, attempt_id: u64) -> std::io::Result<()> {
    let envelope = OutgoingEnvelope {
        version: PROTOCOL_VERSION,
        attempt_id,
        timestamp_ms: now_millis(),
        event: "shutdown_ack",
        request_id: None,
        payload: serde_json::json!({}),
    };
    emit_jsonl(writer, &envelope)
}

#[cfg(windows)]
pub fn get_process_memory_mb() -> u64 {
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    unsafe {
        let mut pmc: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        pmc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let handle = GetCurrentProcess();
        if K32GetProcessMemoryInfo(handle, &mut pmc, pmc.cb) != 0 {
            (pmc.WorkingSetSize / (1024 * 1024)) as u64
        } else {
            0
        }
    }
}

#[cfg(not(windows))]
pub fn get_process_memory_mb() -> u64 {
    if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
        let mut parts = statm.split_whitespace();
        if let Some(_size) = parts.next() {
            if let Some(resident) = parts.next() {
                if let Ok(pages) = resident.parse::<u64>() {
                    return (pages * 4096) / (1024 * 1024);
                }
            }
        }
    }
    0
}

pub fn decode_audio_base64(audio_b64: &str) -> Result<Vec<f32>, String> {
    let trimmed = audio_b64.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let raw_bytes = base64::engine::general_purpose::STANDARD
        .decode(trimmed)
        .map_err(|e| format!("Base64 decoding failed: {e}"))?;

    if raw_bytes.len() % 4 != 0 {
        return Err(format!(
            "Decoded byte count {} is not divisible by 4 for float32 PCM",
            raw_bytes.len()
        ));
    }

    let sample_count = raw_bytes.len() / 4;
    let mut samples = Vec::with_capacity(sample_count);
    for chunk in raw_bytes.chunks_exact(4) {
        let val = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        samples.push(val);
    }
    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcribe_payload_partial_defaults_to_false() {
        let payload: TranscribePayload =
            serde_json::from_str(r#"{"utterance_id":"utt_1","audio_data":""}"#).unwrap();
        assert!(!payload.partial);
        let payload: TranscribePayload =
            serde_json::from_str(r#"{"utterance_id":"utt_1","audio_data":"","partial":true}"#)
                .unwrap();
        assert!(payload.partial);
    }

    #[test]
    fn transcription_result_marks_partial_and_omits_the_flag_for_finals() {
        let mut final_line = Vec::new();
        emit_transcription_result(&mut final_line, 1, "utt_1", 1, "hola", "es", 1.0, 0.1, 0.1, 1, 0.0, false)
            .unwrap();
        let final_json: serde_json::Value = serde_json::from_slice(&final_line).unwrap();
        assert!(final_json["payload"].get("partial").is_none());

        let mut partial_line = Vec::new();
        emit_transcription_result(&mut partial_line, 1, "utt_1", 1, "ho", "es", 1.0, 0.1, 0.1, 1, 0.0, true)
            .unwrap();
        let partial_json: serde_json::Value = serde_json::from_slice(&partial_line).unwrap();
        assert_eq!(partial_json["payload"]["partial"], true);
    }

    #[test]
    fn parse_rust_command_with_extra_fields() {
        let line = r#"{
            "version": 1,
            "cmd": "init",
            "attempt_id": 42,
            "timestamp_ms": 12345678,
            "unknown_top_level": "ignored",
            "payload": {
                "model_name": "base",
                "device": "cpu",
                "custom_future_flag": true,
                "cpu_threads": 4
            }
        }"#;

        let cmd: RustCommand = serde_json::from_str(line).expect("must parse successfully");
        assert_eq!(cmd.cmd, "init");
        assert_eq!(cmd.attempt_id, 42);

        let payload: InitPayload =
            serde_json::from_value(cmd.payload).expect("payload parses successfully");
        assert_eq!(payload.model_name, "base");
        assert_eq!(payload.device, "cpu");
        assert_eq!(payload.cpu_threads, Some(4));
        assert_eq!(payload.auto_cpu_fallback, Some(true));
    }

    #[test]
    fn parse_ping_and_emit_pong() {
        let ping_line = r#"{"version":1,"cmd":"ping","request_id":"req-99","timestamp_ms":100}"#;
        let cmd: RustCommand = serde_json::from_str(ping_line).expect("ping command");
        assert_eq!(cmd.cmd, "ping");
        assert_eq!(cmd.request_id, Some("req-99".to_string()));

        let mut buf = Vec::new();
        emit_pong(
            &mut buf,
            cmd.attempt_id,
            cmd.request_id.clone(),
            "idle",
            None,
            128,
        )
        .expect("emit pong");

        let pong_str = String::from_utf8(buf).expect("valid utf8 output");
        assert!(pong_str.ends_with('\n'));
        let parsed: serde_json::Value = serde_json::from_str(&pong_str).expect("valid json");
        assert_eq!(parsed["event"], "pong");
        assert_eq!(parsed["version"], 1);
        assert_eq!(parsed["request_id"], "req-99");
        assert_eq!(parsed["payload"]["state"], "idle");
        assert_eq!(parsed["payload"]["memory_mb"], 128);
    }

    #[test]
    fn decode_audio_base64_valid_and_invalid() {
        assert_eq!(decode_audio_base64("").unwrap().len(), 0);

        let input = vec![0.0f32, 0.5f32, -0.5f32, 1.0f32];
        let mut raw = Vec::new();
        for &s in &input {
            raw.extend_from_slice(&s.to_le_bytes());
        }
        let b64 = base64::engine::general_purpose::STANDARD.encode(&raw);
        let decoded = decode_audio_base64(&b64).expect("valid audio decode");
        assert_eq!(decoded, input);

        let bad_len_bytes = vec![0u8, 1, 2];
        let bad_b64 = base64::engine::general_purpose::STANDARD.encode(&bad_len_bytes);
        assert!(decode_audio_base64(&bad_b64).is_err());
    }

    #[test]
    fn memory_query_does_not_panic() {
        let mem = get_process_memory_mb();
        assert!(mem < 1_000_000);
    }

    #[test]
    fn emit_ready_serializes_fallback_fields() {
        let mut buf = Vec::new();
        emit_ready(
            &mut buf,
            1,
            "small",
            "directml",
            "float32",
            false,
            None,
            256,
        )
        .expect("emit_ready without fallback");

        let line = String::from_utf8(buf).expect("utf8");
        let val: serde_json::Value = serde_json::from_str(&line).expect("valid json");
        assert_eq!(val["event"], "ready");
        assert_eq!(val["payload"]["model_name"], "small");
        assert_eq!(val["payload"]["device"], "directml");
        assert_eq!(val["payload"]["compute_type"], "float32");
        assert_eq!(val["payload"]["fallback_occurred"], false);
        assert_eq!(val["payload"]["fallback_reason"], serde_json::Value::Null);
        assert_eq!(val["payload"]["memory_mb"], 256);

        let mut buf2 = Vec::new();
        emit_ready(
            &mut buf2,
            2,
            "small",
            "cpu",
            "int8",
            true,
            Some("CUDA and DirectML GPU execution providers are unavailable; fell back to CPU"),
            128,
        )
        .expect("emit_ready with fallback");

        let line2 = String::from_utf8(buf2).expect("utf8");
        let val2: serde_json::Value = serde_json::from_str(&line2).expect("valid json");
        assert_eq!(val2["payload"]["fallback_occurred"], true);
        assert_eq!(
            val2["payload"]["fallback_reason"],
            "CUDA and DirectML GPU execution providers are unavailable; fell back to CPU"
        );
    }
}
