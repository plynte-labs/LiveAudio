# Parakeet local model + OpenCohost language sync

Branch: `feat/parakeet-cohost-language` (from `develop` at c5f50f4). Worktree: `E:\LiveAudio-worktrees\parakeet`.

## Objective
1. OpenCohost drives LiveAudio's speech language. When a new toggle is on, LiveAudio reads OpenCohost's Kira locale and uses it as `asr_language`, so Kira in English means LiveAudio transcribes English.
2. Add NVIDIA Parakeet-TDT 0.6B v3 (int8, ONNX) as a second selectable local model in the native ORT worker, to test on real LiveAudio (CPU-only Linux laptop target). It has fewer hallucinations than Whisper and auto-detects ES/EN.

## Problem / why
- OpenCohost spawns `liveaudio-cli serve` (env `OPENCOHOST_DATA_ROOT` is set by its launcher) but never passes a language. LiveAudio starts in its saved language (often `es`) even when Kira speaks English.
- Whisper hallucinates on silence and noise, and is slow on CPU-only machines.

## Facts from exploration
- OpenCohost locale file: `<OPENCOHOST_DATA_ROOT>/config/locale.json`, `{"locale": "es"|"en"|BCP-47, "set_at": ...}`. Without the env var: `%LOCALAPPDATA%\OpenCohostData\config\locale.json`. OpenCohost applies locale only at its own boot, so reading at LiveAudio startup/session start is enough (no file watcher).
- LiveAudio: `asr_language` in `crates/liveaudio-core/src/config/mod.rs` (~251, normalized ~584), patchable in `config/patch.rs`. The desktop reads it at session start in `desktop/src-tauri/src/commands.rs` (~1346, ~1535, `asr_language_and_prompt` ~2407). The CLI uses it in `crates/liveaudio-cli/src/main.rs` (~845, ~1097).
- Toggle pattern to copy: `check-rnnoise-enabled` (`desktop/src/index.html` ~428, `app.js` ~411/572/2050/2190, `i18n.js` ES+EN).
- Worker seam: the `Engine` trait (`crates/liveaudio-worker-protocol/src/worker.rs:42-53`), with `OrtEngine` in `crates/liveaudio-asr-worker/src/main.rs:11-70`. The model name arrives as `InitPayload.model_name`.
- Parakeet export: HF `istupakov/parakeet-tdt-0.6b-v3-onnx` (CC-BY-4.0). Files: `encoder-model.int8.onnx` (652 MB), `decoder_joint-model.int8.onnx`, `nemo128.onnx` (preprocessor), `vocab.txt`, `config.json` (128 mel features, subsampling 8). Reference decode loop: onnx-asr `src/onnx_asr/models/nemo.py` and `src/onnx_asr/asr.py`.

## Scope / tasks
- [x] T1 OpenCohost language sync. Add config bool `follow_opencohost_language` (default false, patchable). Add a pure resolver for the locale path (env, then LOCALAPPDATA) and its value (primary subtag; only `es`/`en` accepted, otherwise ignored with a log). Apply it as an in-memory override of `asr_language`, not persisted, at CLI `serve` startup and desktop session start. Add the desktop toggle (slider) with EN/ES copy. Route: delegated writer (2+ non-trivial files).
- [x] T2 Parakeet engine (8d072cc). Add `parakeet.rs` in the ORT worker: preprocessor, encoder, TDT greedy loop, vocab/detokenize. Add an `AnyEngine` dispatch on `model_name` in `main.rs`, and a download resolver with a pinned revision, reusing the existing SHA-256 verification. Route: delegated writer.
- [x] T3 Parakeet selection wiring (see the T3 commit). Add `parakeet` to `VALID_MODELS`/normalization, the desktop model picker with EN/ES labels, and the model_name mapping. Language is ignored for Parakeet (it auto-detects). Add a CC-BY-4.0 attribution in THIRD_PARTY_NOTICES. Route: delegated writer (with T2).

## Constraints
- Windows behavior unchanged when the toggle is off and Whisper is selected.
- Strict TDD on (global config). Runners: `cargo test --workspace --locked`, `node desktop/test_frontend.js`, `node --test tests-js/*.mjs`.
- Minimalism ladder (CLAUDE.md). Never commit `desktop/src-tauri/gen/schemas/*`, binaries, models, or `*.wav`.
- Delivery: forecast is above 400 authored lines. One local branch, one commit per task, no push or PR until the user decides.

## Checks
`cargo test --workspace --locked`, `cargo clippy --workspace --all-targets`, `node --test tests-js/*.mjs`, `node desktop/test_frontend.js`. For Parakeet: a real-model smoke transcription on Windows CPU if the model download succeeds.

## Progress
- Branch and worktree created. Exploration done.
- T1 done (delegated writer). `opencohost::effective_asr_language`/`language_override` in liveaudio-core; used by the CLI (`serve`, via `language_override`) and the desktop (`effective_asr_language` in session start and `asr_language_and_prompt`). Toggle `follow_opencohost_language` added (config, patch, restart-required, UI EN/ES). RED observed (4 resolver tests, 5 frontend asserts), then GREEN: cargo test --workspace, clippy (no new warnings), node tests-js 170 pass, test_frontend 883 pass.

- T2 done (delegated writer, commit 8d072cc). `parakeet.rs` (preprocessor, encoder, pure `tdt_greedy` loop, vocab), `AnyEngine`-style `OrtEngine` enum dispatch on `model_name`, `prepare_parakeet` in `download.rs` pinned to `8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce` with the shared verified fetch loop. Parakeet always runs on CPU (int8). RED: compile errors for the missing functions; GREEN: worker 43 unit tests. Real ORT names: preprocessor `waveforms`/`waveforms_lens` -> `features`/`features_lens`; encoder `audio_signal`/`length` -> `outputs` [1,1024,T']/`encoded_lengths`; decoder `encoder_outputs` [1,1024,1], `targets` i32 [1,1], `target_length` i32, `input_states_1/2` f32 [2,1,640] -> `outputs` (8198 = 8193 tokens + 5 durations), `prednet_lengths`, `output_states_1/2`. Smoke test `tests/parakeet_smoke.rs` (ignored): the 11 s JFK sample transcribes correctly on Windows CPU (debug build) in 0.54 s.
- T3 done (delegated writer). `parakeet (Multilingüe ES/EN, CPU)` in `VALID_MODELS`, survives normalization; `effective_backend_pref` now also takes `model_size` so Parakeet always uses the ORT worker (never ct2); picker option + EN/ES labels; CC-BY-4.0 notice. RED: backend/config tests failed to compile, frontend assert failed; GREEN: cargo test --workspace 551 pass, tests-js 170, test_frontend 885.

## Next step
User tests Parakeet in the real app (select it in Advanced > Engine). Push and PR remain the user's decision.
