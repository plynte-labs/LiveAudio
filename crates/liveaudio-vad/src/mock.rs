// SPDX-License-Identifier: MIT

use crate::error::VadError;
use crate::pre_buffer::PreBuffer;
use crate::traits::VadEngine;
use crate::types::{SpeechEndReason, VadConfig, VadFrameDecision, VadTransition};

/// Scriptable mock VAD engine for fast unit tests and pipeline integration testing.
pub struct MockVadEngine {
    config: VadConfig,
    is_speaking: bool,
    silence_counter: usize,
    speech_chunk_count: usize,
    pre_buffer: PreBuffer,
    silence_limit_chunks: usize,
    max_chunks_limit: usize,
    canned_probabilities: Vec<f32>,
    evaluation_index: usize,
}

impl MockVadEngine {
    pub fn new(config: VadConfig) -> Self {
        let silence_limit_chunks = config.silence_limit_chunks();
        let max_chunks_limit = config.max_chunks_limit();
        let pre_buffer_capacity = config.pre_buffer_chunks();

        Self {
            config,
            is_speaking: false,
            silence_counter: 0,
            speech_chunk_count: 0,
            pre_buffer: PreBuffer::new(pre_buffer_capacity),
            silence_limit_chunks,
            max_chunks_limit,
            canned_probabilities: Vec::new(),
            evaluation_index: 0,
        }
    }

    /// Provide a sequence of canned speech probabilities to emit on successive chunks.
    pub fn with_canned_probabilities(mut self, probs: Vec<f32>) -> Self {
        self.canned_probabilities = probs;
        self
    }
}

impl VadEngine for MockVadEngine {
    fn evaluate_chunk(&mut self, chunk: &[f32]) -> Result<VadFrameDecision, VadError> {
        if chunk.len() != self.config.chunk_size {
            return Err(VadError::InvalidChunkSize {
                expected: self.config.chunk_size,
                actual: chunk.len(),
            });
        }

        // Get probability: from canned list or compute simple RMS-based threshold
        let speech_prob = if self.evaluation_index < self.canned_probabilities.len() {
            let p = self.canned_probabilities[self.evaluation_index];
            self.evaluation_index += 1;
            p
        } else {
            // Simple energy calculation: RMS > 0.05 => prob 0.95, else 0.05
            let rms: f32 = (chunk.iter().map(|&s| s * s).sum::<f32>() / chunk.len() as f32).sqrt();
            if rms > 0.05 {
                0.95
            } else {
                0.05
            }
        };

        let mut transition = VadTransition::None;

        if speech_prob >= self.config.threshold {
            if !self.is_speaking {
                self.is_speaking = true;
                self.silence_counter = 0;
                self.speech_chunk_count = self.pre_buffer.len() + 1;
                self.pre_buffer.clear();
                transition = VadTransition::SpeechStart;
            } else {
                self.silence_counter = 0;
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
                }
            }
        } else if self.is_speaking {
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
            self.pre_buffer.push(chunk.to_vec());
        }

        Ok(VadFrameDecision::new(
            speech_prob,
            self.is_speaking,
            transition,
        ))
    }

    fn reset(&mut self) {
        self.is_speaking = false;
        self.silence_counter = 0;
        self.speech_chunk_count = 0;
        self.pre_buffer.clear();
        self.evaluation_index = 0;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_vad_hysteresis_and_silence_timeout() {
        let config = VadConfig {
            silence_timeout_ms: 64, // 2 chunks of 32ms
            speech_pad_ms: 64,      // 2 chunks pre-buffer
            ..Default::default()
        };
        let mut vad = MockVadEngine::new(config);

        // Feed 2 silence chunks: stored in pre-buffer
        let c_silence = vec![0.0f32; 512];
        let d1 = vad.evaluate_chunk(&c_silence).unwrap();
        assert!(!d1.is_speech);
        assert_eq!(d1.transition, VadTransition::None);

        let d2 = vad.evaluate_chunk(&c_silence).unwrap();
        assert!(!d2.is_speech);

        // Feed speech chunk: should trigger SpeechStart
        let c_speech = vec![0.5f32; 512];
        let d3 = vad.evaluate_chunk(&c_speech).unwrap();
        assert!(d3.is_speech);
        assert_eq!(d3.transition, VadTransition::SpeechStart);

        // Feed 1 silence chunk: utterance continues
        let d4 = vad.evaluate_chunk(&c_silence).unwrap();
        assert!(d4.is_speech);
        assert_eq!(d4.transition, VadTransition::None);

        // Feed second silence chunk: utterance continues
        let d5 = vad.evaluate_chunk(&c_silence).unwrap();
        assert!(d5.is_speech);
        assert_eq!(d5.transition, VadTransition::None);

        // Feed third silence chunk (> silence_limit_chunks = 2): triggers SpeechEnd(SilenceTimeout)
        let d6 = vad.evaluate_chunk(&c_silence).unwrap();
        assert!(!d6.is_speech);
        match d6.transition {
            VadTransition::SpeechEnd { reason, .. } => {
                assert_eq!(reason, SpeechEndReason::SilenceTimeout);
            }
            other => panic!("Expected SpeechEnd, got {:?}", other),
        }
    }
}
