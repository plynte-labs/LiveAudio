# Configurable transcription buffers

## Objective

Give users a choice between fast subtitle output and longer-window transcript continuity without creating a second ASR pipeline, changing the WebSocket v1 contract, or implying that long-window subtitles remain low-latency. The accepted design extends the existing single local ASR flow; it does not promise compatibility with an uninspected OpenCoHost consumer.

## Problem and reason

The current phrase-duration setting is oriented around short subtitle latency. Longer phrases can improve transcript continuity but delay final-only subtitle output and consume more queued audio memory. Users need separate purpose/window controls and an independently bounded decode budget, while preserving canonical transcript persistence, existing outputs, privacy boundaries, and process recovery.

## Authorized scope and constraints

- Local implementation is authorized on the isolated feature branch; original checkout remains untouched. No push, PR, merge, installation, or external integration is authorized.
- Keep one ASR pipeline. Reuse the existing GUI/headless configuration flow, hardware presets, audio queue, and supervised decode lifecycle.
- Purpose is orthogonal to hardware preset. Applying a hardware preset must preserve the selected purpose and phrase window.
- Preserve the legacy `max_chunk_duration` key and existing user values. Normalize/validate new values; never accept non-finite numbers, wrong types, or out-of-range values.
- Proposed purpose windows: `subtitles` defaults to 5 seconds and permits 1–15; `transcription` defaults to 30 seconds and permits 1–60; `combined` uses the transcription window and prioritizes transcript continuity. Longer final-only subtitle delay may be tens of seconds. Do not promise a 200 ms or otherwise fixed low-latency result in combined mode.
- Preserve early-on-silence phrase closure, but cap actual phrase duration, including trailing silence, prebuffer and frame tolerance. Duration-derived audio queue sizing is nominal, not a strict memory/RSS bound: account conservatively for these margins and retain a finite item ceiling. Multiprocessing payload copies prevent claiming a hard process-memory cap. Do not alter the text/log queue limits.
- Keep the decode deadline separate from phrase duration: configurable finite 5–120 seconds, default 15 seconds. Loading/provisioning remains separately timed. Preserve child termination/reaping and no automatic replay on timeout.
- Keep JSONL/VTT/OBS sink toggles independent. Canonical JSONL remains full sanitized text; VTT/subtitle and OBS WebSocket v1 `text` remain capped at 600 characters with current fields/schema. Do not claim or implement a new OpenCoHost contract in this feature.
- Diagnostics remain finite-cardinality and transcript/audio/path free. No real audio, microphones, network endpoints, model loads, GPU benchmarks, installs, or downloads during unit verification.

## Execution configuration

| Item | Value |
|---|---|
| Workflow | Organic Driven Development; no SDD/Conductor artifacts |
| TDD | Strict TDD enabled by current session instructions: observed RED, then GREEN, then REFACTOR |
| Route | One delegated-direct writer, sequential B1 then B2 then B3; multi-file source/test work triggers delegation. Keep code, tests, and user docs with each behavior; combine overlapping pieces when that is more coherent than artificial splits. |
| Runner | `<source-workspace>/.venv/Scripts/python.exe`; the placeholder represents the verified local project venv and must be substituted with the checkout root, not run literally. Ruff: `<source-workspace>/.venv/Scripts/ruff.exe`. |
| Baseline | Before B1 implementation, the focused suite passed 270 tests and 22 subtests. The prior safety branch's 776 tests and 47 subtests remain historical context, not this feature branch's baseline. |
| Delivery | `exception-ok`: owner explicitly approved the feature-scoped `size:exception` and coherent local work-unit commit. Observed source/test/docs count is 948 authored lines, plus this recovery document. No push, PR, integration or merge. |

## Work units

### B1 — Add purpose-specific phrase windows and bounded audio backlog

- [x] Add the minimal normalized purpose field and purpose-specific window defaults/ranges while preserving the legacy duration key and current values; keep hardware preset application orthogonal.
- [x] Expose purpose/window through existing GUI and shared GUI/headless configuration path. Preserve save/apply/discard/restart behavior.
- [x] Bound the larger audio backlog using a conservative duration-derived item budget with a finite ceiling. Include phrase limit plus silence/prebuffer/frame tolerance in actual-cap reasoning; do not change premature VAD-drop semantics merely to make the nominal queue calculation look exact.
- [x] Keep one ASR path, early-on-silence closure, independent sinks, canonical JSONL, VTT/OBS 600-character projection, and WS v1 schema unchanged.
- [x] Add concise user-facing help that warns combined/long windows delay final subtitles and describes transcription-only versus subtitle purposes.

