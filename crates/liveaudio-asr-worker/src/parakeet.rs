// SPDX-License-Identifier: MIT
//! NVIDIA Parakeet-TDT 0.6B v3 (int8 ONNX export) on ONNX Runtime: NeMo mel preprocessor,
//! FastConformer encoder and a greedy TDT (token-and-duration transducer) decoder.
//! The model auto-detects the spoken language, so language and prompt are ignored.

use std::time::Instant;

use ort::session::Session;
use ort::value::{DynValue, Tensor, TensorElementType};

use crate::download::ParakeetPaths;
use crate::mel::resample_linear;
use crate::model::{init_ort_runtime, WhisperOrtEngine};

const SAMPLE_RATE: u32 = 16_000;
/// Emitting more than this many tokens on one frame forces the decoder to move on.
const MAX_TOKENS_PER_FRAME: usize = 10;

pub struct Vocab {
    tokens: Vec<String>,
    pub blank: usize,
}

/// Parses `vocab.txt` (`<token> <id>` per line); the blank is the `<blk>` token.
pub fn parse_vocab(text: &str) -> Result<Vocab, String> {
    let mut tokens: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| !l.is_empty()) {
        let (tok, id) = line.rsplit_once(' ').ok_or_else(|| format!("Malformed vocab line: {line:?}"))?;
        let id: usize = id.parse().map_err(|_| format!("Malformed vocab id in line: {line:?}"))?;
        if tokens.len() <= id {
            tokens.resize(id + 1, String::new());
        }
        tokens[id] = tok.to_string();
    }
    let blank = tokens.iter().position(|t| t == "<blk>").ok_or("vocab.txt has no <blk> token")?;
    Ok(Vocab { tokens, blank })
}

impl Vocab {
    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    pub fn detokenize(&self, ids: &[usize]) -> String {
        let mut text = String::new();
        for &id in ids {
            if id != self.blank {
                if let Some(t) = self.tokens.get(id) {
                    text.push_str(t);
                }
            }
        }
        text.replace('\u{2581}', " ").trim().to_string()
    }
}

pub fn argmax(values: &[f32]) -> usize {
    let mut best = 0;
    for (i, &v) in values.iter().enumerate() {
        if v > values[best] {
            best = i;
        }
    }
    best
}

/// Greedy TDT decoding over `frames` encoder frames. `step(t, last_token, state)` returns the flat
/// joint output (`vocab_size` token logits, then duration logits) and the state it would produce.
/// The state is committed only when a non-blank token is emitted.
pub fn tdt_greedy<S>(
    frames: usize,
    blank: usize,
    vocab_size: usize,
    initial: S,
    mut step: impl FnMut(usize, usize, &S) -> Result<(Vec<f32>, S), String>,
) -> Result<Vec<usize>, String> {
    let mut tokens = Vec::new();
    let mut state = initial;
    let (mut t, mut emitted) = (0, 0);
    while t < frames {
        let last = tokens.last().copied().unwrap_or(blank);
        let (out, next) = step(t, last, &state)?;
        if out.len() < vocab_size {
            return Err(format!("Joint output has {} values, expected at least {vocab_size}", out.len()));
        }
        let token = argmax(&out[..vocab_size]);
        let skip = if out.len() > vocab_size { argmax(&out[vocab_size..]) } else { 0 };
        if token != blank {
            tokens.push(token);
            state = next;
            emitted += 1;
        }
        if skip > 0 {
            t += skip;
            emitted = 0;
        } else if token == blank || emitted == MAX_TOKENS_PER_FRAME {
            t += 1;
            emitted = 0;
        }
    }
    Ok(tokens)
}

/// The fp32 encoder (with GPU weights) is used for any non-CPU device.
pub fn uses_gpu_encoder(device: &str) -> bool {
    !matches!(device, "" | "cpu")
}

pub fn encoder_compute_type(device: &str) -> &'static str {
    if uses_gpu_encoder(device) {
        "fp32"
    } else {
        "int8"
    }
}

/// Device to retry on when the encoder cannot load on `device`; `None` when there is nowhere to go.
pub fn gpu_failure_fallback(device: &str, auto_cpu_fallback: bool) -> Option<&'static str> {
    (uses_gpu_encoder(device) && auto_cpu_fallback).then_some("cpu")
}

type State = (Vec<f32>, Vec<f32>);

