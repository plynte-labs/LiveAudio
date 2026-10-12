# Rust Migration Corrections

## Objective
Correct the audited Rust/Tauri migration regressions while preserving the existing Python implementation and user-owned migration work.

## Problem and why
The new migration has desktop privacy, device identity, OBS backlog, settings/runtime, process ownership, and distribution gaps. Existing unit-test success does not establish feature parity. Benchmark claims also compare different scopes.

## Authorized scope and constraints
- User authorized corrections, then specified Rust and the new migration files.
- Production implementation is confined to the new `crates/` and `desktop/` migration trees and their existing workspace manifests. Frontend wiring may change only inside the new Tauri tree where required to expose corrected Rust behavior.
- Do not modify legacy Python sources, Python tests, packaging scripts, existing launcher, unrelated tracks, `.atl/skill-registry.md`, or `.gitignore`.
- New migration documentation and this recovery document are allowed. Do not add invented benchmark figures.
- Keep the hybrid ASR boundary: the existing Python worker remains unchanged; control, policy, validation, and provisioning changes belong to Rust.
- Preserve current branch `feat/rust-tauri-migration` and existing untracked work. No remote operations, dependency downloads, running GUI/microphone, or GPU benchmarking during verification.
- Artifacts and new copy use English; direct conversation uses Spanish.
- Auditor delegates all production edits. One writer at a time; use the four existing Luna agents for implementation and independent domain checking.
- TDD: enabled, source: user-supplied AGENTS.md Strict TDD Mode. Observe RED before each behavior fix, then GREEN and REFACTOR.
- RDD: off, verified by `gentle-ai review mode status --cwd E:/LiveAudio` (global off). Do not invoke native review or enable it.

## Route and delivery
- Route: delegated direct, not SDD. Trigger: multiple non-trivial files and reading that prepares edits.
- Verification runners: `cargo test --workspace --offline --locked`; targeted package/test equivalents with offline/locked; `node test_frontend.js` from `desktop/` when frontend changes. Check-only formatting after normalization. Existing Python IPC tests may be run unchanged with the local venv and cache disabled.
- Delivery strategy: `ask-on-risk`. Forecast: approximately 1,000-1,800 authored changed lines across coherent corrections; heuristic only, do not omit tests or compress code to meet it. No PR/push/merge requested.
- Commit constraint: all migration sources are currently untracked user work at Python HEAD `040644929b433b53e2c13c24069c4b77990e3c1e`. A commit cannot isolate corrections from the pre-existing migration without also admitting that migration. Do not silently stage/commit user work; work-unit commits remain pending until a safe baseline/delivery decision is authorized. Verification does not imply committed closure.

## Tasks and acceptance criteria
The checkboxes below record observed implementation and test completion, not committed or release closure. Work-unit commits and end-to-end release/hardware validation remain explicitly pending.

- [x] T1 — Privacy, device identity, and persistent reset.
  - Remove transcript text from persistent diagnostics; logging regression test uses synthetic sentinel only.
  - Named input and loopback selections resolve the same identity emitted by enumeration; invalid explicit selections fail visibly rather than silently capture another source.
  - Reset persists before success and failure leaves previous config intact.
  - Route: delegated, writer trigger. Checks: desktop/audio focused tests, workspace regression.
- [x] T2 — OBS backlog/catch-up parity in Rust.
  - Reuse Python policy semantics (`auto`, `live_only`, `send_all`) through Rust configuration, real utterance timing retained at dispatch, stale filtering, and bounded paced catch-up.
  - Do not edit the worker or introduce incompatible public protocol fields; retain timing in Rust using existing utterance IDs and command/result events.
  - Test threshold boundaries, normal live emission, delayed/stale emission, reconnection, stop cleanup, and queue bounds.
  - Route: delegated, writer trigger. Checks: network/core/desktop focused tests plus workspace.
