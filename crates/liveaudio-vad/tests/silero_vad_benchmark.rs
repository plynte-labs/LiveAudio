// SPDX-License-Identifier: MIT

use liveaudio_vad::{SileroVad, VadConfig, VadEngine};
use std::time::Instant;

#[test]
fn bench_silero_vad_onnx_latency() {
    let config = VadConfig::default();
    let mut vad = SileroVad::new(config).expect("failed to initialize SileroVad ONNX");

    // Warm-up: 50 frames
    let silence = vec![0.0f32; 512];
    for _ in 0..50 {
        let _ = vad
            .evaluate_chunk(&silence)
            .expect("warm-up evaluation failed");
    }

    // Benchmark: 1,000 frames (representing 32 seconds of audio at 32ms per frame)
    let iterations = 1000;
    let mut latencies_us = Vec::with_capacity(iterations);

    let total_start = Instant::now();
    for _ in 0..iterations {
        let t0 = Instant::now();
        let _ = vad.evaluate_chunk(&silence).expect("inference failed");
        let elapsed = t0.elapsed();
        latencies_us.push(elapsed.as_micros() as f64);
    }
    let total_elapsed = total_start.elapsed();

    latencies_us.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mean_us: f64 = latencies_us.iter().sum::<f64>() / (iterations as f64);
    let min_us = latencies_us[0];
    let max_us = latencies_us[latencies_us.len() - 1];
    let p50_us = latencies_us[(iterations as f64 * 0.50) as usize];
    let p95_us = latencies_us[(iterations as f64 * 0.95) as usize];
    let p99_us = latencies_us[(iterations as f64 * 0.99) as usize];

    let mean_ms = mean_us / 1000.0;
    let frame_duration_ms = 32.0;
    let rtf = mean_ms / frame_duration_ms;

    println!("\n=== SILERO VAD ONNX BENCHMARK (Rust liveaudio-vad) ===");
    println!(
        "Total audio processed: {:.2}s across {} frames (512 samples @ 16kHz)",
        iterations as f64 * 0.032,
        iterations
    );
    println!(
        "Total execution time:  {:.2} ms",
        total_elapsed.as_secs_f64() * 1000.0
    );
    println!(
        "Mean latency per frame: {:.3} ms ({:.1} us)",
        mean_ms, mean_us
    );
    println!(
        "Min latency:           {:.3} ms ({:.1} us)",
        min_us / 1000.0,
        min_us
    );
    println!(
        "P50 latency:           {:.3} ms ({:.1} us)",
        p50_us / 1000.0,
        p50_us
    );
    println!(
        "P95 latency:           {:.3} ms ({:.1} us)",
        p95_us / 1000.0,
        p95_us
    );
    println!(
        "P99 latency:           {:.3} ms ({:.1} us)",
        p99_us / 1000.0,
        p99_us
    );
    println!(
        "Max latency:           {:.3} ms ({:.1} us)",
        max_us / 1000.0,
        max_us
    );
    println!(
        "Real-Time Factor (RTF): {:.6}x (audio duration / compute time)",
        rtf
    );
    println!("Speedup vs Realtime:   {:.1}x realtime", 1.0 / rtf);
    println!("=======================================================\n");

    // Rust Silero ONNX must be sub-millisecond per frame
    assert!(
        mean_ms < 1.0,
        "Silero ONNX frame latency must be under 1ms, got {:.3}ms",
        mean_ms
    );
}
