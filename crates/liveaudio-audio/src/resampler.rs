// SPDX-License-Identifier: MIT

use crate::types::{AudioChunk, DEFAULT_CHUNK_SIZE, DEFAULT_SAMPLE_RATE};

/// Downmix interleaved multichannel float32 audio samples to a single mono channel.
///
/// Multi-channel frames are averaged across all channels:
/// mono[i] = (1 / C) * sum_{c=0}^{C-1} interleaved[i * C + c]
pub fn mix_multichannel_to_mono(interleaved: &[f32], channels: u16, out: &mut Vec<f32>) {
    let channels = channels.max(1) as usize;
    if channels == 1 {
        out.extend_from_slice(interleaved);
        return;
    }

    let frame_count = interleaved.len() / channels;
    out.reserve(frame_count);

    let inv_c = 1.0 / channels as f32;
    for frame in interleaved.chunks_exact(channels) {
        let sum: f32 = frame.iter().sum();
        out.push(sum * inv_c);
    }
}

/// Convert interleaved i16 PCM samples to normalized mono f32 in range [-1.0, 1.0].
pub fn convert_i16_to_mono_f32(interleaved: &[i16], channels: u16, out: &mut Vec<f32>) {
    let channels = channels.max(1) as usize;
    let inv_scale = 1.0 / 32768.0;

    if channels == 1 {
        out.reserve(interleaved.len());
        for &sample in interleaved {
            out.push(sample as f32 * inv_scale);
        }
    } else {
        let frame_count = interleaved.len() / channels;
        out.reserve(frame_count);
        let inv_c = inv_scale / channels as f32;
        for frame in interleaved.chunks_exact(channels) {
            let sum: f32 = frame.iter().map(|&s| s as f32).sum();
            out.push(sum * inv_c);
        }
    }
}

/// Convert interleaved u16 PCM samples to normalized mono f32 in range [-1.0, 1.0].
pub fn convert_u16_to_mono_f32(interleaved: &[u16], channels: u16, out: &mut Vec<f32>) {
    let channels = channels.max(1) as usize;
    let inv_scale = 1.0 / 32768.0;

    if channels == 1 {
        out.reserve(interleaved.len());
        for &sample in interleaved {
            out.push((sample as f32 - 32768.0) * inv_scale);
        }
    } else {
        let frame_count = interleaved.len() / channels;
        out.reserve(frame_count);
        let inv_c = inv_scale / channels as f32;
        for frame in interleaved.chunks_exact(channels) {
            let sum: f32 = frame.iter().map(|&s| s as f32 - 32768.0).sum();
            out.push(sum * inv_c);
        }
    }
}

/// Streaming resampler and chunk framer converting arbitrary sample rates to 16,000 Hz mono.
///
/// Maintains fractional phase and boundary state across calls to ensure click-free,
/// continuous streaming without phase distortion.
pub struct AudioResampler {
    input_sample_rate: u32,
    target_sample_rate: u32,
    chunk_size: usize,
    /// Last sample from previous batch to enable continuous linear interpolation across boundaries.
    last_sample: Option<f32>,
    /// Fractional phase accumulator [0.0, 1.0).
    phase: f64,
    /// Buffer accumulating 16kHz mono samples until a full chunk is formed.
    accumulated_samples: Vec<f32>,
    /// Monotonically increasing sequence ID assigned to output chunks.
    sequence_id: u64,
}

impl AudioResampler {
    /// Create a new resampler for the given hardware input sample rate.
    pub fn new(input_sample_rate: u32, target_sample_rate: u32, chunk_size: usize) -> Self {
        Self {
            input_sample_rate,
            target_sample_rate,
            chunk_size: chunk_size.max(1),
            last_sample: None,
            phase: 0.0,
            accumulated_samples: Vec::with_capacity(chunk_size * 2),
            sequence_id: 0,
        }
    }

    /// Default resampler configured for standard 16kHz target and 512 chunk size.
    pub fn for_input_rate(input_sample_rate: u32) -> Self {
        Self::new(input_sample_rate, DEFAULT_SAMPLE_RATE, DEFAULT_CHUNK_SIZE)
    }

    /// Process incoming mono f32 samples from hardware and return any complete standard chunks.
    pub fn process_mono_samples(&mut self, input: &[f32]) -> Vec<AudioChunk> {
        if input.is_empty() {
            return Vec::new();
        }

        if self.input_sample_rate == self.target_sample_rate {
            // Direct pass-through
            self.accumulated_samples.extend_from_slice(input);
        } else {
            // Resample to target rate
            self.resample_linear(input);
        }

        self.drain_ready_chunks()
    }