pub struct ParakeetEngine {
    preprocessor: Session,
    encoder: Session,
    decoder: Session,
    vocab: Vocab,
    /// Shapes of the two recurrent state inputs of the decoder (dynamic dims resolved to 1).
    state_dims: [Vec<usize>; 2],
    target_length_i32: bool,
}

pub struct ParakeetOutput {
    pub text: String,
    pub duration_sec: f64,
    pub inference_sec: f64,
    pub no_speech_prob: f64,
}

fn run_err(what: &str, e: impl std::fmt::Display) -> String {
    format!("Parakeet {what} failed: {e}")
}

fn f32_tensor(shape: Vec<usize>, data: Vec<f32>) -> Result<Tensor<f32>, String> {
    Tensor::<f32>::from_array((shape, data.into_boxed_slice())).map_err(|e| run_err("tensor creation", e))
}

fn len_tensor(n: usize) -> Result<Tensor<i64>, String> {
    Tensor::<i64>::from_array(([1usize], vec![n as i64].into_boxed_slice())).map_err(|e| run_err("tensor creation", e))
}

impl ParakeetEngine {
    /// Loads the encoder on `device` (`cpu`, `cuda` or `directml`; the fp32 encoder for a GPU, int8
    /// for the CPU). The preprocessor and the decoder run once per frame on tiny tensors, so they
    /// stay on the CPU.
    pub fn load(paths: &ParakeetPaths, device: &str, cpu_threads: Option<usize>) -> Result<Self, String> {
        init_ort_runtime()?;
        let build = |path: &std::path::Path, device: &str| -> Result<Session, String> {
            WhisperOrtEngine::make_session_builder(device, cpu_threads)?
                .commit_from_file(path)
                .map_err(|e| format!("Failed to load {}: {e}", path.display()))
        };
        let preprocessor = build(&paths.preprocessor, "cpu")?;
        let encoder = build(&paths.encoder, device)?;
        let decoder = build(&paths.decoder_joint, "cpu")?;
        let vocab = parse_vocab(&std::fs::read_to_string(&paths.vocab).map_err(|e| e.to_string())?)?;

        let mut state_dims = Vec::new();
        for input in decoder.inputs().iter().filter(|i| i.name().contains("state")) {
            let dims: Vec<usize> = input
                .dtype()
                .tensor_shape()
                .map(|s| s.iter().map(|&d| d.max(1) as usize).collect())
                .ok_or("Decoder state input has no tensor shape")?;
            state_dims.push(dims);
        }
        let [a, b]: [Vec<usize>; 2] =
            state_dims.try_into().map_err(|_| "Decoder must expose exactly two state inputs".to_string())?;
        let target_length_i32 = decoder
            .inputs()
            .iter()
            .find(|i| i.name().contains("length"))
            .is_some_and(|i| i.dtype().tensor_type() == Some(TensorElementType::Int32));
        Ok(Self { preprocessor, encoder, decoder, vocab, state_dims: [a, b], target_length_i32 })
    }

    /// Returns the encoder output as `frames` rows of `dim` values.
    fn encode(&mut self, wav: &[f32]) -> Result<(Vec<f32>, usize, usize), String> {
        let n = wav.len();
        let pre_in: Vec<String> = self.preprocessor.inputs().iter().map(|i| i.name().to_string()).collect();
        let wav_tensor = f32_tensor(vec![1, n], wav.to_vec())?;
        let (feats, t) = {
            let out = self
                .preprocessor
                .run(ort::inputs![pre_in[0].as_str() => wav_tensor, pre_in[1].as_str() => len_tensor(n)?])
                .map_err(|e| run_err("preprocessor", e))?;
            let (shape, data) = out[0].try_extract_tensor::<f32>().map_err(|e| run_err("features", e))?;
            (data.to_vec(), *shape.last().ok_or("empty features shape")? as usize)
        };
        let n_feats = feats.len() / t.max(1);
        let enc_in: Vec<String> = self.encoder.inputs().iter().map(|i| i.name().to_string()).collect();
        let feats_tensor = f32_tensor(vec![1, n_feats, t], feats)?;
        let out = self
            .encoder
            .run(ort::inputs![enc_in[0].as_str() => feats_tensor, enc_in[1].as_str() => len_tensor(t)?])
            .map_err(|e| run_err("encoder", e))?;
        let (shape, data) = out[0].try_extract_tensor::<f32>().map_err(|e| run_err("encoder output", e))?;
        let (dim, frames) = (shape[1] as usize, shape[2] as usize);
        // [1, D, T'] -> [T', D]
        let mut rows = vec![0.0f32; data.len()];
        for d in 0..dim {
            for t in 0..frames {
                rows[t * dim + d] = data[d * frames + t];
            }
        }
        Ok((rows, frames, dim))
    }

