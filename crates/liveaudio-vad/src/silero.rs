// SPDX-License-Identifier: MIT

use ort::session::Session;
use ort::value::TensorRef;
use std::path::Path;

use crate::error::VadError;
use crate::pre_buffer::PreBuffer;
use crate::runtime::ensure_onnx_runtime_initialized;
use crate::traits::VadEngine;
use crate::types::{SpeechEndReason, VadConfig, VadFrameDecision, VadTransition};

/// Bundled Silero VAD v6 ONNX model weights embedded for zero-setup execution.
pub const BUNDLED_SILERO_VAD_V6: &[u8] = include_bytes!("../models/silero_vad_v6.onnx");

const CONTEXT_SIZE: usize = 64;
const STATE_SIZE: usize = 2 * 128;
const SAMPLE_RATE: i64 = 16_000;

/// Concrete Voice Activity Detection engine executing Silero VAD via ONNX Runtime.
///
/// Maintains recurrent hidden states across frames and applies hysteresis phrase segmentation
/// with pre-roll buffering (ADR-007), silence timeout, and phrase duration limits.
pub struct SileroVad {
    session: Session,
    config: VadConfig,
    state: Vec<f32>,
    context: Vec<f32>,
    input_buf: Vec<f32>,
    sr_buf: [i64; 1],
    is_speaking: bool,
    silence_counter: usize,
    speech_chunk_count: usize,
    pre_buffer: PreBuffer,
    silence_limit_chunks: usize,
    max_chunks_limit: usize,
}

impl SileroVad {
    /// Initialize Silero VAD using the embedded default model weights.
    pub fn new(config: VadConfig) -> Result<Self, VadError> {
        Self::from_bytes(BUNDLED_SILERO_VAD_V6, config)
    }

    /// Initialize Silero VAD from in-memory ONNX model bytes.
    pub fn from_bytes(model_bytes: &[u8], config: VadConfig) -> Result<Self, VadError> {
        ensure_onnx_runtime_initialized()?;
        let session = Session::builder()
            .map_err(|e| VadError::InferenceError(format!("Session builder failed: {}", e)))?
            .commit_from_memory(model_bytes)
            .map_err(|e| VadError::ModelNotLoaded(format!("Failed to load ONNX model: {}", e)))?;

        let silence_limit_chunks = config.silence_limit_chunks();
        let max_chunks_limit = config.max_chunks_limit();
        let pre_buffer_capacity = config.pre_buffer_chunks();
        let input_buf = vec![0.0f32; CONTEXT_SIZE + config.chunk_size];

        Ok(Self {
            session,
            config,
            state: vec![0.0f32; STATE_SIZE],
            context: vec![0.0f32; CONTEXT_SIZE],
            input_buf,
            sr_buf: [SAMPLE_RATE],
            is_speaking: false,
            silence_counter: 0,
            speech_chunk_count: 0,
            pre_buffer: PreBuffer::new(pre_buffer_capacity),
            silence_limit_chunks,
            max_chunks_limit,
        })
    }

    /// Initialize Silero VAD from a filesystem path.
    pub fn from_model_path<P: AsRef<Path>>(path: P, config: VadConfig) -> Result<Self, VadError> {
        let model_bytes = std::fs::read(&path).map_err(|e| {
            VadError::ModelNotLoaded(format!("Failed to read {}: {}", path.as_ref().display(), e))
        })?;
        Self::from_bytes(&model_bytes, config)
    }

