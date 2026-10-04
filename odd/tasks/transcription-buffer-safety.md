# Transcription Buffer Safety — ODD Recovery

> **Current checkpoint:** T0–T5 are closed. T3–T5 were delivered in one cohesive local work-unit commit, `9cdd59cc17737792ad93fcd739c4ac81326dbefa` (`fix(pipeline): bound transcription lifecycle and report losses`), with the owner-approved `size:exception`; it contains 1,811 additions and 167 deletions (1,978 authored lines). The original checkout remains untouched. No push, PR, or merge is authorized. Current buffers/configuration and JSONL/WebSocket v1 contracts remain unchanged; larger-buffer profiles remain deferred.

## Objective

Close LiveAudio's transcript-integrity and resource-safety prerequisites before any larger transcription window is considered. Preserve the 5-second default and current WebSocket wire behavior; do not implement profile, streaming, or max-buffer changes here.

## Problem and why

The approved safety plan identifies canonical text truncation before JSONL persistence, unbounded diagnostic sample retention, unbounded session-writer admission and swallowed storage errors, a lazy ASR iterator outside the current future timeout, and distinct capture/queue/shutdown loss domains. Address these foundations first so later product choices do not turn bounded buffering into silent transcript loss or false durability/latency claims.

## Authority, scope, and constraints

- The user approved execution of the local safety plan and consented to an isolated worktree. This worktree is `feature/transcription-buffer-safety`, based on `2e016dc0d6d294be6f084f4a5de9e61ae249729a`; the original checkout remains untouched.
- Feature execution is authorized within the approved plan. Gate 2 selects fail-closed capture stop with a visible alert on writer saturation or storage failure. The owner has also explicitly accepted T4/T5 interruption loss: stop/close with visible warning and no automatic reprocessing. These are scoped product decisions, not permission to add replay or redesign unrelated lifecycle behavior. The user separately authorized the narrow T0 baseline UI test-mock correction; T0 changes test fixtures/coverage only. T3–T5 source and focused test changes are authorized only within their task scopes below. Focused checks remain authorized; benchmarks, microphones, real model loads/downloads, and external integration remain separately unauthorized.
- T0–T5 are technically verified and closed. Gates 1–3 are resolved and must not be re-asked. Gates 4–5 remain deferred to the separately authorized future larger-buffer/profile/wire feature.
- Do not increase `max_chunk_duration`, change defaults/config, add profiles or streaming, or alter the current 600-character WebSocket projection.
- Preserve local-only transcript/audio handling, sanitization, localhost/Origin protections, JSONL schema, diagnostics snapshot/privacy schema, sink toggles, and safe overlay `textContent` behavior. Do not access OpenCoHost repositories, endpoints, credentials, ports, or remote sessions.
- Strict TDD is **true**, sourced from the current user-session instructions. The observed runner was `<source-workspace>/.venv/Scripts/python.exe`, with process cwd `<isolated-worktree>`. These are explanatory placeholders, not literal commands. No installs were authorized.
- Parent observed RDD as globally off; no RDD transaction is active. Native read-only risk assessment was unavailable because of untracked selection/inventory, so risk was treated as high; independent verification was completed, with no RDD lifecycle or receipt. The owner-approved local `size:exception` was used for cohesive T3–T5 commit `9cdd59cc17737792ad93fcd739c4ac81326dbefa`; it authorizes no push/PR/merge. Never add AI attribution.

## Plan and dependencies

The approved plan is `docs/superpowers/plans/2026-10-03-transcription-buffer-safety.md`. All six task IDs below are stable; T0 is the newly authorized baseline-fixture correction, followed by existing T1–T5. Each implementation task follows its assigned route; this document setup does not subdelegate.

| ID | Depends on | State |
|---|---|---|
| T0 — Repair baseline UI prewarm mocks | Explicit baseline-fix authorization | ☑ Complete |
| T1 — Preserve canonical transcript text | T0 complete; execution approval; independent of future Gate 5 | ☑ Complete — `ec15dea562691d848bb2f25c0b5aaac4e857d02e` |
| T2 — Bound diagnostics memory | T0 complete; execution approval; independent | ☑ Complete — `ca0768a9221d9edf35e08ee39461e76a30a49c76` |
| T3 — Bound writer and surface persistence failures | Gate 2 resolved | ☑ Complete — `9cdd59cc17737792ad93fcd739c4ac81326dbefa` (shared T3–T5 unit) |
| T4 — Enforce full-decode deadline at a killable boundary | Gate 1 resolved; T3 verified | ☑ Complete — `9cdd59cc17737792ad93fcd739c4ac81326dbefa` (shared T3–T5 unit) |
| T5 — Account for capture/queue/lifecycle timing and losses | Gate 3 resolved; integrate T3/T4 outcomes | ☑ Complete — `9cdd59cc17737792ad93fcd739c4ac81326dbefa` (shared T3–T5 unit) |

