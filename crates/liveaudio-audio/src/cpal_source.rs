// SPDX-License-Identifier: MIT

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::device::enumerate_cpal_devices;
use crate::error::AudioError;
use crate::resampler::{mix_multichannel_to_mono, AudioResampler};
use crate::ring_buffer::create_audio_ring_buffer;
use crate::rnnoise::RnnoiseProcessor;
use crate::traits::{AudioBoxFuture, AudioChunkReceiver, AudioSource};
use crate::types::{AudioChunk, AudioDeviceInfo, AudioSourceStatus, AudioStreamConfig};

const QUEUE_FULL_WARNING: &str = "Downstream audio queue saturated. Dropped incoming chunk.";

fn handle_stream_error(is_xrun: bool, capture_failed: &AtomicBool) {
    if !is_xrun {
        capture_failed.store(true, Ordering::SeqCst);
    }
}

fn audio_worker_should_stop(stop: &AtomicBool, capture_failed: &AtomicBool) -> bool {
    stop.load(Ordering::Relaxed) || capture_failed.load(Ordering::SeqCst)
}

fn process_captured_chunk(
    chunk: AudioChunk,
    processor: &mut Option<RnnoiseProcessor>,
    capture_failed: &AtomicBool,
) -> Result<Vec<AudioChunk>, AudioError> {
    let result = match processor {
        Some(processor) => processor.process(chunk),
        None => Ok(vec![chunk]),
    };
    if result.is_err() {
        capture_failed.store(true, Ordering::SeqCst);
    }
    result
}

fn flush_capture_processor(
    processor: &mut Option<RnnoiseProcessor>,
) -> Result<Vec<AudioChunk>, AudioError> {
    match processor {
        Some(processor) => processor.flush(),
        None => Ok(Vec::new()),
    }
}

fn create_capture_processor(enabled: bool) -> Result<Option<RnnoiseProcessor>, AudioError> {
    enabled.then(RnnoiseProcessor::new).transpose()
}

fn play_stream_or_stop_worker<E>(
    play: impl FnOnce() -> Result<(), E>,
    stop_signal: Arc<AtomicBool>,
    worker: JoinHandle<()>,
) -> Result<JoinHandle<()>, E> {
    match play() {
        Ok(()) => Ok(worker),
        Err(error) => {
            stop_signal.store(true, Ordering::Relaxed);
            let _ = worker.join();
            Err(error)
        }
    }
}

/// Hardware audio capture source using Cross-Platform Audio Library (CPAL).
///
/// Supports microphone input and Windows WASAPI Loopback capture, with real-time
/// lock-free SPSC buffer isolation and automatic resampling to 16kHz mono.
pub struct CpalAudioSource {
    status: AudioSourceStatus,
    rnnoise_enabled: bool,
    active_stream: Option<cpal::Stream>,
    worker_handle: Option<JoinHandle<()>>,
    stop_signal: Option<Arc<AtomicBool>>,
}

impl Default for CpalAudioSource {
    fn default() -> Self {
        Self::new()
    }
}

impl CpalAudioSource {
    pub fn new() -> Self {
        Self {
            status: AudioSourceStatus::Stopped,
            rnnoise_enabled: false,
            active_stream: None,
            worker_handle: None,
            stop_signal: None,
        }
    }

    /// Configure the shared capture worker to apply local RNNoise before VAD.
    pub fn with_rnnoise_enabled(mut self, enabled: bool) -> Self {
        self.rnnoise_enabled = enabled;
        self
    }

