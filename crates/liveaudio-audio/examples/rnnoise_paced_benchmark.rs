use liveaudio_audio::{AudioChunk, RnnoiseProcessor};
use std::f32::consts::TAU;
use std::time::{Duration, Instant};

const SAMPLE_RATE: usize = 16_000;
const CHUNK_SIZE: usize = 512;
const CHUNK_PERIOD: Duration = Duration::from_millis(32);
const CHUNKS: usize = 60 * SAMPLE_RATE / CHUNK_SIZE;

#[derive(Default)]
struct TimingDiagnostics {
    inputs: Vec<InputTiming>,
}

#[derive(Default)]
struct InputTiming {
    deadline_lateness: Duration,
    actual_ready: Duration,
    output_source_age: Option<Duration>,
}

impl TimingDiagnostics {
    fn record_input_at(&mut self, index: usize, deadline: Duration, actual_ready: Duration) {
        debug_assert_eq!(index, self.inputs.len());
        self.inputs.push(InputTiming {
            deadline_lateness: actual_ready.saturating_sub(deadline),
            actual_ready,
            output_source_age: None,
        });
    }

    fn record_output_at(&mut self, index: usize, emitted_at: Duration) {
        if let Some(input) = self.inputs.get_mut(index) {
            input.output_source_age = Some(emitted_at.saturating_sub(input.actual_ready));
        }
    }
}