    /// Raw forward pass through ONNX model evaluating single frame probability.
    fn forward_onnx(&mut self, chunk: &[f32]) -> Result<f32, VadError> {
        let required_len = CONTEXT_SIZE + chunk.len();
        if self.input_buf.len() != required_len {
            self.input_buf.resize(required_len, 0.0f32);
        }
        self.input_buf[..CONTEXT_SIZE].copy_from_slice(&self.context);
        self.input_buf[CONTEXT_SIZE..required_len].copy_from_slice(chunk);

        let input_tensor =
            TensorRef::from_array_view(([1, self.input_buf.len()], &self.input_buf[..]))
                .map_err(|e| {
                    VadError::InferenceError(format!("Input tensor creation error: {}", e))
                })?;

        let state_tensor = TensorRef::from_array_view(([2, 1, 128], &self.state[..]))
            .map_err(|e| VadError::InferenceError(format!("state tensor creation error: {}", e)))?;
        let sample_rate_tensor = TensorRef::from_array_view(([1], &self.sr_buf[..]))
            .map_err(|e| {
                VadError::InferenceError(format!("sample rate tensor creation error: {}", e))
            })?;

        let outputs = self
            .session
            .run(ort::inputs![
                "input" => input_tensor,
                "state" => state_tensor,
                "sr" => sample_rate_tensor,
            ])
            .map_err(|e| VadError::InferenceError(format!("ONNX inference run failed: {}", e)))?;

        let prob_tensor = outputs[0].try_extract_tensor::<f32>().map_err(|e| {
            VadError::InferenceError(format!("Failed to extract speech probability: {}", e))
        })?;
        let speech_prob = prob_tensor
            .1
            .first()
            .copied()
            .ok_or_else(|| VadError::InferenceError("Empty speech probability tensor".to_string()))?;

        // Silero's pinned 16 kHz op15 wrapper returns probability first and the
        // combined recurrent state second (state shape [2, 1, 128]).
        let state_output = outputs[1].try_extract_tensor::<f32>().map_err(|e| {
            VadError::InferenceError(format!("Failed to extract recurrent state: {}", e))
        })?;

        if state_output.1.len() != self.state.len() {
            return Err(VadError::InferenceError(format!(
                "Unexpected recurrent state size: expected {}, got {}",
                self.state.len(),
                state_output.1.len()
            )));
        }

        self.state.copy_from_slice(state_output.1);

        // Update context to trailing CONTEXT_SIZE samples of current chunk
        if chunk.len() >= CONTEXT_SIZE {
            self.context
                .copy_from_slice(&chunk[chunk.len() - CONTEXT_SIZE..]);
        }

        Ok(speech_prob)
    }
}

impl VadEngine for SileroVad {
    fn evaluate_chunk(&mut self, chunk: &[f32]) -> Result<VadFrameDecision, VadError> {
        if chunk.len() != self.config.chunk_size {
            return Err(VadError::InvalidChunkSize {
                expected: self.config.chunk_size,
                actual: chunk.len(),
            });
        }

        let speech_prob = self.forward_onnx(chunk)?;
        let mut transition = VadTransition::None;

        if speech_prob >= self.config.threshold {
            // Speech detected in current chunk
            if !self.is_speaking {
                // Voice onset transition: claim pre-buffer lead-in chunks
                self.is_speaking = true;
                self.silence_counter = 0;
                self.speech_chunk_count = self.pre_buffer.len() + 1;
                self.pre_buffer.clear();
                transition = VadTransition::SpeechStart;
            } else {
                self.silence_counter = 0;
                self.speech_chunk_count += 1;

                // Duration cap: enforce max phrase ceiling
                if self.speech_chunk_count >= self.max_chunks_limit {
                    self.is_speaking = false;
                    let duration_ms =
                        (self.speech_chunk_count as u64 * self.config.chunk_size as u64 * 1000)
                            / self.config.sample_rate as u64;
                    transition = VadTransition::SpeechEnd {
                        duration_ms,
                        reason: SpeechEndReason::MaxDurationReached,
                    };
                    self.speech_chunk_count = 0;
                    self.silence_counter = 0;
                }
            }
        } else if self.is_speaking {
            // Silence chunk while utterance is active
            self.silence_counter += 1;
            self.speech_chunk_count += 1;

            if self.speech_chunk_count >= self.max_chunks_limit {
                self.is_speaking = false;
                let duration_ms =
                    (self.speech_chunk_count as u64 * self.config.chunk_size as u64 * 1000)
                        / self.config.sample_rate as u64;
                transition = VadTransition::SpeechEnd {
                    duration_ms,
                    reason: SpeechEndReason::MaxDurationReached,
                };
                self.speech_chunk_count = 0;
                self.silence_counter = 0;
            } else if self.silence_counter > self.silence_limit_chunks {
                // Silence timeout triggered: close phrase
                self.is_speaking = false;
                let duration_ms =
                    (self.speech_chunk_count as u64 * self.config.chunk_size as u64 * 1000)
                        / self.config.sample_rate as u64;
                transition = VadTransition::SpeechEnd {
                    duration_ms,
                    reason: SpeechEndReason::SilenceTimeout,
                };
                self.speech_chunk_count = 0;
                self.silence_counter = 0;
            }
        } else {
            // Continuous silence while idle: save chunk into pre-buffer ring
            self.pre_buffer.push(chunk.to_vec());
        }

        Ok(VadFrameDecision::new(
            speech_prob,
            self.is_speaking,
            transition,
        ))
    }