    fn decode_frame(&mut self, frame: &[f32], last: usize, state: &State) -> Result<(Vec<f32>, State), String> {
        let names: Vec<String> = self.decoder.inputs().iter().map(|i| i.name().to_string()).collect();
        let find = |key: &str| {
            names.iter().find(|n| n.contains(key)).cloned().ok_or(format!("Decoder input {key} not found"))
        };
        let (enc_name, tgt_name, len_name) = (find("encoder_outputs")?, find("targets")?, find("length")?);
        let state_names: Vec<&String> = names.iter().filter(|n| n.contains("state")).collect();
        let target_len: DynValue = if self.target_length_i32 {
            Tensor::<i32>::from_array(([1usize], vec![1i32].into_boxed_slice()))
                .map_err(|e| run_err("tensor creation", e))?
                .into_dyn()
        } else {
            len_tensor(1)?.into_dyn()
        };
        let targets = Tensor::<i32>::from_array(([1usize, 1usize], vec![last as i32].into_boxed_slice()))
            .map_err(|e| run_err("tensor creation", e))?;
        let out = self
            .decoder
            .run(ort::inputs![
                enc_name.as_str() => f32_tensor(vec![1, frame.len(), 1], frame.to_vec())?,
                tgt_name.as_str() => targets,
                len_name.as_str() => target_len,
                state_names[0].as_str() => f32_tensor(self.state_dims[0].clone(), state.0.clone())?,
                state_names[1].as_str() => f32_tensor(self.state_dims[1].clone(), state.1.clone())?,
            ])
            .map_err(|e| run_err("decoder", e))?;
        let take = |name: &str| -> Result<Vec<f32>, String> {
            let value = out.get(name).ok_or(format!("Decoder output {name} not found"))?;
            Ok(value.try_extract_tensor::<f32>().map_err(|e| run_err("decoder output", e))?.1.to_vec())
        };
        Ok((take("outputs")?, (take("output_states_1")?, take("output_states_2")?)))
    }

    pub fn transcribe(&mut self, samples: &[f32], sample_rate: u32) -> Result<ParakeetOutput, String> {
        let start = Instant::now();
        let duration_sec = if sample_rate > 0 { samples.len() as f64 / sample_rate as f64 } else { 0.0 };
        let mut wav = Vec::new();
        if sample_rate == SAMPLE_RATE {
            wav.extend_from_slice(samples);
        } else {
            resample_linear(samples, sample_rate, SAMPLE_RATE, &mut wav);
        }
        let rms = (wav.iter().map(|s| s * s).sum::<f32>() / wav.len().max(1) as f32).sqrt();
        let mut text = String::new();
        // Shorter than one encoder frame, or digital silence: nothing to decode.
        if wav.len() >= 1600 && rms >= 1e-5 {
            let (rows, frames, dim) = self.encode(&wav)?;
            let zeros = (
                vec![0.0f32; self.state_dims[0].iter().product()],
                vec![0.0f32; self.state_dims[1].iter().product()],
            );
            let (vocab_size, blank) = (self.vocab.len(), self.vocab.blank);
            let ids = tdt_greedy(frames, blank, vocab_size, zeros, |t, last, state| {
                self.decode_frame(&rows[t * dim..(t + 1) * dim], last, state)
            })?;
            text = self.vocab.detokenize(&ids);
        }
        let no_speech_prob = if text.is_empty() { 1.0 } else { 0.0 };
        Ok(ParakeetOutput { text, duration_sec, inference_sec: start.elapsed().as_secs_f64(), no_speech_prob })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLANK: usize = 3;
    const VOCAB: usize = 4; // ids 0..=3, blank is 3
    const MAX_DUR: usize = 5;

    /// Builds a flat joint output: token logits followed by duration logits.
    fn out(token: usize, step: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; VOCAB + MAX_DUR];
        v[token] = 1.0;
        v[VOCAB + step] = 1.0;
        v
    }