**Acceptance checks:** legacy configs migrate/read unchanged; defaults, allowed bounds and invalid numeric/type cases normalize safely; all three purpose choices reach GUI and headless consumers; preset apply/discard/restart preserves purpose/window; silence can close before the cap and forced-duration closure includes silence/padding/frame tolerance; the maximum supported fake phrase processes through the existing single ASR path; backlog saturation is bounded and reports its selected loss outcome; JSONL/VTT/OBS toggle independence and WS v1 fields/600 cap remain unchanged; combined mode presents a truthful delay warning.

**Focused verification:** `pytest tests/test_config.py tests/test_vad_onset_ui.py tests/test_restart_flags.py tests/test_apply_settings_save.py tests/test_audio.py tests/test_backpressure.py tests/test_engine.py tests/test_output_sinks.py tests/test_network.py tests/test_resilience_service_backend.py -q`; then full `pytest -q`, Ruff `check liveaudio tests`, and `git diff --check`. Add any new focused UI/config test module to the command.

**Route/trigger:** delegated direct, one writer; config, UI, audio queue, engine/output behavior and tests span multiple non-trivial files. Keep this as one behavior unit unless verified coupling supports a safer cohesive split.

### B2 — Make decode deadline independently configurable

- [x] Add a bounded 5–120 second decode-budget setting with default 15 seconds, independent of purpose/window. Keep loading/provisioning timing separate.
- [x] Carry the setting through existing GUI and headless supervision; retain full lazy decode inside the supervised killable process, timeout termination/reaping, clear attempt markers, visible interruption warning, and no automatic replay.
- [x] Preserve stale-attempt isolation and ordinary successful/failure behavior; do not add a second ASR worker or change transcript/wire schemas.
- [x] Update the relevant user guide with the distinction between phrase window and decode deadline.

**Acceptance checks:** config defaults/clamps/rejects invalid inputs and survives preset/apply/discard/restart; timeout applies to complete lazy iteration rather than just future creation; fake blocking child is actually terminated and reaped within the chosen deadline/tolerance; idle capture and model loading do not consume decode budget; current GUI and headless paths warn and stop appropriately; stale markers cannot stop a newer attempt; no timed-out utterance is automatically replayed or emitted as a partial/duplicate transcript.

**Focused verification:** `pytest tests/test_decode_deadline.py tests/test_resilience_asr.py tests/test_engine.py tests/test_resilience_service_backend.py -q`; then full `pytest -q`, Ruff `check liveaudio tests`, and `git diff --check`.

**Route/trigger:** delegated direct, one writer; settings, shared config, supervised GUI/service lifecycles and tests are multi-file behavior. Preserve kill/reap as one cohesive unit with its lifecycle regressions.

### B3 — Close feature acceptance and user guidance

- [x] Verify all B1/B2 criteria together with fake-only CPU tests and inspect the final diff for schema, privacy, one-pipeline, and configuration-compatibility drift.
- [x] Obtain recorded Performance, QA and Research signoffs on the intended local evidence; clearly distinguish unit evidence from unrun real-device/model/quality checks.
- [x] Finish a concise user guide for choosing purpose/window, expected subtitle delay, decode deadline, and known limits. Do not imply OpenCoHost interoperability until its separate contract is verified by an authorized task.
- [x] Record exact focused/full/lint/diff results and each local work-unit commit in this recovery document; check boxes only after observed evidence.

**Acceptance checks:** full suite, Ruff and diff check pass; all requirements and unchanged output contracts are traced to tests; fake 60-second phrase/decode-budget boundaries are covered; reviewer signoffs and manual steps state that real hardware/quality and external consumer compatibility remain unverified; no new hardware preset, WebSocket schema, model, or remote integration is added.

**Focused verification:** rerun the B1 and B2 focused commands plus full `pytest -q`, Ruff `check liveaudio tests`, and `git diff --check`. Any real-device/model check remains a separately authorized release activity, not a blocker to claiming only local fake-test results.

