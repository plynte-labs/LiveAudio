# ASR benchmark: v1.5.1 vs v1.6.0

Locator: `odd/tasks/asr-benchmark-v15-v16.md` · Engram mirror: `odd/asr-benchmark-v15-v16/tasks`

## Objective
Measure, with the same audio and model, whether v1.6.0 (native ONNX Runtime worker, DirectML/CPU) is better or worse than v1.5.1 (Python faster-whisper / CTranslate2, CUDA/CPU) in latency, accuracy and memory.

## Why
Earlier v1.6 numbers came from different clips, possibly different models, and non-official ORT DLLs. Accuracy (WER) was never measured for v1.6.

## Inputs
- Clips (local only, `assets/fixtures/` is gitignored): `es_cortacreo` (6.4 s), `es_cortacreo2` (8.3 s), `es_media` (20.7 s), `es_larga` (36.4 s, > 30 s Whisper window), `reference_speech` (7.2 s, English, v1.5 baseline). Spanish clips are 48 kHz stereo 24-bit with noise, clipping and filler words; each has a `.txt` reference (es_larga reference may be truncated).
- User config: model `small`, language `es`, device `cuda`.
- v1.5.1 settings (git tag v1.5.1, `liveaudio/core/engine.py:422-424`): beam_size 5, vad_filter False, condition_on_previous_text False; faster-whisper 1.2.1, ctranslate2 4.8.0, torch 2.5.1+cu121.

## Scope
- Benchmark tooling lives outside shipped code: scripts in the session scratchpad or `packaging/bench/` (decide minimal), Python env for v1.5.1 only in scratchpad (never in repo).
- No product code changes. No commits.

## Tasks
- [x] B1 — Harness + measurements: prepare 16 kHz mono inputs, v1.6 worker driver, v1.5.1 faster-whisper baseline env, run serially (never two engines on the GPU at once), compute p50/p95 latency, WER (normalized), RSS/VRAM, cold start. Route: delegated (sonnet).
- [x] B2 — Report + ADR note (appended to docs/ADR-018-optimizacion-memoria-pesos-latencia.untracked.md): results table and verdict. Route: inline after B1.

## Progress / evidence
- B1 done (delegated). Scratchpad bench dir: `C:/Users/tavo_/AppData/Local/Temp/claude/53f5c5738d068eb3/6b1bd665-f93a-465c-a7ae-ae5f91d32f82/scratchpad/bench/` (results/summary.md, summary.json, raw A1_gpu/A2_cpu/B1/B2/B3 json). Configs: A1 v1.6 worker `gpu` -> directml fp16 (no fallback; greedy argmax + 1.2 repetition penalty, no beam), A2 worker cpu int8, B1 v1.5.1 CUDA fp16 beam5, B2 CUDA fp16 beam1, B3 CPU int8 beam5 (faster-whisper 1.2.1, ctranslate2 4.8.0, cublas/cudnn pip wheels, no torch). Warm p50 es_media (20.7 s): A1 1250 ms, B1 545, B2 364; CPU A2 3452 vs B3 3114. WER (normalized) es_media: A1 0.297, A2 0.270, B1 0.243, B3 0.216; es_larga total: A1 0.120, B1 0.133, B2 0.133. Caveats: es_larga split at 17.24 s into 2 chunks, es_larga reference may be truncated (ends with "..."); whole-GPU mem.used used for VRAM (per-process N/A under WDDM); desktop app not running; small corpus, WER on 1-3 clips is noisy.