T0–T5 are closed. The shared T3–T5 work-unit commit is recorded in the task table; Gates 1–3 are resolved. Gates 4–5 remain deferred to a separately authorized profile/wire feature.

## Tasks and acceptance

### T0 — Repair baseline UI prewarm mocks

**Authorization:** The user explicitly authorized this correction by answering “ok continua” to the request to repair the baseline UI test failures first. This is a narrow test-only task; production behavior and unrelated tests are out of scope.

**Root cause:** The 12 full-baseline failures in `tests/test_asr_language.py` occur because `_make_mock_app_for_read` and `_make_mock_app_for_load` create `MagicMock(spec=[])` objects without `var_prewarm`. Production `_read_ui_config` reads `self.var_prewarm.get()` (`liveaudio/app.py:1326`), and `_load_ui_from_config` sets it from the config (`liveaudio/app.py:1382`).

**Files:** `tests/test_asr_language.py` only.

**Actual authored changes:** 32 lines in `tests/test_asr_language.py`; commit `1d34056ccd5269a936b77496641e334215f29178` (`test(ui): repair prewarm config mocks`). Rollback boundary: revert this commit's change to that test file only. No production code changed. No real-device or model execution was performed or needed; offline UI config methods were exercised.

- [x] Preserve all existing UI assertions and each helper's `MagicMock(spec=[])` protection while supplying the missing mock attribute.
- [x] Add focused true/false coverage for `_read_ui_config` and `_load_ui_from_config` prewarm round-tripping.
- [x] Make no production-app change, model operation, or unrelated test repair.
- [x] **RED evidence:** Focused module first reproduced 12 failures and 37 passed; the added prewarm tests also failed before the fixture correction on the missing mock attribute.
- [x] **GREEN:** Focused module passes: 51 passed, 4 subtests passed.
- [x] **Verify (from isolated worktree root):** `& '<source-workspace>/.venv/Scripts/python.exe' -m pytest tests/test_asr_language.py -q` — 51 passed, 4 subtests; `& '<source-workspace>/.venv/Scripts/python.exe' -m pytest -q` — 746 passed, 35 subtests; `& '<source-workspace>/.venv/Scripts/ruff.exe' check liveaudio tests` — all checks passed; `git diff --check` — passed. All commands exited 0. Independent Research verification: focused module 51 passed, 4 subtests; spec/quality PASS.
- [x] Record the observed focused/full results; no unrelated failure remained.
- **Route:** Delegated direct; bounded test-file repair is assigned to the test writer, not the Auditor.
- **Commit:** Auditor-created after diff readback and approval: `1d34056ccd5269a936b77496641e334215f29178` (`test(ui): repair prewarm config mocks`).

### T1 — Preserve canonical transcript text

**Files changed:** `liveaudio/core/engine.py`, `tests/test_engine.py`, `README.md`, `docs/GETTING_STARTED.md`, and `HISTORIAL_CAMBIOS.md`. No network or overlay implementation was changed.

**Root cause and boundary:** `_sanitize_text` applied the 600-character cap to final text before the JSONL record was constructed. The change keeps sanitized canonical text complete for JSONL and uses a separate capped presentation projection for VTT, normal WS/OBS output, and WS queue-full diagnostic events. A fake ASR generator exercises the real `asr_consumer` → `SessionWriter` path and reads back the actual JSONL record; no live ASR/device/GPU/audio run was performed.

**Actual change size:** 122 additions and 19 deletions (141 authored lines); cumulative committed feature work is 173 lines including T0. Rollback boundary: revert commit `ec15dea562691d848bb2f25c0b5aaac4e857d02e` only.

- [x] Preserve fully sanitized canonical text in JSONL without applying the current 600-character cap; control/bidi sanitization remains enabled.
- [x] Keep existing WebSocket v1 fields and current 600-character server-side projection; safe overlay behavior and sink toggles remain unchanged.
- [x] Keep JSONL/OBS/VTT sink toggles independent.
- [x] Verify long canonical JSONL text, unsafe-character removal, normal WS presentation cap, and queue-full diagnostic presentation cap. Long-input VTT, OBS-disabled, and replay branches were structurally checked through their shared presentation projection but are not each directly asserted with long input.
- [x] Strict TDD: initial RED had 2 expected failures, with 77 existing tests and 16 subtests passing; the added queue-full regression separately failed when the stale branch used full text (1000 characters observed rather than 600 plus `...`).
- [x] **Final verification:** focused `tests/test_engine.py tests/test_output_sinks.py tests/test_network.py` — 80 passed, 16 subtests; full suite — 748 passed, 35 subtests; Ruff and `git diff --check` exited 0. Independent QA repeated the focused suite (80 passed, 16 subtests) and returned spec/quality PASS.
- **Route:** Delegated direct; engine behavior and regression tests span multiple non-trivial files.
- **Commit:** Auditor-created after review: `ec15dea562691d848bb2f25c0b5aaac4e857d02e` (`fix(transcript): preserve complete canonical JSONL`).

### T2 — Bound diagnostics memory

