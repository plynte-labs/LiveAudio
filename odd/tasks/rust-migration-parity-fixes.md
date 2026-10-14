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
- Delivery: direct work-unit commits on `develop` (owner instruction, 2026-10-14); forecast ~1,500-2,500 authored lines total (heuristic only). Running count: T1 471. No PRs requested; slicing applies only if the owner opens PRs.

## Tasks
- [x] T1 — Supervisor and lifecycle (route: delegated, writer trigger)
  - Failed respawn retries/escalates instead of waiting on a consumed exit notify (`crates/liveaudio-core/src/supervisor/process.rs` respawn `Err` arm); reaches Failed at the limit.
  - Restart ceiling matches Python: fatal on the 3rd failure within window (`respawn.rs`).
  - WS bind failure in `start_service_locked` resets state and emits `service_status_changed`.
  - `stop_service_locked` emits status on cleanup failure and does not report "stopped" while resources may be live.
  - Graceful service stop on app exit/window close (Tauri `RunEvent::ExitRequested`).
  - Single-instance lock for the desktop app (prefer existing deps/stdlib; Tauri single-instance plugin only if justified).
- [x] T2 — Audio identity and RNNoise robustness (route: delegated)
  - `normalize_device_name` no longer merges distinct devices that differ only in parenthesized text.
  - RNNoise clamps out-of-range input/output instead of failing capture.
- [x] T3 — Audio resilience parity (route: delegated)
  - Auto-reconnect on device loss (3 s retry), standby when no input device with hot-plug detection, capture watchdog; reuse Python semantics from `liveaudio/core/audio.py`.
- [x] T1b — Focus existing window on second launch (route: delegated, after T4 to keep one writer)
  - Owner decision 2026-10-14: second instance must bring the running window to front. Authorized to download `tauri-plugin-single-instance` (version compatible with the workspace Tauri 2). Keep the `service.lock` guard from T1 unless the plugin makes it redundant for the desktop; explain.
- [x] T4 — ASR request and transcript output (route: delegated)
  - Enforce `asr_decode_timeout_sec` (forward `timeout_ms` and/or Rust-side deadline).
  - Forward `cpu_threads` in the worker init payload.
  - VTT cues numbered sequentially; cue timing from audio timeline, not emit time.
  - Text sanitization (bidi/control), 600-char presentation cap, `no_speech_prob` filter parity with `engine.py`.
- [x] T5 — Distribution (route: delegated)
  - OBS overlay resolved from `resource_dir()` first; URL percent-encoded; existence checked.
  - `uv` resolution works on Linux (no hard-coded `uv.exe`; vendored `uv` with PATH fallback); Linux bundle target: AppImage only (owner decision 2026-10-14). Linux package must be built on Linux (no cross-build from Windows); building it is outside this task.
- [x] T6 — Minor fixes (route: delegated)
  - `get_recent_logs` arg `maxLines`.
  - Per-key tolerant config load (wrong type falls back to default for that key).
  - No `println!` in library code that pollutes CLI JSON stdout.
  - (CLI items moved to T7b.)