    /// Locate a CPAL device by ID, distinguishing input vs loopback.
    fn resolve_device(
        host: &cpal::Host,
        device_id: Option<&str>,
    ) -> Result<(cpal::Device, bool), AudioError> {
        let is_loopback = device_id
            .map(|id| id.starts_with("loopback:") || id == "default_loopback")
            .unwrap_or(false);

        if is_loopback {
            let selected_id = device_id.unwrap();
            if selected_id == "default_loopback" {
                return host
                    .default_output_device()
                    .map(|device| (device, true))
                    .ok_or_else(|| AudioError::DeviceNotFound(selected_id.to_string()));
            }

            let target_name = selected_id.trim_start_matches("loopback:").trim();

            if let Ok(devices) = host.output_devices() {
                for dev in devices {
                    if let Ok(desc) = dev.description() {
                        if desc.name() == target_name {
                            return Ok((dev, true));
                        }
                    }
                }
            }

            return Err(AudioError::DeviceNotFound(selected_id.to_string()));
        }

        // Standard microphone input
        if let Some(target_id) = device_id {
            if let Ok(devices) = host.input_devices() {
                for dev in devices {
                    if let Ok(desc) = dev.description() {
                        if desc.name() == target_id {
                            return Ok((dev, false));
                        }
                    }
                }
            }
            return Err(AudioError::DeviceNotFound(target_id.to_string()));
        }

        // Default input device
        match host.default_input_device() {
            Some(dev) => Ok((dev, false)),
            None => Err(AudioError::DeviceNotFound(
                "No default input audio device found on system".to_string(),
            )),
        }
    }
}