**Route/trigger:** delegated direct acceptance/research task after B1 and B2; it reads multiple source/test/doc artifacts and consolidates evidence without creating another spec or ledger.

## Progress and next step

- [x] Approved design and scope recorded before implementation.
- [x] B1 — accepted and committed in shared coherent work unit `833463c7281f733825228d968b202248c13daccc`.
- [x] B2 — accepted and committed in shared coherent work unit `833463c7281f733825228d968b202248c13daccc`.
- [x] B3 — all four signoffs, user guidance, fresh checks and work-unit commit evidence recorded below.

Current status: all three tasks are locally complete in work-unit commit `833463c7281f733825228d968b202248c13daccc`; the remainder below preserves historical checkpoints. Real-device validation and external consumer interoperability remain unverified. Branch and worktree are retained; no publish or integration.

### B1 evidence (uncommitted; completion checkbox intentionally remains pending)

- **Baseline RED-free focused run:** the listed focused suite at the base passed: 270 tests and 22 subtests, exit 0 (32.48 s).
- **Observed RED:** after adding B1 regressions, the focused run exited 1 with 15 expected failures and 179 passes. An initial collection error from importing a not-yet-added helper was corrected in the test setup, then the clean RED was observed. A later one-test RED exposed an unhashable purpose value raising `TypeError`; after its guard was added, the test passed. Further one-test REDs showed numeric duration strings were accepted and raw queue sizing treated them as a 60-second value; type-safe fallbacks were added and verified.
- **Focused GREEN:** the exact B1 focused suite passed: 286 tests and 22 subtests, exit 0. One existing `DeprecationWarning` points to a pre-existing Windows-path docstring in `liveaudio/utils/config.py`.
- **Full suite:** first run exited 1 with 16 failures caused by the pre-existing `tests/test_asr_language.py` MagicMock fixtures not defining the new purpose widgets/lookup method; fixtures were updated. Rerun passed: 792 tests and 47 subtests, exit 0.
- **Ruff:** `<source-workspace>/.venv/Scripts/ruff.exe check liveaudio tests` passed, exit 0.
- **Diff check:** `git diff --check` passed, exit 0.
- **Runtime boundary:** CPU-only unit fakes; no real ASR model, GPU, microphone, audio session, or external consumer was run. The 60-second frame-limit helper is checked; a long real/fake audio-to-ASR quality run remains outside this B1 evidence.
- **Changed files:** `README.md`, `docs/GETTING_STARTED.md`, `liveaudio/app.py`, `liveaudio/core/audio.py`, `liveaudio/service/supervisor.py`, `liveaudio/utils/config.py`, `liveaudio/utils/i18n.py`, and tests in `tests/test_asr_language.py`, `tests/test_audio.py`, `tests/test_backpressure.py`, `tests/test_config.py`, `tests/test_resilience_service_backend.py`, `tests/test_restart_flags.py`, `tests/test_vad_onset_ui.py`.
- **Authored diff size:** 448 additions plus deletions across tracked B1 changes, excluding this recovery document; this exceeds the 400-line planning heuristic. No size exception or chain strategy is approved, and no commit was created.

Next: parent reads back and mirrors the evidence update, then resolves independent verification/delivery for B1. B2 and B3 remain unstarted. The only changes on this branch are the approved recovery document and the B1 implementation/tests/user guidance; no feature commit exists.

### B1 independent checkpoint — historical, superseded by correction below

Independent QA repeated the focused suite: 286 tests and 22 subtests passed, exit 0. Acceptance is partial: normalization fills a missing window with global 5 seconds even when an explicit new purpose is transcription/combined (expected purpose default 30). Tests cover frame-limit helpers but not a maximum VAD-generated fake phrase through the single real ASR consumer. Correct these two scoped gaps under strict TDD; no B2 source changes until B1 acceptance is verified. Existing legacy explicit durations must remain preserved. No new user policy or output schema is authorized by the findings.

### B1 scoped correction — implemented; independent acceptance pending