- [ ] T7 — Monitor Mode: owner-bound headless service (port of `conductor/tracks/service-backend_20260903/proposal_monitor_mode.md`, draft never implemented in Python). Owner rules 2026-10-14: spawner is the sole lifecycle owner; desktop opened while a service runs connects instead of starting another; no stop/restart of a foreign service; settings saved with "applies on next start" (Passive Config Persistence); closing the desktop leaves the service; if the owner dies while the desktop is open, the desktop starts its own service. Replaces the stashed `--autostart-service`; keeps the stashed `tauri.localhost` origin. Engram: `architecture/monitor-mode-service-ownership`.
  - [x] T7a — Service ownership core: `service.lock` becomes the service lock (taken by whoever runs the pipeline: desktop `start_service` and CLI `serve`; content `{"pid": N}` like Python `lock.py`), with a holder-alive query; desktop launch no longer exits when the lock is held. Pure JSON-lines stdout (move `println!` in `liveaudio-network/src/server.rs` to tracing). `tauri.localhost` origin from the stash with its tests.
  - [x] T7b — CLI headless parity: configured audio device, `ResilientCapture`, real `devices`, honest `doctor` (moved from T6).
  - [x] T7c — Desktop Monitor Mode UI/state: detect live holder at startup and before Start; status shows service owned by another process, Start disabled, read-only WS feed, deferred-settings banner, close leaves service, takeover when the holder dies.
  - [x] T7d — Installer ships `liveaudio-cli.exe` next to `liveaudio-desktop.exe`.
  - [ ] T7e — OpenCohost side: resolve installed `liveaudio-cli.exe` and spawn `serve --parent-pid`. Owner is handling it in a separate OpenCohost session (2026-10-14), editing `feat/laya-integration` in place without commit (option A) because the desktop-autostart code there is uncommitted. Keep `stash@{0}` (`opencohost-parity`) until T7e is validated end to end: it is the only copy of the `--autostart-service` implementation. End-to-end test requires reinstalling LiveAudio from develop with `liveaudio-cli.exe` bundled (owner).