**Files:** `liveaudio/core/diagnostics.py`; `tests/test_diagnostics.py`.

**Implementation and boundary:** Each metric retains its latest 256 duration samples in chronological order using a fixed-size standard-library deque. The snapshot still exports `durations` as JSON lists; cumulative counters, schema version 1, privacy behavior, metric-key cardinality, and all runtime configuration remain unchanged. This bounds per-metric sample history; it is not evidence of a VRAM leak and does not bound future metric-key cardinality.

**Actual change size and rollback:** 20 additions and 2 deletions (22 authored lines); cumulative committed behavior/docs work is 195 authored lines. Rollback boundary: revert commit `ca0768a9221d9edf35e08ee39461e76a30a49c76` only. No real GPU, model, or audio benchmark was run.

- [x] Retain the most recent **256** duration samples **per metric**, in chronological order; counters stay cumulative.
- [x] Preserve diagnostics snapshot schema version 1, JSON-list `durations`, privacy filtering, and absence of transcript/audio values.
- [x] Keep future metric/tag key cardinality finite; this is a separate constraint, not evidence of a current variable-key leak.
- [x] Test that more than 256 samples yields exactly the newest 256 and that counters/schema/privacy remain unchanged.
- [x] **Strict TDD RED:** Regression exposed 300 retained samples instead of 256; 8 passed, 1 failed.
- [x] **GREEN and verification:** focused diagnostics tests — 9 passed. Writer full suite — 749 passed, 35 subtests; Ruff and `git diff --check` exited 0. Independent Research repeated the focused suite — 9 passed; spec/quality PASS.
- **Verify (from isolated worktree root):** `& '<source-workspace>/.venv/Scripts/python.exe' -m pytest tests/test_diagnostics.py -q`; `& '<source-workspace>/.venv/Scripts/python.exe' -m pytest -q`; `& '<source-workspace>/.venv/Scripts/ruff.exe' check liveaudio tests`; `git diff --check` — all final writer commands exited 0.
- **Route:** Delegated direct; production diagnostics code and its test are two non-trivial files.
- **Commit:** Auditor-created after diff review: `ca0768a9221d9edf35e08ee39461e76a30a49c76` (`fix(diagnostics): bound duration sample retention`).

### T3 — Bound transcript writer and surface persistence failures

**Gate 2 decision:** The user chose **fail-closed capture stop with a visible alert** on writer saturation or storage failure, rather than bounded ASR backpressure. This authorizes requesting capture shutdown through existing lifecycle controls only; it does not resolve partial-audio, queued-audio, or hot-swap behavior under Gate 3.

**Files changed:** `liveaudio/core/engine.py`; `liveaudio/app.py`; `liveaudio/service/supervisor.py`; `liveaudio/utils/i18n.py`; `tests/test_async_io.py`; `tests/test_output_sinks.py`; `tests/test_resilience_asr.py`; `tests/test_resilience_service_backend.py`; and `docs/GETTING_STARTED.md`.

**Original propagation finding:** The initial writer had an unbounded queue and swallowed write errors; GUI and headless paths did not stop on child persistence failure. A single bounded-queue `log_queue` fatal event was not reliable when that queue was full. The scoped correction records an allowlisted, transcript-free `writer_failure_code` in the existing shared config mapping before best-effort log enqueue; GUI `process_logs` and headless `poll_once` both inspect that sticky state. A fresh explicit GUI start and fresh service `start()` clear the session-local signal; automatic restart does not clear it. The GUI requests its existing `toggle_system` stop before showing the modal alert. The signal excludes transcript, audio, and filesystem paths.

- [x] Enforce a finite writer admission bound (capacity **32 remains a plan proposal to validate**, not an immovable policy). Queue-full must reject the new, not-yet-accepted record, emit a visible fail-closed signal, and request capture shutdown; do not add bounded ASR backpressure.
- [x] Track enabled JSONL/VTT sinks independently as pending until their write succeeds; report saved only after successful completion, rejected when the record was never admitted, and failed/unconfirmed when an accepted sink write or bounded drain fails. Never silently discard accepted records or claim persistence that did not complete.
- [x] Keep writer error state sticky and make a failure trigger one sanitized structured alert/code; preserve independent sink outcomes if only one artifact write fails.
- [x] Start the stop deadline **before** sentinel/control admission. Stop signaling must remain bounded even with a full/stuck data queue; account for accepted pending records as unresolved/failed rather than false saved or silently dropped.
- [x] Use existing GUI `toggle_system` and service `ProcessSupervisor.shutdown()` lifecycle paths; do not redesign stop, queue-drain, or partial-audio semantics reserved for Gate 3.
- [x] Strict TDD: bounded admission/rejection, accepted-but-unsaved write failure, per-sink outcome independence, flush ordering/sticky errors, and stop deadline including control admission. Integration tests prove visible GUI handling and that the headless service shuts down and emits fatal rather than restarting after a child writer-failure signal.
- **Verify (from isolated worktree root):** `& '<source-workspace>/.venv/Scripts/python.exe' -m pytest tests/test_async_io.py tests/test_output_sinks.py tests/test_resilience_asr.py tests/test_resilience_service_backend.py -q`; add the focused GUI-event test module to this command if one is created.
- **Route:** Delegated direct; writer/ASR integration and GUI/headless event propagation span multiple non-trivial source/test files.
- **Forecast:** T3 currently totals **835 authored changed lines** across its nine implementation/test/doc files after the stale-session isolation correction; T0–T2 total 195 committed authored lines, for 1,030 overall. This exceeds the approximate 400-line planning heuristic. Keep this cohesive behavior and its regression coverage intact; do not create an incomplete writer-only or alert-only slice. Delivery remains unresolved under `ask-on-risk`; the Auditor must ask before any commit/chain decision. No chain strategy is selected.
- **Commit intent (future, only after Auditor approval):** `fix(transcript): bound writer backlog and surface errors`.

