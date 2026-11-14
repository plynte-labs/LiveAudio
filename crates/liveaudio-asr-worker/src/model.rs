// SPDX-License-Identifier: MIT

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

use ort::ep::{self, ExecutionProvider};
use ort::memory::{AllocationDevice, AllocatorType, MemoryInfo, MemoryType};
use ort::session::builder::SessionBuilder;
use ort::session::Session;
use ort::value::{DynValue, Tensor, TensorElementType, TensorRef};

use crate::download::ResolvedModelPaths;
use crate::mel::MelExtractor;
use crate::tokenizer::{build_byte_to_unicode, WhisperTokenizer};

static ORT_INITIALIZATION: OnceLock<Result<(), String>> = OnceLock::new();

pub fn init_ort_runtime() -> Result<(), String> {
    ORT_INITIALIZATION
        .get_or_init(|| {
            let dylib_path = find_ort_dylib()?;
            if let Some(parent) = dylib_path.parent() {
                let current_path = std::env::var_os("PATH").unwrap_or_default();
                let mut paths: Vec<std::path::PathBuf> = std::env::split_paths(&current_path).collect();
                if !paths.iter().any(|p| p.as_path() == parent) {
                    paths.insert(0, parent.to_path_buf());
                    if let Ok(joined) = std::env::join_paths(paths) {
                        std::env::set_var("PATH", joined);
                    }
                }
            }
            let env = ort::init_from(&dylib_path)
                .map_err(|e| format!("Failed to initialize ONNX Runtime from {}: {e}", dylib_path.display()))?;
            env.commit();
            Ok(())
        })
        .clone()
}

pub fn resolve_effective_device(
    requested_device: &str,
    auto_cpu_fallback: bool,
) -> Result<(String, bool, Option<String>), String> {
    init_ort_runtime()?;
    match requested_device.to_lowercase().as_str() {
        "cuda" | "gpu" => {
            if ep::CUDA::default().is_available().unwrap_or(false) {
                Ok(("cuda".to_string(), false, None))
            } else if cfg!(target_os = "windows") && ep::DirectML::default().is_available().unwrap_or(false) {
                Ok(("directml".to_string(), false, None))
            } else if auto_cpu_fallback {
                Ok((
                    "cpu".to_string(),
                    true,
                    Some("CUDA and DirectML GPU execution providers are unavailable; fell back to CPU".to_string()),
                ))
            } else {
                Err("No GPU execution provider (CUDA or DirectML) is available in ONNX Runtime".to_string())
            }
        }
        "dml" | "directml" => {
            if cfg!(target_os = "windows") && ep::DirectML::default().is_available().unwrap_or(false) {
                Ok(("directml".to_string(), false, None))
            } else if auto_cpu_fallback {
                Ok((
                    "cpu".to_string(),
                    true,
                    Some("DirectML execution provider is unavailable; fell back to CPU".to_string()),
                ))
            } else {
                Err("DirectML execution provider is not available in loaded runtime".to_string())
            }
        }
        "auto" => {
            if ep::CUDA::default().is_available().unwrap_or(false) {
                Ok(("cuda".to_string(), false, None))
            } else if cfg!(target_os = "windows") && ep::DirectML::default().is_available().unwrap_or(false) {
                Ok(("directml".to_string(), false, None))
            } else {
                Ok(("cpu".to_string(), false, None))
            }
        }
        _ => Ok(("cpu".to_string(), false, None)),
    }
}

fn find_ort_dylib() -> Result<PathBuf, String> {
    if let Ok(env_path) = std::env::var("LIVEAUDIO_ORT_DYLIB_PATH") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return Ok(p);
        }
    }

    #[cfg(target_os = "windows")]
    let rel_target = Path::new("onnxruntime/windows-x64/onnxruntime.dll");
    #[cfg(target_os = "linux")]
    let rel_target = Path::new("onnxruntime/linux-x64/libonnxruntime.so.1.24.4");
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    let rel_target = Path::new("onnxruntime/onnxruntime.dll");

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            let mut candidates = Vec::new();
            candidates.push(exe_dir.join(rel_target));
            candidates.push(exe_dir.join("resources").join(rel_target));
            candidates.push(exe_dir.join("packaging").join(rel_target));

            for ancestor in ["..", "../..", "../../.."] {
                let base = exe_dir.join(ancestor);
                candidates.push(base.join(rel_target));
                candidates.push(base.join("resources").join(rel_target));
                candidates.push(base.join("packaging").join(rel_target));
            }

            for cand in candidates {
                if cand.is_file() {
                    if let Ok(canon) = std::fs::canonicalize(&cand) {
                        return Ok(canon);
                    }
                    return Ok(cand);
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    let local_dev = PathBuf::from("packaging/onnxruntime/windows-x64/onnxruntime.dll");
    #[cfg(target_os = "linux")]
    let local_dev = PathBuf::from("packaging/onnxruntime/linux-x64/libonnxruntime.so.1.24.4");
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    let local_dev = PathBuf::from("packaging/onnxruntime/onnxruntime.dll");

    if local_dev.is_file() {
        if let Ok(canon) = std::fs::canonicalize(&local_dev) {
            return Ok(canon);
        }
        return Ok(local_dev);
    }

    // Debug builds and tests: newer cargo layouts run test binaries outside target/<profile>/deps,
    // so resolve the checkout's packaging dir from the crate location instead of the exe path.
    #[cfg(debug_assertions)]
    {
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging").join(rel_target);
        if checkout.is_file() {
            return Ok(std::fs::canonicalize(&checkout).unwrap_or(checkout));
        }
    }

    Err("Could not locate ONNX Runtime shared library. Set LIVEAUDIO_ORT_DYLIB_PATH or install LiveAudio.".to_string())
}

#[derive(Debug, Clone)]
pub struct TranscriptionOutput {
    pub text: String,
    pub duration_sec: f64,
    pub inference_sec: f64,
    pub no_speech_prob: f64,
}

fn cpu_output_memory_info() -> Result<MemoryInfo<'static>, String> {
    MemoryInfo::new(
        AllocationDevice::CPU,
        0,
        AllocatorType::Device,
        MemoryType::CPUOutput,
    )
    .map_err(|e| format!("Failed to create CPU output MemoryInfo: {e}"))
}

pub struct WhisperOrtEngine {
    encoder: Session,
    decoder: Session,
    decoder_with_past: Option<Session>,
    tokenizer: WhisperTokenizer,
    mel: MelExtractor,
    pub device: String,
    pub compute_type: String,
    pub fallback_occurred: bool,
    pub fallback_reason: Option<String>,
    pub encoder_expects_f16: bool,
    num_decoder_layers: usize,
    d_model: usize,
    vocab_size: usize,
    suppress_tokens: Vec<i64>,
    begin_suppress_tokens: Vec<i64>,
    encoder_input_name: String,
    encoder_output_name: String,
    decoder_input_ids_name: String,
    decoder_enc_hidden_name: String,
    decoder_past_input_ids_name: String,
}

