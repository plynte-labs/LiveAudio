// SPDX-License-Identifier: MIT
//! Real-model smoke test (downloads ~0.7 GB on first run). Run with:
//!   LIVEAUDIO_PARAKEET_WAV=speech.wav cargo test -p liveaudio-asr-worker --test parakeet_smoke -- --ignored --nocapture
//! `LIVEAUDIO_PARAKEET_WAV` is a 16-bit PCM mono WAV; without it, only silence is decoded.
//! The ONNX Runtime library is found through `LIVEAUDIO_ORT_DYLIB_PATH` or the packaging directory.

use liveaudio_asr_worker::download::prepare_parakeet;
use liveaudio_asr_worker::parakeet::ParakeetEngine;

/// Reads the sample data of a 16-bit PCM mono WAV and its sample rate.
fn read_wav(path: &str) -> (Vec<f32>, u32) {
    let bytes = std::fs::read(path).expect("read wav");
    let rate = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    let data_pos = bytes.windows(4).position(|w| w == b"data").expect("data chunk") + 8;
    let samples = bytes[data_pos..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| i16::from_le_bytes(*c) as f32 / 32768.0)
        .collect();
    (samples, rate)
}

#[test]
#[ignore = "downloads the Parakeet model and needs the ONNX Runtime library"]
fn parakeet_loads_and_transcribes() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let paths = rt
        .block_on(prepare_parakeet(None, false, |_, name, pct, _, _, _, _| {
            if pct == Some(100.0) {
                eprintln!("downloaded {name}");
            }
        }))
        .expect("prepare model");
    let mut engine = ParakeetEngine::load(&paths, None).expect("load engine");

    let silence = engine.transcribe(&vec![0.0f32; 32_000], 16_000).expect("silence");
    assert_eq!(silence.text, "");

    // A non-silent signal exercises the preprocessor, encoder and decoder end to end.
    let tone: Vec<f32> = (0..48_000).map(|i| 0.2 * (i as f32 * 0.05).sin()).collect();
    let out = engine.transcribe(&tone, 16_000).expect("tone");
    eprintln!("tone text: {:?}, inference {:.2}s", out.text, out.inference_sec);

    if let Ok(wav) = std::env::var("LIVEAUDIO_PARAKEET_WAV") {
        let (samples, rate) = read_wav(&wav);
        let out = engine.transcribe(&samples, rate).expect("transcribe");
        eprintln!("text: {:?}\naudio {:.2}s, inference {:.2}s", out.text, out.duration_sec, out.inference_sec);
        assert!(!out.text.is_empty());
    }
}
