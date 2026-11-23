# CT2 worker release cycle and NVIDIA offer fix

Branch: `feat/ct2-worker-release-cycle` (from `develop` 0f4cc63). Worktree: `E:\LiveAudio-worktrees\ct2-pin`.

## Objective

Release the CUDA (CTranslate2) worker on its own cycle so app releases do not rebuild, upload and re-pin it. Also stop the NVIDIA acceleration offer from looping when no worker is published.

## Problem

- The test at `crates/liveaudio-core/src/nvidia_runtime.rs` (around line 493) requires `pin.version == CARGO_PKG_VERSION` (the app version). The pin URL is `releases/download/v{app}/liveaudio-asr-worker-cuda.exe`, and `packaging/build_ct2_worker.ps1 -Pin` writes the app version. Every app release therefore needs a CUDA rebuild and upload, even though the worker crate (`crates/liveaudio-asr-worker-ct2`, still version 1.6.0) has changed only once (c190b98).
- On a build with the placeholder pin, "Turn on acceleration" downloads about 1.2 GB of NVIDIA libraries and reports success. Status still says `installed: false` (worker not published), so the offer reappears (`nvidiaCardView` and `nvidiaOfferVisible` in `desktop/src/runtime-settings.js`). It also saves `device: cuda` for nothing.

## Scope

- T1: decouple the pin from the app version.
  - The pin version equals the `liveaudio-asr-worker-ct2` crate version.
  - The URL tag is `ct2-worker-v{version}`.
  - Update `build_ct2_worker.ps1` (version read from the ct2 crate's `Cargo.toml`, and the new URL), the pin test, `packaging/ct2-worker.pin.json` (placeholder, version 1.6.0, new URL), and the docs (`docs/PACKAGING_AND_UPDATES.md` and any release notes or steps that mention re-pinning per release).
  - Verify the app rejects a worker that speaks a different protocol version. If the supervisor or CT2 backend does not already check `PROTOCOL_VERSION` at handshake, add the smallest check that fails clearly and falls back to the ORT engine.
- T2: fix the NVIDIA offer on unpublished builds.
  - Expose `worker_published` in the GPU backend status.
  - `nvidiaOfferVisible` is false when it is false.
  - `install_nvidia_runtime` returns an error before any download when the worker is unpublished.
  - The UI shows the existing `nvidia_card_unavailable` text for that error.

## Out of scope

- Building the CT2 worker in CI.
- Actually publishing a `ct2-worker-v1.6.0` release. That is a user-owned step.

## Constraints

- TDD strict: write the test, observe it fail (RED), then make it pass (GREEN).
- Runners: `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets`, `node desktop/test_frontend.js`, `node --test tests-js/*.mjs`.
- Generated artifacts are in English. Never commit `gen/schemas`, ORT binaries, `dist`, or `*.wav`.
- Make no commits without user approval (project `CLAUDE.md`). The user authorized implementation on this branch. Work-unit commits on the feature branch follow ODD.

## Tasks

- [x] T1 Decouple the CT2 worker pin from the app version, with a protocol-version guard. Route: delegated writer (3+ files: Rust, PowerShell, JSON, docs).
- [x] T2 Hide the NVIDIA offer and refuse to install when the worker is unpublished. Route: delegated writer (Rust and JS, 3+ files).
- [x] T3 Fix adversarial review findings (platform-check order, protocol guard hardening and shared PROTOCOL_VERSION, structured unpublished code, CI pin verification, docs and control-char fixes). Route: delegated writer.

## Acceptance

- A future app version bump does not fail the pin test while the worker crate version is unchanged.
- On a placeholder-pin build, the offer is hidden, and calling install fails fast without downloading.
- All four test commands pass. Clippy has no new warnings.

## Delivery

Strategy: `ask-on-risk`. Forecast: about 250 authored lines.

## Progress / evidence

- T1 done, commit 8866a0e, route delegated writer. Pin version now tracks `crates/liveaudio-asr-worker-ct2` (1.6.0), tag `ct2-worker-v1.6.0`; `build_ct2_worker.ps1 Write-Pin`, `ct2-worker.pin.json`, docs and CHANGELOG updated. Protocol guard: the supervisor did NOT check it (`PROTOCOL_VERSION` is unused by liveaudio-core; process.rs only hardcoded `"version": 1` in outgoing commands). Added `protocol_mismatch()` in `supervisor/process.rs`: a `ready` event with a version other than 1 is not accepted and the child is killed, so the handshake fails (`Child exited prematurely`) and the start falls back to ORT (`ct2_start_failed`). A missing version counts as 1. RED: `tracked_pin_is_placeholder_or_valid_release_pin` failed (left "1.6.5", right "1.6.0"); `ready_with_foreign_protocol_version_is_rejected` failed to compile (no `protocol_mismatch`). GREEN: both pass.
- T2 done, route delegated writer. `DownloadError::WorkerUnpublished` (text starts with `worker_not_published`), `nvidia_runtime::worker_published()` (exported as `nvidia_worker_published`), `install_nvidia_runtime_with(dir, published, ..)` refuses first. `GpuBackendStatusDto.worker_published`; `nvidiaOfferVisible` requires it; `nvidiaResultText` matches `/HTTP status 404|worker_not_published/`; mock and fixtures updated. RED: `install_refuses_unpublished_worker_before_any_download` (compile error, missing fn); test_frontend: 3 FAIL (offer visibility, unpublished refusal text, hidden offer). GREEN: all pass.
- Verification: cargo test --workspace --locked 557 passed, 0 failed; node desktop/test_frontend.js 920 passed, 0 failed; node --test tests-js/*.mjs 170 pass, 0 fail; clippy: only pre-existing warnings (no new ones in touched code observed).
- T3 done, route delegated writer. (1) `install_nvidia_runtime_with` checks the platform first; the unpublished test is windows-only. (3) rejected `ready` is not forwarded; its own error "worker protocol version mismatch: expected 1, got X" propagates through the existing start-failure paths, so the CT2 to ORT fallback in desktop commands.rs and the CLI still happens. (4) strict `protocol_mismatch`. (5) `liveaudio-worker-protocol` added to liveaudio-core, `PROTOCOL_VERSION` replaces literal 1; mock-worker integration test. (6) `packaging/verify_ct2_pin.mjs` + release.yml step. (2,7,8) docs: `--latest=false`, immutability rule, backspace byte fixed. (9) a 404 on the worker asset maps to `WorkerUnpublished` (code `worker_not_published`); UI matches that code only, NVIDIA 404s show the generic error. RED: `ready_with_foreign_protocol_version_is_rejected` (malformed versions), `test_worker_with_foreign_protocol_version_fails_start_and_is_not_forwarded` ("Child exited prematurely" instead of mismatch), `missing_worker_asset_fails_before_large_archives` (Status(404) not WorkerUnpublished). Commit is the T3 commit on this branch (see git log). Finding 1 RED could not be observed (non-Windows only); fixed by cfg reasoning.

## Next step

Publish the worker once as release `ct2-worker-v1.6.0` (`build_ct2_worker.ps1 -Pin`, then upload). This is user-owned.