fn extract_step0_logits_and_prob(
    logits_val: &DynValue,
    current_tokens: &[i64],
    sot_token: i64,
    nospeech_token: i64,
    fallback_vocab_size: usize,
) -> Result<(usize, Vec<f32>, f64), String> {
    let elem_type = logits_val.dtype().tensor_type();
    let is_f16 = matches!(elem_type, Some(TensorElementType::Float16));

    let (vs, sot_logits_vec, last_step_logits_vec) = if is_f16 {
        let (shape, data) = logits_val
            .try_extract_tensor::<half::f16>()
            .map_err(|e| format!("Failed to extract step 0 f16 logits: {e}"))?;
        let vs = if shape.len() == 3 && shape[2] > 0 {
            shape[2] as usize
        } else {
            fallback_vocab_size
        };
        let seq_len = current_tokens.len();
        let sot_index = current_tokens
            .iter()
            .position(|&t| t == sot_token)
            .unwrap_or(0);
        let sot_offset = sot_index * vs;
        let sot_end = (sot_offset + vs).min(data.len());
        let sot_start = sot_offset.min(sot_end);
        let sot_logits: Vec<f32> = data[sot_start..sot_end].iter().map(|v| v.to_f32()).collect();

        let last_step_offset = seq_len.saturating_sub(1) * vs;
        let last_end = (last_step_offset + vs).min(data.len());
        let last_start = last_step_offset.min(last_end);
        let last_step_logits: Vec<f32> = data[last_start..last_end].iter().map(|v| v.to_f32()).collect();

        (vs, sot_logits, last_step_logits)
    } else {
        let (shape, data) = logits_val
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("Failed to extract step 0 f32 logits: {e}"))?;
        let vs = if shape.len() == 3 && shape[2] > 0 {
            shape[2] as usize
        } else {
            fallback_vocab_size
        };
        let seq_len = current_tokens.len();
        let sot_index = current_tokens
            .iter()
            .position(|&t| t == sot_token)
            .unwrap_or(0);
        let sot_offset = sot_index * vs;
        let sot_end = (sot_offset + vs).min(data.len());
        let sot_start = sot_offset.min(sot_end);
        let sot_logits = data[sot_start..sot_end].to_vec();

        let last_step_offset = seq_len.saturating_sub(1) * vs;
        let last_end = (last_step_offset + vs).min(data.len());
        let last_start = last_step_offset.min(last_end);
        let last_step_logits = data[last_start..last_end].to_vec();

        (vs, sot_logits, last_step_logits)
    };

    let prob = calculate_token_softmax_prob(&sot_logits_vec, nospeech_token as usize);
    Ok((vs, last_step_logits_vec, prob))
}

fn extract_step_logits_vec(
    logits_val: &DynValue,
    offset: usize,
    fallback_vocab_size: usize,
) -> Result<Vec<f32>, String> {
    let elem_type = logits_val.dtype().tensor_type();
    let is_f16 = matches!(elem_type, Some(TensorElementType::Float16));

    if is_f16 {
        let (shape, data) = logits_val
            .try_extract_tensor::<half::f16>()
            .map_err(|e| format!("Failed to extract step f16 logits: {e}"))?;
        let step_vocab = if shape.len() == 3 && shape[2] > 0 {
            shape[2] as usize
        } else {
            fallback_vocab_size
        };
        let start = offset.min(data.len());
        let end = (offset + step_vocab).min(data.len());
        Ok(data[start..end].iter().map(|v| v.to_f32()).collect())
    } else {
        let (shape, data) = logits_val
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("Failed to extract step f32 logits: {e}"))?;
        let step_vocab = if shape.len() == 3 && shape[2] > 0 {
            shape[2] as usize
        } else {
            fallback_vocab_size
        };
        let start = offset.min(data.len());
        let end = (offset + step_vocab).min(data.len());
        Ok(data[start..end].to_vec())
    }
}

