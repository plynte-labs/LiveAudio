// SPDX-License-Identifier: MIT

use crate::error::AudioError;
use crate::types::{AudioChunk, DEFAULT_CHUNK_SIZE, DEFAULT_SAMPLE_RATE};
use nnnoiseless::DenoiseState;

const INPUT_FRAME_SIZE: usize = DenoiseState::FRAME_SIZE;
const INPUT_SAMPLES_PER_FRAME: usize = INPUT_FRAME_SIZE / 3;
const MAX_OUTPUT_CHUNKS_PER_CALL: usize = 2;
const PCM_SCALE: f32 = 32768.0;

fn to_pcm_scale(sample: f32) -> f32 {
    (sample * PCM_SCALE).clamp(-32_768.0, 32_767.0)
}

/// Stateful RNNoise processor adapting normalized mono 16 kHz chunks to RNNoise frames.
pub struct RnnoiseProcessor {
    denoiser: Box<DenoiseState<'static>>,
    input_frame: [f32; INPUT_FRAME_SIZE],
    input_frame_len: usize,
    pending_output_samples: usize,
    output_staging: [f32; DEFAULT_CHUNK_SIZE],
    output_staging_len: usize,
    previous_input: Option<f32>,
    first_input_timestamp_ns: Option<u64>,
    first_input_sequence_id: Option<u64>,
    output_samples_emitted: u64,
    flushed: bool,
    failed: bool,
}

impl RnnoiseProcessor {
    /// Create and prewarm a processor with one discarded synthetic silence frame.
    pub fn new() -> Result<Self, AudioError> {
        let mut processor = Self {
            denoiser: DenoiseState::new(),
            input_frame: [0.0; INPUT_FRAME_SIZE],
            input_frame_len: 0,
            pending_output_samples: 0,
            output_staging: [0.0; DEFAULT_CHUNK_SIZE],
            output_staging_len: 0,
            previous_input: None,
            first_input_timestamp_ns: None,
            first_input_sequence_id: None,
            output_samples_emitted: 0,
            flushed: false,
            failed: false,
        };
        let mut discarded = [0.0; INPUT_FRAME_SIZE];
        processor
            .denoiser
            .process_frame(&mut discarded, &processor.input_frame);
        Ok(processor)
    }

    /// Process one standard mono 16 kHz input chunk.
    ///
    /// The returned vector contains zero or more complete 512-sample chunks. Call
    /// [`flush`](Self::flush) once at end-of-stream to drain the final valid sample tail.
    pub fn process(&mut self, chunk: AudioChunk) -> Result<Vec<AudioChunk>, AudioError> {
        self.ensure_usable()?;
        if chunk.sample_rate != DEFAULT_SAMPLE_RATE
            || chunk.channels != 1
            || chunk.samples.len() != DEFAULT_CHUNK_SIZE
        {
            return Err(AudioError::InvalidParameter(
                "RNNoise requires mono 16 kHz chunks of exactly 512 samples".to_string(),
            ));
        }
        if chunk.samples.iter().any(|sample| !sample.is_finite()) {
            return Err(AudioError::InvalidParameter(
                "RNNoise input samples must be finite".to_string(),
            ));
        }

        if self.first_input_timestamp_ns.is_none() {
            self.first_input_timestamp_ns = Some(chunk.timestamp_ns);
            self.first_input_sequence_id = Some(chunk.sequence_id);
        }

        let mut output = Vec::with_capacity(MAX_OUTPUT_CHUNKS_PER_CALL);
        for sample in chunk.samples {
            // Float devices and loopback can peak above full scale; clip like PCM would.
            let sample = sample.clamp(-1.0, 1.0);
            let previous = self.previous_input.unwrap_or(sample);
            for step in 1..=3 {
                let fraction = step as f32 / 3.0;
                let interpolated = previous + (sample - previous) * fraction;
                self.input_frame[self.input_frame_len] = to_pcm_scale(interpolated);
                self.input_frame_len += 1;
                if self.input_frame_len == INPUT_FRAME_SIZE {
                    if let Err(error) =
                        self.process_input_frame(INPUT_SAMPLES_PER_FRAME, &mut output)
                    {
                        self.failed = true;
                        return Err(error);
                    }
                    self.input_frame_len = 0;
                }
            }
            self.previous_input = Some(sample);
        }
        Ok(output)
    }

