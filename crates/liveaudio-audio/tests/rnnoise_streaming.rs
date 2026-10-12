use liveaudio_audio::{AudioChunk, RnnoiseProcessor};
use nnnoiseless::DenoiseState;
use std::f32::consts::TAU;

fn input_chunk(samples: Vec<f32>, timestamp_ns: u64, sequence_id: u64) -> AudioChunk {
    AudioChunk {
        samples,
        sample_rate: 16_000,
        channels: 1,
        timestamp_ns,
        sequence_id,
    }
}

fn synthetic_samples(start: usize, count: usize) -> Vec<f32> {
    (start..start + count)
        .map(|index| {
            let tone = (TAU * 440.0 * index as f32 / 16_000.0).sin();
            tone * 0.75
        })
        .collect()
}

fn boundary_burst_sample(index: usize, length: usize) -> f32 {
    let local = if index < 160 {
        Some(index)
    } else if index >= length - 160 {
        Some(index - (length - 160))
    } else {
        None
    };
    local.map_or(0.0, |sample| {
        let envelope = (std::f32::consts::PI * sample as f32 / 159.0).sin();
        let voice = (TAU * 220.0 * sample as f32 / 16_000.0).sin()
            + 0.35 * (TAU * 440.0 * sample as f32 / 16_000.0).sin();
        0.8 * envelope * voice
    })
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32).sqrt()
}

fn reference_output(samples: &[f32]) -> Vec<f32> {
    let mut state = DenoiseState::new();
    let mut discarded = [0.0; DenoiseState::FRAME_SIZE];
    state.process_frame(&mut discarded, &[0.0; DenoiseState::FRAME_SIZE]);
    let mut output = Vec::with_capacity(samples.len());
    let mut input_frame = [0.0; DenoiseState::FRAME_SIZE];
    let mut previous = None;
    let mut frame_len = 0;
    let mut processed_frames = 0;
    for sample in samples {
        let prior = previous.unwrap_or(*sample);
        for step in 1..=3 {
            let fraction = step as f32 / 3.0;
            input_frame[frame_len] = (prior + (*sample - prior) * fraction) * 32768.0;
            frame_len += 1;
            if frame_len == DenoiseState::FRAME_SIZE {
                let mut output_frame = [0.0; DenoiseState::FRAME_SIZE];
                state.process_frame(&mut output_frame, &input_frame);
                if processed_frames > 0 {
                    output.extend(
                        (2..DenoiseState::FRAME_SIZE)
                            .step_by(3)
                            .map(|index| output_frame[index] / 32768.0),
                    );
                }
                processed_frames += 1;
                frame_len = 0;
            }
        }
        previous = Some(*sample);
    }
    if processed_frames > 0 {
        let mut output_frame = [0.0; DenoiseState::FRAME_SIZE];
        state.process_frame(&mut output_frame, &[0.0; DenoiseState::FRAME_SIZE]);
        output.extend(
            (2..DenoiseState::FRAME_SIZE)
                .step_by(3)
                .map(|index| output_frame[index] / 32768.0),
        );
    }
    output
}

#[test]
fn processor_accepts_a_standard_16khz_audio_chunk() {
    let mut processor = RnnoiseProcessor::new().expect("built-in RNNoise model initializes");
    let chunk = AudioChunk {
        samples: vec![0.0; 512],
        sample_rate: 16_000,
        channels: 1,
        timestamp_ns: 1_000_000,
        sequence_id: 1,
    };

    processor
        .process(chunk)
        .expect("valid audio chunk processes");
}

#[test]
fn five_chunks_preserve_alignment_and_match_the_prewarmed_rnnoise_algorithm() {
    let samples = synthetic_samples(0, 5 * 512);
    let expected = reference_output(&samples);
    let mut processor = RnnoiseProcessor::new().expect("built-in RNNoise model initializes");
    let mut actual: Vec<f32> = Vec::new();
    let mut actual_metadata = Vec::new();

    for index in 0..5 {
        let chunk = input_chunk(
            samples[index * 512..(index + 1) * 512].to_vec(),
            2_000_000_000 + index as u64 * 32_000_000,
            40 + index as u64,
        );
        for output in processor.process(chunk).expect("input processes") {
            actual_metadata.push((output.timestamp_ns, output.sequence_id));
            actual.extend(output.samples);
        }
    }
    for output in processor.flush().expect("algorithm tail drains") {
        actual_metadata.push((output.timestamp_ns, output.sequence_id));
        actual.extend(output.samples);
    }

    assert_eq!(actual.len(), samples.len());
    assert!(actual[..160].iter().any(|sample| sample.abs() > 1e-5));
    assert_eq!(
        actual, expected,
        "prewarm must not shift or discard the first user frame"
    );
    assert_eq!(actual_metadata.first(), Some(&(2_000_000_000, 40)));
    assert_eq!(actual_metadata.last(), Some(&(2_128_000_000, 44)));
}

