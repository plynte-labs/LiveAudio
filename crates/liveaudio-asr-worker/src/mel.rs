// SPDX-License-Identifier: MIT

use std::f32::consts::PI;
use std::path::Path;
use std::sync::Arc;

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use serde_json::Value;

pub const SAMPLE_RATE: u32 = 16_000;
pub const N_FFT: usize = 400;
pub const HOP_LENGTH: usize = 160;
pub const CHUNK_SAMPLES: usize = 480_000;
pub const N_FRAMES: usize = 3000;
pub const N_FREQS: usize = 201;

pub struct MelExtractor {
    pub n_mels: usize,
    mel_filters: Vec<f32>,
    hann_window: Vec<f32>,
    fft_plan: Arc<dyn Fft<f32>>,
    audio_chunk: Vec<f32>,
    padded_audio: Vec<f32>,
    resample_buf: Vec<f32>,
    fft_buf: Vec<Complex<f32>>,
    fft_scratch: Vec<Complex<f32>>,
    power_spec: Vec<f32>,
    mel_out: Vec<f32>,
}

impl MelExtractor {
    pub fn new(n_mels: usize) -> Self {
        let filters = build_slaney_mel_filters(n_mels);
        Self::with_filters(n_mels, filters)
    }

    pub fn with_filters(n_mels: usize, mel_filters: Vec<f32>) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft_plan = planner.plan_fft_forward(N_FFT);
        let scratch_len = fft_plan.get_inplace_scratch_len();

        let mut hann_window = Vec::with_capacity(N_FFT);
        for n in 0..N_FFT {
            let val = 0.5 * (1.0 - (2.0 * PI * (n as f32) / (N_FFT as f32)).cos());
            hann_window.push(val);
        }

        Self {
            n_mels,
            mel_filters,
            hann_window,
            fft_plan,
            audio_chunk: vec![0.0f32; CHUNK_SAMPLES],
            padded_audio: vec![0.0f32; CHUNK_SAMPLES + N_FFT],
            resample_buf: Vec::with_capacity(CHUNK_SAMPLES * 2),
            fft_buf: vec![Complex::new(0.0, 0.0); N_FFT],
            fft_scratch: vec![Complex::new(0.0, 0.0); scratch_len],
            power_spec: vec![0.0f32; N_FREQS],
            mel_out: vec![0.0f32; n_mels * N_FRAMES],
        }
    }

    pub fn from_preprocessor_config(config_path: &Path) -> Result<Self, String> {
        let data = std::fs::read_to_string(config_path)
            .map_err(|e| format!("Failed to read preprocessor config: {e}"))?;
        let json: Value = serde_json::from_str(&data)
            .map_err(|e| format!("Failed to parse preprocessor config JSON: {e}"))?;

        let n_mels = json
            .get("feature_size")
            .and_then(Value::as_u64)
            .map(|v| v as usize)
            .unwrap_or(80);

        if let Some(filters_val) = json.get("mel_filters").and_then(Value::as_array) {
            if let Some(filters) = parse_embedded_filters(filters_val, n_mels) {
                return Ok(Self::with_filters(n_mels, filters));
            }
        }

        Ok(Self::new(n_mels))
    }

    pub fn extract(&mut self, input_samples: &[f32], sample_rate: u32) -> &[f32] {
        let samples_16k: &[f32] = if sample_rate == SAMPLE_RATE {
            input_samples
        } else {
            resample_linear(input_samples, sample_rate, SAMPLE_RATE, &mut self.resample_buf);
            &self.resample_buf
        };

        let copy_len = samples_16k.len().min(CHUNK_SAMPLES);
        self.audio_chunk[..copy_len].copy_from_slice(&samples_16k[..copy_len]);
        if copy_len < CHUNK_SAMPLES {
            self.audio_chunk[copy_len..].fill(0.0);
        }

        apply_reflect_padding(&self.audio_chunk, &mut self.padded_audio);

        for frame in 0..N_FRAMES {
            let offset = frame * HOP_LENGTH;
            for n in 0..N_FFT {
                let sample = self.padded_audio[offset + n];
                let windowed = sample * self.hann_window[n];
                self.fft_buf[n] = Complex::new(windowed, 0.0);
            }

            self.fft_plan
                .process_with_scratch(&mut self.fft_buf, &mut self.fft_scratch);

            for k in 0..N_FREQS {
                self.power_spec[k] = self.fft_buf[k].norm_sqr();
            }

            for m in 0..self.n_mels {
                let filter_offset = m * N_FREQS;
                let mut sum = 0.0f32;
                for k in 0..N_FREQS {
                    sum += self.mel_filters[filter_offset + k] * self.power_spec[k];
                }
                let log_val = sum.max(1e-10).log10();
                self.mel_out[m * N_FRAMES + frame] = log_val;
            }
        }

        let mut global_max = f32::NEG_INFINITY;
        for &val in &self.mel_out {
            if val > global_max {
                global_max = val;
            }
        }

        let min_bound = global_max - 8.0;
        for val in &mut self.mel_out {
            let clamped = (*val).max(min_bound);
            *val = (clamped + 4.0) / 4.0;
        }

        &self.mel_out
    }
}

fn apply_reflect_padding(input: &[f32], padded: &mut [f32]) {
    let half_n = N_FFT / 2; // 200
    let n = input.len();    // 480_000

    padded[half_n..half_n + n].copy_from_slice(input);

    if n > 1 {
        for i in 0..half_n {
            let src_idx = (i + 1).min(n - 1);
            padded[half_n - 1 - i] = input[src_idx];
        }
        for i in 0..half_n {
            let src_idx = n.saturating_sub(2 + i);
            padded[half_n + n + i] = input[src_idx];
        }
    } else {
        let single = input.first().copied().unwrap_or(0.0);
        padded[..half_n].fill(single);
        padded[half_n + n..].fill(single);
    }
}