- [x] T3 — Bounded process lifecycle and compatibility.
  - Shutdown deadline covers command enqueue and grace wait; bounded kill/reap fallback.
  - Failed Windows Job Object assignment terminates/reaps the spawned child and returns an error.
  - Invalid parent PID rejected; legacy ASR state mapping matches Python.
  - Queue-full message accurately says incoming data is dropped; preserve existing policy.
  - Route: delegated, writer trigger. Checks: deterministic mock-worker, CLI parser, core emitter/audio tests.
- [x] T4 — Truthful settings apply, profiles, and sessions.
  - Separate saved vs active runtime state; changes while running require confirmation and actual apply/restart before presenting them as active.
  - Restore existing Python profile choices and wire existing Rust session sinks to safe per-session paths and visible state/output access.
  - Avoid a generalized new settings/session subsystem; reuse existing migration helpers.
  - Final lifecycle check: audio-task nested command sends must select cancellation so stop cannot join a task blocked behind a saturated worker queue.
  - Route: delegated, writer trigger. Checks: Rust behavior tests and existing frontend test harness.
- [x] T5 — Rust-side runtime readiness and honest migration documentation.
  - Rust validates/resolves the required ASR environment with an actionable failure, and exposes/uses a first-run provisioning path compatible with existing managed runtime behavior, without altering Python/packaging scripts.
  - No downloads are executed here. Never label a package self-contained/offline without its required runtime/dependencies.
  - Document exact runtime layout/provisioning requirements and qualify existing startup/ASR/shutdown/RAM claims by their measured scope; no replacement numbers.
  - Timeout must terminate/reap the import-probe child, not merely drop its output future. Charts must use the same qualified scopes as report prose.
  - Route: delegated, writer trigger. Checks: isolated runtime-discovery/provisioning command tests, offline workspace checks, doc structural readback. Clean-VM/network provisioning remains manual.

