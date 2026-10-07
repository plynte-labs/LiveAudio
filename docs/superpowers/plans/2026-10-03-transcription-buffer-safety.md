# Transcription Buffer Safety Implementation Plan

> **DRAFT — planning only.** This document does not authorize implementation, tests, model loading, configuration changes, branch changes, commits, downloads, or external integration work. The user must review this plan and separately approve execution.

**Goal:** Make LiveAudio's transcript, queue, persistence, failure, and latency behavior explicit and bounded before exposing longer 30/60-second transcription windows, while keeping fast-caption settings available.

**Architecture:** Preserve one final ASR result as the canonical transcript record and keep the existing localhost WebSocket sink separate from disk persistence. This plan covers only the integrity/resource prerequisites that must be addressed before larger windows are considered. It does not design or implement the later 30/60-second configuration/profile feature.

**Tech Stack:** Existing Python 3.10–3.12, `faster-whisper` `WhisperModel`, multiprocessing queues, `queue.Queue`, Tk/CustomTkinter UI, WebSocket v1 payload, and pytest. No dependency additions.

**Spec:** The user-approved outcome is to prepare this correction plan before considering larger transcription windows. Fast subtitles, 30/60-second choices, and combined-mode delay warnings are future product intent only; this plan does not select their UI/config semantics. 1–3/5 seconds are measurement goals, not promises; 200 ms is aspirational. No separate written feature specification was approved.

## Global constraints

- Session-injected instructions enable Strict TDD; every implementation task follows observed RED → GREEN → REFACTOR. The on-disk root `AGENTS.md` does not state this mode.
- Preserve today's defaults and sink toggles unless a decision gate below is resolved; do not alter the user's persisted configuration as part of defaults or migration.
- Never claim 30/60-second data is lossless. Surface capture overflow, dropped phrases, ASR timeout, disk failure, and unflushed shutdown distinctly.
- Keep transcripts/audio local; do not add transcript text to diagnostics. Preserve sanitization, localhost binding, Origin validation, and safe `textContent` rendering.
- No OpenCoHost repository, endpoint, credential, port, or remote session access. External protocol compatibility is unknown and must not be claimed.
- Do not substitute `BatchedInferencePipeline` for the current ordinary `WhisperModel`; do not add it as a dependency.
- Keep the current 5-second default, current `silence_timeout`, and existing output-sink settings unchanged until the user reviews the proposed UI semantics.
- Model-only benchmark results below do not prove end-to-end latency, transcript quality, process memory attribution, or long-session stability.

## Quick path

1. Review the evidence and resolve the decision gates before implementation.
2. Implement and verify the integrity/resource prerequisites below, task by task, only after separate execution approval.
3. After prerequisites close, request a separate feature design/authorization for any 30/60-second configuration or usage profile.

## Dependency map

| Work unit | Depends on | May proceed independently? |
|---|---|---|
| Task 1: canonical JSONL preservation | Execution approval | Yes; preserve current WS wire behavior until Gate 5 is resolved. Only a later, conditional wire change/test depends on Gate 5. |
| Task 2: bounded diagnostics | Execution approval | Yes |
| Task 3: writer admission/drain | Gate 2 | No |
| Task 4: real decode deadline | Gate 1 | No |
| Task 5: lifecycle accounting/latency | Gate 3; integrates Tasks 3–4 outcomes | No |
| Future profile/long-window feature | Separate feature authorization and review of Gates 4–5 | Outside this plan |

The source-and-test work spans multiple files; after execution is separately approved, use delegated implementation work units. Any branch/worktree and commits require separate execution approval; this draft creates no execution state.

## Current evidence