fn synthetic_chunk(index: usize) -> Vec<f32> {
    (0..CHUNK_SIZE)
        .map(|offset| {
            let sample = index * CHUNK_SIZE + offset;
            let carrier = (TAU * 220.0 * sample as f32 / SAMPLE_RATE as f32).sin();
            let harmonic = (TAU * 440.0 * sample as f32 / SAMPLE_RATE as f32).sin() * 0.25;
            (carrier + harmonic) * 0.35
        })
        .collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut processor = RnnoiseProcessor::new()?;
    let start = Instant::now();
    let mut processing_time = Duration::ZERO;
    let mut output_samples = 0usize;
    let mut maximum_chunks_per_call = 0usize;
    let mut maximum_added_delay = Duration::ZERO;
    let mut expected_sequence = 0u64;
    let mut bypass_ready_at = Vec::with_capacity(CHUNKS);
    let mut timing_diagnostics = TimingDiagnostics::default();

    for index in 0..CHUNKS {
        let deadline = start + CHUNK_PERIOD * (index as u32 + 1);
        pace_until(deadline);

        let input = AudioChunk {
            samples: synthetic_chunk(index),
            sample_rate: SAMPLE_RATE as u32,
            channels: 1,
            timestamp_ns: CHUNK_PERIOD.as_nanos() as u64 * index as u64,
            sequence_id: index as u64,
        };
        let actual_ready_at = Instant::now();
        bypass_ready_at.push(actual_ready_at);
        timing_diagnostics.record_input_at(
            index,
            deadline.duration_since(start),
            actual_ready_at.duration_since(start),
        );
        let processing_started = Instant::now();
        let output = processor.process(input)?;
        processing_time += processing_started.elapsed();
        maximum_chunks_per_call = maximum_chunks_per_call.max(output.len());
        if output.len() > 2 {
            return Err(format!("processor returned {} chunks in one call", output.len()).into());
        }
        record_output(
            output,
            &bypass_ready_at,
            &mut expected_sequence,
            &mut output_samples,
            &mut maximum_added_delay,
            &mut timing_diagnostics,
            start,
        )?;
    }

    let processing_started = Instant::now();
    let tail = processor.flush()?;
    processing_time += processing_started.elapsed();
    record_output(
        tail,
        &bypass_ready_at,
        &mut expected_sequence,
        &mut output_samples,
        &mut maximum_added_delay,
        &mut timing_diagnostics,
        start,
    )?;

    let audio_duration = CHUNK_PERIOD * CHUNKS as u32;
    let rtf = processing_time.as_secs_f64() / audio_duration.as_secs_f64();
    println!(
        "Target: {}-{}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("Paced synthetic input: 60 seconds at 16 kHz mono");
    println!("Processor wall time: {:.3}s", processing_time.as_secs_f64());
    println!("DSP processing RTF: {:.4}", rtf);
    println!(
        "Maximum added staging delay vs bypass: {:.3}ms",
        maximum_added_delay.as_secs_f64() * 1_000.0
    );
    println!("Maximum output chunks per process call: {maximum_chunks_per_call}");
    println!("Output samples after flush: {output_samples}");
    println!("Per-input timing (deadline_lateness_us, actual_ready_ms, output_source_age_ms):");
    for (index, timing) in timing_diagnostics.inputs.iter().enumerate() {
        let source_age = timing
            .output_source_age
            .map(|age| format!("{:.3}", age.as_secs_f64() * 1_000.0))
            .unwrap_or_else(|| "pending".to_string());
        println!(
            "timing,{index},{:.0},{:.3},{source_age}",
            timing.deadline_lateness.as_secs_f64() * 1_000_000.0,
            timing.actual_ready.as_secs_f64() * 1_000.0
        );
    }

    if output_samples != 60 * SAMPLE_RATE {
        return Err(format!("sample count mismatch: {output_samples}").into());
    }
    if rtf >= 1.0 {
        return Err(format!("DSP real-time factor target failed: {rtf:.4}").into());
    }
    if maximum_added_delay > Duration::from_millis(64) {
        return Err(format!(
            "added staging delay target failed: {:.3}ms",
            maximum_added_delay.as_secs_f64() * 1_000.0
        )
        .into());
    }

    Ok(())
}

fn pace_until(deadline: Instant) {
    let final_spin = Duration::from_millis(2);
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining > final_spin {
        std::thread::sleep(remaining - final_spin);
    }
    while Instant::now() < deadline {
        std::hint::spin_loop();
    }
}

fn record_output(
    chunks: Vec<AudioChunk>,
    bypass_ready_at: &[Instant],
    expected_sequence: &mut u64,
    output_samples: &mut usize,
    maximum_added_delay: &mut Duration,
    diagnostics: &mut TimingDiagnostics,
    benchmark_start: Instant,
) -> Result<(), Box<dyn std::error::Error>> {
    let emitted_at = Instant::now();
    for chunk in chunks {
        if chunk.sequence_id != *expected_sequence {
            return Err(format!(
                "output sequence mismatch: expected {}, got {}",
                *expected_sequence, chunk.sequence_id
            )
            .into());
        }
        if chunk.sample_rate != SAMPLE_RATE as u32 || chunk.channels != 1 {
            return Err("processor changed the output stream format".into());
        }
        let bypass_boundary = bypass_ready_at
            .get(chunk.sequence_id as usize)
            .copied()
            .ok_or_else(|| {
                format!(
                    "output refers to missing source chunk {}",
                    chunk.sequence_id
                )
            })?;
        *maximum_added_delay =
            (*maximum_added_delay).max(emitted_at.saturating_duration_since(bypass_boundary));
        diagnostics.record_output_at(
            chunk.sequence_id as usize,
            emitted_at.duration_since(benchmark_start),
        );
        *output_samples += chunk.samples.len();
        *expected_sequence += 1;
    }
    Ok(())
}

#[cfg(test)]
mod timing_tests {
    use super::TimingDiagnostics;
    use std::time::Duration;

    #[test]
    fn injected_clock_separates_deadline_lateness_readiness_and_output_age() {
        let mut diagnostics = TimingDiagnostics::default();
        diagnostics.record_input_at(0, Duration::from_millis(32), Duration::from_millis(35));
        diagnostics.record_output_at(0, Duration::from_millis(40));

        let input = &diagnostics.inputs[0];
        assert_eq!(input.deadline_lateness, Duration::from_millis(3));
        assert_eq!(input.actual_ready, Duration::from_millis(35));
        assert_eq!(input.output_source_age, Some(Duration::from_millis(5)));
    }
}
