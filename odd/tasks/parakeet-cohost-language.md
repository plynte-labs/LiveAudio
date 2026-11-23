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

### UI round (user feedback, 2026-11-23)
- [x] T4 Engine section (7fb53ad). Rename "Whisper Model Size" to a generic "Model", with options grouped Whisper / Parakeet. When Parakeet is selected, disable "Compute device" and "Transcription engine", and make the engine status say it runs on CPU (int8). Today the screen shows CUDA/DirectML, which is wrong for Parakeet.
- [x] T5 OpenCohost slider (d850c52). Rewrite the hint to be accurate: it works whether or not OpenCohost launched LiveAudio; the CLI reads once at startup; the desktop reads on each phrase; the env var decides which file. Add a status line under the slider showing the detected language and file path, or "OpenCohost not found". This needs a small Tauri command that reuses the core resolver.
- [x] T6 Full Advanced tab redesign (4e5ae72). Regroup into fewer, clearly titled sections with icons and progressive disclosure, so the tab feels less dense. No settings removed; behavior unchanged.
- [x] T7 Main window (this commit; see git log) (from the annotated screenshot):
  - Make the header "vX · Info & stats" button larger and give it an icon.
  - Add icons to the section titles "Live Subtitles Feed", "OBS Studio Integration", and "Event Log".
  - Remove the header "Stopped" status pill and the feed's "Idle" pill.
- [x] T8 (this commit; see git log) Parakeet on GPU, downloaded on demand. When the effective device is a GPU (CUDA, then DirectML, the same resolution Whisper uses), download and load the full-precision encoder on the GPU, and keep the decoder_joint and preprocessor on CPU. With CPU or no GPU, use only the int8 set (about 0.7 GB). If GPU session creation fails, fall back to CPU int8. Nothing downloads until Parakeet is selected and started. UI: re-enable "Compute device" for Parakeet, keep "Transcription engine" disabled, and show the real effective device in the status pill. Benchmark GPU against CPU on the fixtures.
- Benchmark (measurement only, scratchpad `bench-parakeet/summary.md`). On CPU, Parakeet is 2–2.5x faster than Whisper small (es_media p50 1390 vs 2877 ms). WER is tied on short clips and Whisper is slightly better on long ones (0.270 vs 0.243 and 0.080 vs 0.067; tiny corpus). Parakeet's cold start is 7.6 s vs 2.9 s, and it is worse on the English clip.

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

- T4 done (7fb53ad). `isParakeetModel` in runtime-settings.js; `syncEngineControls` in app.js disables device and engine selectors and shows `hint-parakeet-engine`; engine status reads "Runs on CPU (int8)". Model label generic, optgroups Whisper/Parakeet. RED: 9 frontend asserts; GREEN: test_frontend.
- T5 done (d850c52). Core `LocaleStatus` + `locale_status` (resolver now also returns the checked path; `resolve_language` reuses it), Tauri command `opencohost_language_status` (no capability entry needed for app commands), status line under the slider on the Home tab, refreshed on toggle, tab open, config load and language switch. RED: Rust compile error + 7 frontend asserts; GREEN: core 7 opencohost tests, test_frontend.
- T6 done (4e5ae72). Advanced tab: 6 icon sections (Engine, Voice detection, Live subtitles, Vocabulary, OBS and integrations, Updates), each with a one-line description; four collapsed details (technical details and maintenance, fine tuning, delay and backlog, access and permissions). Allowed connections folded into the network section. All pre-existing IDs kept. RED: 11 asserts; GREEN: test_frontend.
- T7 done. Header Info button larger with icon; icons on the feed, OBS and log titles; the header status pill and the feed Idle pill removed (markup, JS, CSS, feed_badge_* copy). Start/Stop state announced via `role="status"` on the master action bar. status_* i18n keys are left in place (still referenced by tests and unused by the UI). RED: 7 asserts; GREEN: test_frontend 914.
- Verification (after T7): cargo test --workspace --locked 552 passed; clippy no new warnings in touched code; tests-js 170 pass; test_frontend 914 pass; cargo build -p liveaudio-desktop OK.

- T8 done (single writer). HF API at the pinned revision has no fp16 encoder, only `encoder-model.onnx` (41 MB graph) plus `encoder-model.onnx.data` (about 2.4 GB external weights), so the GPU set is fp32: `parakeet_files(gpu)` in download.rs (GPU: fp32 encoder + `.data` + int8 decoder_joint + nemo128 + vocab + config; CPU: unchanged int8 set, no GPU file). The decoder_joint output dims/dtypes are the same with either encoder (fp32 [1,1024,T']), so the int8 decoder stays on CPU. Device resolved with `resolve_effective_device` (same as Whisper); encoder loaded through `make_session_builder(device)`; preprocessor and decoder stay on `cpu`. Pure helpers `uses_gpu_encoder`, `encoder_compute_type`, `gpu_failure_fallback`; on GPU prepare/load failure (and `auto_cpu_fallback`) `load_parakeet` warns, downloads int8 and retries on CPU, reporting `fallback_occurred`/`fallback_reason`. ReadyInfo reports `directml|cuda`/`fp32` or `cpu`/`int8`. UI: Compute device enabled for Parakeet, engine select still disabled, pill follows the device (`engine_parakeet_cpu` removed), hint EN/ES updated. Core already passes the device through (`effective_backend_pref` only forces ort). RED: compile errors for the missing helpers/`parakeet_files` (Rust), 6 frontend asserts; GREEN: cargo test --workspace 555 pass, tests-js 170, test_frontend 917. Benchmark: see the commit message / report.

## Next step
User tests Parakeet in the real app (select it in Advanced > Engine). Push and PR remain the user's decision.
- Release prep: version bumped to 1.6.5 (4b4e3d7). The CT2 worker pin is set to the documented all-zero placeholder for 1.6.5, so the app reports the CUDA worker as not published until it is re-pinned. At release, run `packaging/build_ct2_worker.ps1 -CudaRoot <root> -Pin` before tagging (docs/PACKAGING_AND_UPDATES.md).
