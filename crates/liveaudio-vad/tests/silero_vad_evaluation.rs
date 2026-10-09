// SPDX-License-Identifier: MIT

use liveaudio_vad::{
    MockVadEngine, PreBuffer, SileroVad, SpeechEndReason, VadConfig, VadEngine, VadTransition,
    BUNDLED_SILERO_VAD_V6,
};

#[test]
fn test_silero_vad_bundled_model_load_and_silence_inference() {
    let config = VadConfig::default();
    let mut vad = SileroVad::new(config).expect("load SileroVad");

    let silence = vec![0.0f32; 512];
    let decision = vad.evaluate_chunk(&silence).expect("evaluate chunk");

    // Silence must yield low speech probability (< 0.1)
    assert!(decision.speech_probability < 0.1);
    assert!(!decision.is_speech);
    assert_eq!(decision.transition, VadTransition::None);
}

#[test]
fn test_silero_vad_python_parity_inference() {
    // Python Silero VAD v6 on 512 zeros with 64 zero context outputs exactly 0.02382863
    let config = VadConfig::default();
    let mut vad = SileroVad::from_bytes(BUNDLED_SILERO_VAD_V6, config).expect("load from bytes");

    let silence = vec![0.0f32; 512];
    let decision = vad.evaluate_chunk(&silence).expect("evaluate chunk");

    let python_reference = 0.02382863f32;
    let diff = (decision.speech_probability - python_reference).abs();
    assert!(
        diff < 1e-5,
        "Parity mismatch with Python Silero VAD: expected ~{}, got {}, diff {}",
        python_reference,
        decision.speech_probability,
        diff
    );
}

#[test]
fn test_silero_vad_recurrent_state_preservation_and_reset() {
    let config = VadConfig::default();
    let mut vad = SileroVad::new(config).expect("load SileroVad");

    let chunk = vec![0.0f32; 512];

    // Evaluate 5 frames
    for _ in 0..5 {
        let dec = vad.evaluate_chunk(&chunk).expect("evaluate");
        assert!(dec.speech_probability < 0.1);
    }

    // Reset clears recurrent memory and pre-buffer
    vad.reset();
    let dec_after_reset = vad.evaluate_chunk(&chunk).expect("evaluate after reset");
    assert!(!dec_after_reset.is_speech);
}

#[test]
fn test_pre_buffer_onset_recovery_200ms_7_chunks() {
    // Verify 200ms @ 16kHz, 512 samples = 7 chunks (as specified in WU5/WU6 prompt and Python audio.py)
    let chunks = PreBuffer::chunks_from_pad(200, 16000, 512);
    assert_eq!(chunks, 7);

    let config = VadConfig {
        speech_pad_ms: 200,
        ..Default::default()
    };
    assert_eq!(config.pre_buffer_chunks(), 7);

    // Mock engine verification
    let mut vad = MockVadEngine::new(config);

    // Feed 10 silence chunks: fills pre-buffer (capacity 7, oldest 3 discarded)
    let silence = vec![0.0f32; 512];
    for _ in 0..10 {
        let dec = vad.evaluate_chunk(&silence).expect("silence");
        assert!(!dec.is_speech);
        assert_eq!(dec.transition, VadTransition::None);
    }

    // Speech onset: transition SpeechStart
    let speech = vec![0.8f32; 512];
    let dec_onset = vad.evaluate_chunk(&speech).expect("speech onset");
    assert!(dec_onset.is_speech);
    assert_eq!(dec_onset.transition, VadTransition::SpeechStart);
}

#[test]
fn test_silence_timeout_boundary_parity_25_chunks() {
    // silence_timeout: 0.8s @ 16kHz, 512 chunk = 25 chunks (Python audio.py: int(16000/512 * 0.8) = 25)
    let config = VadConfig {
        silence_timeout_ms: 800,
        max_chunk_duration_sec: 10.0,
        ..Default::default()
    };
    assert_eq!(config.silence_limit_chunks(), 25);

    let mut vad = MockVadEngine::new(config);
    let speech = vec![0.8f32; 512];
    let silence = vec![0.0f32; 512];

    // Start speech
    let d_start = vad.evaluate_chunk(&speech).expect("start");
    assert_eq!(d_start.transition, VadTransition::SpeechStart);

    // 25 silence chunks: utterance must remain open (matches Python: silence_count > silence_limit)
    for i in 1..=25 {
        let d = vad.evaluate_chunk(&silence).expect("silence frame");
        assert!(
            d.is_speech,
            "Frame {} should still be marked as active speech",
            i
        );
        assert_eq!(d.transition, VadTransition::None);
    }

    // 26th silence chunk (> 25): phrase must terminate with SilenceTimeout
    let d_end = vad.evaluate_chunk(&silence).expect("end frame");
    assert!(!d_end.is_speech);
    match d_end.transition {
        VadTransition::SpeechEnd {
            reason,
            duration_ms,
        } => {
            assert_eq!(reason, SpeechEndReason::SilenceTimeout);
            assert!(duration_ms > 0);
        }
        other => panic!("Expected SpeechEnd, got {:?}", other),
    }
}

#[test]
fn test_max_chunk_duration_guillotine_cap_157_chunks() {
    // max_chunk_duration: 5.0s @ 16kHz, 512 chunk = ceil(5.0 * 16000 / 512) = 157 chunks
    let config = VadConfig {
        max_chunk_duration_sec: 5.0,
        silence_timeout_ms: 5000,
        speech_pad_ms: 0, // no pre-roll for direct chunk count testing
        ..Default::default()
    };
    assert_eq!(config.max_chunks_limit(), 157);

    let mut vad = MockVadEngine::new(config);
    let speech = vec![0.8f32; 512];

    // Chunk 1: SpeechStart
    let d1 = vad.evaluate_chunk(&speech).expect("chunk 1");
    assert_eq!(d1.transition, VadTransition::SpeechStart);

    // Chunks 2 to 156: Speech continues
    for _ in 2..=156 {
        let d = vad.evaluate_chunk(&speech).expect("speech frame");
        assert!(d.is_speech);
        assert_eq!(d.transition, VadTransition::None);
    }

    // Chunk 157: Guillotine forces phrase closure (MaxDurationReached)
    let d_max = vad.evaluate_chunk(&speech).expect("chunk 157");
    assert!(!d_max.is_speech);
    match d_max.transition {
        VadTransition::SpeechEnd {
            reason,
            duration_ms,
        } => {
            assert_eq!(reason, SpeechEndReason::MaxDurationReached);
            // 157 chunks * 32ms = ~5024ms
            assert_eq!(duration_ms, 5024);
        }
        other => panic!("Expected MaxDurationReached, got {:?}", other),
    }
}

#[test]
fn test_dynamic_vad_config_update() {
    let mut vad = SileroVad::new(VadConfig::default()).expect("load vad");
    assert_eq!(vad.config().silence_timeout_ms, 700);

    let new_config = VadConfig {
        threshold: 0.75,
        silence_timeout_ms: 800,
        max_chunk_duration_sec: 5.0,
        ..Default::default()
    };
    vad.update_config(new_config).expect("update config");
    assert_eq!(vad.config().threshold, 0.75);
    assert_eq!(vad.config().silence_timeout_ms, 800);
    assert_eq!(vad.config().max_chunk_duration_sec, 5.0);
}