    fn reset(&mut self) {
        self.state.fill(0.0);
        self.context.fill(0.0);
        self.input_buf.fill(0.0);
        self.is_speaking = false;
        self.silence_counter = 0;
        self.speech_chunk_count = 0;
        self.pre_buffer.clear();
    }

    fn config(&self) -> &VadConfig {
        &self.config
    }

    fn update_config(&mut self, config: VadConfig) -> Result<(), VadError> {
        self.silence_limit_chunks = config.silence_limit_chunks();
        self.max_chunks_limit = config.max_chunks_limit();
        let pre_buffer_capacity = config.pre_buffer_chunks();
        self.pre_buffer = PreBuffer::new(pre_buffer_capacity);
        if self.input_buf.len() != CONTEXT_SIZE + config.chunk_size {
            self.input_buf.resize(CONTEXT_SIZE + config.chunk_size, 0.0f32);
        }
        self.config = config;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{cpu_runtime_library_path, initialize_onnx_runtime};

    fn try_init_runtime() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging");
        if let Ok(library) = cpu_runtime_library_path(root) {
            let _ = initialize_onnx_runtime(library);
        }
    }

    #[test]
    fn test_silero_vad_allocation_and_buffer_dimensions() {
        try_init_runtime();
        let config = VadConfig {
            chunk_size: 512,
            ..Default::default()
        };
        if let Ok(mut vad) = SileroVad::new(config.clone()) {
            assert_eq!(vad.input_buf.len(), CONTEXT_SIZE + 512);
            assert_eq!(vad.sr_buf, [SAMPLE_RATE]);

            let mut updated_config = config;
            updated_config.chunk_size = 256;
            vad.update_config(updated_config).expect("update config");
            assert_eq!(vad.input_buf.len(), CONTEXT_SIZE + 256);

            vad.reset();
            assert!(vad.input_buf.iter().all(|&v| v == 0.0));
        }
    }

    #[test]
    fn test_silero_vad_evaluate_chunk_size_validation() {
        try_init_runtime();
        let config = VadConfig {
            chunk_size: 512,
            ..Default::default()
        };
        if let Ok(mut vad) = SileroVad::new(config) {
            let invalid_chunk = vec![0.0f32; 100];
            let result = vad.evaluate_chunk(&invalid_chunk);
            assert!(matches!(
                result,
                Err(VadError::InvalidChunkSize {
                    expected: 512,
                    actual: 100
                })
            ));
        }
    }

    #[test]
    fn test_silero_vad_inference_and_prebuffer_bounded() {
        try_init_runtime();
        let config = VadConfig {
            chunk_size: 512,
            speech_pad_ms: 64,
            threshold: 0.5,
            ..Default::default()
        };
        if let Ok(mut vad) = SileroVad::new(config.clone()) {
            let silence = vec![0.0f32; 512];
            for _ in 0..20 {
                let decision = vad.evaluate_chunk(&silence).expect("evaluate chunk");
                assert!(!decision.is_speech);
            }
            assert!(vad.pre_buffer.len() <= vad.config.pre_buffer_chunks());
        }
    }
}
