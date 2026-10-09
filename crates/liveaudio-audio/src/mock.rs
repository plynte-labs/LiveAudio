// SPDX-License-Identifier: MIT

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use tokio::sync::mpsc;

use crate::error::AudioError;
use crate::traits::{AudioBoxFuture, AudioChunkReceiver, AudioSource};
use crate::types::{AudioChunk, AudioDeviceInfo, AudioSourceStatus, AudioStreamConfig};

/// Deterministic mock audio capture source for headless CI and integration tests.
pub struct MockAudioSource {
    status: AudioSourceStatus,
    devices: Vec<AudioDeviceInfo>,
    mock_samples: Option<Vec<f32>>,
    real_time_pace: bool,
    worker_handle: Option<JoinHandle<()>>,
    stop_signal: Option<Arc<AtomicBool>>,
}

impl Default for MockAudioSource {
    fn default() -> Self {
        Self::new()
    }
}

impl MockAudioSource {
    pub fn new() -> Self {
        let default_devices = vec![
            AudioDeviceInfo::new(
                "mock_mic_1",
                "🎤 Mock Studio Microphone",
                true,
                1,
                vec![16000, 48000],
                false,
            ),
            AudioDeviceInfo::new(
                "loopback:mock_out",
                "🔊 Mock Speakers (Loopback)",
                false,
                2,
                vec![48000],
                true,
            ),
        ];

        Self {
            status: AudioSourceStatus::Stopped,
            devices: default_devices,
            mock_samples: None,
            real_time_pace: false,
            worker_handle: None,
            stop_signal: None,
        }
    }

    /// Provide custom sample data for the mock stream to emit.
    pub fn with_samples(mut self, samples: Vec<f32>) -> Self {
        self.mock_samples = Some(samples);
        self
    }

    /// Enable real-time pacing (sleeps between chunk emissions).
    pub fn with_real_time_pacing(mut self, pace: bool) -> Self {
        self.real_time_pace = pace;
        self
    }
}

impl AudioSource for MockAudioSource {
    fn enumerate_devices(&self) -> AudioBoxFuture<'_, Result<Vec<AudioDeviceInfo>, AudioError>> {
        Box::pin(async move { Ok(self.devices.clone()) })
    }

    fn start<'a>(
        &'a mut self,
        config: &'a AudioStreamConfig,
    ) -> AudioBoxFuture<'a, Result<AudioChunkReceiver, AudioError>> {
        Box::pin(async move {
            if self.status == AudioSourceStatus::Capturing {
                return Err(AudioError::AlreadyRunning);
            }

            self.status = AudioSourceStatus::Initializing;
            let (tx, rx) = mpsc::channel(config.ring_buffer_capacity);
            let stop_flag = Arc::new(AtomicBool::new(false));
            let stop_flag_clone = Arc::clone(&stop_flag);

            let samples_to_stream = self
                .mock_samples
                .clone()
                .unwrap_or_else(|| vec![0.0f32; config.chunk_size * 20]);

            let chunk_size = config.chunk_size;
            let sample_rate = config.sample_rate;
            let pace = self.real_time_pace;

            let handle = std::thread::Builder::new()
                .name("liveaudio-mock-source".to_string())
                .spawn(move || {
                    let mut seq = 0u64;
                    let chunk_duration =
                        std::time::Duration::from_secs_f32(chunk_size as f32 / sample_rate as f32);

                    for chunk_slice in samples_to_stream.chunks(chunk_size) {
                        if stop_flag_clone.load(Ordering::Relaxed) {
                            break;
                        }

                        let mut chunk_vec = chunk_slice.to_vec();
                        if chunk_vec.len() < chunk_size {
                            chunk_vec.resize(chunk_size, 0.0);
                        }

                        seq += 1;
                        let chunk = AudioChunk::new(chunk_vec, sample_rate, 1, seq);

                        if tx.blocking_send(chunk).is_err() {
                            break;
                        }

                        if pace {
                            std::thread::sleep(chunk_duration);
                        }
                    }
                })
                .map_err(|e| AudioError::StreamError(e.to_string()))?;

            self.worker_handle = Some(handle);
            self.stop_signal = Some(stop_flag);
            self.status = AudioSourceStatus::Capturing;

            Ok(AudioChunkReceiver::new(rx))
        })
    }

    fn stop(&mut self) -> AudioBoxFuture<'_, Result<(), AudioError>> {
        Box::pin(async move {
            if let Some(signal) = self.stop_signal.take() {
                signal.store(true, Ordering::Relaxed);
            }

            if let Some(handle) = self.worker_handle.take() {
                let _ = handle.join();
            }

            self.status = AudioSourceStatus::Stopped;
            Ok(())
        })
    }

    fn status(&self) -> AudioSourceStatus {
        self.status
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DEFAULT_CHUNK_SIZE, DEFAULT_SAMPLE_RATE};

    #[tokio::test]
    async fn test_mock_audio_source_lifecycle() {
        let mut source = MockAudioSource::new();
        assert_eq!(source.status(), AudioSourceStatus::Stopped);

        let devices = source.enumerate_devices().await.unwrap();
        assert_eq!(devices.len(), 2);
        assert!(devices[0].is_default);
        assert!(devices[1].is_loopback);

        let config = AudioStreamConfig::default();
        let mut receiver = source.start(&config).await.unwrap();
        assert_eq!(source.status(), AudioSourceStatus::Capturing);

        let first_chunk = receiver.recv().await;
        assert!(first_chunk.is_some());
        let chunk = first_chunk.unwrap();
        assert_eq!(chunk.samples.len(), DEFAULT_CHUNK_SIZE);
        assert_eq!(chunk.sample_rate, DEFAULT_SAMPLE_RATE);

        source.stop().await.unwrap();
        assert_eq!(source.status(), AudioSourceStatus::Stopped);
    }
}