## Progress and proof
- Initial investigation: four Luna read-only audits and four design handoffs; relevant existing CodeGraph index verified current.
- Existing audit checks: Rust 88 tests and Python IPC 9 tests passed before corrections; these are not proof of the upcoming fixes.
- T1 implementation and independent verification PASS; committed task closure still pending. Privacy tests cover normal and unknown worker events. Device IDs match CPAL, default loopback works, unavailable explicit devices error, and failed startup rolls back actual local mock worker/WebSocket resources. DSP worker is stopped/joined if stream playback fails. Save/reset persist before state mutation, including `Ok(false)` and error paths.
- T1 TDD: privacy/device/log/rollback/CPAL regressions observed RED then GREEN. Persistence initially received an after-implementation sensitivity check; this did not satisfy strict TDD, so the writer restored only its faulty persistence order, observed two expected RED failures, then reapplied the fix and observed GREEN.
- T1 checks: desktop 14 tests, audio 21 tests, full offline/locked workspace passed on writer's final run. GUI/microphone/GPU were not exercised.
- T1 independent QA: desktop 14/audio 20 passed before final CPAL cleanup; final focused `failed_stream_play_stops_and_joins_the_dsp_worker` and `failed_audio_start_releases_started_resources_and_resets_state` each passed with no remaining T1 blockers. Real mock-worker and loopback WebSocket cleanup exercised, no microphone/model. Rollback boundary: only T1 edits to `desktop/src-tauri/src/commands.rs`, `desktop/src-tauri/src/tests.rs`, and `crates/liveaudio-audio/src/cpal_source.rs`.
- T2 implementation verified: shared Rust helper uses total subtitle age (including inference) for policy, queue-delay metadata remains separate, wire latency is inference time. Unknown/evicted timing drops Auto/LiveOnly but preserves SendAll. Bounded timing tracking and paced/reconnect replay wired in both CLI and desktop without worker edits or protocol changes.
- T2 verification: writer reports network37/desktop14/workspace106 passing. Performance and compatibility reviewers confirmed corrected production semantics, connected-client pacing, pending stop cleanup, and protocol compatibility. Reconnect proof initially kept one socket half alive; final test closes both halves and waits for client count zero before reconnect. Its disconnect assertion went RED with an open writer and GREEN after cleanup. Parent focused reconnect spot check recorded below. Original policy/timing RED provenance was reconciled through explicit faulty-behavior regression runs before the final corrected implementation.
- T2 rollback boundary: T2-only edits in network replay/server/lib and replay/server-lifecycle tests, CLI main, and desktop commands; preserve T1.
- Parent T2 spot check: `cargo test -p liveaudio-network --offline --locked reconnect` passed 1 test, exit 0; incremental-cache hard-link fallback warning only.
- T3 implementation independently verified. Writer reports core24/CLI2/audio22/workspace112 passing. Shutdown enqueue/exit share deadline, exit notification retains state; assignment failure kills/reaps; strict parent PID parse and ASR legacy projection added. CPAL incoming-drop diagnostic corrected. Writer reports RED/GREEN mutation regressions for assignment/PID/emitter and original non-reading-worker hang RED; initial test-first chronology for helper additions is not independently established.
- T3 independent performance PASS: core24, CLI parent-PID2, audio diagnostic1 passed. Real sleeping/nonreading child saturates command queue and is confirmed dead after bounded stop. Native Job Object failure is injected at cleanup boundary, not induced in the Windows API itself. Compatibility mapping/stdout discipline verified.
- T3 scope ruling: `serve` without any supplied parent PID stays a deliberate standalone mode, consistent with the user's manual-service scenario. Reject invalid supplied PID and missing flag value; do not require a parent for standalone service. This is an intentional documented difference from Python's ownership-required service entry, not a defect to remove.
- T3 rollback boundary: T3-only changes in core supervisor/process and emitter, CLI main, and CPAL source; preserve T1/T2.
- T4 implementation independently verified. Active runtime snapshot separated from saved config. UI confirms runtime-affecting save/restart, restart errors distinct from save success. Stop awaits tasks, flushes sessions and releases WebSocket listener; rebind test. Rust presets and frontend selector restored. Recording flags independently enable JSONL/WebVTT; continuity affects directory reuse during explicit restart only, ordinary stop/start creates a new session. Session path visible with backend-only folder action.
- T4 writer checks: desktop19/core24/frontend62/workspace passed. Reported RED includes active-vs-saved port, stopped port, real WS rebind, profile/session behavior, and frontend confirmation order. Live GUI/mic/GPU/OS folder action not tested. Some profile/session regression evidence used restoring implementations; initial test-first chronology not independently established.
- T4 rollback boundary: T4-only desktop Rust commands/state/dto/lib/tests, frontend app/runtime-settings/index/types and frontend tests, network server stop/join changes; preserve T1-T3.
- T4 accepted review fixes: immutable saved baseline vs profile draft; active device visible; recording independent of continuity; any sink stop/drain failure persists sanitized error while all cleanup proceeds. QA desktop22/frontend66 PASS. Final compatibility session5 PASS after manual-start rotation and pending-only drain error tests.
- T4 real verification failure: filtered session suite initially failed because timestamp-only temporary roots collided across parallel tests, allowing another test cleanup to remove output. Test-only roots now use atomic IDs plus exclusive create-directory ownership; writer20 repeated filtered runs PASS, concurrent uniqueness test PASS, desktop24/core24/workspace/frontend66 PASS. File-existence assertions retained. No production workaround/sleep added.
- T5 design evidence: local uv0.11.6 supports locked sync with `--no-install-project`; existing root manifests define cpu/cu121 extras. Plan explicit Rust `setup-runtime --backend cpu|cu121`, never implicit downloads during ordinary start. Embed/materialize manifests under managed runtime root, reuse staged package source, dependency probe on start. Backend choice is explicit command input, not an inferred user preference. Clean-VM/network check remains manual.
- T5 implemented/self-verified; frozen final domain checks pending. New core runtime module embeds unchanged root manifests, stages worker source under managed data-home runtime, resolves bundled uv before PATH, exposes explicit required-backend CLI setup, and preflights imports before CLI/desktop service resources. No setup/install/download was executed. Runtime setup refresh avoids copying managed package onto itself. New requirements doc and WU9 scope corrections added; no new metrics.
- T5 writer proof: initial new API missing-symbol compile RED; real self-refresh/self-copy regression RED then GREEN with fake uv; core30/CLI3/desktop24/workspace129 passed. Rustfmt/diff checks passed with benign hard-link cache warnings. New functionality test-first compile evidence is distinct from behavioral proof; real uv/network/clean-VM/CUDA/model/GUI/microphone verification remains pending.
- T5 rollback boundary: T5-only core runtime module/lib/supervisor discovery, runtime-provisioning tests, CLI main, desktop commands and new migration runtime/WU9 docs; preserve prior corrections. Legacy Python/package scripts/manifests/locks untouched by writer report.
- Final frozen reviews: QA frontend66 PASS; compatibility runtime-provisioning5 PASS, locked command/layout/control and truthful release limits verified. Performance found two additional current lifecycle blockers: desktop task join before supervisor stop can hang on uncancelled nested command enqueue; preflight timeout did not kill probe child. Also historical chart captions still implied unsupported comparative speedups. Scoped final Rust/docs correction in progress; no final completion claimed yet.
- Final scoped correction verified: actual desktop VAD-command send selects cancellation during queue saturation; regression uses real non-reading worker. Preflight explicitly kills and waits for timed-out child, with retained-handle cleanup test and kill-on-drop safety. All five SVGs qualify their historical scopes; ASR RTF attributed to warmed Python worker rather than Rust migration. Incremental cost of repeated preflight/worker imports explicitly unmeasured in requirements doc.
- Final domain sign-offs: architecture/privacy PASS with notes; QA PASS on automated scope, not full-parity sign-off; performance PASS with notes after its three blockers addressed; research/compatibility PASS with real provisioning/clean-VM still unproven.
- Final verification after normalization: `cargo test --workspace --offline --locked` exit0, 131 passed (audio22, CLI3, core31, desktop25, IPC3, network37, VAD10), zero failed/ignored; doc-test targets contain zero tests. `node test_frontend.js` in desktop exit0, 66 passed. Independent QA ran full suites before format-only normalization; writer reran them after normalization with identical counts. Parent reran frontend66 and full formatter/diff checks after normalization.
- Formatting: final `cargo fmt --all -- --check` and `git diff --check` exit0. Earlier whole-workspace check failed70hunks17files; all targets were confirmed inside new Rust migration roots and normalized without behavior changes. Parent source check emitted only the pre-existing registry line-ending warning; no test failure remained.
- Manual proof still pending: live Tauri GUI, physical audio/loopback/device disconnect, GPU/CUDA/model behavior, real OBS/OpenCohost end-to-end integration, real uv/network provisioning, clean-VM and packaged artifact validation. No release binary was deployed or app launched in this task.
- Source scope check: tracked diff remains only the two pre-existing `.atl/skill-registry.md` and `.gitignore` edits. Checked worker/protocol/packager/PythonIPC file hashes unchanged since T1 boundary; HEAD remains original Python baseline. No commits created.
- Native risk assessment unavailable because current migration additions are untracked; treated as high/unassessable and independently checked rather than inferring low risk. RDD remains off.
- Work-unit commits: pending safe baseline decision; no commits authorized to sweep in unrelated/user-authored migration bytes.
- Mirror: initial save/readback succeeded; updates mirrored after task outcomes.

## Next step
Implementation/testing complete for T1-T5. Next: owner closes any running desktop binary, rebuilds/runs the new Rust/Tauri app, and performs the documented manual checks. Before commits/PR, establish a safe baseline for the pre-existing untracked migration and select delivery slices; do not sweep user work into an automatic commit. Do not reimplement completed fixes on resume.