impl WhisperOrtEngine {
    pub fn load(
        model_paths: &ResolvedModelPaths,
        requested_device: &str,
        compute_type: Option<&str>,
        cpu_threads: Option<usize>,
        auto_cpu_fallback: bool,
    ) -> Result<Self, String> {
        init_ort_runtime()?;

        let tokenizer = WhisperTokenizer::from_file(&model_paths.tokenizer_json)?;
        let mel = MelExtractor::from_preprocessor_config(&model_paths.preprocessor_config_json)?;

        let try_device = match requested_device.to_lowercase().as_str() {
            "cuda" | "gpu" => "cuda",
            "dml" | "directml" => "directml",
            "auto" => {
                if ep::CUDA::default().is_available().unwrap_or(false) {
                    "cuda"
                } else if cfg!(target_os = "windows") && ep::DirectML::default().is_available().unwrap_or(false) {
                    "directml"
                } else {
                    "cpu"
                }
            }
            _ => "cpu",
        };

        if try_device == "cuda" {
            match Self::build_sessions(model_paths, "cuda", cpu_threads) {
                Ok((encoder, decoder, decoder_with_past)) => {
                    return Self::finish_initialization(
                        model_paths,
                        encoder,
                        decoder,
                        decoder_with_past,
                        tokenizer,
                        mel,
                        "cuda".to_string(),
                        compute_type,
                        false,
                        None,
                    );
                }
                Err(cuda_err) => {
                    tracing::warn!("CUDA session initialization failed ({cuda_err}); trying DirectML cascade");
                    if cfg!(target_os = "windows") && ep::DirectML::default().is_available().unwrap_or(false) {
                        match Self::build_sessions(model_paths, "directml", cpu_threads) {
                            Ok((encoder, decoder, decoder_with_past)) => {
                                return Self::finish_initialization(
                                    model_paths,
                                    encoder,
                                    decoder,
                                    decoder_with_past,
                                    tokenizer,
                                    mel,
                                    "directml".to_string(),
                                    compute_type,
                                    false,
                                    None,
                                );
                            }
                            Err(dml_err) => {
                                tracing::warn!("DirectML session initialization failed ({dml_err}); falling back to CPU");
                                if auto_cpu_fallback || requested_device == "auto" {
                                    let cpu_fallback_paths = model_paths.fallback_cpu_paths().unwrap_or_else(|| model_paths.clone());
                                    let (encoder, decoder, decoder_with_past) = Self::build_sessions(&cpu_fallback_paths, "cpu", cpu_threads)
                                        .map_err(|cpu_err| format!("CPU fallback session creation failed: {cpu_err}"))?;
                                    return Self::finish_initialization(
                                        &cpu_fallback_paths,
                                        encoder,
                                        decoder,
                                        decoder_with_past,
                                        tokenizer,
                                        mel,
                                        "cpu".to_string(),
                                        compute_type,
                                        true,
                                        Some(format!("GPU session initialization failed (CUDA: {cuda_err}; DirectML: {dml_err}); fell back to CPU")),
                                    );
                                } else {
                                    return Err(format!("GPU session initialization failed (CUDA: {cuda_err}; DirectML: {dml_err})"));
                                }
                            }
                        }
                    } else if auto_cpu_fallback || requested_device == "auto" {
                        let cpu_fallback_paths = model_paths.fallback_cpu_paths().unwrap_or_else(|| model_paths.clone());
                        let (encoder, decoder, decoder_with_past) = Self::build_sessions(&cpu_fallback_paths, "cpu", cpu_threads)
                            .map_err(|cpu_err| format!("CPU fallback session creation failed: {cpu_err}"))?;
                        return Self::finish_initialization(
                            &cpu_fallback_paths,
                            encoder,
                            decoder,
                            decoder_with_past,
                            tokenizer,
                            mel,
                            "cpu".to_string(),
                            compute_type,
                            true,
                            Some(format!("GPU session initialization failed ({cuda_err}); fell back to CPU")),
                        );
                    } else {
                        return Err(format!("CUDA session initialization failed: {cuda_err}"));
                    }
                }
            }
        } else if try_device == "directml" {
            match Self::build_sessions(model_paths, "directml", cpu_threads) {
                Ok((encoder, decoder, decoder_with_past)) => {
                    return Self::finish_initialization(
                        model_paths,
                        encoder,
                        decoder,
                        decoder_with_past,
                        tokenizer,
                        mel,
                        "directml".to_string(),
                        compute_type,
                        false,
                        None,
                    );
                }
                Err(dml_err) => {
                    tracing::warn!("DirectML session initialization failed ({dml_err}); falling back to CPU");
                    if auto_cpu_fallback || requested_device == "auto" {
                        let cpu_fallback_paths = model_paths.fallback_cpu_paths().unwrap_or_else(|| model_paths.clone());
                        let (encoder, decoder, decoder_with_past) = Self::build_sessions(&cpu_fallback_paths, "cpu", cpu_threads)
                            .map_err(|cpu_err| format!("CPU fallback session creation failed: {cpu_err}"))?;
                        return Self::finish_initialization(
                            &cpu_fallback_paths,
                            encoder,
                            decoder,
                            decoder_with_past,
                            tokenizer,
                            mel,
                            "cpu".to_string(),
                            compute_type,
                            true,
                            Some(format!("GPU session initialization failed ({dml_err}); fell back to CPU")),
                        );
                    } else {
                        return Err(format!("DirectML session initialization failed: {dml_err}"));
                    }
                }
            }
        } else {
            let (encoder, decoder, decoder_with_past, effective_paths) =
                match Self::build_sessions(model_paths, "cpu", cpu_threads) {
                    Ok((e, d, p)) => (e, d, p, model_paths.clone()),
                    Err(cpu_err) => {
                        if let Some(fallback_paths) = model_paths.fallback_cpu_paths() {
                            tracing::warn!(
                                "CPU session creation failed with FP16 ({cpu_err}); falling back to non-FP16 model variant"
                            );
                            let (e, d, p) = Self::build_sessions(&fallback_paths, "cpu", cpu_threads)
                                .map_err(|e| format!("Session creation failed: {e}"))?;
                            (e, d, p, fallback_paths)
                        } else {
                            return Err(format!("Session creation failed: {cpu_err}"));
                        }
                    }
                };
            Self::finish_initialization(
                &effective_paths,
                encoder,
                decoder,
                decoder_with_past,
                tokenizer,
                mel,
                "cpu".to_string(),
                compute_type,
                false,
                None,
            )
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_initialization(
        model_paths: &ResolvedModelPaths,
        encoder: Session,
        decoder: Session,
        decoder_with_past: Option<Session>,
        tokenizer: WhisperTokenizer,
        mel: MelExtractor,
        device: String,
        _compute_type: Option<&str>,
        fallback_occurred: bool,
        fallback_reason: Option<String>,
    ) -> Result<Self, String> {
        let encoder_expects_f16 = matches!(
            encoder.inputs().first().and_then(|i| i.dtype().tensor_type()),
            Some(TensorElementType::Float16)
        );

        let encoder_name = model_paths
            .encoder_onnx
            .file_name()
            .map(|f| f.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let decoder_name = model_paths
            .decoder_onnx
            .file_name()
            .map(|f| f.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let effective_compute = if encoder_name.contains("quantized")
            || encoder_name.contains("int8")
            || encoder_name.contains("uint8")
            || decoder_name.contains("quantized")
            || decoder_name.contains("int8")
            || decoder_name.contains("uint8")
        {
            "int8".to_string()
        } else if encoder_name.contains("fp16")
            || decoder_name.contains("fp16")
            || encoder_expects_f16
        {
            "float16".to_string()
        } else {
            "float32".to_string()
        };

        let encoder_input_name = encoder
            .inputs()
            .first()
            .map(|i| i.name().to_string())
            .unwrap_or_else(|| "input_features".to_string());

        let encoder_output_name = encoder
            .outputs()
            .first()
            .map(|o| o.name().to_string())
            .unwrap_or_else(|| "last_hidden_state".to_string());

        let decoder_input_ids_name = decoder
            .inputs()
            .iter()
            .find(|i| i.name() == "input_ids")
            .map(|i| i.name().to_string())
            .unwrap_or_else(|| "input_ids".to_string());

        let decoder_enc_hidden_name = decoder
            .inputs()
            .iter()
            .find(|i| i.name().contains("encoder_hidden_states"))
            .map(|i| i.name().to_string())
            .unwrap_or_else(|| "encoder_hidden_states".to_string());

        let decoder_past_input_ids_name = decoder_with_past
            .as_ref()
            .and_then(|s| {
                s.inputs()
                    .iter()
                    .find(|i| i.name() == "input_ids" || i.name().contains("input_ids"))
                    .map(|i| i.name().to_string())
            })
            .unwrap_or_else(|| "input_ids".to_string());

        let mut num_decoder_layers = 0;
        for l in 0..64 {
            let key_name = format!("present.{l}.decoder.key");
            if decoder.outputs().iter().any(|o| o.name() == key_name) {
                num_decoder_layers += 1;
            } else {
                break;
            }
        }

        let decoder_with_past = if num_decoder_layers == 0 {
            None
        } else {
            decoder_with_past
        };

        let model_cfg = parse_model_config(&model_paths.config_json);

        let mut engine = Self {
            encoder,
            decoder,
            decoder_with_past,
            tokenizer,
            mel,
            device,
            compute_type: effective_compute,
            fallback_occurred,
            fallback_reason,
            encoder_expects_f16,
            num_decoder_layers,
            d_model: model_cfg.d_model,
            vocab_size: model_cfg.vocab_size,
            suppress_tokens: model_cfg.suppress_tokens,
            begin_suppress_tokens: model_cfg.begin_suppress_tokens,
            encoder_input_name,
            encoder_output_name,
            decoder_input_ids_name,
            decoder_enc_hidden_name,
            decoder_past_input_ids_name,
        };

        if engine.device == "directml" || engine.device == "cuda" {
            let _ = engine.warmup_gpu();
        }

        Ok(engine)
    }

    fn warmup_gpu(&mut self) -> Result<(), String> {
        let n_mels = self.mel.n_mels;
        let mut enc_outputs = if self.encoder_expects_f16 {
            let mel_tensor = Tensor::<half::f16>::from_array((
                [1usize, n_mels, 3000usize],
                vec![half::f16::ZERO; n_mels * 3000].into_boxed_slice(),
            ))
            .map_err(|e| format!("Warmup mel f16 tensor error: {e}"))?;
            self.encoder
                .run(ort::inputs![
                    &self.encoder_input_name => mel_tensor,
                ])
                .map_err(|e| format!("Warmup enc run error: {e}"))?
        } else {
            let mel_tensor = Tensor::<f32>::from_array((
                [1usize, n_mels, 3000usize],
                vec![0.0f32; n_mels * 3000].into_boxed_slice(),
            ))
            .map_err(|e| format!("Warmup mel tensor error: {e}"))?;
            self.encoder
                .run(ort::inputs![
                    &self.encoder_input_name => mel_tensor,
                ])
                .map_err(|e| format!("Warmup enc run error: {e}"))?
        };

        let enc_hidden = enc_outputs
            .remove(&self.encoder_output_name)
            .ok_or_else(|| "Warmup missing encoder output".to_string())?;
        drop(enc_outputs);

        let prefix = vec![
            self.tokenizer.sot_token,
            50262,
            self.tokenizer.transcribe_token,
            self.tokenizer.notimestamps_token,
        ];
        let ids_tensor = Tensor::from_array(([1usize, prefix.len()], prefix.into_boxed_slice()))
            .map_err(|e| format!("Warmup ids tensor error: {e}"))?;

        let mut step0_outputs = self
            .decoder
            .run(ort::inputs![
                &self.decoder_input_ids_name => ids_tensor,
                &self.decoder_enc_hidden_name => enc_hidden,
            ])
            .map_err(|e| format!("Warmup dec0 run error: {e}"))?;

        if let Some(decoder_past) = self.decoder_with_past.as_mut() {
            let cpu_out_mem = cpu_output_memory_info()?;
            let mut past_binding = decoder_past
                .create_binding()
                .map_err(|e| format!("Warmup past binding error: {e}"))?;
            let mut dec_kv = Vec::with_capacity(self.num_decoder_layers);
            for l in 0..self.num_decoder_layers {
                let dk = step0_outputs
                    .remove(format!("present.{l}.decoder.key"))
                    .ok_or_else(|| "Missing warmup dk".to_string())?;
                let dv = step0_outputs
                    .remove(format!("present.{l}.decoder.value"))
                    .ok_or_else(|| "Missing warmup dv".to_string())?;
                let ek = step0_outputs
                    .remove(format!("present.{l}.encoder.key"))
                    .ok_or_else(|| "Missing warmup ek".to_string())?;
                let ev = step0_outputs
                    .remove(format!("present.{l}.encoder.value"))
                    .ok_or_else(|| "Missing warmup ev".to_string())?;
                past_binding
                    .bind_input(format!("past_key_values.{l}.encoder.key"), &ek)
                    .map_err(|e| format!("Warmup past enc key error: {e}"))?;
                past_binding
                    .bind_input(format!("past_key_values.{l}.encoder.value"), &ev)
                    .map_err(|e| format!("Warmup past enc val error: {e}"))?;
                dec_kv.push((dk, dv));
            }
            drop(step0_outputs);

            let tok_tensor =
                Tensor::from_array(([1usize, 1usize], vec![self.tokenizer.eot_token].into_boxed_slice()))
                    .map_err(|e| format!("Warmup tok error: {e}"))?;
            past_binding
                .bind_input(&self.decoder_past_input_ids_name, &tok_tensor)
                .map_err(|e| format!("Warmup past ids error: {e}"))?;
            for l in 0..self.num_decoder_layers {
                past_binding
                    .bind_input(format!("past_key_values.{l}.decoder.key"), &dec_kv[l].0)
                    .map_err(|e| format!("Warmup past dk error: {e}"))?;
                past_binding
                    .bind_input(format!("past_key_values.{l}.decoder.value"), &dec_kv[l].1)
                    .map_err(|e| format!("Warmup past dv error: {e}"))?;
                past_binding
                    .bind_output_to_device(format!("present.{l}.decoder.key"), &cpu_out_mem)
                    .map_err(|e| format!("Warmup past out dk error: {e}"))?;
                past_binding
                    .bind_output_to_device(format!("present.{l}.decoder.value"), &cpu_out_mem)
                    .map_err(|e| format!("Warmup past out dv error: {e}"))?;
            }
            past_binding
                .bind_output_to_device("logits", &cpu_out_mem)
                .map_err(|e| format!("Warmup past logits error: {e}"))?;
            let _ = past_binding.synchronize_inputs();
            let _ = decoder_past.run_binding(&past_binding);
            let _ = past_binding.synchronize_outputs();
        }

        Ok(())
    }

    fn build_sessions(
        model_paths: &ResolvedModelPaths,
        device: &str,
        cpu_threads: Option<usize>,
    ) -> Result<(Session, Session, Option<Session>), String> {
        let mut encoder_builder = Self::make_session_builder(device, cpu_threads)?;
        let encoder = encoder_builder
            .commit_from_file(&model_paths.encoder_onnx)
            .map_err(|e| format!("Failed to load encoder from {}: {e}", model_paths.encoder_onnx.display()))?;

        let mut decoder_builder = Self::make_session_builder(device, cpu_threads)?;
        let decoder = decoder_builder
            .commit_from_file(&model_paths.decoder_onnx)
            .map_err(|e| format!("Failed to load decoder from {}: {e}", model_paths.decoder_onnx.display()))?;

        let decoder_with_past = if let Some(ref past_path) = model_paths.decoder_with_past_onnx {
            match Self::make_session_builder(device, cpu_threads).and_then(|mut b| {
                b.commit_from_file(past_path)
                    .map_err(|e| format!("Failed to commit decoder_with_past session: {e}"))
            }) {
                Ok(s) => Some(s),
                Err(err) => {
                    eprintln!("Warning: Failed to load decoder_with_past_model ({err}); falling back to standard decoder");
                    None
                }
            }
        } else {
            None
        };

        Ok((encoder, decoder, decoder_with_past))
    }

    fn make_session_builder(
        device: &str,
        cpu_threads: Option<usize>,
    ) -> Result<SessionBuilder, String> {
        let mut builder = Session::builder()
            .map_err(|e| format!("Failed to create SessionBuilder: {e}"))?;

        if let Some(threads) = cpu_threads {
            builder = builder
                .with_intra_threads(threads)
                .map_err(|e| format!("Failed to configure intra threads: {e}"))?;
        }

        match device {
            "cuda" => {
                let cuda = ep::CUDA::default().with_device_id(0);
                if !cuda.is_available().unwrap_or(false) {
                    return Err("CUDA execution provider is not available in loaded runtime".to_string());
                }
                let cuda = cuda.build();
                builder = builder
                    .with_execution_providers([cuda])
                    .map_err(|e| format!("Failed to register CUDAExecutionProvider: {e}"))?;
            }
            "directml" | "dml" => {
                let dml = ep::DirectML::default().with_device_id(0);
                if !dml.is_available().unwrap_or(false) {
                    return Err("DirectML execution provider is not available in loaded runtime".to_string());
                }
                builder = builder
                    .with_memory_pattern(false)
                    .map_err(|e| format!("Failed to disable memory pattern for DirectML: {e}"))?
                    .with_parallel_execution(false)
                    .map_err(|e| format!("Failed to set sequential execution for DirectML: {e}"))?
                    .with_execution_providers([dml.build()])
                    .map_err(|e| format!("Failed to register DirectMLExecutionProvider: {e}"))?;
            }
            _ => {}
        }

        Ok(builder)
    }

    pub fn transcribe(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
        language: Option<&str>,
        prompt: Option<&str>,
    ) -> Result<TranscriptionOutput, String> {
        let start_time = Instant::now();
        let duration_sec = if sample_rate > 0 {
            samples.len() as f64 / sample_rate as f64
        } else {
            0.0
        };

        let mut sum_sq = 0.0f32;
        for &s in samples {
            sum_sq += s * s;
        }
        let rms = (sum_sq / (samples.len().max(1) as f32)).sqrt();
        if rms < 1e-5 {
            return Ok(TranscriptionOutput {
                text: String::new(),
                duration_sec,
                inference_sec: start_time.elapsed().as_secs_f64(),
                no_speech_prob: 1.0,
            });
        }

        let n_mels = self.mel.n_mels;
        let mel_vec = self.mel.extract(samples, sample_rate).to_vec();

        let lang_token = language
            .and_then(|l| self.tokenizer.language_token_id(l))
            .or_else(|| self.tokenizer.language_token_id("es"))
            .unwrap_or(50262);

        let mut prefix_tokens = Vec::with_capacity(256);
        if let Some(user_prompt) = prompt {
            let encoded_prompt = self.tokenizer.encode_prompt(user_prompt, 192);
            if !encoded_prompt.is_empty() {
                prefix_tokens.push(self.tokenizer.prev_token);
                prefix_tokens.extend_from_slice(&encoded_prompt);
            }
        }

        prefix_tokens.push(self.tokenizer.sot_token);
        prefix_tokens.push(lang_token);
        prefix_tokens.push(self.tokenizer.transcribe_token);
        prefix_tokens.push(self.tokenizer.notimestamps_token);

        let mut current_tokens = prefix_tokens.clone();
        let mut generated_tokens = Vec::with_capacity(160);
        let mut sum_logprob = 0.0f64;
        let no_speech_prob: f64;

        if self.decoder_with_past.is_some() && self.num_decoder_layers > 0 {
            let mut enc_outputs = if self.encoder_expects_f16 {
                let mel_boxed = mel_vec
                    .iter()
                    .map(|&v| half::f16::from_f32(v))
                    .collect::<Vec<_>>()
                    .into_boxed_slice();
                let mel_tensor = Tensor::<half::f16>::from_array((
                    [1usize, n_mels, 3000usize],
                    mel_boxed,
                ))
                .map_err(|e| format!("Failed to create mel f16 tensor: {e}"))?;
                self.encoder
                    .run(ort::inputs![
                        &self.encoder_input_name => mel_tensor,
                    ])
                    .map_err(|e| format!("Encoder forward pass failed: {e}"))?
            } else {
                let mel_tensor = Tensor::<f32>::from_array((
                    [1usize, n_mels, 3000usize],
                    mel_vec.clone().into_boxed_slice(),
                ))
                .map_err(|e| format!("Failed to create mel tensor: {e}"))?;
                self.encoder
                    .run(ort::inputs![
                        &self.encoder_input_name => mel_tensor,
                    ])
                    .map_err(|e| format!("Encoder forward pass failed: {e}"))?
            };

            let enc_hidden: DynValue = enc_outputs
                .remove(&self.encoder_output_name)
                .ok_or_else(|| "Missing encoder hidden output tensor".to_string())?;
            drop(enc_outputs);

            let input_ids_tensor = Tensor::from_array((
                [1usize, current_tokens.len()],
                current_tokens.clone().into_boxed_slice(),
            ))
            .map_err(|e| format!("Failed to create initial decoder input_ids tensor: {e}"))?;

            let mut step0_outputs = self
                .decoder
                .run(ort::inputs![
                    &self.decoder_input_ids_name => input_ids_tensor,
                    &self.decoder_enc_hidden_name => enc_hidden,
                ])
                .map_err(|e| format!("Decoder step 0 forward pass failed: {e}"))?;

            let (vocab_size, step0_logits, nsp) = {
                let logits_val = step0_outputs
                    .get("logits")
                    .ok_or_else(|| "Missing step 0 logits".to_string())?;
                extract_step0_logits_and_prob(
                    logits_val,
                    &current_tokens,
                    self.tokenizer.sot_token,
                    self.tokenizer.nospeech_token,
                    self.vocab_size,
                )?
            };
            no_speech_prob = nsp;

            let mut enc_kv: Vec<(DynValue, DynValue)> = Vec::with_capacity(self.num_decoder_layers);
            let mut dec_kv: Vec<(DynValue, DynValue)> = Vec::with_capacity(self.num_decoder_layers);
            for l in 0..self.num_decoder_layers {
                let dk = step0_outputs
                    .remove(format!("present.{l}.decoder.key"))
                    .ok_or_else(|| format!("Missing step 0 present.{l}.decoder.key"))?;
                let dv = step0_outputs
                    .remove(format!("present.{l}.decoder.value"))
                    .ok_or_else(|| format!("Missing step 0 present.{l}.decoder.value"))?;
                let ek = step0_outputs
                    .remove(format!("present.{l}.encoder.key"))
                    .ok_or_else(|| format!("Missing step 0 present.{l}.encoder.key"))?;
                let ev = step0_outputs
                    .remove(format!("present.{l}.encoder.value"))
                    .ok_or_else(|| format!("Missing step 0 present.{l}.encoder.value"))?;
                dec_kv.push((dk, dv));
                enc_kv.push((ek, ev));
            }
            drop(step0_outputs);

            let first_gen_token = select_next_token(
                &step0_logits,
                &self.tokenizer,
                &self.suppress_tokens,
                &self.begin_suppress_tokens,
                0,
            );

            if first_gen_token != self.tokenizer.eot_token {
                sum_logprob += calculate_token_logprob(&step0_logits, first_gen_token as usize);
                current_tokens.push(first_gen_token);
                generated_tokens.push(first_gen_token);

                let cpu_out_mem = cpu_output_memory_info()?;
                let decoder_past = self
                    .decoder_with_past
                    .as_mut()
                    .ok_or_else(|| "decoder_with_past unexpectedly missing".to_string())?;
                let mut past_binding = decoder_past
                    .create_binding()
                    .map_err(|e| format!("Failed to create decoder_with_past IoBinding: {e}"))?;

                // Bind static 110.6 MB cross-attention encoder KV tensors ONCE onto decoder_with_past's GPU
                for l in 0..self.num_decoder_layers {
                    past_binding
                        .bind_input(format!("past_key_values.{l}.encoder.key"), &enc_kv[l].0)
                        .map_err(|e| format!("Failed to bind encoder key {l}: {e}"))?;
                    past_binding
                        .bind_input(format!("past_key_values.{l}.encoder.value"), &enc_kv[l].1)
                        .map_err(|e| format!("Failed to bind encoder val {l}: {e}"))?;
                }
                past_binding
                    .synchronize_inputs()
                    .map_err(|e| format!("Failed to synchronize encoder KV inputs: {e}"))?;

                let max_gen_tokens = 160;
                for step in 1..max_gen_tokens {
                    let last_token = *current_tokens.last().unwrap_or(&self.tokenizer.eot_token);
                    let last_token_tensor = Tensor::from_array((
                        [1usize, 1usize],
                        vec![last_token].into_boxed_slice(),
                    ))
                    .map_err(|e| format!("Failed to create step {step} token tensor: {e}"))?;

                    past_binding.clear_outputs();
                    past_binding
                        .bind_input(&self.decoder_past_input_ids_name, &last_token_tensor)
                        .map_err(|e| format!("Failed to bind step {step} input_ids: {e}"))?;
                    past_binding
                        .bind_output_to_device("logits", &cpu_out_mem)
                        .map_err(|e| format!("Failed to bind step {step} logits: {e}"))?;
                    for l in 0..self.num_decoder_layers {
                        past_binding
                            .bind_input(format!("past_key_values.{l}.decoder.key"), &dec_kv[l].0)
                            .map_err(|e| format!("Failed to bind step {step} dec key {l}: {e}"))?;
                        past_binding
                            .bind_input(format!("past_key_values.{l}.decoder.value"), &dec_kv[l].1)
                            .map_err(|e| format!("Failed to bind step {step} dec val {l}: {e}"))?;
                        past_binding
                            .bind_output_to_device(format!("present.{l}.decoder.key"), &cpu_out_mem)
                            .map_err(|e| format!("Failed to bind step {step} out dk {l}: {e}"))?;
                        past_binding
                            .bind_output_to_device(format!("present.{l}.decoder.value"), &cpu_out_mem)
                            .map_err(|e| format!("Failed to bind step {step} out dv {l}: {e}"))?;
                    }
                    past_binding
                        .synchronize_inputs()
                        .map_err(|e| format!("Failed to sync step {step} inputs: {e}"))?;

                    let mut outputs = decoder_past
                        .run_binding(&past_binding)
                        .map_err(|e| format!("Decoder with past IoBinding step {step} failed: {e}"))?;
                    past_binding
                        .synchronize_outputs()
                        .map_err(|e| format!("Failed to sync step {step} outputs: {e}"))?;

                    let step_logits_vec = {
                        let logits_val = outputs
                            .get("logits")
                            .ok_or_else(|| format!("Missing step {step} logits"))?;
                        extract_step_logits_vec(logits_val, 0, vocab_size)?
                    };

                    let mut new_dec_kv = Vec::with_capacity(self.num_decoder_layers);
                    for l in 0..self.num_decoder_layers {
                        let dk = outputs
                            .remove(format!("present.{l}.decoder.key"))
                            .ok_or_else(|| format!("Missing step {step} present.{l}.decoder.key"))?;
                        let dv = outputs
                            .remove(format!("present.{l}.decoder.value"))
                            .ok_or_else(|| format!("Missing step {step} present.{l}.decoder.value"))?;
                        new_dec_kv.push((dk, dv));
                    }
                    drop(outputs);
                    dec_kv = new_dec_kv;

                    let next_token = select_next_token(
                        &step_logits_vec,
                        &self.tokenizer,
                        &self.suppress_tokens,
                        &self.begin_suppress_tokens,
                        step,
                    );

                    if next_token == self.tokenizer.eot_token {
                        break;
                    }

                    sum_logprob += calculate_token_logprob(&step_logits_vec, next_token as usize);
                    current_tokens.push(next_token);
                    generated_tokens.push(next_token);

                    if has_repeated_4gram(&generated_tokens) {
                        break;
                    }
                }
            }
        } else {
            let encoder_outputs = if self.encoder_expects_f16 {
                let mel_boxed = mel_vec
                    .iter()
                    .map(|&v| half::f16::from_f32(v))
                    .collect::<Vec<_>>()
                    .into_boxed_slice();
                let mel_tensor = Tensor::<half::f16>::from_array((
                    [1usize, n_mels, 3000usize],
                    mel_boxed,
                ))
                .map_err(|e| format!("Failed to create mel f16 tensor: {e}"))?;
                self.encoder
                    .run(ort::inputs![
                        &self.encoder_input_name => mel_tensor,
                    ])
                    .map_err(|e| format!("Encoder forward pass failed: {e}"))?
            } else {
                let mel_tensor = Tensor::<f32>::from_array((
                    [1usize, n_mels, 3000usize],
                    mel_vec.into_boxed_slice(),
                ))
                .map_err(|e| format!("Failed to create mel tensor: {e}"))?;
                self.encoder
                    .run(ort::inputs![
                        &self.encoder_input_name => mel_tensor,
                    ])
                    .map_err(|e| format!("Encoder forward pass failed: {e}"))?
            };

            let (d_model, encoder_hidden_states) = {
                let elem_type = encoder_outputs[0].dtype().tensor_type();
                if matches!(elem_type, Some(TensorElementType::Float16)) {
                    let (enc_shape, enc_data) = encoder_outputs[0]
                        .try_extract_tensor::<half::f16>()
                        .map_err(|e| format!("Failed to extract encoder hidden states: {e}"))?;
                    let dm = if enc_shape.len() == 3 && enc_shape[2] > 0 {
                        enc_shape[2] as usize
                    } else {
                        self.d_model
                    };
                    (dm, enc_data.iter().map(|v| v.to_f32()).collect::<Vec<f32>>())
                } else {
                    let (enc_shape, enc_data) = encoder_outputs[0]
                        .try_extract_tensor::<f32>()
                        .map_err(|e| format!("Failed to extract encoder hidden states: {e}"))?;
                    let dm = if enc_shape.len() == 3 && enc_shape[2] > 0 {
                        enc_shape[2] as usize
                    } else {
                        self.d_model
                    };
                    (dm, enc_data.to_vec())
                }
            };

            let input_ids_tensor = TensorRef::from_array_view((
                [1usize, current_tokens.len()],
                current_tokens.as_slice(),
            ))
            .map_err(|e| format!("Failed to create initial decoder input_ids tensor: {e}"))?;

            let enc_tensor = TensorRef::from_array_view((
                [1usize, 1500usize, d_model],
                encoder_hidden_states.as_slice(),
            ))
            .map_err(|e| format!("Failed to create encoder_hidden_states view: {e}"))?;

            let step0_outputs = self
                .decoder
                .run(ort::inputs![
                    &self.decoder_input_ids_name => input_ids_tensor,
                    &self.decoder_enc_hidden_name => enc_tensor,
                ])
                .map_err(|e| format!("Decoder step 0 forward pass failed: {e}"))?;

            let (vocab_size, step0_logits, nsp) = extract_step0_logits_and_prob(
                &step0_outputs[0],
                &current_tokens,
                self.tokenizer.sot_token,
                self.tokenizer.nospeech_token,
                self.vocab_size,
            )?;
            no_speech_prob = nsp;
            drop(step0_outputs);

            let first_gen_token = select_next_token(
                &step0_logits,
                &self.tokenizer,
                &self.suppress_tokens,
                &self.begin_suppress_tokens,
                0,
            );

            if first_gen_token != self.tokenizer.eot_token {
                sum_logprob += calculate_token_logprob(&step0_logits, first_gen_token as usize);
                current_tokens.push(first_gen_token);
                generated_tokens.push(first_gen_token);

                let max_gen_tokens = 160;
                for step in 1..max_gen_tokens {
                    let input_ids_tensor = TensorRef::from_array_view((
                        [1usize, current_tokens.len()],
                        current_tokens.as_slice(),
                    ))
                    .map_err(|e| format!("Failed to create decoder input_ids tensor: {e}"))?;

                    let enc_tensor = TensorRef::from_array_view((
                        [1usize, 1500usize, d_model],
                        encoder_hidden_states.as_slice(),
                    ))
                    .map_err(|e| format!("Failed to create encoder_hidden_states view: {e}"))?;

                    let outputs = self
                        .decoder
                        .run(ort::inputs![
                            &self.decoder_input_ids_name => input_ids_tensor,
                            &self.decoder_enc_hidden_name => enc_tensor,
                        ])
                        .map_err(|e| format!("Decoder autoregressive step failed: {e}"))?;

                    let step_seq_len = current_tokens.len();
                    let step_offset = (step_seq_len.saturating_sub(1)) * vocab_size;
                    let step_logits_vec = extract_step_logits_vec(&outputs[0], step_offset, vocab_size)?;

                    let next_token = select_next_token(
                        &step_logits_vec,
                        &self.tokenizer,
                        &self.suppress_tokens,
                        &self.begin_suppress_tokens,
                        step,
                    );

                    if next_token == self.tokenizer.eot_token {
                        break;
                    }

                    sum_logprob += calculate_token_logprob(&step_logits_vec, next_token as usize);
                    current_tokens.push(next_token);
                    generated_tokens.push(next_token);

                    if has_repeated_4gram(&generated_tokens) {
                        break;
                    }
                }
            }
        }

        let avg_logprob = if !generated_tokens.is_empty() {
            sum_logprob / (generated_tokens.len() as f64)
        } else {
            f64::NEG_INFINITY
        };

        let is_silence = no_speech_prob > 0.6 && avg_logprob < -1.0;
        let decoded_text = if is_silence {
            String::new()
        } else {
            self.tokenizer.decode(&generated_tokens)
        };

        Ok(TranscriptionOutput {
            text: decoded_text,
            duration_sec,
            inference_sec: start_time.elapsed().as_secs_f64(),
            no_speech_prob,
        })
    }
}

#[derive(Debug, Clone)]
pub struct WhisperModelConfig {
    pub d_model: usize,
    pub vocab_size: usize,
    pub suppress_tokens: Vec<i64>,
    pub begin_suppress_tokens: Vec<i64>,
}

pub fn parse_model_config(config_path: &Path) -> WhisperModelConfig {
    let mut d_model = 384;
    let mut vocab_size = 51865;
    let mut suppress_tokens = Vec::new();
    let mut begin_suppress_tokens = vec![220];

    if let Ok(content) = std::fs::read_to_string(config_path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(dm) = json.get("d_model").and_then(|v| v.as_u64()) {
                d_model = dm as usize;
            }
            if let Some(vs) = json.get("vocab_size").and_then(|v| v.as_u64()) {
                vocab_size = vs as usize;
            }
            if let Some(arr) = json.get("suppress_tokens").and_then(|v| v.as_array()) {
                suppress_tokens = arr.iter().filter_map(|x| x.as_i64()).collect();
            }
            if let Some(arr) = json.get("begin_suppress_tokens").and_then(|v| v.as_array()) {
                for tok in arr.iter().filter_map(|x| x.as_i64()) {
                    if !begin_suppress_tokens.contains(&tok) {
                        begin_suppress_tokens.push(tok);
                    }
                }
            }
        }
    }

    if suppress_tokens.is_empty() {
        if let Some(parent) = config_path.parent() {
            let gen_config = parent.join("generation_config.json");
            if gen_config.is_file() {
                if let Ok(gen_content) = std::fs::read_to_string(&gen_config) {
                    if let Ok(gen_json) = serde_json::from_str::<serde_json::Value>(&gen_content) {
                        if let Some(arr) = gen_json.get("suppress_tokens").and_then(|v| v.as_array()) {
                            suppress_tokens = arr.iter().filter_map(|x| x.as_i64()).collect();
                        }
                        if let Some(arr) = gen_json.get("begin_suppress_tokens").and_then(|v| v.as_array()) {
                            for tok in arr.iter().filter_map(|x| x.as_i64()) {
                                if !begin_suppress_tokens.contains(&tok) {
                                    begin_suppress_tokens.push(tok);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    WhisperModelConfig {
        d_model,
        vocab_size,
        suppress_tokens,
        begin_suppress_tokens,
    }
}

pub fn calculate_token_softmax_prob(logits: &[f32], target_token: usize) -> f64 {
    if target_token >= logits.len() {
        return 0.0;
    }

    let mut max_logit = f32::NEG_INFINITY;
    for &l in logits {
        if l > max_logit {
            max_logit = l;
        }
    }

    let mut sum_exp = 0.0f64;
    for &l in logits {
        sum_exp += ((l - max_logit) as f64).exp();
    }

    if sum_exp <= 0.0 {
        return 0.0;
    }

    let target_exp = ((logits[target_token] - max_logit) as f64).exp();
    target_exp / sum_exp
}

pub fn calculate_token_logprob(logits: &[f32], token: usize) -> f64 {
    if token >= logits.len() {
        return f64::NEG_INFINITY;
    }

    let mut max_logit = f32::NEG_INFINITY;
    for &l in logits {
        if l > max_logit {
            max_logit = l;
        }
    }

    let mut sum_exp = 0.0f64;
    for &l in logits {
        sum_exp += ((l - max_logit) as f64).exp();
    }

    if sum_exp <= 0.0 {
        return f64::NEG_INFINITY;
    }

    ((logits[token] - max_logit) as f64) - sum_exp.ln()
}

pub fn select_next_token(
    logits: &[f32],
    tokenizer: &WhisperTokenizer,
    suppress_tokens: &[i64],
    begin_suppress_tokens: &[i64],
    step: usize,
) -> i64 {
    let mut modified = logits.to_vec();

    let sot_or_eot = tokenizer.sot_or_eot_start as usize;
    if sot_or_eot < modified.len() {
        for idx in sot_or_eot..modified.len() {
            if idx as i64 != tokenizer.eot_token {
                modified[idx] = f32::NEG_INFINITY;
            }
        }
    }

    let special_to_suppress = [
        tokenizer.sot_token,
        tokenizer.prev_token,
        tokenizer.transcribe_token,
        tokenizer.translate_token,
        tokenizer.nospeech_token,
        tokenizer.notimestamps_token,
    ];

    for &sup in &special_to_suppress {
        let idx = sup as usize;
        if idx < modified.len() {
            modified[idx] = f32::NEG_INFINITY;
        }
    }

    for &sup in suppress_tokens {
        let idx = sup as usize;
        if idx < modified.len() {
            modified[idx] = f32::NEG_INFINITY;
        }
    }

    if step == 0 {
        let eot_idx = tokenizer.eot_token as usize;
        if eot_idx < modified.len() {
            modified[eot_idx] = f32::NEG_INFINITY;
        }
        if 220 < modified.len() {
            modified[220] = f32::NEG_INFINITY;
        }
        for &sup in begin_suppress_tokens {
            let idx = sup as usize;
            if idx < modified.len() {
                modified[idx] = f32::NEG_INFINITY;
            }
        }
    }

    let mut best_idx = 0;
    let mut best_val = f32::NEG_INFINITY;
    for (i, &val) in modified.iter().enumerate() {
        if val > best_val {
            best_val = val;
            best_idx = i;
        }
    }

    best_idx as i64
}

fn has_repeated_4gram(tokens: &[i64]) -> bool {
    if tokens.len() < 12 {
        return false;
    }

    let target = &tokens[tokens.len() - 4..];
    let mut count = 0;
    for window in tokens.windows(4) {
        if window == target {
            count += 1;
            if count >= 3 {
                return true;
            }
        }
    }

    false
}

pub fn run_self_test() -> Result<(), String> {
    init_ort_runtime()?;

    let mut extractor = MelExtractor::new(80);
    let synthetic = vec![0.0f32; 16000];
    let mel = extractor.extract(&synthetic, 16000);
    if mel.len() != 80 * 3000 {
        return Err(format!(
            "Mel extraction sanity failed: expected {} samples, got {}",
            80 * 3000,
            mel.len()
        ));
    }

    let (b2c, c2b) = build_byte_to_unicode();
    if b2c.len() != 256 || c2b.len() != 256 {
        return Err("Tokenizer unicode mapping sanity failed".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn softmax_and_logprob_are_numerically_consistent() {
        let logits = vec![1.0f32, 2.0, 5.0, -1.0];
        let prob = calculate_token_softmax_prob(&logits, 2);
        let logprob = calculate_token_logprob(&logits, 2);

        assert!(prob > 0.0 && prob <= 1.0);
        assert!((prob.ln() - logprob).abs() < 1e-6);

        let out_of_bounds = calculate_token_softmax_prob(&logits, 10);
        assert_eq!(out_of_bounds, 0.0);
        let logprob_oob = calculate_token_logprob(&logits, 10);
        assert!(logprob_oob.is_infinite() && logprob_oob.is_sign_negative());
    }

    #[test]
    fn begin_suppress_tokens_prevents_eot_and_220_on_step0() {
        let tokenizer = WhisperTokenizer::from_json_str(
            r#"{"model":{"vocab":{},"merges":[]},"added_tokens":[]}"#,
        )
        .expect("valid minimal tokenizer");

        let mut logits = vec![0.0f32; 51865];
        logits[50257] = 100.0;
        logits[220] = 90.0;
        logits[100] = 50.0;

        let begin_suppress = vec![220, 50257];
        let suppress = vec![];

        let token_step0 = select_next_token(&logits, &tokenizer, &suppress, &begin_suppress, 0);
        assert_eq!(token_step0, 100);

        let token_step1 = select_next_token(&logits, &tokenizer, &suppress, &begin_suppress, 1);
        assert_eq!(token_step1, 50257);
    }

    #[test]
    fn repeated_legitimate_token_is_not_penalized() {
        let tokenizer = WhisperTokenizer::from_json_str(
            r#"{"model":{"vocab":{},"merges":[]},"added_tokens":[]}"#,
        )
        .expect("valid minimal tokenizer");

        let mut logits = vec![0.0f32; 51865];
        logits[100] = 10.0;
        logits[200] = 9.0;

        // faster-whisper default repetition_penalty is 1.0: no history-based logit change.
        let token = select_next_token(&logits, &tokenizer, &[], &[], 5);
        assert_eq!(token, 100);
    }

    #[test]
    fn silence_decision_requires_both_high_no_speech_and_low_logprob() {
        let no_speech_1 = 0.75;
        let avg_logprob_1 = -0.45;
        let is_silence_1 = no_speech_1 > 0.6 && avg_logprob_1 < -1.0;
        assert!(!is_silence_1);

        let no_speech_2 = 0.85;
        let avg_logprob_2 = -1.45;
        let is_silence_2 = no_speech_2 > 0.6 && avg_logprob_2 < -1.0;
        assert!(is_silence_2);

        let no_speech_3 = 0.15;
        let avg_logprob_3 = -0.3;
        let is_silence_3 = no_speech_3 > 0.6 && avg_logprob_3 < -1.0;
        assert!(!is_silence_3);
    }

    #[test]
    fn parse_model_config_defaults_and_custom() {
        let tmp = std::env::temp_dir().join(format!("test_config_{}.json", std::process::id()));
        let json = r#"{
            "d_model": 512,
            "vocab_size": 51865,
            "suppress_tokens": [1, 2, 7],
            "begin_suppress_tokens": [220, 50257]
        }"#;
        std::fs::write(&tmp, json).unwrap();

        let cfg = parse_model_config(&tmp);
        let _ = std::fs::remove_file(&tmp);

        assert_eq!(cfg.d_model, 512);
        assert_eq!(cfg.vocab_size, 51865);
        assert_eq!(cfg.suppress_tokens, vec![1, 2, 7]);
        assert!(cfg.begin_suppress_tokens.contains(&220));
        assert!(cfg.begin_suppress_tokens.contains(&50257));
    }

    #[test]
    fn resolve_effective_device_fallback_and_defaults() {
        let (dev, fallback, reason) = resolve_effective_device("cpu", false).expect("cpu device");
        assert_eq!(dev, "cpu");
        assert!(!fallback);
        assert!(reason.is_none());

        let (dev2, fallback2, reason2) = resolve_effective_device("unsupported_xyz", false).expect("fallback device");
        assert_eq!(dev2, "cpu");
        assert!(!fallback2);
        assert!(reason2.is_none());
    }

    #[test]
    fn half_f16_conversion_and_softmax_consistency() {
        let float_values = vec![1.0f32, 2.0, 5.0, -1.0];
        let f16_values: Vec<half::f16> = float_values.iter().map(|&v| half::f16::from_f32(v)).collect();
        let recovered: Vec<f32> = f16_values.iter().map(|v| v.to_f32()).collect();

        for (orig, rec) in float_values.iter().zip(recovered.iter()) {
            assert!((orig - rec).abs() < 1e-3);
        }

        let prob_orig = calculate_token_softmax_prob(&float_values, 2);
        let prob_rec = calculate_token_softmax_prob(&recovered, 2);
        assert!((prob_orig - prob_rec).abs() < 1e-4);
    }
}