| Concern | Current behavior and source | Consequence |
|---|---|---|
| Text completeness | `MAX_TRANSCRIPT_CHARS = 600` is applied by `_sanitize_text` before both JSONL persistence and WebSocket emission (`liveaudio/core/engine.py:24,163-171,667-700,734-748`). The overlay also independently caps presentation to 600 chars using `textContent` (`liveaudio/assets/subtitulos_obs.html:290,778-780,897`). | The canonical transcript is truncated before it is saved. Keep the UI presentation cap separate from canonical text. |
| VAD and duration | `max_chunk_duration` defaults to 5 seconds and config clamps it to 2–15 seconds (`liveaudio/utils/config.py:111-112,188-190`). VAD closes on silence or the duration ceiling (`liveaudio/core/audio.py:178-186,354-380`). | 30/60-second windows need an intentional config/UI change; `subtitle_max_live_delay_sec` is an OBS policy threshold, not an ASR window (`engine.py:220-236`, `config.py:251-257`). |
| ASR deadline | `_transcribe_with_timeout` times `future.result(timeout=15)` (`engine.py:239-297`), but `model.transcribe()` returns a lazy segment iterator; the consumer advances it later (`engine.py:646-674`). Python cannot cancel already-running native/CUDA inference by cancelling the future. | The current 15-second timeout does not bound lazy decoding and cannot be presented as a hard decode deadline. |
| Persistence and diagnostics | `SessionWriter` has an unbounded queue, suppresses file errors, and joins for two seconds (`engine.py:111-152`); its `flush()` is not called by the ASR shutdown path (`engine.py:822-825`). `DiagnosticsStore.record_duration` appends to unbounded lists (`liveaudio/core/diagnostics.py:74-111`). | A long session can grow memory without bound; disk errors or incomplete shutdown can be invisible. |
| Queue and lifecycle | Audio/text queues are each capped at 100 *items* (`liveaudio/app.py:90,340-343`). Audio queue full drops a phrase (`liveaudio/core/audio.py:295-315`); text queue full drops OBS output after persistence is queued (`engine.py:697-700,746-790`). Hot-swap terminates workers and replaces/drains the audio queue (`app.py:2038-2071`). | These are different loss domains, not a seconds-based window. Settings apply and stop need explicit in-flight semantics. |
| Delay telemetry | `created_at` is assigned only when VAD enqueues a completed phrase (`audio.py:295-303`); `total_delay` is computed from that timestamp (`engine.py:624-676`). | Current totals omit capture and utterance-formation time and cannot support a truthful end-to-end warning. |
| Model feasibility | Offline Spanish synthetic-TTS benchmark; cached small model, CUDA/float16; faster-whisper 1.2.1, CTranslate2 4.8.0, torch 2.5.1+cu121. Three trials each: 5s median 0.404s; 15s 0.725s; 30s 1.374s; 60s 2.318s. One 60s output had 886 characters, exceeding the current 600-character canonical cap. | Model-only inference looked feasible on that machine/input. VRAM readings were global-system samples, not attributable per process; RSS measurement was invalid/unknown. No leak was proved. No end-to-end, P95, quality, or long-session test was run. |

The measured configuration was the persisted small/CUDA profile, 5-second max, and 0.8-second silence timeout; this is not proof of a currently running worker's effective settings. The ordinary locked `WhisperModel` supports long input through internal windows; `condition_on_previous_text=False` means text continuity across internal windows is not guaranteed (`liveaudio/core/engine.py:249-257`; `uv.lock`). The benchmark was offline and synthetic; it is not evidence of user-audio quality, application latency, per-process VRAM/RSS, or sustained-session behavior.

## Files and responsibilities

| File | Planned responsibility |
|---|---|
| `liveaudio/core/engine.py` | Separate canonical transcript sanitization from presentation limits; ASR decode deadline/failure contract; writer backpressure, errors, and drain. |
| `liveaudio/core/audio.py` | Capture/utterance timing and explicit bounded-buffer overflow evidence. |
| `liveaudio/core/diagnostics.py` | Bounded duration sample retention while preserving counters and snapshot shape. |
| `liveaudio/app.py` | Worker lifecycle and truthful process/drain failure reporting. |
| `tests/test_engine.py`, `tests/test_output_sinks.py` | Canonical transcript and independent sink behavior. |
| `tests/test_resilience_asr.py`, `tests/test_async_io.py` | Lazy decode timeout and writer drain/error behavior. |
| `tests/test_diagnostics.py` | Bounded duration retention and stable snapshot shape. |
| `tests/test_audio.py`, `tests/test_backpressure.py` | Capture timestamps, queue overflow and accounting. |
| `tests/test_config.py`, `tests/test_network.py` | Window validation and existing wire/replay contract regression. |

Only the plan file is created by this task. No code, tests, config, or separate feature/spec artifacts are authorized here.

## Decision gates — resolve before the dependent tasks

