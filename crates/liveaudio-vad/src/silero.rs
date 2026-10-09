// SPDX-License-Identifier: MIT

use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;

use crate::error::VadError;
use crate::pre_buffer::PreBuffer;
use crate::traits::VadEngine;
use crate::types::{SpeechEndReason, VadConfig, VadFrameDecision, VadTransition};

/// Bundled Silero VAD v6 ONNX model weights embedded for zero-setup execution.
pub const BUNDLED_SILERO_VAD_V6: &[u8] = include_bytes!("../models/silero_vad_v6.onnx");

const CONTEXT_SIZE: usize = 64;
const LSTM_STATE_SIZE: usize = 128;

/// Concrete Voice Activity Detection engine executing Silero VAD via ONNX Runtime.
///
/// Maintains recurrent hidden states across frames and applies hysteresis phrase segmentation
/// with pre-roll buffering (ADR-007), silence timeout, and phrase duration limits.
pub struct SileroVad {
    session: Session,
    config: VadConfig,
    h: Vec<f32>,
    c: Vec<f32>,
    context: Vec<f32>,
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
        let session = Session::builder()
            .map_err(|e| VadError::InferenceError(format!("Session builder failed: {}", e)))?
            .commit_from_memory(model_bytes)
            .map_err(|e| VadError::ModelNotLoaded(format!("Failed to load ONNX model: {}", e)))?;

        let silence_limit_chunks = config.silence_limit_chunks();
        let max_chunks_limit = config.max_chunks_limit();
        let pre_buffer_capacity = config.pre_buffer_chunks();

        Ok(Self {
            session,
            config,
            h: vec![0.0f32; LSTM_STATE_SIZE],
            c: vec![0.0f32; LSTM_STATE_SIZE],
            context: vec![0.0f32; CONTEXT_SIZE],
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
        let mut input_vec = Vec::with_capacity(CONTEXT_SIZE + chunk.len());
        input_vec.extend_from_slice(&self.context);
        input_vec.extend_from_slice(chunk);

        let input_tensor = Tensor::from_array(([1, input_vec.len()], input_vec.into_boxed_slice()))
            .map_err(|e| VadError::InferenceError(format!("Input tensor creation error: {}", e)))?;

        let h_tensor =
            Tensor::from_array(([1, 1, LSTM_STATE_SIZE], self.h.clone().into_boxed_slice()))
                .map_err(|e| VadError::InferenceError(format!("h tensor creation error: {}", e)))?;

        let c_tensor =
            Tensor::from_array(([1, 1, LSTM_STATE_SIZE], self.c.clone().into_boxed_slice()))
                .map_err(|e| VadError::InferenceError(format!("c tensor creation error: {}", e)))?;

        let outputs = self
            .session
            .run(ort::inputs![
                "input" => input_tensor,
                "h" => h_tensor,
                "c" => c_tensor,
            ])
            .map_err(|e| VadError::InferenceError(format!("ONNX inference run failed: {}", e)))?;

        let prob_tensor = outputs["speech_probs"]
            .try_extract_tensor::<f32>()
            .map_err(|e| {
                VadError::InferenceError(format!("Failed to extract speech_probs: {}", e))
            })?;
        let speech_prob = prob_tensor.1[0];

        // Update recurrent states
        let hn_tensor = outputs["hn"]
            .try_extract_tensor::<f32>()
            .map_err(|e| VadError::InferenceError(format!("Failed to extract hn: {}", e)))?;
        let cn_tensor = outputs["cn"]
            .try_extract_tensor::<f32>()
            .map_err(|e| VadError::InferenceError(format!("Failed to extract cn: {}", e)))?;

        self.h.copy_from_slice(hn_tensor.1);
        self.c.copy_from_slice(cn_tensor.1);

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
        self.h.fill(0.0);
        self.c.fill(0.0);
        self.context.fill(0.0);
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
        self.config = config;
        Ok(())
    }
}