- **Purpose default root cause and correction:** `_normalize_config` filled the global 5-second duration before resolving the explicit purpose, erasing whether the legacy key had been absent. It now derives a missing/invalid duration fallback from the normalized purpose: 5 seconds for subtitles and 30 seconds for transcription/combined. Explicit valid duration values remain preserved within their purpose range.
- **Config RED:** added missing/invalid duration fallback regressions first. The focused config run exited 1 with 6 expected subtest failures (missing or invalid duration under transcription/combined returned 5 seconds), 93 passed, and 4 subtests passed. After correction, config plus the new integration module passed: 95 passed and 10 subtests.
- **End-to-end fake-path coverage:** `tests/test_vad_asr_integration.py` feeds fake 32 ms capture frames through `audio_producer`, fake Silero VAD, the real audio queue item, and `asr_consumer` with a fake lazy Whisper generator. It asserts a 60-second forced-duration phrase reaches ASR once with attempt identity preserved, both lazy segments are fully materialized, one full canonical JSONL record is persisted, and exactly one capped 600-character OBS/event projection is emitted. A separate early-silence case confirms the VAD closes before the duration cap.
- **Scoped negative controls:** temporarily forcing a 100-frame VAD cap made the 60-second integration assertion fail at 51,200 samples versus the expected minimum 959,488. Temporarily truncating lazy iteration to its first segment made the canonical JSONL assertion fail because the final segment was missing. Both temporary source mutations were restored byte-for-byte; no production test hooks were added.
- **Focused GREEN:** B1 focused command plus `tests/test_vad_asr_integration.py` passed: 291 tests and 32 subtests, exit 0.
- **Full GREEN:** full suite passed: 797 tests and 57 subtests, exit 0. Ruff check passed, exit 0; `git diff --check` passed, exit 0 (Git reported only existing LF-to-CRLF working-copy warnings).
- **Changed files:** the prior B1 files remain changed; this correction also changes `tests/test_config.py`, adds `tests/test_vad_asr_integration.py`, and updates this recovery document. No B2/B3 source work began.
- **Current authored source/test size:** 719 additions plus deletions across tracked B1 source/test/docs and the new untracked integration test, excluding this recovery document. This is a count, not delivery authority; no commit or staging was performed.
- **Runtime boundary:** verification used fake capture/VAD/Whisper and local temporary session files only. No microphone, model, GPU, external consumer, install, or remote integration was used.

Next: parent performs the requested independent acceptance check and reconciles the recovery mirror. B1 remains pending that check and local delivery decision; B2 and B3 remain unstarted.

### B1 acceptance checkpoint

Independent QA and a fresh parent spot check both ran config plus VAD-to-ASR integration: 95 tests and 10 subtests passed, exit 0. Both scoped blockers are addressed. B1 is technically accepted; local work-unit delivery remains uncommitted pending the new feature's delivery decision. B2 may now start under the approved scope. No hardware, microphone, model, GPU, or external compatibility result is implied.

### B2 — configurable decode deadline (implementation and checks observed; commit pending)