1. **Decode timeout recovery:** Choose between (A) a supervised ASR-process restart on a genuine decode deadline, with an explicitly surfaced loss of the in-flight utterance, or (B) a replayable/idempotent handoff that retains the in-flight audio across worker restart, at higher queue/protocol complexity. A Python thread timeout is not an acceptable substitute. No 30/60-second release until this is resolved and tested.
2. **Writer failure/full queue:** Choose bounded ASR backpressure with a finite admission wait, or fail-closed capture stop with a visible storage error. The first choice can fill the bounded audio queue and lose later capture; after its admission deadline, any not-yet-accepted record must be explicitly rejected rather than waiting forever. In either branch, never silently discard an accepted transcript. No lossless-storage promise while disk errors remain possible.
3. **Stop/hot-swap/overflow:** Decide the user-visible policy for partial speech, queued audio, and writer-pending records when stopping or applying a setting. Options are bounded graceful drain with an explicit timeout and surfaced remainder, or immediate interruption with a precise loss warning. Do not clear queues and report a clean stop.
4. **Profile semantics:** Confirm whether usage profiles control segmentation/warnings only while the existing independent `obs_enabled`, `save_transcript_enabled`, `save_vtt_enabled`, and `subtitle_backlog_policy` settings remain authoritative, or whether selecting a profile should also change sink/backlog behavior. Proposed default is to leave these settings independent.
5. **Long caption presentation/wire:** The built-in overlay and current server WS projection cap text at 600 chars; the external OpenCoHost contract is unknown. For a later feature, confirm whether those caps remain while canonical JSONL stays complete, or whether long final phrases should be split into ordered caption messages. Do not claim external compatibility until the actual consumer contract is verified.

Gates 1–3 are prerequisites only for dependent safety behavior. Gates 4–5 concern a later feature and must not block Tasks 1–3 or any independent prerequisite. These decisions are not assumptions an implementer may fill in; stop only the dependent behavior and report its exact gate.

## Review focus

1. Lazy segment iteration hangs beyond the existing timeout; the timeout test must advance a blocking generator, not only mock `Future.result()`.
2. Slow/failing storage must not silently drop records or block the UI indefinitely; tests must exercise queue-full, write-error, flush timeout, and shutdown behavior.
3. Long canonical transcripts must remain complete in JSONL; display bounds and browser-safe text rendering remain independently tested.
4. Capture overflow and hot-swap must identify which audio was retained or lost; metrics must start at capture, not phrase enqueue.
5. Longer windows must not weaken localhost/Origin boundaries or leak transcript text into diagnostics; existing WS contract and redaction tests remain in the regression suite.

## Tasks

### Task 1: Preserve canonical transcript text

**Files:** `liveaudio/core/engine.py`; `tests/test_engine.py`; `tests/test_output_sinks.py`; `tests/test_network.py`.

**Interface:** Canonical JSONL text is sanitized for trust-boundary safety but not length-truncated. Keep the current WS v1 wire text behavior (including its existing server-side 600-character cap) and overlay presentation unchanged until Gate 5 is resolved; implement any altered WS text/splitting behavior only as a separately gated change. Gate 5 does not block complete local JSONL preservation. Keep the overlay's `textContent` rendering.

- [ ] **RED:** Replace `test_truncates_long_text` with `test_canonical_sanitizer_preserves_long_text_and_removes_unsafe_chars`; add `test_long_canonical_text_is_preserved_in_jsonl` proving JSONL remains complete, plus a WS regression assertion that the current v1 field/cap is unchanged. Do not test a new WS wire representation until Gate 5 is resolved.
- [ ] **GREEN:** Separate canonical text from the existing 600-character WS presentation projection; preserve independent disk sink toggles and current WS message fields/behavior.
- [ ] **REFACTOR:** Keep one sanitizer path for canonical text and remove stale `MAX_TRANSCRIPT_CHARS` imports/usages where no longer valid.
- [ ] **Verify:** `.\.venv\Scripts\python.exe -m pytest tests/test_engine.py tests/test_output_sinks.py tests/test_network.py -q`.
- **Commit intent (future only, after auditor approval):** `fix(transcript): preserve complete canonical text`.

### Task 2: Bound diagnostics memory

**Files:** `liveaudio/core/diagnostics.py`; `tests/test_diagnostics.py`.

**Interface:** Proposed cap: keep the most recent 256 duration samples per metric; this bounds memory and snapshot size while retaining a recent operational trend, and is a planning proposal rather than approved policy. Preserve snapshot schema version 1 and JSON-list `durations`; counters remain cumulative. Future metric/tag keys must have finite cardinality; this is a design constraint, not a claim that current callsites have a variable-key leak. Do not persist transcript/audio values.

- [ ] **RED:** Add `test_duration_samples_are_bounded_and_recent`, appending more than 256 values and asserting snapshot keeps exactly the latest 256 in order.
- [ ] **GREEN:** Use a fixed-size rolling sample container internally; materialize lists in `snapshot_runtime_health()` without changing privacy filtering or the snapshot schema.
- [ ] **REFACTOR:** Confirm tags/metric keys do not create a new payload field or bypass privacy sanitization.
- [ ] **Verify:** `.\.venv\Scripts\python.exe -m pytest tests/test_diagnostics.py -q`.
- **Commit intent (future only, after auditor approval):** `fix(diagnostics): bound duration sample retention`.

