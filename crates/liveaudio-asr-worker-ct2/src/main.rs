// SPDX-License-Identifier: MIT

//! `liveaudio-asr-worker-cuda`: CTranslate2 Whisper worker (NVIDIA CUDA only).
//!
//! Speaks the shared worker protocol. Any failure to bring CUDA up is reported as a
//! non-recoverable error so the host falls back to the ONNX Runtime worker.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Instant;

use ct2rs::{ComputeType, Config, Device, Whisper, WhisperOptions};
use liveaudio_asr_worker_ct2::download::prepare_model;
use liveaudio_asr_worker_ct2::model::{missing_cuda_dlls, plan_load, resolve_cache_dir, resolve_repo_id};
use liveaudio_worker_protocol::protocol::InitPayload;
use liveaudio_worker_protocol::{
    run_self_test_cli, run_worker, Engine, LoadError, ProgressFn, ReadyInfo, Transcript,
};

const SAMPLE_RATE: u32 = 16_000;

struct Ct2Engine {
    whisper: Whisper,
    beam_size: usize,
}

fn compute_type(name: &str) -> ComputeType {
    match name {
        "float32" => ComputeType::FLOAT32,
        "int8" => ComputeType::INT8,
        "int8_float16" => ComputeType::INT8_FLOAT16,
        "int8_float32" => ComputeType::INT8_FLOAT32,
        "bfloat16" => ComputeType::BFLOAT16,
        _ => ComputeType::FLOAT16,
    }
}

fn load_error(code: &'static str, message: String) -> LoadError {
    LoadError { code, message, recoverable: false }
}

impl Engine for Ct2Engine {
    fn load(init: &InitPayload, progress: &mut ProgressFn) -> Result<(Self, ReadyInfo), LoadError> {
        let plan = plan_load(init)?;

        // Fail fast (before any model download) when the CUDA runtime is not reachable.
        let mut dirs: Vec<std::path::PathBuf> = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .into_iter()
            .collect();
        if let Some(path) = std::env::var_os("PATH") {
            dirs.extend(std::env::split_paths(&path));
        }
        let missing = missing_cuda_dlls(&dirs);
        if !missing.is_empty() {
            return Err(load_error(
                "ENGINE_LOAD_FAILED",
                format!("CUDA runtime DLLs not found on PATH: {}", missing.join(", ")),
            ));
        }

        let candidate = std::path::PathBuf::from(&init.model_name);
        let model_dir = if candidate.is_dir() {
            candidate
        } else {
            let repo = resolve_repo_id(&init.model_name)
                .map_err(|m| load_error("MODEL_PREPARATION_FAILED", m))?;
            let env_cache = std::env::var("LIVEAUDIO_MODEL_CACHE").ok();
            let dir = resolve_cache_dir(
                init.cache_dir.as_deref(),
                env_cache.as_deref(),
                dirs::data_local_dir().as_deref(),
                &repo,
            );
            prepare_model(&repo, &dir, init.local_files_only.unwrap_or(false), progress)
                .map_err(|m| load_error("MODEL_PREPARATION_FAILED", m))?;
            dir
        };

        let config = Config {
            device: Device::CUDA,
            compute_type: compute_type(&plan.compute_type),
            ..Config::default()
        };
        // Missing CUDA DLLs or an unsupported GPU surface here; report them as a clean error.
        let whisper = match catch_unwind(AssertUnwindSafe(|| Whisper::new(&model_dir, config))) {
            Ok(Ok(w)) => w,
            Ok(Err(e)) => {
                return Err(load_error("ENGINE_LOAD_FAILED", format!("CTranslate2 CUDA load failed: {e}")))
            }
            Err(_) => return Err(load_error("ENGINE_LOAD_FAILED", "CTranslate2 CUDA load panicked".into())),
        };

        let info = ReadyInfo {
            device: "cuda".into(),
            compute_type: plan.compute_type.clone(),
            fallback_occurred: false,
            fallback_reason: None,
        };
        Ok((Ct2Engine { whisper, beam_size: plan.beam_size }, info))
    }

    /// `prompt` is not forwarded (the ct2rs `Whisper` wrapper has no prompt hook) and
    /// `no_speech_prob` is always 0.0 (the wrapper does not expose it).
    fn transcribe(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
        language: Option<&str>,
        _prompt: Option<&str>,
    ) -> Result<Transcript, String> {
        if sample_rate != SAMPLE_RATE {
            return Err(format!(
                "Unsupported sample rate {sample_rate} Hz: the CTranslate2 worker needs {SAMPLE_RATE} Hz mono"
            ));
        }
        let options = WhisperOptions { beam_size: self.beam_size, ..Default::default() };
        let start = Instant::now();
        let segments = self
            .whisper
            .generate(samples, language, false, &options)
            .map_err(|e| format!("CTranslate2 decode failed: {e}"))?;
        let text = segments.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
        Ok(Transcript {
            text,
            duration_sec: samples.len() as f64 / SAMPLE_RATE as f64,
            inference_sec: start.elapsed().as_secs_f64(),
            no_speech_prob: 0.0,
        })
    }
}

fn main() {
    // Must not touch the GPU or load CUDA: it runs on machines without NVIDIA hardware.
    run_self_test_cli("liveaudio-asr-worker-ct2", || Ok(()));
    run_worker::<Ct2Engine, _, _>(std::io::BufReader::new(std::io::stdin()), std::io::stdout());
}