- **Observed RED:** the focused B2 regression run exited 1 with 20 expected failures and 265 passed plus 24 subtests. It exposed the missing normalized setting, missing timeout in the decode marker/worker call, fixed GUI warning, missing headless warning budget, missing GUI read/load, restart classification, and save rollback coverage. A follow-up invalid-large-integer regression also failed before correction with `OverflowError` during float conversion; the validator now falls back safely.
- **Focused GREEN:** the exact B2 command from the task passed: `pytest tests/test_decode_deadline.py tests/test_resilience_asr.py tests/test_engine.py tests/test_resilience_service_backend.py tests/test_config.py tests/test_vad_onset_ui.py tests/test_restart_flags.py tests/test_apply_settings_save.py tests/test_vad_asr_integration.py -q` — 219 tests and 34 subtests, exit 0. Including the GUI UI read/load regression module as an additional focused run passed 272 tests and 38 subtests, exit 0. The existing invalid-escape `DeprecationWarning` in the config module remains unrelated.
- **Full GREEN:** `pytest -q` passed: 803 tests and 75 subtests, exit 0. Ruff `check liveaudio tests` and `git diff --check` both passed, exit 0; Git reported only LF-to-CRLF working-copy notices.
- **Setting and lifecycle:** new `asr_decode_timeout_sec` defaults to 15 seconds and normalizes to a finite integer in [5, 120], rejecting booleans, strings, non-finite values and non-numeric types (out-of-range numeric values clamp). GUI draft/load/save/rollback carries it; changing only this budget restarts ASR, not audio. The headless config load already runs shared normalization. Each ASR phrase records its validated budget in the private decode marker and applies it to complete lazy iteration; the existing GUI and service watchdogs consume that marker, terminate/reap the child, stop capture, and warn with the actual configured budget. Model loading and phrase/audio queue waiting are outside the decode marker. No WS or transcript schema changed.
- **Tests:** config validation covers missing/default, 5/15/60/120, invalid and out-of-range values; GUI tests cover read/load, ASR-only restart and save rollback. A fake Whisper consumer processes each configured budget and checks marker duration and passed worker timeout. The CPU-only blocking-lazy-iterator child test uses a shortened monotonic deadline and observes process exit/reaping through GUI and headless paths; it verifies no automatic retry/partial transcript. B1 fake VAD-to-real consumer integration remains covered.
- **Changed B2 files:** `liveaudio/utils/config.py`, `liveaudio/core/engine.py`, `liveaudio/app.py`, `liveaudio/service/supervisor.py`, `liveaudio/utils/i18n.py`, `tests/test_config.py`, `tests/test_decode_deadline.py`, `tests/test_asr_language.py`, `tests/test_restart_flags.py`, `tests/test_apply_settings_save.py`, `README.md`, `docs/GETTING_STARTED.md`, and `HISTORIAL_CAMBIOS.md`.
- **Current worktree inventory:** aggregate against the feature base, including B1 and B2 but excluding this recovery document, is 714 tracked additions plus deletions (680 additions, 34 deletions) and the 234-line untracked B1 integration test. This is an observed aggregate, not a B2-only estimate or delivery approval. No staging or commit was performed.
- **Runtime boundary and limitations:** tests used fake Whisper, fake audio, local temporary sessions, and a CPU subprocess. The actual supervised child termination/reap path is covered, but no real ASR model, GPU/VRAM, microphone, live session, external consumer, installation, or network endpoint was used. The configured budgets were checked without waiting 120 seconds; the blocking test injects an elapsed deadline.

Next: parent reads back and mirrors this update, then decides whether B2 may be locally committed. B3 remains unstarted.


### Final local acceptance checkpoint (B1+B2+B3; uncommitted)

- **QA independent acceptance:** no deterministic blocker. Exact focused union: `<source-workspace>/.venv/Scripts/python.exe -m pytest tests/test_config.py tests/test_vad_onset_ui.py tests/test_restart_flags.py tests/test_apply_settings_save.py tests/test_audio.py tests/test_backpressure.py tests/test_engine.py tests/test_output_sinks.py tests/test_network.py tests/test_resilience_service_backend.py tests/test_decode_deadline.py tests/test_resilience_asr.py tests/test_asr_language.py tests/test_vad_asr_integration.py -q` passed 366 tests and 60 subtests, exit 0. Full `-m pytest -q` passed 803 tests and 75 subtests; Ruff `check liveaudio tests` and `git diff --check` passed, exit 0. GUI timeout tests prove the stopped state and warning content, not state observation during an open modal.
- **Performance acceptance:** no deterministic blocker. Targeted fake-only groups passed 205 tests/28 subtests, 21 diagnostics tests, and 97 tests/10 subtests. Conservative nominal audio capacity is one queued item for a 60-second window; no hard RSS or VRAM bound is claimed. Diagnostics duration history remains capped at 256 with fixed metric keys.
- **Research acceptance:** guide and requirement-to-test tracing accepted. Existing Audio/VAD purpose/window and independent decode budget instructions tell users to apply changes, distinguish phrase formation from decode time, warn about tens-of-seconds combined subtitle delay, and do not promise 200 ms or external consumer compatibility.
- **Architecture readback:** writer self-readback accepted, separately from independent QA/Performance/Research. Attempt-bound markers, full lazy iteration in the existing supervised process, kill/reap, canonical sanitized JSONL and capped presentation boundaries remain; WebSocket v1 and OBS HTML untouched. Parent inspected the config/engine/supervisor and GUI diffs without finding scope drift.
- **Fresh parent spot check:** `<source-workspace>/.venv/Scripts/python.exe -m pytest tests/test_decode_deadline.py tests/test_config.py -q` passed 100 tests and 28 subtests, exit 0. Parent `git diff --check` passed, exit 0; line-ending notices only.
- **Runtime harness:** synthetic capture/VAD through actual producer and consumer verifies a 60-second phrase, complete lazy iteration and distinct persistence/projection boundaries. Fake blocked lazy iterator in a real CPU child is terminated/reaped in GUI and headless supervision. Configured deadlines 5/15/60/120 are validated using fakes and elapsed-clock injection; no 120-second live GPU performance measurement is claimed.
- **Limitations:** no real ASR model, microphone, GPU/VRAM or occupied-GPU latency, quality benchmark, live GUI visual run, or OpenCoHost/OBS consumer integration was run. Hardware quality/performance remains a separate release activity. RDD remains off; native risk assessment was unavailable due to untracked declaration, so independent verification was treated as high-risk rather than downgraded. No receipt, remote operation or review approval is fabricated.
- **Follow-ups, not new leak claims:** GUI queue replacement currently drains/replaces without explicit `close()`; audit identifies lifecycle hygiene, not proof of a leak. Queue-full rejects the new completed phrase (does not evict older audio); warning delivery is best-effort when the log queue is full. These are not demonstrated new behavior changes; changing their policy is outside this feature's accepted scope.
- **One bounded slicing pass:** B1 alone required 719 authored source/test/docs lines; B2 overlaps config, UI, supervision and guide files. Independent delivery slices would not put the existing B1 behavior under 400 without splitting its regressions from behavior. Current coherent B1+B2 candidate is 948 authored lines (680 additions, 34 deletions, new 234-line integration test), excluding this recovery document. Recommend an explicit new feature-scoped `size:exception`, not test omission or code-golf. No exception is assumed from the earlier safety feature.
- **Rollback boundary:** the new purpose/window/queue-capacity and decode-budget changes listed above, with their tests and user guidance, can be removed together relative to feature base `4c3efcb`, preserving the separately committed prior safety lifecycle, canonical transcript preservation, and bounded diagnostics. No automatic reset/revert is authorized.