    /// Drain the final partial RNNoise frame and return its valid tail in padded chunks.
    pub fn flush(&mut self) -> Result<Vec<AudioChunk>, AudioError> {
        self.ensure_usable()?;
        self.flushed = true;
        let mut output_chunks = Vec::with_capacity(MAX_OUTPUT_CHUNKS_PER_CALL);

        if self.input_frame_len > 0 {
            let valid_samples = self.input_frame_len / 3;
            self.input_frame[self.input_frame_len..].fill(0.0);
            self.input_frame_len = INPUT_FRAME_SIZE;
            if let Err(error) = self.process_input_frame(valid_samples, &mut output_chunks) {
                self.failed = true;
                return Err(error);
            }
            self.input_frame_len = 0;
        }

        if self.pending_output_samples > 0 {
            self.input_frame.fill(0.0);
            if let Err(error) = self.process_input_frame(0, &mut output_chunks) {
                self.failed = true;
                return Err(error);
            }
        }

        if self.output_staging_len > 0 {
            self.output_staging[self.output_staging_len..].fill(0.0);
            self.output_staging_len = DEFAULT_CHUNK_SIZE;
            output_chunks.push(self.take_output_chunk()?);
        }
        Ok(output_chunks)
    }

    fn ensure_usable(&self) -> Result<(), AudioError> {
        if self.failed {
            return Err(AudioError::BackendError(
                "RNNoise processor is unavailable after a frame-processing failure".to_string(),
            ));
        }
        if self.flushed {
            return Err(AudioError::InvalidParameter(
                "RNNoise processor cannot accept audio after flush".to_string(),
            ));
        }
        Ok(())
    }

    fn process_input_frame(
        &mut self,
        valid_samples: usize,
        output_chunks: &mut Vec<AudioChunk>,
    ) -> Result<(), AudioError> {
        let mut output_frame = [0.0; INPUT_FRAME_SIZE];
        self.denoiser
            .process_frame(&mut output_frame, &self.input_frame);

        for sample in (2..INPUT_FRAME_SIZE)
            .step_by(3)
            .take(self.pending_output_samples)
        {
            let normalized = output_frame[sample] / PCM_SCALE;
            if !normalized.is_finite() {
                return Err(AudioError::BackendError(
                    "RNNoise produced a non-finite sample".to_string(),
                ));
            }
            let normalized = normalized.clamp(-1.0, 1.0);
            self.output_staging[self.output_staging_len] = normalized;
            self.output_staging_len += 1;
            self.output_samples_emitted += 1;
            if self.output_staging_len == DEFAULT_CHUNK_SIZE {
                output_chunks.push(self.take_output_chunk()?);
            }
        }
        self.pending_output_samples = valid_samples;
        Ok(())
    }

    fn take_output_chunk(&mut self) -> Result<AudioChunk, AudioError> {
        let timestamp = self.first_input_timestamp_ns.ok_or_else(|| {
            AudioError::BackendError("RNNoise output has no source timestamp".to_string())
        })?;
        let sequence = self.first_input_sequence_id.ok_or_else(|| {
            AudioError::BackendError("RNNoise output has no source sequence".to_string())
        })?;
        let start_sample = self.output_samples_emitted - DEFAULT_CHUNK_SIZE as u64;
        let offset_ns = start_sample
            .checked_mul(1_000_000_000)
            .and_then(|samples| samples.checked_div(DEFAULT_SAMPLE_RATE as u64))
            .ok_or_else(|| AudioError::BackendError("RNNoise timestamp overflow".to_string()))?;
        let sequence_offset = start_sample / DEFAULT_CHUNK_SIZE as u64;
        let timestamp_ns = timestamp
            .checked_add(offset_ns)
            .ok_or_else(|| AudioError::BackendError("RNNoise timestamp overflow".to_string()))?;
        let sequence_id = sequence
            .checked_add(sequence_offset)
            .ok_or_else(|| AudioError::BackendError("RNNoise sequence overflow".to_string()))?;
        let chunk = AudioChunk {
            samples: self.output_staging.to_vec(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            channels: 1,
            timestamp_ns,
            sequence_id,
        };
        self.output_staging_len = 0;
        Ok(chunk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_adapter_uses_signed_pcm_scaling_and_480_samples() {
        assert_eq!(DenoiseState::FRAME_SIZE, 480);
        assert_eq!(INPUT_SAMPLES_PER_FRAME, 160);
        assert_eq!(0.5 * PCM_SCALE, 16_384.0);
        assert_eq!(-1.0 * PCM_SCALE, -32_768.0);
    }

    #[test]
    fn normalized_positive_full_scale_stays_within_signed_i16_range() {
        assert_eq!(to_pcm_scale(1.0), 32_767.0);
        assert_eq!(to_pcm_scale(-1.0), -32_768.0);
        assert_eq!(to_pcm_scale(0.5), 16_384.0);
    }

    #[test]
    fn retained_staging_never_exceeds_the_derived_remainders() {
        let mut processor = RnnoiseProcessor::new().expect("built-in RNNoise model initializes");
        for index in 0..25 {
            let chunk = AudioChunk {
                samples: vec![0.0; DEFAULT_CHUNK_SIZE],
                sample_rate: DEFAULT_SAMPLE_RATE,
                channels: 1,
                timestamp_ns: index * 32_000_000,
                sequence_id: index,
            };
            processor.process(chunk).expect("silence processes");
            assert!(processor.input_frame_len <= 479);
            assert!(processor.output_staging_len <= DEFAULT_CHUNK_SIZE - 1);
        }
    }
}