**Initial T3 implementation evidence (preserved from the first candidate):** `SessionWriter` introduced a finite default queue capacity of 32, nonblocking admission, per-sink pending/saved/rejected/failed outcomes, bounded flush and stop deadlines, and draining of previously accepted records where possible. The first candidate was 432 authored lines. Its initial focused/full checks passed, but subsequent independent verification found the blockers recorded below, so those earlier green results do not close T3.

- **Strict TDD RED:** The first exploratory focused run hung because the new service test attempted to terminate the loop through a `ServiceError` raised by its sleep stub; the supervisor intentionally swallows sleep exceptions. That run was interrupted and is not treated as a base failure. After fixing test cleanup and using a finite fallback only when the fatal event is not handled, the clean RED command exited 1 with **13 expected failures, 72 passed, 14 subtests passed**. Failures demonstrated missing queue API/outcomes, bounded flush/stop, sink independence, translated GUI alert, ASR fatal signal, and headless shutdown propagation.
- **Initial candidate GREEN:** Focused writer/integration suite — **79 passed, 20 subtests passed**; full suite — **756 passed, 41 subtests passed**; Ruff — all checks passed; `git diff --check` — exit 0 (Git emitted line-ending normalization warnings only). These are historical results for the uncorrected candidate.

### T3 scoped correction — current candidate evidence

- **Strict TDD RED:** A first correction test attempt was interrupted by `KeyboardInterrupt` used as a harness stop condition; it is not counted as proof. After replacing that harness stop with a finite `ServiceError` fallback, the required focused command exited 1 with **7 expected failures, 77 passed, 20 subtests passed**. Failures reproduced saved-on-admission wording, modal-before-stop order, missing GUI/service session reset, and missing sticky failure propagation when the log queue was full.
- **Corrected behavior:** Writer failure is published to the existing shared-config state before best-effort bounded-log notification. The GUI polls this state even when the log queue has no capacity; the headless service polls it before child-liveness/restart handling, then uses its real `run()` fatal/shutdown path. Fresh explicit GUI/service starts reset it; automatic child restart does not. GUI stop is requested before the modal message. OBS-disabled/backlog notices now say processed/not sent, not saved; no per-record acknowledgment or WebSocket field was added.
- **GREEN:** Focused command `& '<source-workspace>/.venv/Scripts/python.exe' -m pytest tests/test_async_io.py tests/test_output_sinks.py tests/test_resilience_asr.py tests/test_resilience_service_backend.py -q` — **84 passed, 20 subtests passed**. Full `& '<source-workspace>/.venv/Scripts/python.exe' -m pytest -q` — **761 passed, 41 subtests passed**. `& '<source-workspace>/.venv/Scripts/ruff.exe' check liveaudio tests` — all checks passed. `git diff --check` — exit 0; Git emitted only LF-to-CRLF normalization warnings.
- **Prior correction checkpoint size:** **682 additions + 30 deletions = 712 authored lines** before stale-session isolation was added. The current size is recorded in the verification forecast above. The correction is one integrated fail-closed behavior; splitting it into writer, signal, UI, and service commits would leave intermediate revisions unable to honor the approved capture-stop policy. No files, tests, or behaviors were omitted or compressed to fit the heuristic.
- No real ASR/model/GPU/audio or external integration was run. Tests use fake models, a full bounded log queue, local temporary files, controlled file-open failures, and observe actual GUI `process_logs` and service `run()` shutdown paths.
- T3 remains unchecked until independent verification and Auditor readback; no commit or delivery strategy has been selected.

### T4 — Enforce full-decode deadline at a killable boundary

**Owner decision:** Gate 1 is resolved: on full-decode deadline expiry, stop capture with a visible warning that pending audio/transcript data may be lost; do not automatically replay/reprocess the utterance.

**Files:** `liveaudio/core/engine.py`; `liveaudio/app.py`; `liveaudio/service/supervisor.py`; `liveaudio/utils/i18n.py`; `tests/test_resilience_asr.py`; `tests/test_resilience_service_backend.py`; and new `tests/test_decode_deadline.py`; `docs/GETTING_STARTED.md`.