#[test]
fn flush_drains_the_incomplete_rnnoise_frame_without_losing_user_samples() {
    let samples = synthetic_samples(0, 512);
    let mut processor = RnnoiseProcessor::new().expect("built-in RNNoise model initializes");
    let mut actual: Vec<f32> = processor
        .process(input_chunk(samples.clone(), 5_000_000, 7))
        .expect("input processes")
        .into_iter()
        .flat_map(|chunk| chunk.samples)
        .collect::<Vec<f32>>();
    actual.extend(
        processor
            .flush()
            .expect("flush processes pending tail")
            .into_iter()
            .flat_map(|chunk| chunk.samples),
    );

    assert_eq!(actual.len(), samples.len());
    assert!(actual.iter().all(|sample: &f32| sample.is_finite()));
    assert!(
        processor.flush().is_err(),
        "processing must stop after final flush"
    );
}

#[test]
fn onset_and_final_burst_remain_at_their_source_positions_after_flush() {
    let length = 2 * 512;
    let samples = (0..length)
        .map(|index| boundary_burst_sample(index, length))
        .collect::<Vec<_>>();
    let mut processor = RnnoiseProcessor::new().expect("built-in RNNoise model initializes");
    let mut output = Vec::new();
    for (index, chunk) in samples.chunks_exact(512).enumerate() {
        output.extend(
            processor
                .process(input_chunk(
                    chunk.to_vec(),
                    3_000_000_000 + index as u64 * 32_000_000,
                    index as u64,
                ))
                .expect("burst processes")
                .into_iter()
                .flat_map(|chunk| chunk.samples),
        );
    }
    output.extend(
        processor
            .flush()
            .expect("flush drains delayed final frame")
            .into_iter()
            .flat_map(|chunk| chunk.samples),
    );

    assert_eq!(output.len(), length);
    let onset_at_source = rms(&output[..160]);
    let onset_one_frame_late = rms(&output[160..320]);
    let tail_at_source = rms(&output[length - 160..]);
    let tail_one_frame_early = rms(&output[length - 320..length - 160]);
    let final_64_samples = rms(&output[length - 64..]);
    assert!(
        onset_at_source > onset_one_frame_late
            && tail_at_source > tail_one_frame_early
            && final_64_samples > 0.05,
        "boundary signal misaligned or final tail undrained: onset source={onset_at_source}, onset delayed={onset_one_frame_late}, final tail={tail_at_source}, final one-frame-early={tail_one_frame_early}, final 64={final_64_samples}"
    );
}

#[test]
fn sixty_second_synthetic_stream_has_no_sample_loss_or_unbounded_output() {
    const CHUNKS: usize = 60 * 16_000 / 512;
    let mut processor = RnnoiseProcessor::new().expect("built-in RNNoise model initializes");
    let mut samples_processed = 0usize;
    let mut output_samples = 0usize;
    let mut maximum_returned_chunks = 0usize;

    for index in 0..CHUNKS {
        let samples = synthetic_samples(index * 512, 512);
        samples_processed += samples.len();
        let output = processor
            .process(input_chunk(
                samples,
                1_000_000_000 + index as u64 * 32_000_000,
                index as u64,
            ))
            .expect("synthetic input processes");
        maximum_returned_chunks = maximum_returned_chunks.max(output.len());
        assert!(output.len() <= 2);
        output_samples += output
            .iter()
            .map(|chunk| chunk.samples.len())
            .sum::<usize>();
    }
    output_samples += processor
        .flush()
        .expect("tail flushes")
        .iter()
        .map(|chunk| chunk.samples.len())
        .sum::<usize>();

    assert_eq!(samples_processed, 60 * 16_000);
    assert_eq!(output_samples, samples_processed);
    assert!(maximum_returned_chunks <= 2);
}

#[test]
fn invalid_samples_fail_without_corrupting_the_processor_state() {
    let mut processor = RnnoiseProcessor::new().expect("built-in RNNoise model initializes");
    for invalid_sample in [f32::NAN, f32::INFINITY, 1.01, -1.01] {
        let mut invalid = vec![0.0; 512];
        invalid[17] = invalid_sample;
        assert!(processor.process(input_chunk(invalid, 1, 1)).is_err());
    }

    let valid = synthetic_samples(0, 512);
    assert!(
        processor.process(input_chunk(valid, 2, 2)).is_ok(),
        "rejected data must not mutate the stream"
    );
}

#[test]
fn invalid_chunk_format_is_rejected_before_stream_state_changes() {
    let mut processor = RnnoiseProcessor::new().expect("built-in RNNoise model initializes");
    let samples = synthetic_samples(0, 512);
    assert!(processor
        .process(AudioChunk {
            sample_rate: 44_100,
            ..input_chunk(samples.clone(), 1, 1)
        })
        .is_err());
    assert!(processor
        .process(AudioChunk {
            channels: 2,
            ..input_chunk(samples.clone(), 1, 1)
        })
        .is_err());
    assert!(processor
        .process(input_chunk(samples[..511].to_vec(), 1, 1))
        .is_err());
    assert!(processor.process(input_chunk(samples, 1, 1)).is_ok());
}