impl AudioSource for CpalAudioSource {
    fn enumerate_devices(&self) -> AudioBoxFuture<'_, Result<Vec<AudioDeviceInfo>, AudioError>> {
        Box::pin(async move { enumerate_cpal_devices() })
    }

    fn start<'a>(
        &'a mut self,
        config: &'a AudioStreamConfig,
    ) -> AudioBoxFuture<'a, Result<AudioChunkReceiver, AudioError>> {
        Box::pin(async move {
            if self.status == AudioSourceStatus::Capturing {
                return Err(AudioError::AlreadyRunning);
            }

            let mut rnnoise_processor = create_capture_processor(self.rnnoise_enabled)?;
            self.status = AudioSourceStatus::Initializing;
            let host = cpal::default_host();

            let (device, is_loopback) =
                match Self::resolve_device(&host, config.device_id.as_deref()) {
                    Ok(pair) => pair,
                    Err(err) => {
                        self.status = AudioSourceStatus::Standby;
                        return Err(err);
                    }
                };

            let supported_config = if is_loopback {
                device.default_output_config().map_err(|e| {
                    AudioError::StreamError(format!(
                        "Failed to query output config for loopback: {}",
                        e
                    ))
                })?
            } else {
                device.default_input_config().map_err(|e| {
                    AudioError::StreamError(format!("Failed to query input config: {}", e))
                })?
            };

            let hw_channels = supported_config.channels();
            let hw_sample_rate = supported_config.sample_rate();
            let sample_format = supported_config.sample_format();

            info!(
                "Initializing audio stream: hw_rate={}Hz, channels={}, format={:?}, loopback={}",
                hw_sample_rate, hw_channels, sample_format, is_loopback
            );

            // Ring buffer sized for capacity
            let capacity_samples =
                config.ring_buffer_capacity * config.chunk_size * (hw_channels as usize);
            let (mut ring_producer, mut ring_consumer) =
                create_audio_ring_buffer(capacity_samples, config.chunk_size);

            let capture_failed = Arc::new(AtomicBool::new(false));
            let callback_capture_failed = Arc::clone(&capture_failed);
            let err_fn = move |err: cpal::Error| {
                if err.kind() == cpal::ErrorKind::Xrun {
                    handle_stream_error(true, &callback_capture_failed);
                    debug!("CPAL audio stream buffer glitch (Xrun): {}", err);
                } else {
                    handle_stream_error(false, &callback_capture_failed);
                    error!("CPAL audio stream hardware error: {}", err);
                }
            };

            let stream_config: cpal::StreamConfig = supported_config.into();

            let stream = match sample_format {
                cpal::SampleFormat::F32 => device.build_input_stream(
                    stream_config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        ring_producer.push_samples(data);
                    },
                    err_fn,
                    None,
                ),
                cpal::SampleFormat::I16 => device.build_input_stream(
                    stream_config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        // Stack conversion without heap allocation
                        let mut scratch = [0.0f32; 1024];
                        let inv_scale = 1.0 / 32768.0;
                        for chunk in data.chunks(1024) {
                            for (i, &s) in chunk.iter().enumerate() {
                                scratch[i] = s as f32 * inv_scale;
                            }
                            ring_producer.push_samples(&scratch[..chunk.len()]);
                        }
                    },
                    err_fn,
                    None,
                ),
                cpal::SampleFormat::U16 => device.build_input_stream(
                    stream_config,
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        let mut scratch = [0.0f32; 1024];
                        let inv_scale = 1.0 / 32768.0;
                        for chunk in data.chunks(1024) {
                            for (i, &s) in chunk.iter().enumerate() {
                                scratch[i] = (s as f32 - 32768.0) * inv_scale;
                            }
                            ring_producer.push_samples(&scratch[..chunk.len()]);
                        }
                    },
                    err_fn,
                    None,
                ),
                unsupported => {
                    return Err(AudioError::StreamError(format!(
                        "Unsupported CPAL sample format: {:?}",
                        unsupported
                    )));
                }
            }
            .map_err(|e| AudioError::StreamError(format!("build_input_stream failed: {}", e)))?;

            // Downstream async channel for chunks
            let (tx, rx) = mpsc::channel(config.ring_buffer_capacity);
            let stop_flag = Arc::new(AtomicBool::new(false));
            let stop_flag_clone = Arc::clone(&stop_flag);
            let worker_capture_failed = Arc::clone(&capture_failed);

            let chunk_size = config.chunk_size;
            let target_sample_rate = config.sample_rate;
            let mut resampler =
                AudioResampler::new(hw_sample_rate, target_sample_rate, chunk_size)?;

            let worker = std::thread::Builder::new()
                .name("liveaudio-dsp-consumer".to_string())
                .spawn(move || {
                    let mut read_buf = vec![0.0f32; 4096 * (hw_channels as usize)];
                    let mut mono_buf = Vec::with_capacity(4096);
                    let mut processing_failed = false;

                    while !audio_worker_should_stop(&stop_flag_clone, &worker_capture_failed) {
                        let available = ring_consumer.available_samples();
                        if available == 0 {
                            std::thread::sleep(std::time::Duration::from_millis(5));
                            continue;
                        }

                        let read_count = ring_consumer.pop_samples(&mut read_buf);
                        if read_count == 0 {
                            continue;
                        }

                        mono_buf.clear();
                        mix_multichannel_to_mono(
                            &read_buf[..read_count],
                            hw_channels,
                            &mut mono_buf,
                        );

                        let chunks = resampler.process_mono_samples(&mono_buf);
                        for chunk in chunks {
                            let processed = match process_captured_chunk(
                                chunk,
                                &mut rnnoise_processor,
                                &worker_capture_failed,
                            ) {
                                Ok(processed) => processed,
                                Err(error) => {
                                    error!("RNNoise audio processing failed: {}", error);
                                    processing_failed = true;
                                    break;
                                }
                            };
                            for output in processed {
                                // Non-blocking try_send: drop the incoming chunk on saturation.
                                if let Err(mpsc::error::TrySendError::Full(_)) = tx.try_send(output)
                                {
                                    warn!("{}", QUEUE_FULL_WARNING);
                                }
                            }
                        }
                    }

                    if !processing_failed {
                        // Flush the source resampler before draining RNNoise's delayed tail.
                        if let Some(final_chunk) = resampler.flush() {
                            match process_captured_chunk(
                                final_chunk,
                                &mut rnnoise_processor,
                                &worker_capture_failed,
                            ) {
                                Ok(processed) => {
                                    for output in processed {
                                        if let Err(mpsc::error::TrySendError::Full(_)) =
                                            tx.try_send(output)
                                        {
                                            warn!("{}", QUEUE_FULL_WARNING);
                                        }
                                    }
                                }
                                Err(error) => {
                                    error!("RNNoise final audio processing failed: {}", error);
                                    processing_failed = true;
                                }
                            }
                        }
                    }

                    if !processing_failed {
                        match flush_capture_processor(&mut rnnoise_processor) {
                            Ok(processed) => {
                                for output in processed {
                                    if let Err(mpsc::error::TrySendError::Full(_)) =
                                        tx.try_send(output)
                                    {
                                        warn!("{}", QUEUE_FULL_WARNING);
                                    }
                                }
                            }
                            Err(error) => {
                                error!("RNNoise tail drain failed: {}", error);
                                worker_capture_failed.store(true, Ordering::SeqCst);
                            }
                        }
                    }
                })
                .map_err(|e| {
                    AudioError::StreamError(format!("Failed to spawn audio worker thread: {}", e))
                })?;

            let worker = match play_stream_or_stop_worker(
                || {
                    stream
                        .play()
                        .map_err(|e| AudioError::StreamError(format!("Stream play failed: {}", e)))
                },
                stop_flag.clone(),
                worker,
            ) {
                Ok(worker) => worker,
                Err(error) => {
                    drop(stream);
                    self.status = AudioSourceStatus::Standby;
                    return Err(error);
                }
            };

            self.active_stream = Some(stream);
            self.worker_handle = Some(worker);
            self.stop_signal = Some(stop_flag);
            self.status = AudioSourceStatus::Capturing;

            Ok(AudioChunkReceiver::with_capture_error(rx, capture_failed))
        })
    }

    fn stop(&mut self) -> AudioBoxFuture<'_, Result<(), AudioError>> {
        Box::pin(async move {
            if self.status != AudioSourceStatus::Capturing
                && self.status != AudioSourceStatus::Initializing
            {
                return Ok(());
            }

            if let Some(signal) = self.stop_signal.take() {
                signal.store(true, Ordering::Relaxed);
            }

            if let Some(stream) = self.active_stream.take() {
                let _ = stream.pause();
                drop(stream);
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

    #[test]
    fn queue_full_diagnostic_identifies_the_incoming_chunk_as_dropped() {
        assert!(QUEUE_FULL_WARNING.contains("Dropped incoming chunk"));
    }

    #[test]
    fn disabled_capture_processing_preserves_the_original_16khz_chunk() {
        let chunk = AudioChunk {
            samples: (0..512).map(|sample| sample as f32 / 512.0).collect(),
            sample_rate: 16_000,
            channels: 1,
            timestamp_ns: 123_000,
            sequence_id: 9,
        };

        let source = CpalAudioSource::new();
        let mut processor = create_capture_processor(source.rnnoise_enabled)
            .expect("disabled capture needs no processor");
        let capture_failed = AtomicBool::new(false);
        let output = process_captured_chunk(chunk.clone(), &mut processor, &capture_failed)
            .expect("disabled processing must pass through");

        assert_eq!(output, vec![chunk]);
        assert!(!capture_failed.load(Ordering::SeqCst));
    }

    #[test]
    fn enabled_capture_processing_uses_the_shared_processor_and_drains_real_tail() {
        let source = CpalAudioSource::new().with_rnnoise_enabled(true);
        let processor = create_capture_processor(source.rnnoise_enabled)
            .expect("enabled capture processor initializes")
            .expect("enabled capture owns one processor");
        let mut processor = Some(processor);
        let capture_failed = AtomicBool::new(false);
        let mut output = Vec::new();

        for sequence_id in 0..5 {
            let chunk = AudioChunk {
                samples: (0..512)
                    .map(|sample| {
                        let index = sequence_id as usize * 512 + sample;
                        (std::f32::consts::TAU * 440.0 * index as f32 / 16_000.0).sin() * 0.5
                    })
                    .collect(),
                sample_rate: 16_000,
                channels: 1,
                timestamp_ns: 1_000_000_000 + sequence_id * 32_000_000,
                sequence_id,
            };
            output.extend(
                process_captured_chunk(chunk, &mut processor, &capture_failed)
                    .expect("enabled processor must process captured chunks"),
            );
        }
        output
            .extend(flush_capture_processor(&mut processor).expect("clean stop drains valid tail"));

        assert_eq!(
            output
                .iter()
                .map(|chunk| chunk.samples.len())
                .sum::<usize>(),
            5 * 512
        );
        assert!(output.iter().all(|chunk| {
            chunk.sample_rate == 16_000
                && chunk.channels == 1
                && chunk.samples.iter().all(|sample| sample.is_finite())
        }));
        assert_eq!(output.first().unwrap().timestamp_ns, 1_000_000_000);
        assert_eq!(output.last().unwrap().timestamp_ns, 1_128_000_000);
        assert_eq!(output.first().unwrap().sequence_id, 0);
        assert_eq!(output.last().unwrap().sequence_id, 4);
    }

    #[test]
    fn invalid_enabled_audio_fails_instead_of_falling_back_to_raw_samples() {
        let mut processor = Some(RnnoiseProcessor::new().expect("built-in model initializes"));
        let capture_failed = AtomicBool::new(false);
        let invalid = AudioChunk {
            samples: vec![f32::NAN; 512],
            sample_rate: 16_000,
            channels: 1,
            timestamp_ns: 1,
            sequence_id: 0,
        };

        assert!(process_captured_chunk(invalid, &mut processor, &capture_failed).is_err());
        assert!(capture_failed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn fatal_stream_error_terminates_receiver_and_is_not_normal_stop() {
        let capture_failed = Arc::new(AtomicBool::new(false));
        let worker_capture_failed = capture_failed.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let (tx, rx) = mpsc::channel(1);
        let worker = std::thread::spawn(move || {
            while !audio_worker_should_stop(&worker_stop, &worker_capture_failed) {
                std::thread::yield_now();
            }
            drop(tx);
        });
        let mut receiver = AudioChunkReceiver::with_capture_error(rx, capture_failed.clone());

        handle_stream_error(false, &capture_failed);
        worker.join().expect("DSP worker exits after stream error");

        assert!(receiver.recv().await.is_none());
        assert!(receiver.capture_failed());
    }

    #[test]
    fn xrun_does_not_mark_capture_as_fatal() {
        let capture_failed = AtomicBool::new(false);
        handle_stream_error(true, &capture_failed);
        assert!(!capture_failed.load(Ordering::SeqCst));
    }

    #[test]
    fn normal_worker_stop_does_not_look_like_capture_failure() {
        let stop = AtomicBool::new(true);
        let capture_failed = AtomicBool::new(false);
        assert!(audio_worker_should_stop(&stop, &capture_failed));
        assert!(!capture_failed.load(Ordering::SeqCst));
    }

    #[test]
    fn explicitly_missing_loopback_device_is_not_replaced_by_default_output() {
        let host = cpal::default_host();
        if host.default_output_device().is_some() {
            let result = CpalAudioSource::resolve_device(
                &host,
                Some("loopback:__missing_liveaudio_device__"),
            );
            assert!(matches!(result, Err(AudioError::DeviceNotFound(_))));
        }
    }

    #[test]
    fn default_loopback_resolves_to_default_output() {
        let host = cpal::default_host();
        if host.default_output_device().is_some() {
            let result = CpalAudioSource::resolve_device(&host, Some("default_loopback"));
            assert!(result.is_ok());
            assert!(result.unwrap().1);
        }
    }

    #[test]
    fn every_enumerated_device_id_resolves_without_starting_capture() {
        let host = cpal::default_host();
        for device in crate::enumerate_cpal_devices().expect("enumerate CPAL devices") {
            let (_, is_loopback) = CpalAudioSource::resolve_device(&host, Some(&device.id))
                .unwrap_or_else(|error| {
                    panic!("enumerated ID {} did not resolve: {}", device.id, error)
                });
            assert_eq!(
                is_loopback, device.is_loopback,
                "device identity changed: {}",
                device.id
            );
        }
    }

    #[test]
    fn failed_stream_play_stops_and_joins_the_dsp_worker() {
        let stop_signal = Arc::new(AtomicBool::new(false));
        let worker_stop_signal = stop_signal.clone();
        let worker_stopped = Arc::new(AtomicBool::new(false));
        let worker_stopped_flag = worker_stopped.clone();
        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            while !worker_stop_signal.load(Ordering::Relaxed) {
                std::thread::yield_now();
            }
            worker_stopped_flag.store(true, Ordering::SeqCst);
            let _ = finished_tx.send(());
        });

        let result = play_stream_or_stop_worker(
            || Err::<(), _>("injected stream play failure"),
            stop_signal.clone(),
            worker,
        );
        let signaled_before_cleanup = stop_signal.load(Ordering::SeqCst);
        if !signaled_before_cleanup {
            stop_signal.store(true, Ordering::SeqCst);
        }
        let worker_finished = finished_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .is_ok();

        assert!(result.is_err());
        assert!(
            signaled_before_cleanup,
            "play failure must signal worker stop"
        );
        assert!(worker_finished, "play failure must join the DSP worker");
        assert!(worker_stopped.load(Ordering::SeqCst));
    }
}
