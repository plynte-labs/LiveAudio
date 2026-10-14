# Rust Migration Parity Fixes

## Objective
Fix verified defects and port must-have Python behaviors missing from the Rust/Tauri v1.5.0 migration, without porting features the owner deferred by design.

## Problem and why
A read-only audit (2026-10-14; 2 bug-hunt + 2 parity agents, parent-verified key findings) showed that passing suites (cargo 202, frontend 293) do not cover several broken or unwired behaviors: a supervisor that can wedge forever, a dead OBS overlay URL in installed builds, non-sequential VTT cues, merged audio devices, unenforced decode deadline, ignored `cpu_threads`, no single-instance lock, no audio reconnect/standby/watchdog, and no Linux package.

## Scope decisions (owner)
- Deferred by design, out of scope: hot-swap engine (backlog), updater, welcome/first-run screen, crash dialog, output-folder picker, other UI extras.
- Must port now: audio auto-reconnect (3 s retry), standby with hot-plug detection, capture watchdog.
- Intentional, keep: boundary-overlap trimming (`a4dffe8`), Silero v6 ONNX (`d5129dc`), 300 ms minimum utterance.
- Treated as bugs unless owner objects: bidi/control-char sanitization, 600-char presentation cap, `no_speech_prob` segment filter.
- Linux is a target (`d5129dc`).
- OpenCohost integration (tauri.localhost origin, `--autostart-service`) is stashed and out of scope.

## Constraints
- Branch: `develop` (owner-designated working branch). Work-unit commit per task; no push/PR/merge without owner request. No AI attribution in commits (owner global rule).
- Keep the hybrid ASR boundary: Python worker protocol stays compatible; worker edits only if required and minimal.
- Minimalism ladder (project CLAUDE.md). Implementation delegation uses `model: fable` (project policy).
- No network downloads, microphone, GPU, or GUI runs during verification. Unverified agent findings must be confirmed in code before fixing.

## Checks
- TDD: enabled, source: user global CLAUDE.md "Strict TDD Mode: enabled". RED → GREEN → REFACTOR.
- Runners: `cargo test --workspace --offline --locked`; targeted `cargo test -p <crate> --offline --locked <filter>`; `node test_frontend.js` from `desktop/` on frontend changes; `cargo fmt --check` after normalization.
- RDD: off (global), verified via `gentle-ai review mode status`. No native review.
- Delivery: direct work-unit commits on `develop`; forecast ~1,500-2,500 authored lines total (heuristic only). PR slicing deferred to owner.

## Tasks
- [x] T1 — Supervisor and lifecycle (route: delegated, writer trigger)
  - Failed respawn retries/escalates instead of waiting on a consumed exit notify (`crates/liveaudio-core/src/supervisor/process.rs` respawn `Err` arm); reaches Failed at the limit.
  - Restart ceiling matches Python: fatal on the 3rd failure within window (`respawn.rs`).
  - WS bind failure in `start_service_locked` resets state and emits `service_status_changed`.
  - `stop_service_locked` emits status on cleanup failure and does not report "stopped" while resources may be live.
  - Graceful service stop on app exit/window close (Tauri `RunEvent::ExitRequested`).
  - Single-instance lock for the desktop app (prefer existing deps/stdlib; Tauri single-instance plugin only if justified).
- [ ] T2 — Audio identity and RNNoise robustness (route: delegated)
  - `normalize_device_name` no longer merges distinct devices that differ only in parenthesized text.
  - RNNoise clamps out-of-range input/output instead of failing capture.
- [ ] T3 — Audio resilience parity (route: delegated)
  - Auto-reconnect on device loss (3 s retry), standby when no input device with hot-plug detection, capture watchdog; reuse Python semantics from `liveaudio/core/audio.py`.
- [ ] T4 — ASR request and transcript output (route: delegated)
  - Enforce `asr_decode_timeout_sec` (forward `timeout_ms` and/or Rust-side deadline).
  - Forward `cpu_threads` in the worker init payload.
  - VTT cues numbered sequentially; cue timing from audio timeline, not emit time.
  - Text sanitization (bidi/control), 600-char presentation cap, `no_speech_prob` filter parity with `engine.py`.
- [ ] T5 — Distribution (route: delegated)
  - OBS overlay resolved from `resource_dir()` first; URL percent-encoded; existence checked.
  - `uv` resolution works on Linux (no hard-coded `uv.exe`); Linux bundle config/targets.
- [ ] T6 — Minor fixes (route: delegated)
  - `get_recent_logs` arg `maxLines`.
  - Per-key tolerant config load (wrong type falls back to default for that key).
  - No `println!` in library code that pollutes CLI JSON stdout.
  - CLI honors configured audio device; `devices` lists real devices; `doctor` reports only tested facts.

## Progress and evidence
- Audit evidence: Engram `audit/rust-migration-completeness`; scope decisions: `audit/rust-migration-scope-decisions`, `audit/rust-migration-scope-decisions-audio`.

- T1 (delegated writer, model fable; writer trigger: 7 non-trivial files). All 6 items confirmed in code and fixed.
  - RED observed: failed_respawn (Elapsed, stuck degraded), max_failures/respawn_policy (old ceiling), websocket_bind_failure ("starting" vs "stopped"), failed_shutdown ("stopped" on failed cleanup), instance_lock (compile-only RED, new primitive). GREEN on each after fix.
  - Writer checks: liveaudio-core all suites ok; liveaudio-desktop 47 passed; workspace 0 failed; `cargo fmt -p liveaudio-core --check` clean. Desktop rustfmt diffs at commands.rs/tests.rs pre-exist on HEAD (not introduced).
  - Parent spot check: `cargo test -p liveaudio-core --offline --locked --test supervisor_mock_worker`: 7 passed, 0 failed. Lock verified not to collide with the Python worker (InstanceLock only taken in `liveaudio/service/__init__.py:main`).
  - Not exercised: RunEvent::ExitRequested graceful stop (needs GUI runtime). Second instance exits with a log line, no window focus (single-instance plugin not available offline). CLI takes no instance lock.
  - Review assessment: risk medium (executable_change), RDD off → writer self-verification + parent spot check.

## Next step
T2 audio identity and RNNoise robustness.
