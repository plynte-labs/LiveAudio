# OBS overlay review fixes

Locator: `odd/tasks/obs-overlay-review-fixes.md` · Engram mirror: `odd/obs-overlay-review-fixes/tasks`

## Objective
Fix the verified OBS word-gap bug and the small inconsistencies found in the 2026-11-07 review of the uncommitted Rust migration work.

## Problem / why
- OBS shows huge gaps between subtitle words. Root cause (reproduced in headless Edge with the user's theme): `assets/subtitulos_obs.html` `.style-karaoke .word` has base `transform: scale(0.5)` that only `popIn` resets; `html[data-anim="fade"] .word` swaps the animation for `subFade` (opacity only), so words stay at half scale while keeping full layout width. Same for `.style-rgb .word` (`translateY(10px)` stays). The desktop preview uses its own CSS copy (`desktop/src/styles.css:934`) without the base transform, so it looks fine.
- The previous "fix" (margin removal, justify guard, word-spacing token) is present in the served file but did not address this cause.

## Scope (authorized)
- T1 overlay fade fix + regression test.
- T2 mechanical consistency fixes: `desktop/package.json` version, `desktop/src/app.js:94` mock version, stale Faster-Whisper comment in `crates/liveaudio-core/src/asr/traits.rs:15`, restore `--locked` in `.github/workflows/ci.yml`.

## Out of scope / needs user decision
- Updater pubkey placeholder (`tauri.conf.json:61`) — needs the user's real Minisign key.
- Policy for vendored binaries (DirectML.dll, onnxruntime.dll, reference wav).
- Excluding `desktop/src-tauri/gen/schemas/*` at commit time.
- ORT version split (Windows 1.24.4-directml vs Linux 1.26.0).
- Removing uv/Python shims in `liveaudio-core` (`setup_managed_runtime` still used by CLI `setup-runtime`, main.rs:691).
- Local release sidecar staging for `liveaudio-asr-worker`.

## Constraints
- TDD: strict (source: global user config). Runners: `node --test tests-js/*.mjs`, `node desktop/test_frontend.js`, `cargo test -p <crate>`.
- No commits: `AGENTS.md:20` requires explicit auditor approval. Work-unit commits stay pending.
- RDD: not checked/managed this task.

## Tasks
- [x] T1 — Overlay: words must render at scale 1 under `data-anim="fade"` for karaoke and rgb. RED test first in `tests-js/`, then fix in `assets/subtitulos_obs.html`. Route: delegated writer (switched fable -> sonnet at user request). Check: node tests green.
- [x] T2 — Consistency: package.json 1.6.0, app.js mock 1.6.0, traits.rs comment, CI `--locked` (verify `cargo metadata --locked` passes). Route: delegated writer (switched fable -> sonnet at user request). Check: node + `cargo check -p liveaudio-core --locked`.

- [x] T3 — WAV fixtures out of the repo: `*.wav` added to `.gitignore` (local file kept; no code references it). Route: inline (one mechanical file). Check: `git check-ignore -v assets/fixtures/reference_speech.wav` -> `.gitignore:115:*.wav`. The tracked `tests/reference_speech.wav` is already deleted in the worktree.
- [x] T4 — Remove uv/Python leftovers (user-authorized 2026-11-07): `runtime.rs` shims (`managed_python`, `discover_managed_python`, `discover_uv_in`, `find_uv_executable`, `provision_runtime_with_uv`, `setup_managed_runtime`?, `RuntimeError::UvNotFound/UvFailed`), re-exports in `liveaudio-core/src/lib.rs`, CLI `setup-runtime` usage (`liveaudio-cli/src/main.rs:691`), comments citing deleted Python files. Route: delegated writer (sonnet; 4+ files). Check: `cargo test --workspace`, `cargo clippy --workspace`.
- [x] R1 (done: ORT is MIT; DirectML.dll 1.15.4 is under Microsoft Software License Terms, redistributable inside apps, license text missing from bundle and THIRD_PARTY_NOTICES.md; last ORT DirectML build on NuGet is 1.24.4, DirectML in maintenance mode; ort rc.13 api-17 loads any ORT >= 1.17; ci.yml:64 asserts 1.26.0 so Windows CI breaks; DirectML.dll untracked so clean clones break) — Research (read-only): redistribution licenses of vendored DLLs (onnxruntime, DirectML) and ORT version alignment Windows 1.24.4-directml vs Linux 1.26.0 with `ort` 2.0.0-rc.13 `api-17`. Route: delegated research (haiku).

- [x] T5 — Align ONNX Runtime to one version on Windows and Linux (user decision 2026-11-07). Target version chosen from R1 evidence (DirectML build availability). Route: delegated writer (sonnet) after R1. Check: README/VERSION_NUMBER match, CI asserts parity, `cargo test --workspace`.

  - User decision 2026-11-07: do NOT commit ORT/DirectML binaries; CI/release downloads them from official sources with pinned SHA-256; both platforms on ORT 1.24.4 (only DirectML build available); ship DirectML license text.
- [x] T6 — Updater public key set in `desktop/src-tauri/tauri.conf.json` `plugins.updater.pubkey` (from user's `~/.tauri/liveaudio.key.pub`). Route: inline. Private key never read.

## Pending user actions
- Updater key: user generates the Minisign key pair with the Tauri signer, puts the `.pub` content in `tauri.conf.json` `plugins.updater.pubkey`, and stores the private key + password as GitHub secrets `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (already read by `release.yml:83-84`).

## Acceptance criteria
- With anim=fade, computed transform of `.word` is `none` (karaoke, rgb); fade still animates opacity; pop/none unchanged.
- All existing JS suites pass.

## Progress / evidence
- T1: fade rule `transform: none` present (specificity (0,2,1) beats (0,2,0); no later `.word` override, only `.hiding` rules which set animation). RED observed: removing the line fails `resets karaoke/rgb base transform under data-anim="fade"`; GREEN after restore: `node --test tests-js/*.mjs` pass 130 / fail 0.
- T2: package.json 1.6.0, app.js mock 1.6.0, traits.rs comment (native ONNX Runtime ASR worker sidecar), ci.yml `cargo test --workspace --locked`. `cargo check -p liveaudio-core --locked`: Finished OK. `node desktop/test_frontend.js`: 568 passed, 0 failed.
- T4: removed from `runtime.rs` the `AsrBackend` enum, `managed_python`, `discover_managed_python/worker`, `discover_uv_in`, `find_uv_executable`, `setup_managed_runtime`, `provision_runtime_with_uv(_cancellable)`, `managed_runtime_root` and `RuntimeError::{UnsupportedBackend,UvNotFound,PackageSourceMissing,UvFailed,Cancelled}`; trimmed `lib.rs` re-exports; removed CLI `setup-runtime` (it only checked that the worker exists and created an empty dir) with its parser/tests/help; removed `get_torch_home` + `TORCH_HOME` env. Config: no Python-only fields exist (`device` cpu|cuda is still used), so nothing changed. Docs: CONTRIBUTING, GETTING_STARTED, WEBSOCKET_OBS, bug_report template, CHANGELOG line. Evidence: `cargo build --workspace` OK; `cargo test --workspace` 394 passed / 0 failed; `node --test tests-js/*.mjs` 130/0; `node desktop/test_frontend.js` 568/0. `cargo clippy --workspace --all-targets -- -D warnings` fails on pre-existing unrelated lints (field_reassign_with_default in core tests/config, items-after-test-module in emitter.rs, 14 errors in liveaudio-asr-worker). Kept: tests using `python` mock workers (supervisor_mock_worker.rs, job_object_benchmark.rs), hooks.nsh legacy cleanup, docs/PACKAGING_AND_UPDATES.md and ARCHITECTURE_DECISIONS.md (historical, need a dedicated rewrite).
- T5: `packaging/fetch_onnxruntime.mjs` (dependency-free; in-process zip/tgz readers) downloads pinned archives, aborts on SHA-256 mismatch, idempotent via `.pins` stamp. Pins: Microsoft.ML.OnnxRuntime.DirectML 1.24.4 nupkg 57e9f11b73437bef7a309496135d4c1f96b1a8e9ddba60013fa27bfc1d788681; Microsoft.AI.DirectML 1.15.4 nupkg 4e7cb7ddce8cf837a7a75dc029209b520ca0101470fcdf275c1f49736a3615b9; onnxruntime-linux-x64-1.24.4.tgz 3a211fbea252c1e66290658f1b735b772056149f28321e71c308942cdb54b747. VERSION_NUMBER = `1.24.4` on both platforms (CI asserts it). Linux .so renamed to `libonnxruntime.so.1.24.4` (runtime.rs, asr-worker model.rs, tauri.linux.conf.json, CI, release). DirectML license ships as `DirectML-LICENSE.txt` (tauri.windows.conf.json resource, release staging check, THIRD_PARTY_NOTICES.md). Finding: the previously local Windows binaries were NOT the official 1.24.4 nupkg bytes (DirectML.dll 18527768 vs official 18527776 B; onnxruntime.dll 21111840 vs 17328152 B; providers_shared differs); LICENSE and ThirdPartyNotices matched. Local dir now holds the official files. Tests: RED (module missing) then `node --test tests-js/*.mjs` pass 135 / fail 0; fetch run twice (2nd "up to date"); Linux fetch verified into scratchpad; `cargo test --workspace` see report.
- **At commit time (human):** `git rm --cached` these tracked binaries/notices (now gitignored): `packaging/onnxruntime/linux-x64/{LICENSE,ThirdPartyNotices.txt,VERSION_NUMBER,libonnxruntime.so.1.26.0,libonnxruntime_providers_shared.so}` and `packaging/onnxruntime/windows-x64/{LICENSE,ThirdPartyNotices.txt,VERSION_NUMBER,onnxruntime.dll,onnxruntime_providers_shared.dll}`. Keep `packaging/onnxruntime/README.md` and `packaging/fetch_onnxruntime.mjs` tracked. The stale local `linux-x64/libonnxruntime.so.1.26.0` can be deleted.
- Commit: pending (AGENTS.md approval).

## Next step
User verifies in OBS (restart liveaudio-desktop or just reload the OBS browser source; overlay is read from assets/ on every request with no-store).