    /// Runs the loop with a scripted sequence of outputs; state counts committed tokens.
    fn run(frames: usize, script: Vec<Vec<f32>>) -> (Vec<usize>, Vec<(usize, usize, usize)>) {
        let mut calls = Vec::new();
        let mut it = script.into_iter();
        let tokens = tdt_greedy(frames, BLANK, VOCAB, 0usize, |t, last, state: &usize| {
            calls.push((t, last, *state));
            let o = it.next().expect("script exhausted");
            Ok((o, state + 1))
        })
        .unwrap();
        (tokens, calls)
    }

    #[test]
    fn duration_skips_advance_frames() {
        // frame 0: token 1 with step 2 -> jump to frame 2; frame 2: blank step 1 -> frame 3 (end).
        let (tokens, calls) = run(3, vec![out(1, 2), out(BLANK, 1)]);
        assert_eq!(tokens, vec![1]);
        assert_eq!(calls.iter().map(|c| c.0).collect::<Vec<_>>(), vec![0, 2]);
    }

    #[test]
    fn blank_does_not_commit_state_and_advances_one_frame() {
        // blank step 0 -> advance by one; state must still be 0 on the next call.
        let (tokens, calls) = run(2, vec![out(BLANK, 0), out(BLANK, 0)]);
        assert!(tokens.is_empty());
        assert_eq!(calls, vec![(0, BLANK, 0), (1, BLANK, 0)]);
    }

    #[test]
    fn emitted_token_commits_state_and_becomes_last_token() {
        // token 2 with step 0 stays on frame 0; the next call sees last=2 and state=1.
        let (tokens, calls) = run(1, vec![out(2, 0), out(BLANK, 0)]);
        assert_eq!(tokens, vec![2]);
        assert_eq!(calls, vec![(0, BLANK, 0), (0, 2, 1)]);
    }

    #[test]
    fn at_most_ten_tokens_per_frame() {
        let script = (0..10).map(|_| out(1, 0)).collect();
        let (tokens, calls) = run(1, script);
        assert_eq!(tokens.len(), 10);
        assert_eq!(calls.len(), 10, "the 10th emission forces the frame step");
    }

    #[test]
    fn terminates_with_no_frames() {
        let (tokens, calls) = run(0, vec![]);
        assert!(tokens.is_empty() && calls.is_empty());
    }

    #[test]
    fn step_error_is_propagated() {
        let r = tdt_greedy(1, BLANK, VOCAB, 0usize, |_, _, _: &usize| Err("boom".to_string()));
        assert_eq!(r, Err("boom".to_string()));
    }

    #[test]
    fn vocab_parses_and_finds_blank() {
        let v = parse_vocab("▁Hello 0\nworld 1\n<unk> 2\n<blk> 3\n").unwrap();
        assert_eq!(v.len(), 4);
        assert_eq!(v.blank, 3);
    }

    #[test]
    fn vocab_without_blank_is_rejected() {
        assert!(parse_vocab("a 0\nb 1\n").is_err());
    }

    #[test]
    fn detokenize_joins_and_trims() {
        let v = parse_vocab("▁Hola 0\n▁mun 1\ndo 2\n<blk> 3\n").unwrap();
        assert_eq!(v.detokenize(&[0, 1, 2]), "Hola mundo");
        assert_eq!(v.detokenize(&[]), "");
        assert_eq!(v.detokenize(&[3]), "", "blank and out-of-range ids are skipped");
        assert_eq!(v.detokenize(&[99]), "");
    }

    #[test]
    fn only_a_gpu_device_selects_the_fp32_encoder() {
        assert!(!uses_gpu_encoder("cpu"));
        assert!(!uses_gpu_encoder(""));
        assert!(uses_gpu_encoder("cuda"));
        assert!(uses_gpu_encoder("directml"));
        assert_eq!(encoder_compute_type("cpu"), "int8");
        assert_eq!(encoder_compute_type("cuda"), "fp32");
        assert_eq!(encoder_compute_type("directml"), "fp32");
    }

    #[test]
    fn gpu_failure_falls_back_to_cpu_only_when_allowed() {
        assert_eq!(gpu_failure_fallback("cuda", true), Some("cpu"));
        assert_eq!(gpu_failure_fallback("directml", true), Some("cpu"));
        assert_eq!(gpu_failure_fallback("cuda", false), None);
        assert_eq!(gpu_failure_fallback("cpu", true), None, "a CPU failure has nowhere to fall back to");
    }

    #[test]
    fn argmax_picks_first_maximum() {
        assert_eq!(argmax(&[0.1, 0.9, 0.9, 0.2]), 1);
    }
}