**Implementation:** Preserve the production 15-second budget. The ASR child publishes one transcript-free `asr_decode` marker containing status, current ASR attempt, utterance ID, monotonic start, and deadline immediately before invoking `model.transcribe`. `_transcribe_with_timeout` now fully materializes the lazy iterator in that child; the future/thread timeout was removed because it could not cancel native work. GUI `process_logs` and headless `ProcessSupervisor.poll_once` compare only an active marker for their current attempt. On expiry, the existing GUI stop path or service fatal/shutdown path terminates and reaps the ASR process, emits the localized/service warning, and never restarts/reprocesses the utterance. Model provisioning/loading and waiting on the audio queue remain outside this per-utterance deadline. Normal and error completion clear the marker; fresh starts clear stale markers.

- [x] **RED:** Replaced future-only timeout expectations with a lazy-generator materialization regression. Added a CPU-only child whose `model.transcribe()` returns promptly but whose generator blocks only on iteration; its real child process must be terminated/reaped through both GUI and service supervision.
- [x] **RED evidence:** Focused command exited 1 with **3 expected failures, 1 pass**: GUI supervision left the blocked child alive; service supervision returned `test-stop` instead of `asr-decode-timeout`; helper returned an unconsumed generator instead of the full segment list. A later exact GUI test also failed when the attempt guard was temporarily removed, proving the old attempt was not accepted as current.
- [x] **GREEN:** Full segment iterator is consumed in the ASR child; parent uses the monotonic marker to detect expiry. GUI stops/reaps then warns; service emits warning + fatal and uses bounded shutdown without scheduling restart. No partial canonical/WS record is emitted because record construction occurs only after iterator completion.
- [x] **GREEN and checks:** Focused `tests/test_decode_deadline.py tests/test_resilience_asr.py tests/test_engine.py tests/test_resilience_service_backend.py -q` — exit 0, **96 passed, 6 subtests passed**. Full `-m pytest -q` — exit 0, **765 passed, 47 subtests passed**. Ruff — exit 0, all checks passed. `git diff --check` — exit 0; Git emitted only LF-to-CRLF normalization notices. CPU-only fake processes/generators only; no real ASR/model/GPU/audio/microphone or external integration.
- [x] Keep the **15-second** decode budget unchanged and separate from provisioning/model loading and audio queue wait.
- [x] Prove timeout from lazy iteration kills/reaps the actual child process within deadline plus bounded supervisor cleanup; cover both GUI and headless service, current-attempt matching, and idle/loading markers that must not timeout.
- [x] No partial canonical record or duplicate transcript/WS event on timeout; preserve model-load failure reporting and avoid automatic reprocessing.
- **Verify (from isolated worktree root):** `& '<source-workspace>/.venv/Scripts/python.exe' -m pytest tests/test_decode_deadline.py tests/test_resilience_asr.py tests/test_engine.py tests/test_resilience_service_backend.py -q`; `& '<source-workspace>/.venv/Scripts/python.exe' -m pytest -q`; `& '<source-workspace>/.venv/Scripts/ruff.exe' check liveaudio tests`; `git diff --check`.
- **Route:** Single delegated-direct writer; ASR worker, GUI, service lifecycle, localization/docs, and process-boundary tests span multiple non-trivial files.
- **Size readback:** current tracked diff is 1,085 authored lines plus 226 lines in the new decode-deadline test (1,311 cumulative), versus the prior T3 checkpoint of 835 lines. The cumulative candidate grew by 476 lines; this is not an exact task-only add/delete count because T4 replaced earlier future-timeout tests in `tests/test_resilience_asr.py`.
- **Commit intent (future, only after Auditor approval):** `fix(asr): enforce decode deadline at supervised boundary`.

### T5 — Account for capture, queue, and lifecycle outcomes

**Files:** `liveaudio/core/audio.py`; `liveaudio/core/engine.py`; `liveaudio/core/network.py`; `liveaudio/app.py`; `liveaudio/service/supervisor.py`; `tests/test_audio.py`; `tests/test_backpressure.py`; `tests/test_network.py`; `tests/test_resilience_asr.py`; `tests/test_decode_deadline.py`; `tests/test_resilience_service_backend.py`; `tests/test_test_health_diagnostics.py`.