- [x] T8 — Remaining audit findings (verified 2026-10-14 against 2bd370f by a read-only Sonnet agent; parent re-verified #1 and #20; numbers refer to that verification).
  - [x] T8a — WebSocket broadcast correctness: #1 shared retry buffer duplicates/reorders cues for healthy clients (`server.rs:238-283`); #7 client_count wraps after `stop()`; #8 handshake timeout, accept-error backoff (client cap only if trivial).
  - [x] T8b — Worker/CLI lifecycle: #3 CLI shutdown never calls `stop_internal`, `send_command` has no timeout; #2 child not killed when stop aborts a respawn handshake (`kill_on_drop`); #4+#26 ready timeout during first-run model download counts as a crash (progress-aware timeout / stalled state); #10 emitter must not panic on a broken stdout.
  - [x] T8c — Config parity: #20 Rust flips `transcription_purpose` subtitles→transcription for 15<max_chunk<=60 (introduced in bulk migration `355cb8b`, no stated intent) — follow Python: keep purpose, clamp; #21 missing `max_chunk_duration` default 30.0 for transcription/combined.
  - [x] T8d — Hardening/privacy/packaging: #5 uv/python discovery trusts cwd before PATH; #23 diagnostics export unscrubbed paths/URLs (Python regex parity); #18 NSIS legacy predicate time bomb at 1.6.0; #17 capability scope `["main","*"]` → `main`, drop unused `allow-emit` (CSP deferred: needs GUI verification, see below); #12 resampler `rate==0` guard; #15 strip `-->` from VTT cue text; #19 hard-coded Spanish log line.
  - Won't fix / deferred with reason: #6 `restart_child` and #16 cancelled token are latent (no callers; document single-use supervisor); #22 browsers lowercase Origin; #13 PreBuffer is bounded (claim false); #9 health tmp race, #14 probe timeout / non-staged setup-runtime, #24 JSONL schema differs from Python, #25 sink toggles at session start (consistent with Passive Config Persistence), #11 payload scrubbing — low risk. CSP (`csp: null` + `withGlobalTauri`): a wrong CSP silently breaks the webview (IPC, ws:// feed), so it needs a GUI-verified pass; deferred.

## Progress and evidence
- Audit evidence: Engram `audit/rust-migration-completeness`; scope decisions: `audit/rust-migration-scope-decisions`, `audit/rust-migration-scope-decisions-audio`.

- T1 (delegated writer, model fable; writer trigger: 7 non-trivial files). All 6 items confirmed in code and fixed.
  - RED observed: failed_respawn (Elapsed, stuck degraded), max_failures/respawn_policy (old ceiling), websocket_bind_failure ("starting" vs "stopped"), failed_shutdown ("stopped" on failed cleanup), instance_lock (compile-only RED, new primitive). GREEN on each after fix.
  - Writer checks: liveaudio-core all suites ok; liveaudio-desktop 47 passed; workspace 0 failed; `cargo fmt -p liveaudio-core --check` clean. Desktop rustfmt diffs at commands.rs/tests.rs pre-exist on HEAD (not introduced).
  - Parent spot check: `cargo test -p liveaudio-core --offline --locked --test supervisor_mock_worker`: 7 passed, 0 failed. Lock verified not to collide with the Python worker (InstanceLock only taken in `liveaudio/service/__init__.py:main`).
  - Not exercised: RunEvent::ExitRequested graceful stop (needs GUI runtime). Second instance exits with a log line, no window focus (single-instance plugin not available offline). CLI takes no instance lock.
  - Review assessment: risk medium (executable_change), RDD off → writer self-verification + parent spot check.
  - Commit: `92f0cac` fix(supervisor): recover from failed respawns and harden desktop lifecycle.

- T2 (delegated writer, model fable). Both items confirmed and fixed in liveaudio-audio only.
  - Device dedup now keeps parenthesized text (lowercase, punctuation/whitespace noise merged). Device IDs unchanged (raw name), saved `audio_device.id` remains compatible; previously hidden devices now appear. Python `_normalize_device_name` has the same bug (out of scope, legacy).
  - RNNoise clamps finite out-of-range input/output to [-1, 1]; NaN/inf still rejected.
  - RED: 3 behavioral failures (normalize unit test, dedup pipeline test, overdriven RNNoise input). GREEN after fix.
  - Checks: liveaudio-audio 25/7/8 passed; workspace 0 failed (writer); fmt clean. Parent spot check `cargo test -p liveaudio-audio --offline --locked`: all ok. Running count: T1 471 + T2 ~102.

  - T2 commit: `5f6ff74` fix(audio): keep distinct devices and clamp RNNoise range.
- T3 (delegated writer, model fable; writer trigger: 11 files). New `crates/liveaudio-audio/src/resilient.rs` (`ResilientCapture`) wraps any `AudioSource`, keeps one downstream receiver across losses; states Capturing/Standby/Reconnecting/Stopped via `watch`. Wired in desktop only; CLI deferred to T6.
  - Python timings ported from `liveaudio/core/audio.py`: reconnect 3 s (589-597), standby poll 1 s (518, 586), watchdog tick 500 ms / grace 3 s / limit 5 s (545-560), standby vs reconnect after loss (583-597). Selected device retried by same id, no default fallback (Python parity).
  - Deliberate deviation: watchdog disabled for loopback (WASAPI loopback is silent while nothing plays). Kept unless owner objects.
  - UI: `audio_state` in status DTO; pill shows waiting-for-microphone / reconnecting (es+en i18n).
  - RED: 6 behavioral failures against a stub (standby, reconnect 3 s, retry cadence, standby after loss, watchdog, stop in standby/reconnect); loopback exemption RED by guard removal; desktop DTO compile-only RED; frontend 3 failed. GREEN: resilient 9 passed (5 repeated runs), audio 34/7/8, desktop 48, workspace 0 failed, frontend 296 passed; fmt clean on touched lines.
  - Parent spot check: `cargo test -p liveaudio-audio --offline --locked --lib resilient`: 9 passed; `node test_frontend.js`: 296 passed, 0 failed. Risk assessment: medium (681 lines).
  - Not exercised: real hot-plug/unplug, cpal removal signaling, Tauri task wiring at runtime. Notes: desktop `capture_failed` branches now unreachable (left in place); standby polls cpal device enumeration every 1 s from an async task (blocking call, minor). Running count: ~1,250.

  - T3 commit: `b7b8b9b` feat(audio): reconnect, standby and stall watchdog for capture.
- T4 (delegated writer, model fable; interrupted once by an API rate limit with no persisted edits, then resumed). All 4 items confirmed and fixed; Python worker untouched.
  - Decode deadline: worker declares `timeout_ms` but never reads it, and answers pings from its reader thread while decoding. Supervisor now tracks `payload.state`/`payload.active_utterance_id` from pongs (verified in `asr_worker.py:841-850`) and fires the existing `kill_notify` when one utterance stays "transcribing" past `asr_decode_timeout_sec`; exit goes through the T1 respawn loop. Deviation: Python was fatal on first breach; Rust treats it as a respawnable failure (in-flight utterance lost). `timeout_ms` also forwarded (inert).
  - `cpu_threads` forwarded via shared `worker_init_payload()` (only `InitPayload`-declared keys; tested); desktop and CLI deduplicated.
  - VTT cues 1..N per session, resume from existing `subtitles.vtt`; start/end from capture timeline (SpeechStart minus pre-roll). Evicted-timing fallback: emit time minus duration (Python skipped the cue).
  - Text hygiene: per-segment sanitize, drop `no_speech_prob > 0.6` or <=2 chars, fallback to sanitized text; 600-char cap on presentation (UI/OBS) only; sinks keep full text (Python parity).
  - RED: deadline kill test, 5 asr::worker tests, last_cue_counter (0 vs 7), capture offset (None vs 12.5), resumed session (0 vs 4). GREEN all.
  - Checks: core all ok; desktop 49; workspace 220 passed 0 failed; fmt clean on core/network/cli and own desktop lines.
  - Parent spot check: `--test supervisor_mock_worker decode`: 2 passed; `--lib asr::worker`: 5 passed. Risk medium (576 lines).
  - Not covered: desktop pipeline wiring inside async closures (compiles; helpers tested). Risk: slow CPU + long chunks vs 15 s default can escalate to Failed after 3 kills. UI `SubtitleCueDto.id` still timestamp (out of scope). Running count ~1,830.

  - T4 commit: `9a823a3` fix(asr): enforce decode deadline and restore transcript parity.
- T1b (delegated writer, model fable; owner-authorized download). `tauri-plugin-single-instance` 2.5.3 (compatible with pinned tauri 2.12.0), registered first; callback unminimizes/shows/focuses "main". Cargo.lock +35 packages (mostly Linux D-Bus deps), none changed.
  - `service.lock` kept but moved after `build()` and before `run()`: second desktop instance exits inside plugin init (in `build()`), so it never reaches the lock or the ExitRequested handler; the lock still excludes a legacy Python service on the same data home. Parent verified in tauri-2.12.0 `app.rs`: config windows are created by `setup()` on `RunEvent::Ready` inside `run()` (line 1443), not in `build()`, and no tray is configured — so a lock-held exit creates no window.
  - TDD: no RED/GREEN possible (needs live AppHandle; Tauri mock runtime not enabled); no new pure logic. Existing instance_lock test green.
  - Checks: desktop 49 passed; workspace 220 passed; `cargo build -p liveaudio-desktop` ok; lib.rs rustfmt clean. Parent spot check: `cargo test -p liveaudio-desktop --offline --locked instance_lock`: 1 passed.
  - Manual pending (owner): launch twice from an installed build — window restores/focuses, service uninterrupted. Risks: Windows foreground rules may only flash the taskbar; Linux needs a D-Bus session bus.

  - T1b commit: `814d47c` feat(desktop): focus the running window on second launch.
- T7a (delegated writer, model fable). New `crates/liveaudio-core/src/service_lock.rs`: `ServiceLock::try_acquire` (writes `{"pid": N}` after acquiring), `service_lock_holder` (tries the lock; stale pid file → None). Windows: `LockFileEx` byte 0 length 1 (Python `msvcrt.locking` parity, mutually exclusive with the Python service); non-Windows: std flock. T1 instance lock moved here. Desktop acquires before any state change/bind, releases on every stop/failure/exit path; launch no longer exits when held; Start fails `service-already-running` without port fallback. CLI holds `_service_lock` for the service lifetime; already-running → one JSON fatal line, exit 2; unopenable lock → `service-lock-unwritable`. Removed duplicate `println!` in network server + `stdout_purity` guard test. Applied only the stash's `tauri.localhost` network hunks.
  - RED/GREEN: origin test, 4 core lock tests, 2 desktop lock tests, CLI lock helper test, stdout_purity — all RED then GREEN.
  - Checks: core/network/cli/desktop suites ok; workspace exit 0. Parent spot check: desktop `service_lock` 2 passed; `origin_security` 10 passed.
  - Review assessment: risk HIGH (hot_path, unsafe FFI) → independent verifier (fable, read-only): PASS, workspace 229 passed 0 failed, no blocking defects.
  - Non-blocking findings carried forward: L1 holder probe briefly takes the lock → a concurrent acquire can spuriously see "already running" — T7c must poll sparingly and/or retry acquire once. L2 lock released even when stop cleanup fails (consistent with is_running=false). L3 desktop fails open if lock file unopenable, CLI fails closed (intentional). L4 Linux flock vs Python lockf do not interlock (parity holds on Windows only). L5 stdout_purity scans network src top-level only.
  - Excluded from commit: build-regenerated `desktop/src-tauri/gen/schemas/*.json`. Not tested on Linux.

  - T7a commit: `24611f7` feat(service): make service.lock the owner lock for any pipeline.
- T5 + T7d (one delegated writer, model fable; same packaging files).
  - Overlay: `resolve_obs_overlay_path` tries `resource_dir()/liveaudio/assets/subtitulos_obs.html` first, then dev paths, requires existence (`std::path::absolute`, no `\?\` prefix); `obs_overlay_url` uses `tauri::Url::from_file_path` (percent-encoded) + `?port=N`; missing file returns an actionable error (frontend already catches it).
  - uv: `BUNDLED_UV_RESOURCE` per platform; `select_uv_executable` prefers the existing bundled file, else `runtime::find_uv_executable()`, else error naming both.
  - Linux: `tauri.linux.conf.json` `targets: ["appimage"]`, resource `packaging/vendor/uv`. Base `tauri.conf.json` no longer lists `uv.exe` (platform configs merge-patch over base; `tauri.windows.conf.json` keeps `uv.exe`).
  - CLI in installer: `bundle.externalBin: ["binaries/liveaudio-cli"]` in new `tauri.release.conf.json` (release-only because tauri-build copies externalBin on every cargo build — parent verified `tauri-build-2.7.2/src/lib.rs:736`). `desktop/src-tauri/binaries/` gitignored. Parent added `npm run build:release` script (`tauri build --config src-tauri/tauri.release.conf.json`) in `desktop/package.json`.
  - Release steps: `cargo build --release -p liveaudio-cli`; copy to `desktop/src-tauri/binaries/liveaudio-cli-<host-triple>[.exe]` (`rustc -vV`); vendor `packaging/vendor/uv.exe` (Windows) or `packaging/vendor/uv` chmod 755 (Linux, build on Linux); `npm run build:release` from `desktop/`.
  - RED: 9 new tests compile-fail (helpers absent); GREEN 9/9. Checks: desktop 58 passed; workspace 0 failed; `cargo build -p liveaudio-desktop` ok; 4 tauri JSON parse ok. Parent spot check: desktop 58 passed.
  - Not exercised: real NSIS/AppImage bundle; installed sidecar path from Tauri docs (bundler source not local). Risk: `tauri build` without `--config` silently omits the CLI.

  - T5/T7d commit: `79b49a5` fix(packaging): resolve bundled overlay and uv, ship the CLI sidecar.
- T7b (delegated writer, model fable). `LiveAudioConfig::capture_device_id()` in liveaudio-core shared by desktop and CLI (no setting/`default_microphone` → OS default; `wasapi_loopback` → `default_loopback`; else pass-through). CLI `serve` honors `audio_device.id` (other capture/VAD fields were already honored) and uses `ResilientCapture` (stopped on shutdown). Capture state → stderr only: no v1 event type fits, and Python emitted none (parent verified OpenCohost `parse_service_line` accepts any `type`, `supervisor.py:284`; Python service has no audio-state events). `devices` lists real ids like the desktop; `doctor` probes Job Object (N/A off Windows) and a real Unicode temp-dir round trip.
  - RED: compile-only (new helpers) for core `capture_device_id` and 4 CLI `headless_parity` tests; GREEN all.
  - Checks: cli 14, audio 34/7/8, workspace 242 passed 0 failed; fmt clean cli/core. Parent spot check: cli 14 passed, desktop 58 passed.
  - Risks: `serve` with no mic now runs in Standby with no JSON signal to OpenCohost; stale persisted device id → reconnect/standby instead of silent default mic; `devices` output format changed. CLI installs no tracing subscriber (ResilientCapture tracing warnings dropped). Not hardware-tested.

  - T7b commit: `93e5789` feat(cli): give the headless service desktop capture parity.
- T7c (delegated writer, model fable). `AppState.foreign_service` via `refresh_foreign_service` (ownership = `state.service_lock.is_some()`, not PID compare); detection at launch (`watch_foreign_service` task in `.setup()`), before every Start (refused acquire enters Monitor Mode), and a 3 s poll only while monitoring. DTO adds `monitor_mode`, `foreign_service_pid`; `ws_port` = holder's recorded port or config port.
  - Port discovery: lock JSON extended to `{"pid": N, "ws_port": P}` (pid first; Python never parses content) via `ServiceLock::record_ws_port`, called by CLI `serve` after bind.
  - L1 fixed on Windows: holder probe reads byte 0 (fails with `ERROR_LOCK_VIOLATION` when held) instead of taking the lock (stress test RED 516/2000 spurious failures → GREEN x3). Non-Windows keeps the try-lock probe (mitigated by the 3 s monitor-only poll + retry). Parent note: during an owner's metadata rewrite (`set_len(0)` → write) a probe may read 0 bytes and see "free"; takeover then fails to acquire and returns to Monitor Mode (benign).
  - Takeover: poll sees no holder → `start_service_guarded` with `retry_once_if_already_running` (1 s). Close in Monitor Mode is a no-op for the foreign service (tested).
  - Frontend: disabled toggle "running in another process", "Monitor" pill, PID banner (never claims OpenCohost), read-only feed via webview `WebSocket('ws://127.0.0.1:<ws_port>')`, deferred-save/reset notice (es/en).
  - RED: core stress (behavioral) + ws_port (compile); 6 desktop tests (compile); frontend 7 assertions + TypeError. GREEN all.
  - Checks: desktop 64, core lib 42, cli 14, workspace 250 passed 0 failed; frontend 310 passed 0 failed after the parent fixed a T5 regression (frontend test still expected `uv.exe` in base `tauri.conf.json`; now asserts Windows/Linux platform configs). Parent missed running `node test_frontend.js` in T5 — corrected here.
  - Parent spot check: core `service_lock` 6 passed; desktop 64 passed. Risk medium (539 lines).
  - Manual test (owner): `liveaudio-cli serve --parent-pid <shell pid>` → open desktop → Monitor UI, PID banner, feed, deferred save → close/reopen desktop keeps CLI → kill shell → desktop takes over within ~3 s. Not GUI-exercised; not tested on Linux.

  - T7c commit: `05f361a` feat(desktop): add Monitor Mode for services owned by another process.
- T6 (delegated writer, model fable; CLI items were done in T7b). `get_recent_logs` now `{ maxLines: 40 }` (tauri-macros 2.7.0 `wrapper.rs:51` default `ArgumentCase::Camel`); only snake_case invoke key in app.js; new frontend test fails on any `_` key in single-line invoke literals. Tolerant config load: parse to Value, keep each key that deserializes alone, drop failing keys with a warning (field default applies), unknown keys preserved in `extra`; non-object/malformed JSON keeps the previous error path.
  - RED: frontend 2 failed (max_lines); core 3 failed (InvalidData). GREEN all.
  - Checks: core 46/1/8/9, desktop 64, workspace 254 passed 0 failed; frontend 312 passed 0 failed; fmt core clean. Parent spot check: core `config` 16 passed; frontend 312 passed.
  - Risks: rejected values only replaced in memory until normalize/save; `"0.8"` strings fall back instead of coercing (Python coerced); bare `NaN` literal still invalid JSON → defaults.

  - T6 commit: `2bd370f` fix(config): tolerate invalid keys and pass camelCase log args.
- T8a + T8c (one delegated writer, model fable; interrupted once by an API rate limit with no persisted edits, then resumed).
  - T8a: per-client retry queues (`RetryState`, cap `max_retry_buffer`) replace the shared buffer — healthy clients get each payload once, a slow client gets its backlog in order before newer cues; queues dropped on disconnect/stop; metrics summed. Deliberately not Python's global gate (it stalls healthy OBS clients). Saturating client-count decrement. `WsServerConfig.handshake_timeout` (5 s, counted as rejected) and 100 ms shutdown-aware accept-error backoff. Client cap skipped.
  - T8c: subtitles→transcription flip removed from `normalize()`; `max_chunk_bounds(purpose)` = (60, 30) transcription/combined, (15, 5.0) subtitles; non-finite → purpose default; half-even rounding like Python `round(x, 1)`; missing/invalid key on load gets the purpose default. Limit: direct serde deserialization (not via `load_from_path`) still defaults to 5.0.
  - RED: client count wrap (usize::MAX), silent TCP peer never dropped, broadcast duplicate/reorder; normalize keeps purpose, half-even, non-finite, missing-duration default. GREEN all.
  - Checks: network all suites, core 50 lib + integration, desktop 64, workspace 0 failed; fmt clean. Pre-existing clippy deny in `job_object/mod.rs:73` blocks clippy on core (untouched). Parent spot check: `server_lifecycle` 13 passed; core `config` 20 passed.
  - Open: desktop UI (`app.js:1186`) still sets purpose=transcription when the max-chunk slider exceeds 15 s (only UI path to long chunks; left as-is). Accept-error backoff untested (no fault injection). Handshake timeout test uses real time.

  - T8a commit: `c328022` fix(network): deliver each cue once per client and harden the accept loop. T8c commit: `1babf86` fix(config): keep transcription purpose and clamp chunks like Python.
- T8b (delegated writer, model fable).
  - CLI shutdown goes through `ProcessSupervisor::stop_internal` (now `&self`; loop handle behind a sync mutex) via `shut_down_worker` (1.5 s grace); stderr reports only verified outcomes ("0 orphan processes" claim removed). `send_command` has a 2 s bounded enqueue.
  - Respawn-abort orphan: `kill_on_drop(true)`, `StartupKillGuard` (kill on drop until handshake success), cancel token passed into respawn handshakes, stop joins the loop under its deadline; kill/ready signalling switched from `notify_waiters` to `notify_one`.
  - Progress-aware readiness: the fixed outer ready timeout is replaced by a window that restarts on every worker stdout line; silence for the window kills startup and counts as a failure. Parent changed the writer's 150 s (`DEFAULT_STALL_SEC`) to 300 s (`ABSOLUTE_STARTUP_SEC`): the worker emits one "loading" status and then stays silent while loading (`asr_worker.py:276`), so 150 s could kill slow but healthy CPU loads; 300 s since last progress is never earlier than the previous 300 s total.
  - Emitter: `write_json_line` (writeln + flush) replaces `println!`; after a failed write stdout is marked closed and later lines are skipped, no panic.
  - RED (behavioral): stop during respawn handshake left worker alive; slow progressing startup killed at 1 s; `send_command` blocked 10 s. RED (compile): emitter closed-stdout test, CLI `shut_down_worker` test. GREEN all. Writer also fixed a pre-existing flaky pid-file read in `cancelling_startup_reaps_worker_while_waiting_for_ready`.
  - Checks: core lib 52 + 8 + 12 + 1, cli 15, desktop 64, workspace (twice) 0 failed; fmt clean. Parent re-run after the 300 s change: core all ok, cli 15 passed. Risk medium (457 lines).
  - Risks: join overrun falls back to abort (async kill); emitter drops events silently after stdout closes; a worker that keeps printing forever never fails startup (same as Python). Not tested on Linux or with a real first-run download.

  - T8b commit: `fd3799e` fix(supervisor): bound CLI shutdown, reap aborted handshakes, track startup progress.
- T8d (delegated writer, model fable; partial on item 3 only) — commit `3b23723` fix(security): harden discovery, scrub diagnostics, and guard edge inputs.
  - #5 discovery: `find_uv_executable` roots = exe dir only, PATH entries must be absolute; python: `LIVEAUDIO_PYTHON` -> managed runtime -> exe-relative (non-Windows names added) -> absolute PATH; cwd `.venv` removed; worker-script cwd fallback only in debug builds. New `crates/liveaudio-core/tests/uv_discovery_order.rs`. Residual: bare `python` fallback can still let Windows CreateProcess search cwd when no absolute PATH python exists.
  - #23 diagnostics: new `desktop/src-tauri/src/diagnostics.rs` `scrub_diagnostics_value` over the whole export (Python key-token rule + Windows path + URL redaction, plus POSIX home paths). Trade-off (Python parity): keys containing "transcript"/"audio" (e.g. `transcription_purpose`, audio device) are redacted too.
  - #18 NSIS: modern if `MainBinaryName`, or the install dir holds `liveaudio-desktop.exe`, or numeric major.minor >= 1.5 (1.x only). The writer's registry query failed because Git Bash converted `/s` `/f` into paths; parent re-ran it read-only with `MSYS_NO_PATHCONV=1`: the HKCU Uninstall `LiveAudio` key has `MainBinaryName = liveaudio-desktop.exe` and a QUOTED `InstallLocation`. So current Tauri installs were already safe; the `IfFileExists` marker cannot match a quoted path (the version fallback still covers 1.6+; a 2.x install without MainBinaryName would read as legacy). Structural only; no makensis.
  - #17: capability `windows: ["main"]`, explicit `allow-emit` removed, but `core:default` still grants emit via `core:event:default`; real revocation needs explicit permissions, deferred with CSP (GUI verification).
  - #12 resampler zero rate -> `AudioError::InvalidParameter` (constructed before the worker spawns). #15 VTT cue text `-->` -> `- ->`. #19 `log_events_ready` i18n (es/en); `index.html:429` static placeholder left.
  - RED: uv discovery (behavioral: picked the exe/.. vendor uv), diagnostics leaked "José" (behavioral); compile REDs for python helpers, resampler, VTT; capability assertions written after reading the pre-change content (not observed failing). GREEN all.
  - Checks: frontend 320 passed; core lib 55 + integrations ok; audio 35/7/8; desktop 65; cli 15; workspace 0 failed; fmt clean on touched crates/own lines. Parent spot check: `uv_discovery_order` 1 passed; desktop 65 passed; frontend 320 passed. Risk medium (469 lines).
  - Dev impact: dev runs from cwd now need `LIVEAUDIO_PYTHON`, `setup-runtime`, or an exe-ancestor `.venv`; dev builds use PATH uv instead of `target/.../packaging/vendor/uv.exe`.


## Next step
T8 complete. Deferred: CSP + explicit core permissions (GUI-verified pass), NSIS predicate check on a real installer build, bare-python cwd residual. T7e in the OpenCohost session (owner). Manual checks pending: T1b second launch focus, T7c Monitor Mode script, real NSIS/AppImage builds, hardware hot-plug.