### Task 3: Bound transcript writer and make persistence failures visible

**Files:** `liveaudio/core/engine.py`; `tests/test_async_io.py`; `tests/test_output_sinks.py`; `tests/test_resilience_asr.py`.

**Interface:** Preserve the current `SessionWriter.write_record(record, vtt_start, vtt_end, texto_final, cue_counter, write_transcript=True, write_vtt=True) -> None` call shape unless implementation evidence requires a narrow compatible extension. Proposed finite queue capacity: 32 records (planning value, to validate). Admission-full behavior is conditional on Gate 2: either bounded backpressure or fail-closed capture stop; in both cases an unaccepted/rejected record is reported as not accepted, while an accepted record is never silently dropped. A later write failure must be surfaced as “accepted but unsaved/failed,” not confused with rejection. Proposed `flush(timeout_sec: float) -> bool` returns true only after all accepted writes finish successfully, false on deadline, and reports sticky write error. Proposed `stop(timeout_sec: float) -> bool` reports whether the writer drained/exited. Start the stop deadline before attempting sentinel/control admission; control signaling must not wait indefinitely on a full/stuck data queue (use a separate nonblocking control signal or equivalent bounded mechanism). Propagate failure/drain status to ASR/UI before process termination. These signatures and limits are proposals, not approved policy.

- [ ] **RED:** Add `test_writer_admission_is_bounded_and_accounts_for_rejection`, `test_writer_error_distinguishes_accepted_unsaved_record`, `test_flush_waits_for_all_prior_writes`, and `test_stop_deadline_includes_nonblocking_sentinel_admission`. Exercise slow/stuck writer, JSONL/VTT failures, drain completion, queue full, and a stop signal that cannot block indefinitely. Assert accepted records are never silently discarded and unaccepted records are explicitly rejected.
- [ ] **GREEN:** Implement bounded enqueue and an observable writer error/result; integrate orderly drain in ASR shutdown. Do not claim disk durability until the write completed successfully.
- [ ] **REFACTOR:** Keep JSONL/VTT sink toggles independent and ensure one sink's failure is not misreported as the other sink succeeding.
- [ ] **Verify:** `.\.venv\Scripts\python.exe -m pytest tests/test_async_io.py tests/test_output_sinks.py tests/test_resilience_asr.py -q`.
- **Commit intent (future only, after auditor approval):** `fix(transcript): bound writer backlog and surface errors`.

**Blocker:** Resolve Decision Gate 2 before choosing full-queue/storage-failure behavior. Both choices must retain a finite admission bound, bounded stop-control signaling, explicit rejected-vs-accepted failure accounting, and no silent drop of accepted transcripts. Capacity is a planning proposal, not a durability guarantee.

### Task 4: Enforce a real decode deadline at a killable boundary

**Files:** `liveaudio/core/engine.py`; `liveaudio/core/workers.py`; `liveaudio/app.py`; `liveaudio/service/supervisor.py`; `tests/test_resilience_asr.py`; `tests/test_resilience_service_backend.py`.

**Interface:** Proposed deadline: preserve the existing 15-second constant as the full-decode budget (iterator creation plus complete lazy iteration) until a separate review approves another value; this is not an assertion that 15 seconds is already enforced. Implement at a killable supervised process boundary and prove the expired ASR process exits, not merely that a future/thread reports timeout. Integrate both GUI lifecycle (`app.py`) and headless `ServiceSupervisor`; its provisioning watchdog is advisory and does not kill children, while child cleanup has terminate/kill fallbacks (`liveaudio/service/supervisor.py:391-419,438-462,540-580`). Keep model provisioning/loading separately timed from per-utterance decode.