- [x] **Gate 3:** Preserve owner-selected bounded interruption, visible possible-loss warning and no automatic replay/reprocessing; do not add a lossless drain or recovery contract.
- [x] Carry monotonic capture timestamps and attempt-correlated sequence through VAD, ASR and internal WS metadata. Measure utterance formation, queue wait, complete decode, actual per-sink disk write and server-side WS queue/broadcast call time separately.
- [x] Count ring overwrites, VAD partial/ring shutdown discards, audio/text queue drops or shutdown discards, accepted writer pending outcomes, decode interruption, and WS retry/replay eviction or queued replay shutdown separately with fixed metric names.
- [x] GUI and service record rejected stop controls and discarded queue items without blocking; both emit transcript-free warnings when queued items are discarded. Timeout interruption is counted once per active attempt.
- [x] Keep internal timing off WS v1 payload and JSONL schema; do not claim client delivery timing without an ACK contract. Diagnostics contain no transcript content.
- [x] Strict TDD RED observed: the first focused batch had **5 expected failures** for missing ring/VAD, ASR stage telemetry, writer timing, and WS metadata stripping; the capture-attempt test separately failed for the missing helper. A later four-test batch had **4 expected failures** for shutdown-counter/sentinel-warning and replay-discard behavior. Two decode deadline tests failed on missing interruption accounting. GUI/service queue-discard warning regressions failed on the old silent drain path. A separate regression failed when the ASR consumer did not pass capture timestamps through the writer to the per-sink capture-to-write metric. Each targeted regression passed after implementation.
- [x] **Focused GREEN:** `& '<source-workspace>/.venv/Scripts/python.exe' -m pytest tests/test_audio.py tests/test_backpressure.py tests/test_network.py tests/test_resilience_asr.py tests/test_decode_deadline.py tests/test_resilience_service_backend.py tests/test_test_health_diagnostics.py tests/test_output_sinks.py -q` — exit 0, **166 passed, 28 subtests passed**.
- [x] **Full GREEN:** `& '<source-workspace>/.venv/Scripts/python.exe' -m pytest -q` — exit 0, **776 passed, 47 subtests passed**.
- [x] `& '<source-workspace>/.venv/Scripts/ruff.exe' check liveaudio tests` — exit 0, all checks passed. `git diff --check` — exit 0; Git emitted line-ending normalization warnings only.
- **Limitations:** no real ASR/model, microphone/audio, GPU, external integration, or client-delivery ACK test. A forcibly terminated child cannot report losses from its private VAD ring; warning semantics say pending outcomes may be lost/unknown rather than claiming zero or saved.
- **Size readback:** current cumulative tracked diff against the feature base is **1,409 additions + 167 deletions = 1,576 authored lines**, excluding untracked artifacts and generated files. This includes earlier uncommitted T3/T4 changes; exact isolated T5 count is unavailable because no task-start snapshot exists. Correctness/tests were not reduced to fit the heuristic.
- **Route:** Delegated direct; capture, engine, network, UI lifecycle, and regression tests exceed the multi-file threshold.
- **Commit intent (future, only after Auditor approval):** `fix(audio): report end-to-end latency and overflow`.

## Verification, forecast, and delivery

- **Baseline verification (before T0):** Import from the new checkout verified. Focused command covering `tests/test_engine.py`, `tests/test_output_sinks.py`, `tests/test_network.py`, and `tests/test_diagnostics.py` exited 0: **86 passed, 16 subtests passed**, with one deprecation warning. Full baseline collected 744 tests and exited 1: **732 passed, 12 failed, 31 subtests passed**. All failures were `tests/test_asr_language.py` mock failures caused by missing `var_prewarm` attributes in the strict helpers. T0 then repaired those test fixtures. T1 focused/full results and independent QA are recorded above. Global RDD remains off; native assessment was unavailable and treated as high for verification, so independent QA was used; no RDD lifecycle or receipt was opened.
- **Final checks:** `& '<source-workspace>/.venv/Scripts/python.exe' -m pytest -q`; `& '<source-workspace>/.venv/Scripts/ruff.exe' check liveaudio tests`; `git diff --check`. The source-workspace and isolated-worktree placeholders above are explanatory, not literal commands.
- **Current committed count:** T0 + T1 + T2 total **195 authored lines** (32 from T0, 141 from T1, and 22 from T2). T3–T5 remain one uncommitted cohesive work unit; the owner has accepted its size exception for local delivery.
- **Observed task size:** T3 checkpoint was 835 authored lines. T4 increased the cumulative candidate by 476 lines, with the test rewrite caveat recorded under T4. T5's cumulative tracked diff is measured above; task-only delta is not isolated. These are planning measurements, not a forced split or a cap.
- **Delivery:** owner-approved local `size:exception`; the cohesive T3–T5 unit is committed as `9cdd59cc17737792ad93fcd739c4ac81326dbefa`, `fix(pipeline): bound transcription lifecycle and report losses` (1,811 additions + 167 deletions = 1,978 authored lines). No push, PR, or merge is authorized.

## Progress and next step

- [x] T0 — complete and committed; baseline UI test fixtures repaired after observed `var_prewarm` mock failures.
- [x] T1 — complete and committed; full sanitized canonical JSONL is separate from unchanged 600-character presentation projection.
- [x] T2 — complete and committed; diagnostic sample retention is bounded with counters and privacy/snapshot schema preserved.
- [x] T3 — technically accepted and closed in shared commit `9cdd59cc17737792ad93fcd739c4ac81326dbefa` with T4/T5.
- [x] T4 — technically accepted and closed in shared commit `9cdd59cc17737792ad93fcd739c4ac81326dbefa` with T3/T5.
- [x] T5 — technically accepted and closed in shared commit `9cdd59cc17737792ad93fcd739c4ac81326dbefa` with T3/T4.