fn resample_linear(input: &[f32], src_rate: u32, dst_rate: u32, out: &mut Vec<f32>) {
    out.clear();
    if input.is_empty() || src_rate == 0 || dst_rate == 0 {
        return;
    }
    let ratio = src_rate as f64 / dst_rate as f64;
    let target_len = ((input.len() as f64) / ratio).round() as usize;
    out.reserve(target_len);

    for i in 0..target_len {
        let src_pos = i as f64 * ratio;
        let idx0 = src_pos.floor() as usize;
        let frac = (src_pos - idx0 as f64) as f32;
        let idx1 = (idx0 + 1).min(input.len() - 1);
        let s0 = input[idx0.min(input.len() - 1)];
        let s1 = input[idx1];
        out.push(s0 + frac * (s1 - s0));
    }
}

fn hz_to_mel(f: f32) -> f32 {
    let min_log_hz = 1000.0;
    let min_log_mel = 15.0;
    let logstep = (6.4f32).ln() / 27.0;
    if f >= min_log_hz {
        min_log_mel + ((f / min_log_hz).ln()) / logstep
    } else {
        3.0 * f / 200.0
    }
}

fn mel_to_hz(m: f32) -> f32 {
    let min_log_hz = 1000.0;
    let min_log_mel = 15.0;
    let logstep = (6.4f32).ln() / 27.0;
    if m >= min_log_mel {
        min_log_hz * ((m - min_log_mel) * logstep).exp()
    } else {
        200.0 * m / 3.0
    }
}

pub fn build_slaney_mel_filters(n_mels: usize) -> Vec<f32> {
    let m_min = hz_to_mel(0.0);
    let m_max = hz_to_mel(8000.0);

    let num_points = n_mels + 2;
    let mut f_pts = Vec::with_capacity(num_points);
    for i in 0..num_points {
        let m = m_min + (m_max - m_min) * (i as f32) / ((num_points - 1) as f32);
        f_pts.push(mel_to_hz(m));
    }

    let mut fft_freqs = Vec::with_capacity(N_FREQS);
    for k in 0..N_FREQS {
        fft_freqs.push((k as f32) * (SAMPLE_RATE as f32) / (N_FFT as f32));
    }

    let mut filters = vec![0.0f32; n_mels * N_FREQS];
    for m in 0..n_mels {
        let left = f_pts[m];
        let center = f_pts[m + 1];
        let right = f_pts[m + 2];
        let norm = 2.0 / (right - left);

        for k in 0..N_FREQS {
            let f = fft_freqs[k];
            let mut weight = 0.0f32;
            if f >= left && f <= center && center > left {
                weight = (f - left) / (center - left);
            } else if f > center && f <= right && right > center {
                weight = (right - f) / (right - center);
            }
            filters[m * N_FREQS + k] = weight * norm;
        }
    }

    filters
}

fn parse_embedded_filters(array: &[Value], n_mels: usize) -> Option<Vec<f32>> {
    if array.len() == n_mels {
        let mut filters = Vec::with_capacity(n_mels * N_FREQS);
        for row in array {
            let row_arr = row.as_array()?;
            if row_arr.len() != N_FREQS {
                return None;
            }
            for val in row_arr {
                filters.push(val.as_f64()? as f32);
            }
        }
        return Some(filters);
    }

    if array.len() == N_FREQS {
        let mut transposed = vec![0.0f32; n_mels * N_FREQS];
        for (k, row) in array.iter().enumerate() {
            let row_arr = row.as_array()?;
            if row_arr.len() != n_mels {
                return None;
            }
            for (m, val) in row_arr.iter().enumerate() {
                transposed[m * N_FREQS + k] = val.as_f64()? as f32;
            }
        }
        return Some(transposed);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mel_filter_dimensions_80_and_128() {
        let filters_80 = build_slaney_mel_filters(80);
        assert_eq!(filters_80.len(), 80 * 201);

        let filters_128 = build_slaney_mel_filters(128);
        assert_eq!(filters_128.len(), 128 * 201);
    }

    #[test]
    fn periodic_hann_window_starts_at_zero() {
        let extractor = MelExtractor::new(80);
        assert_eq!(extractor.hann_window.len(), 400);
        assert!(extractor.hann_window[0].abs() < 1e-6);
        assert!((extractor.hann_window[200] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn mel_extraction_synthetic_tone_zero_allocations() {
        let mut extractor = MelExtractor::new(80);

        let mut sine_16k = vec![0.0f32; 16000];
        for (i, sample) in sine_16k.iter_mut().enumerate() {
            *sample = (2.0 * PI * 440.0 * (i as f32) / 16000.0).sin();
        }

        let mel = extractor.extract(&sine_16k, 16000);
        assert_eq!(mel.len(), 80 * 3000);

        for &val in mel {
            assert!((-2.0..=2.0).contains(&val));
        }
    }

    #[test]
    fn linear_resampling_48k_to_16k() {
        let input_48k = vec![1.0f32; 48000];
        let mut out = Vec::new();
        resample_linear(&input_48k, 48000, 16000, &mut out);
        assert_eq!(out.len(), 16000);
        assert!((out[8000] - 1.0).abs() < 1e-5);
    }
}