Current next step: ask the owner for a feature-scoped delivery exception before any local feature commit. B1/B2 implementation and B3 technical checks are accepted, but all work-unit completion boxes remain open until authorized local commit evidence is recorded. No staging, commit, push, PR or merge has occurred for this feature.

### Owner delivery authorization

Owner explicitly approved the new feature-scoped size exception and local commit after the 948-line candidate and no-publish/no-integration limits were presented. Auditor approves staging only this feature's exact inventory. This approval does not authorize remote work, integration, merge or additional source changes. All prior technical signoffs remain; a fresh precommit full suite, Ruff and diff check are being run. Commit evidence and closure will be recorded after the work-unit commit is observed.

### Local closeout

- **Feature commit:** `833463c7281f733825228d968b202248c13daccc` — `feat(buffering): configure phrase windows and decode deadlines`. It contains the coherent B1/B2 behavior and B3 user guidance/tests across 19 files: 914 insertions, 34 deletions. All three tasks share this work-unit evidence rather than artificial file-type splits. The prior safety base `4c3efcbcff1debee1cf04ff66bd8c4a91fe0cf14` is preserved.
- **Approved delivery:** owner granted feature-scoped `size:exception`; effective strategy `exception-ok`. This is local delivery only, without push, PR, integration or merge. Auditor approved the exact staged inventory before commit. No AI attribution was added.
- **Fresh precommit verification:** delegated QA reran full `pytest -q`: 803 passed and 75 subtests, exit 0. Ruff `check liveaudio tests` and `git diff --check` passed, exit 0. Parent reran config plus decode-deadline suite: 100 passed and 28 subtests, exit 0. Staged `git diff --cached --check` passed, exit 0. Source bytes did not change between verification and commit.
- **Review state:** RDD remains off (global decision); no native review or receipt was run or claimed. Existing independent signoffs and CPU-fake runtime evidence are recorded above.
- **Closure record:** this recovery document is the only remaining local documentation item and is committed as a bookkeeping-only closeout after the feature commit, so its exact work-unit identity can be recorded without a circular self-reference. Its Engram mirror is read back after update.
- **Next steps:** owner may test the retained feature worktree with Audio/VAD purpose/window and independent ASR timeout settings. Real microphone/model quality, GPU/VRAM pressure and external consumer compatibility remain unverified; no automatic integration or new scope is authorized. Lifecycle-hygiene/loss-policy caveats above remain follow-ups, not claimed leaks.