**Next:** T0–T5 are closed. Larger 30–60-second buffers/profiles and any WebSocket presentation changes remain a separate, not-yet-authorized feature. No real model/RTX 3060/GPU/leak validation, microphone/audio, or external integration was performed. Do not push, open a PR, or merge.
## Historical checkpoint — T3 scoped correction before independent acceptance

Independent QA and Research repeated the first candidate's focused checks (79 passed/20 subtests and 47 service tests, respectively); Performance also reproduced fatal-signal loss with a saturated bounded queue. Those checks did not establish acceptance. The Auditor accepted one bounded T3 correction, now implemented and writer-verified, covering:

- Candidate blocker: the single fatal signal uses the same saturated log queue and swallows enqueue failure; capture stop/alert and service shutdown may never occur. Add a reliable, transcript-free sticky failure route that both GUI and headless lifecycle owners observe even if log admission fails. Preserve finite writer/stop bounds, and reset session-local error state on an explicit fresh start, not an automatic restart.
- Candidate blocker: a blocking storage dialog currently precedes capture shutdown. Request existing capture stop before showing the modal alert; prove ordering while the alert remains open.
- Within existing T3 truthful-persistence scope: the pre-existing OBS-disabled/backlog log copy says saved on queue admission. Use neutral processed/queued wording rather than claiming disk success; do not add a larger persistence-ack feature or alter WS fields.

Correction RED is recorded above (7 expected failures, 77 passed, 20 subtests). Corrected writer checks then passed: 84 focused tests/20 subtests, 761 full-suite tests/41 subtests, Ruff, and diff check. T3 remains unchecked and uncommitted pending independent verification and Auditor readback; no commit or delivery decision is authorized here. Gates 1/3, partial-speech/drain/restart policy, larger buffers, remote work and real-device/model runs remain outside scope. The T3 diff at that checkpoint was 712 authored lines; retain all required tests and the cohesive fail-closed behavior. No chain strategy or size exception has been selected.

## Historical checkpoint — corrected T3 candidate before owner policy and delivery resolution

The prior independent checkpoint confirmed fail-closed writer reporting but identified one remaining stale-session risk: a fatal event queued by an older ASR worker could remain in the persistent GUI log queue and stop a later capture session. That finding is addressed in the current candidate using the existing monotonically increasing `asr_attempt` identity; no new signaling framework was added.

On each explicit GUI start, `asr_attempt` advances and is shared with the ASR worker. The worker includes its attempt in fatal events and publishes it beside the sticky writer-failure code. The GUI accepts writer-fatal events only when the code is allowlisted and `attempt` is an exact integer equal to the current attempt; missing, malformed, boolean/float, stale, or future attempts are ignored. The sticky fallback is also correlated to the current attempt. The full-log-queue current-session failure path remains covered and continues to stop capture before the alert.

**Strict TDD RED:** New focused regressions first failed before production changes: an old attempt-1 fatal stopped an active attempt-2 session; missing, malformed, and future attempts were accepted. The allowlisted-code rejection case already passed. After strengthening the stale-event regression to call the real explicit GUI start path, the handler's attempt guard was temporarily removed and the test failed because the old queued fatal called the stop path; restoring the guard made that test pass. The first command used an incorrect pytest class selector and ran no tests; the corrected RED invocation reached intended assertions. This selector mistake was not a product failure.

**GREEN:** Focused `tests/test_async_io.py tests/test_output_sinks.py tests/test_resilience_asr.py tests/test_resilience_service_backend.py -q` — exit 0, **86 passed, 26 subtests passed**. Full `-m pytest -q` — exit 0, **763 passed, 47 subtests passed**. Ruff at the prescribed `.venv` path — exit 0, all checks passed. A first Ruff invocation used a mistyped executable path and emitted a command-not-found error; the corrected exact path then passed. `git diff --check` — exit 0; only line-ending normalization warnings. Tests prove stale queued fatal isolation after actual explicit start, strict allowlist/attempt validation, current-session failure stop-before-alert, saturated log queue sticky fallback, and fresh-start identity advancement/reset. No real ASR/model/GPU/audio or external integration was run.

No T3 commit or staging, push, PR or merge. T0–T2 commits remain intact. Delivery remains ask-on-risk: preserve T3 as one cohesive behavior rather than leave an unsafe intermediate version; do not select a chain strategy or size exception without Auditor/owner decision. Gates 1/3 and larger buffers remain held. Independent verification of this final stale-session correction and Auditor readback are still pending.

## Historical completion directive — superseded by later owner decisions

The earlier instruction to resolve T3 first and hold T4/T5 for owner choices was completed and superseded. T3 stale-session isolation was implemented and independently checked; the owner later resolved Gates 1 and 3 as recorded above and explicitly authorized T4 followed by T5. The current status and verification evidence are in the T4 section and progress summary. RDD remains off. No larger-buffer/profile/wire feature, real-device/model benchmark, push or merge is authorized. Size exception and chain strategy are unset; work-unit delivery still requires Auditor direction.
## Historical checkpoint before owner policy resolution