- [ ] **RED:** Add `test_decode_deadline_covers_lazy_segment_iteration` with a generator that blocks only when advanced; assert the supervised ASR process actually exits by the deadline and timeout state includes utterance ID and elapsed decode duration. Add GUI and headless-service lifecycle tests (including `tests/test_resilience_service_backend.py`) proving supervision observes/reaps/restarts or reports the worker according to Gate 1. A mocked future timeout alone is insufficient.
- [ ] **RED:** Add tests for worker termination/restart state and the selected in-flight utterance policy from Decision Gate 1.
- [ ] **GREEN:** Implement only the owner-approved process supervision/replay design. Do not use `future.cancel()` or `shutdown(wait=False)` as proof that native inference stopped.
- [ ] **REFACTOR:** Keep a timed-out decode from creating a partial canonical record or duplicate transcript/WS event; preserve model-load error reporting.
- [ ] **Verify:** `.\.venv\Scripts\python.exe -m pytest tests/test_resilience_asr.py tests/test_engine.py tests/test_resilience_service_backend.py -q`.
- **Commit intent (future only, after auditor approval):** `fix(asr): enforce decode deadline at supervised boundary`.

**Blocker:** The existing ASR child process can be terminated, but its current queue item has already been consumed. Do not implement restart/replay until Decision Gate 1 chooses whether to surface that loss or preserve/retry it idempotently.

### Task 5: Track capture-to-final and server-side sink timing with explicit overflow/lifecycle outcomes

**Files:** `liveaudio/core/audio.py`; `liveaudio/core/engine.py`; `liveaudio/core/network.py`; `liveaudio/app.py`; `tests/test_audio.py`; `tests/test_backpressure.py`; `tests/test_network.py`.

**Interface:** Carry a monotonic capture timestamp and stable sequence through VAD closure, ASR completion, persistence, and WS enqueue. Keep wall-clock timestamps only where serialized records need them. Record phase durations/drop counters without transcript text. Distinguish capture-to-final, queue wait, decode, disk write, and server-side WS queue/send timing. There is no client delivery acknowledgment; do not report client delivery latency without a separately authorized acknowledgment contract.

- [ ] **RED:** Add `test_capture_to_finalize_includes_utterance_formation_time`; queue, writer, ring-buffer, and WS overflow tests assert per-boundary counters and explicit reasons.
- [ ] **RED:** Add `test_stop_reports_unflushed_vad_partial_phrase` for speech still held in the VAD-local `speech_buffer`, distinct from `test_stop_reports_pending_audio_and_writer_records` for queued audio and writer-pending records; add `test_hot_swap_reports_interrupted_phrase`. Exercise whichever partial-phrase/drain policy Gate 3 selects. Source currently joins the VAD thread after clearing its run flag without a final enqueue (`liveaudio/core/audio.py:283-304,496-510`).
- [ ] **GREEN:** Timestamp capture chunks and propagate first/last audio timing; expose clear stop/hot-swap outcome events and preserve observed drop accounting.
- [ ] **REFACTOR:** Keep replay pacing separate from ASR and server-side WS queue/send metrics; retain localhost bind, Origin handling, bounded WS replay/retry, and v1 event compatibility.
- [ ] **Verify:** `.\.venv\Scripts\python.exe -m pytest tests/test_audio.py tests/test_backpressure.py tests/test_network.py tests/test_resilience_asr.py -q`.
- **Commit intent (future only, after auditor approval):** `fix(audio): report end-to-end latency and overflow`.

**Blocker:** Apply the explicit policy from Decision Gate 3; do not translate a queue drain or process exit into a claim that all captured speech was preserved.

**Release gate:** Do not increase the validated max window or expose new choices within this prerequisite work. After Tasks 1–5 and Decision Gates 1–3 close, create a separate feature design for 30/60-second options, profile/sink interactions, truthful warnings, and any external compatibility work; resolve Gates 4–5 there. Gates 4–5 do not block prerequisite tasks. OpenCoHost compatibility and cross-window linguistic continuity remain unverified; users must not be promised either based solely on this plan.

## Verification and delivery

- Focused commands are listed per task; final functional gate: `.\.venv\Scripts\python.exe -m pytest -q`.
- Static check after source changes: `.\.venv\Scripts\ruff.exe check liveaudio tests`.
- Do not run tests/model inference as part of this planning task. At execution time, a synthetic local end-to-end ASR benchmark using only the already-cached model must be separately authorized; no microphone capture, model download, or external service is part of the default test suite.
- Each task is a reviewable work unit with tests beside the behavior. The roughly 400-authored-line figure is a planning heuristic, not a forced split or a reason to omit tests. Future commits require auditor approval and a feature branch/worktree decision; no branch, commit, push, or PR is authorized by this plan.
- Ordinary repository policy owns delivery; this plan does not select SDD, create execution tracking state, or authorize a review/commit lifecycle.

## Next step

Review this draft and resolve Gates 1–3 before their dependent prerequisite tasks. Gates 4–5 are deferred to the separately authorized future larger-buffer feature. Separately authorize implementation before any changes; retain the current 5-second default until then.