    /// Linear interpolation resampler with fractional phase tracking.
    fn resample_linear(&mut self, input: &[f32]) {
        let ratio = self.input_sample_rate as f64 / self.target_sample_rate as f64;
        let mut idx = 0usize;

        while idx < input.len() {
            let s0 = if idx == 0 {
                self.last_sample.unwrap_or(input[0])
            } else {
                input[idx - 1]
            };
            let s1 = input[idx];

            while self.phase < 1.0 && idx < input.len() {
                let interpolated = s0 + (s1 - s0) * (self.phase as f32);
                self.accumulated_samples.push(interpolated);
                self.phase += ratio;
            }

            while self.phase >= 1.0 {
                self.phase -= 1.0;
                idx += 1;
            }
        }

        self.last_sample = input.last().copied();
    }

    /// Slice accumulated 16kHz samples into fixed-size `AudioChunk` blocks.
    fn drain_ready_chunks(&mut self) -> Vec<AudioChunk> {
        let mut chunks = Vec::new();

        while self.accumulated_samples.len() >= self.chunk_size {
            let chunk_data: Vec<f32> = self.accumulated_samples.drain(..self.chunk_size).collect();
            self.sequence_id += 1;
            chunks.push(AudioChunk::new(
                chunk_data,
                self.target_sample_rate,
                1,
                self.sequence_id,
            ));
        }

        chunks
    }

    /// Flush remaining accumulated samples into a final zero-padded chunk if any remain.
    pub fn flush(&mut self) -> Option<AudioChunk> {
        if self.accumulated_samples.is_empty() {
            None
        } else {
            let mut chunk_data = std::mem::take(&mut self.accumulated_samples);
            chunk_data.resize(self.chunk_size, 0.0);
            self.sequence_id += 1;
            Some(AudioChunk::new(
                chunk_data,
                self.target_sample_rate,
                1,
                self.sequence_id,
            ))
        }
    }

    /// Reset internal buffers and phase counters.
    pub fn reset(&mut self) {
        self.last_sample = None;
        self.phase = 0.0;
        self.accumulated_samples.clear();
        self.sequence_id = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multichannel_to_mono_mixing() {
        // Stereo: L=1.0, R=0.0 -> mono=0.5; L=-0.5, R=0.5 -> mono=0.0
        let stereo = vec![1.0f32, 0.0, -0.5, 0.5];
        let mut mono = Vec::new();
        mix_multichannel_to_mono(&stereo, 2, &mut mono);
        assert_eq!(mono.len(), 2);
        assert!((mono[0] - 0.5).abs() < 1e-6);
        assert!((mono[1] - 0.0).abs() < 1e-6);

        // Mono pass-through
        let single = vec![0.7f32, -0.3];
        let mut mono_out = Vec::new();
        mix_multichannel_to_mono(&single, 1, &mut mono_out);
        assert_eq!(mono_out, single);
    }

    #[test]
    fn test_i16_and_u16_conversion() {
        let i16_samples = vec![32767i16, -32768, 0];
        let mut out = Vec::new();
        convert_i16_to_mono_f32(&i16_samples, 1, &mut out);
        assert!((out[0] - 1.0).abs() < 1e-4);
        assert!((out[1] - -1.0).abs() < 1e-4);
        assert!((out[2] - 0.0).abs() < 1e-4);

        let u16_samples = vec![65535u16, 0, 32768];
        let mut u_out = Vec::new();
        convert_u16_to_mono_f32(&u16_samples, 1, &mut u_out);
        assert!((u_out[0] - 1.0).abs() < 1e-4);
        assert!((u_out[1] - -1.0).abs() < 1e-4);
        assert!((u_out[2] - 0.0).abs() < 1e-4);
    }

    #[test]
    fn test_resampler_passthrough_16k() {
        let mut resampler = AudioResampler::new(16000, 16000, 512);
        let input = vec![0.1f32; 1024];
        let chunks = resampler.process_mono_samples(&input);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].samples.len(), 512);
        assert_eq!(chunks[0].sample_rate, 16000);
        assert_eq!(chunks[0].sequence_id, 1);
        assert_eq!(chunks[1].sequence_id, 2);
    }

    #[test]
    fn test_resampler_48k_to_16k_ratio() {
        // 48kHz -> 16kHz (3:1 ratio).
        // 1536 input samples @ 48kHz = 512 output samples @ 16kHz (exactly 1 chunk)
        let mut resampler = AudioResampler::new(48000, 16000, 512);
        let input = vec![0.25f32; 1536];
        let chunks = resampler.process_mono_samples(&input);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].samples.len(), 512);
        assert_eq!(chunks[0].sample_rate, 16000);
        assert!((chunks[0].samples[0] - 0.25).abs() < 1e-4);
    }

    #[test]
    fn test_resampler_44_1k_to_16k() {
        // 44.1kHz -> 16kHz
        let mut resampler = AudioResampler::new(44100, 16000, 512);
        // Feed 44100 samples (1 second) -> expect approximately 16000 samples = ~31 chunks
        let input = vec![0.5f32; 44100];
        let chunks = resampler.process_mono_samples(&input);
        let total_samples: usize = chunks.iter().map(|c| c.samples.len()).sum();
        let expected_samples = (44100.0 * 16000.0 / 44100.0) as usize; // 16000
        assert!((total_samples as i64 - expected_samples as i64).abs() < 512);
    }
}
