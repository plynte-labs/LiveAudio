// SPDX-License-Identifier: MIT

use liveaudio_audio::{
    create_audio_ring_buffer, enumerate_cpal_devices, mix_multichannel_to_mono,
    normalize_device_name, AudioResampler, AudioSource, AudioSourceStatus, AudioStreamConfig,
    MockAudioSource, DEFAULT_CHUNK_SIZE, DEFAULT_SAMPLE_RATE,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_mock_audio_source_capture_stream() {
    let mut source = MockAudioSource::new();
    assert_eq!(source.status(), AudioSourceStatus::Stopped);

    let devices = source.enumerate_devices().await.expect("enumerate devices");
    assert!(!devices.is_empty());
    assert!(devices.iter().any(|d| d.is_default));
    assert!(devices.iter().any(|d| d.is_loopback));

    let config = AudioStreamConfig {
        device_id: None,
        sample_rate: DEFAULT_SAMPLE_RATE,
        channels: 1,
        chunk_size: DEFAULT_CHUNK_SIZE,
        ring_buffer_capacity: 50,
    };

    let mut rx = source.start(&config).await.expect("start source");
    assert_eq!(source.status(), AudioSourceStatus::Capturing);

    // Read 5 chunks
    for i in 1..=5 {
        let chunk = rx.recv().await.expect("receive chunk");
        assert_eq!(chunk.samples.len(), DEFAULT_CHUNK_SIZE);
        assert_eq!(chunk.sample_rate, DEFAULT_SAMPLE_RATE);
        assert_eq!(chunk.channels, 1);
        assert_eq!(chunk.sequence_id, i);
        assert!(chunk.duration_sec() > 0.031 && chunk.duration_sec() < 0.033);
    }

    source.stop().await.expect("stop source");
    assert_eq!(source.status(), AudioSourceStatus::Stopped);
}

#[test]
fn test_multichannel_downmixing_parity() {
    // 5.1 channel interleaved: 6 channels per frame
    // Frame 1: [1.0, 1.0, 1.0, 1.0, 1.0, 1.0] -> mono: 1.0
    // Frame 2: [0.0, 0.6, 0.0, 0.6, 0.0, 0.0] -> mono: 0.2
    let multichannel = vec![
        1.0f32, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.6, 0.0, 0.6, 0.0, 0.0,
    ];
    let mut mono = Vec::new();
    mix_multichannel_to_mono(&multichannel, 6, &mut mono);
    assert_eq!(mono.len(), 2);
    assert!((mono[0] - 1.0).abs() < 1e-6);
    assert!((mono[1] - 0.2).abs() < 1e-6);
}

#[test]
fn test_resampler_streaming_continuity_48k_to_16k() {
    // 48 kHz to 16 kHz with continuous tone
    let mut resampler = AudioResampler::new(48000, 16000, 512);

    // Feed in two consecutive batches of 1536 samples (1536 samples @ 48kHz = 512 samples @ 16kHz)
    let b1 = vec![0.5f32; 1536];
    let chunks1 = resampler.process_mono_samples(&b1);
    assert_eq!(chunks1.len(), 1);
    assert_eq!(chunks1[0].samples.len(), 512);
    assert_eq!(chunks1[0].sequence_id, 1);

    let b2 = vec![0.5f32; 1536];
    let chunks2 = resampler.process_mono_samples(&b2);
    assert_eq!(chunks2.len(), 1);
    assert_eq!(chunks2[0].samples.len(), 512);
    assert_eq!(chunks2[0].sequence_id, 2);

    // Partial batch followed by flush
    let b3 = vec![0.5f32; 768];
    let chunks3 = resampler.process_mono_samples(&b3);
    assert_eq!(chunks3.len(), 0); // Not enough for full chunk yet

    let flushed = resampler.flush();
    assert!(flushed.is_some());
    let last = flushed.unwrap();
    assert_eq!(last.samples.len(), 512); // Padded to chunk size
    assert_eq!(last.sequence_id, 3);
}

#[test]
fn test_ring_buffer_concurrency_stress_and_overflow() {
    let capacity_samples = 4096;
    let chunk_size = 512;
    let (mut prod, mut cons) = create_audio_ring_buffer(capacity_samples, chunk_size);

    let stop = Arc::new(AtomicBool::new(false));
    let stop_cons = Arc::clone(&stop);

    // Spawn consumer thread
    let cons_handle = std::thread::spawn(move || {
        let mut read_buf = vec![0.0f32; 1024];
        let mut total_read = 0usize;
        while !stop_cons.load(Ordering::Relaxed) {
            let read = cons.pop_samples(&mut read_buf);
            total_read += read;
            if read == 0 {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        total_read
    });

    // Producer writes 10 batches of 512 samples
    let batch = vec![0.123f32; 512];
    for _ in 0..10 {
        prod.push_samples(&batch);
        std::thread::sleep(Duration::from_millis(2));
    }

    std::thread::sleep(Duration::from_millis(20));
    stop.store(true, Ordering::Relaxed);
    let total_read = cons_handle.join().unwrap();

    let stats = prod.metrics().snapshot();
    assert_eq!(stats.total_samples_written, 5120);
    assert_eq!(stats.dropped_samples, 0);
    assert_eq!(stats.dropped_chunks, 0);
    assert!(total_read >= 5120);
}

#[test]
fn test_ring_buffer_overflow_drop_oldest_never_blocks() {
    // Small ring buffer to force overflow
    let capacity_samples = 1024;
    let chunk_size = 512;
    let (mut prod, _cons) = create_audio_ring_buffer(capacity_samples, chunk_size);

    // Push 3 chunks of 512 samples without reading
    // Chunk 1 & 2 fit (1024 samples)
    let chunk1 = vec![0.1f32; 512];
    let p1 = prod.push_samples(&chunk1);
    assert_eq!(p1, 512);

    let p2 = prod.push_samples(&chunk1);
    assert_eq!(p2, 512);

    // Chunk 3 overflows!
    let p3 = prod.push_samples(&chunk1);
    assert_eq!(p3, 0); // No space, dropped without blocking

    let stats = prod.metrics().snapshot();
    assert_eq!(stats.total_samples_written, 1024);
    assert_eq!(stats.dropped_samples, 512);
    assert_eq!(stats.dropped_chunks, 1);
    assert_eq!(prod.dropped_chunks(), 1);
}

#[test]
fn test_device_normalization_dedup() {
    assert_eq!(
        normalize_device_name("Headphones (Realtek(R) Audio)"),
        normalize_device_name("Headphones (Realtek Audio)")
    );
    assert_eq!(
        normalize_device_name("Microphone Array [Intel Smart Sound]"),
        normalize_device_name("Microphone Array (Intel)")
    );
}

#[test]
fn test_cpal_device_enumeration() {
    let devices = enumerate_cpal_devices().expect("enumerate CPAL devices");
    for dev in &devices {
        assert!(!dev.name.is_empty());
        assert!(dev.channels > 0);
        assert!(!dev.supported_sample_rates.is_empty());
    }
}
