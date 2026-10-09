# Transcript export timing and completeness

## Objective and authorized scope
Fix new-capture VTT timing, retain full filtered transcription in exports, and persist safe per-segment duration/config diagnostics. Preserve all historical session files. Do not change Whisper, VAD segmentation, preview/WS clipping, audio recovery or queue-loss policies.

## Approach and constraints
Use capture monotonic anchors relative to one session origin retained across GUI/service worker restarts. Derive duration from PCM samples at 16 kHz. Preserve real interruption gaps. Never substitute processing delay for missing capture times or rewrite old records. Configuration metadata must be explicitly allowlisted, excluding prompts, paths, credentials and device identifiers.

## Tasks and route
- [x] T1: Correct continuous audio-based VTT timing and session-origin lifecycle; synthetic GUI hot-swap/new-session and service-lazy-start checks pass.
- [x] T2: Export full filtered text to VTT, retaining the 600-character OBS/WebSocket projection; synthetic 700-character transcript regression passes.
- [x] T3: Persist PCM-derived audio duration and allowlisted capture/decode settings; privacy regression and documentation update completed.

Route: delegated direct, one bounded writer; four specialist mapping handoffs completed. Trigger: multiple production files and analysis before writing. Scope approved by owner, including future-only behavior.

## Acceptance and checks
Strict TDD enabled by AGENTS.md: observe RED, then GREEN and REFACTOR. Runner: `.venv/Scripts/python.exe -m pytest`. Focus: tests/test_engine.py, tests/test_vad_asr_integration.py, tests/test_async_io.py, tests/test_output_sinks.py, tests/test_apply_settings_save.py; add focused lifecycle checks as needed. Run Ruff and git diff --check. No real models, microphone or private session fixtures.

Existing environment caveat: previous full/decode suites stalled with multiprocessing Manager pipe failures; unrelated decode assertion remains undiagnosed. Do not claim full-suite proof from focused results.

## Delivery and recovery
Current feature branch: feature/configurable-transcription-buffers. Initial estimate was 150–220 authored lines; after the bounded correction, current source/test/doc diff is 363 insertions and 6 deletions (369 authored changed lines), advisory only. Strategy: ask-on-risk. No push or PR authorized. Auditor must approve commits after observed verification; preserve unrelated untracked files. RDD reported off, but status access failed; do not enable it or fabricate approval.

## Progress
Final auditor update: independent QA correction checks passed 4 tests in 3.83s; architecture recheck confirmed the service clock blocker addressed. Parent spot-check passed 3 timeline/legacy/restart tests in 3.23s with workspace-local TEMP after default-temp permission failures. Broader suite proof remains partial; no real hardware/model tests. A local commit attempt was rejected before execution by auto-review because explicit auditor approval was not recognized. No workaround or retry; commit identity pending explicit owner approval. No push/PR or historical session edits. This update supersedes the pending-verification/no-attempt wording in the prior writer snapshot below. Engram mirror refreshed with this complete document.

Strict TDD RED: the capture VTT/config tests and fake 60-second producer integration failed on absent required behavior before production edits; the GUI apply/new-session epoch regression failed with the old epoch retained; and the service clock regression failed with supervisor clock 1000 instead of monotonic origin 400. The service fix now uses `time.monotonic()` for session origin without changing supervisor `_clock` scheduling semantics. Added synthetic multi-phrase VTT verification with different decode delays and direct consumer restart: offsets 5–6 and 50–51 remain capture-derived and cue IDs continue 1/2. The 60-second fake VAD-to-ASR integration now verifies VTT output. Final targeted command: 10 passed. `ruff check liveaudio tests` passed; `git diff --check` passed. Separate async/output/app tests had 30 passed and 20 subtests passed. The prescribed broad focus-set previously hit an environment failure in `tests/test_engine.py::TestObsEnabledGate::test_obs_enabled_true_emits_to_queue` (`multiprocessing.Manager().dict` Windows named-pipe `FileNotFoundError`); an unbounded combined run stalled and was interrupted. Full suite intentionally not run. Implementation remains uncommitted and unstaged; no commit attempted. Engram mirror pending initial synchronization. Next: independent verification and auditor readback.
