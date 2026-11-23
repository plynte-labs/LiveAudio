// SPDX-License-Identifier: MIT

use std::sync::OnceLock;

use liveaudio_asr_worker::download::{is_parakeet_model, prepare_model, prepare_parakeet};
use liveaudio_asr_worker::model::{self, WhisperOrtEngine};
use liveaudio_asr_worker::parakeet::ParakeetEngine;
use liveaudio_asr_worker::protocol::InitPayload;
use liveaudio_worker_protocol::{
    run_self_test_cli, run_worker, Engine, LoadError, ProgressFn, ReadyInfo, Transcript,
};

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

enum OrtEngine {
    Whisper(Box<WhisperOrtEngine>),
    Parakeet(Box<ParakeetEngine>),
}

fn load_parakeet(init: &InitPayload, progress: &mut ProgressFn) -> Result<(OrtEngine, ReadyInfo), LoadError> {
    model::init_ort_runtime()
        .map_err(|message| LoadError { code: "DEVICE_UNAVAILABLE", message, recoverable: false })?;
    let rt = RUNTIME.get().expect("runtime initialized in main");
    let paths = rt
        .block_on(prepare_parakeet(init.cache_dir.as_deref(), init.local_files_only.unwrap_or(false), progress))
        .map_err(|message| LoadError { code: "MODEL_PREPARATION_FAILED", message, recoverable: true })?;
    let engine = ParakeetEngine::load(&paths, init.cpu_threads)
        .map_err(|message| LoadError { code: "ENGINE_LOAD_FAILED", message, recoverable: true })?;
    // The int8 export runs on the CPU whatever device was requested.
    let info = ReadyInfo {
        device: "cpu".to_string(),
        compute_type: "int8".to_string(),
        fallback_occurred: false,
        fallback_reason: None,
    };
    Ok((OrtEngine::Parakeet(Box::new(engine)), info))
}

impl Engine for OrtEngine {
    fn load(init: &InitPayload, progress: &mut ProgressFn) -> Result<(Self, ReadyInfo), LoadError> {
        if is_parakeet_model(&init.model_name) {
            return load_parakeet(init, progress);
        }
        let auto_fallback = init.auto_cpu_fallback.unwrap_or(true);
        let (effective_device, pre_fallback, pre_fallback_reason) =
            model::resolve_effective_device(&init.device, auto_fallback).map_err(|message| {
                LoadError { code: "DEVICE_UNAVAILABLE", message, recoverable: false }
            })?;

        let rt = RUNTIME.get().expect("runtime initialized in main");
        let model_paths = rt
            .block_on(prepare_model(
                &init.model_name,
                &effective_device,
                init.compute_type.as_deref(),
                init.cache_dir.as_deref(),
                init.local_files_only.unwrap_or(false),
                progress,
            ))
            .map_err(|message| LoadError {
                code: "MODEL_PREPARATION_FAILED",
                message,
                recoverable: true,
            })?;

        let engine = WhisperOrtEngine::load(
            &model_paths,
            &effective_device,
            init.compute_type.as_deref(),
            init.cpu_threads,
            auto_fallback,
        )
        .map_err(|message| LoadError { code: "ENGINE_LOAD_FAILED", message, recoverable: true })?;

        let info = ReadyInfo {
            device: engine.device.clone(),
            compute_type: engine.compute_type.clone(),
            fallback_occurred: pre_fallback || engine.fallback_occurred,
            fallback_reason: if engine.fallback_occurred {
                engine.fallback_reason.clone()
            } else if pre_fallback {
                pre_fallback_reason
            } else {
                None
            },
        };
        Ok((OrtEngine::Whisper(Box::new(engine)), info))
    }

    fn transcribe(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
        language: Option<&str>,
        prompt: Option<&str>,
    ) -> Result<Transcript, String> {
        match self {
            OrtEngine::Whisper(e) => e.transcribe(samples, sample_rate, language, prompt).map(|o| Transcript {
                text: o.text,
                duration_sec: o.duration_sec,
                inference_sec: o.inference_sec,
                no_speech_prob: o.no_speech_prob,
            }),
            // Parakeet auto-detects the language and takes no prompt.
            OrtEngine::Parakeet(e) => e.transcribe(samples, sample_rate).map(|o| Transcript {
                text: o.text,
                duration_sec: o.duration_sec,
                inference_sec: o.inference_sec,
                no_speech_prob: o.no_speech_prob,
            }),
        }
    }
}

fn main() {
    run_self_test_cli("liveaudio-asr-worker-ort", model::run_self_test);

    let runtime = tokio::runtime::Runtime::new().expect("failed to start tokio runtime");
    let _ = RUNTIME.set(runtime);
    run_worker::<OrtEngine, _, _>(std::io::BufReader::new(std::io::stdin()), std::io::stdout());
}