T3 technical acceptance is verified: independent QA rechecked 86 focused tests and 26 subtests (exit 0), confirming captured producer ASR attempt correlation, real fresh-start/queued-old-fatal rejection, current-event stop-before-modal, full-log-queue fallback and service fatal/no restart. Parent spot check passed 15 ASR tests and 6 subtests; source/diff readback and whitespace checks passed. Writer full suite passed 763 tests and 47 subtests; Ruff passed. Initial selector/path errors and the guard-disabled failing regression are retained above. T3 acceptance checkboxes are observed outcomes, not a commit/receipt; the task remains unclosed until its work-unit commit. Current uncommitted T3 is 835 authored lines; combined feature count would be 1030 with prior 195. No chain strategy or size exception is authorized; no commit, staging, push, PR or merge.

T4/T5 mapping was completed by Performance and Research. A killable deadline changes more than the old skipped utterance: process termination can interrupt its asynchronous writer and GUI recovery drains queued audio. Existing VAD shutdown does not flush partial speech, and bounded joins do not guarantee persistence. These are real product-loss choices, not routine correction permissions. At this checkpoint, Gates 1 and 3 still appeared unresolved. The owner subsequently answered yes to the timeout/closure policy, as recorded in the following owner-resolution section. Larger 30–60 second profiles and OpenCoHost wire changes remain separate and unimplemented.
## Owner policy resolution — historical product decisions (Gates 1–3 resolved)

The owner answered yes to the explicit timeout/closure policy: pending audio or transcripts may be discarded, with visible warning and no automatic reprocessing. Gates 1 and 3 are now resolved for this safety plan. Do not ask again or select lossless replay. T4 may enforce the existing 15-second complete-decode deadline at the supervised process boundary, surface the interrupted/unknown pending outcomes and stop capture without silently replaying the utterance. T5 preserves interruption semantics while separately accounting for VAD-local partial, queued audio, in-flight decode and accepted-but-unsaved writer outcomes; use bounded stop/control waits and truthful warnings, not invented successful persistence or client WS delivery.

Implement T4 then T5 with one writer, strict RED/GREEN and independent proof. T4 implementation, writer verification and independent acceptance are recorded above; its work-unit delivery remains pending. Production buffers/config/profiles and WS v1 presentation remain unchanged. Tests use offline stubs/CPU-only subprocesses, no mic/GPU/models/install/remote. Retain sink independence, T1 full canonical JSONL, T2 bounded diagnostics, and T3 current-attempt failure handling. Size exception/chain strategy is not authorized; do not fabricate commit/delivery approval or push/merge.

## T4 independent acceptance — historical verification

Independent Performance verification passed the exact affected command: 96 tests and 6 subtests, exit 0. The CPU-only lazy-decode child-process regression blocks during generator iteration and proves deadline termination/reaping; Parent spot check of tests/test_decode_deadline.py passed all 4 tests with actual CPU subprocess termination/reaping. The preservation check confirmed both committed and current transcribe kwargs retain vad_filter=False and condition_on_previous_text=False; no correction was needed. Ruff and whitespace checks passed. This is offline supervised-process proof, not real GPU latency or a VRAM leak proof.

T4 technical acceptance is verified; its work-unit commit remains pending delivery strategy. The owner-authorized next step is T5 implementation and verification, without further policy questions. Gate 3 is resolved: interrupt, explicit possible pending-loss warning, no automatic replay. No source/staging/commit delivery permission is inferred from a checkbox; no chain strategy or size exception has been selected.

## Final technical acceptance — local work-unit close pending

T0–T5 source outcomes are implemented and technically accepted within the authorized safety scope. The source work unit committed as `9cdd59cc17737792ad93fcd739c4ac81326dbefa` contains 16 source/test/doc files: 1,811 additions and 167 deletions (1,978 authored lines). Independent QA reran the complete suite: 776 tests and 47 subtests passed, exit 0; Ruff passed (exit 0), `git diff --check` passed, with line-ending notices only.

The existing wall-clock `asr.shutdown_sec` finalizer metric is a base-only follow-up, not a newly introduced timing regression. No real model, RTX 3060/GPU/VRAM/leak validation, microphone/audio, client-delivery ACK, or OpenCoHost test was performed; forced child-local remainder remains unknown. Larger 30–60-second buffers/profiles were NOT enabled and remain a separate feature.

T0–T5 are closed locally. T3–T5 were committed together because the final writer, GUI, ASR-supervision, and lifecycle changes share source and regression coverage; partial filesystem slices would omit safety coupling. The owner-approved `size:exception` was local only. The original source checkout remains untouched. Rolling the source work unit back to `ca0768a` removes T3–T5 only and retains T0–T2; rollback was not run. No push, PR, or merge is authorized.
