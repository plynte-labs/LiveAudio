# Implementation Plan: Test Foundation — Red-First TDD

## Phase 1: Test Infrastructure Setup
- [x] Task: Create `tests/` directory with `__init__.py`
- [x] Task: Verify `python -m unittest discover -s tests` runs (empty suite)
- [x] Task: Create test helper module `tests/helpers.py` with common mocks (audio device, WebSocket, ASR)
- [x] Task: Conductor - User Manual Verification 'Test Infrastructure Setup' (Protocol in workflow.md)

## Phase 2: Config Validation Tests (REQ-2)
- [x] Task: Write `tests/test_config.py` — Red phase (tests fail)
    - [x] Test `_clamp_number()` rejects values below min
    - [x] Test `_clamp_number()` rejects values above max
    - [x] Test `_clamp_number()` accepts values within range
    - [x] Test `_normalize_config()` produces consistent output for equivalent inputs
    - [x] Test `_sanitize_text()` strips dangerous characters
    - [x] Test `_sanitize_text()` handles None input
    - [x] Test `_sanitize_text()` handles empty string
    - [x] Test config validation rejects negative durations
    - [x] Test config validation rejects ports outside 1-65535
    - [x] Test config validation rejects empty required fields
- [x] Task: Fix config code to make tests pass — Green phase
- [x] Task: Verify all config tests pass
- [x] Task: Conductor - User Manual Verification 'Config Validation Tests' (Protocol in workflow.md)

## Phase 3: Subtitle Engine Logic Tests (REQ-3)
- [x] Task: Write `tests/test_engine.py` — Red phase (tests fail)
    - [x] Test `_obs_emit_decision()` with `live_only` policy drops old text
    - [x] Test `_obs_emit_decision()` with `send_all` policy queues all text
    - [x] Test `_obs_emit_decision()` with `auto` policy applies default behavior
    - [x] Test text formatting produces no empty lines
    - [x] Test VTT output includes proper `WEBVTT` header
    - [x] Test VTT output includes valid cue timestamps (`HH:MM:SS.mmm --> HH:MM:SS.mmm`)
    - [x] Test VTT output includes cue index numbers
- [x] Task: Fix engine code to make tests pass — Green phase
- [x] Task: Verify all engine tests pass
- [x] Task: Conductor - User Manual Verification 'Subtitle Engine Logic Tests' (Protocol in workflow.md)

## Phase 4: Audio Pipeline Tests (REQ-4)
- [x] Task: Write `tests/test_audio.py` — Red phase (tests fail)
    - [x] Test VAD threshold: silence produces no transcription output
    - [x] Test VAD threshold: noise below threshold produces no output
    - [x] Test audio energy threshold: low-quality segment rejected before ASR
    - [x] Test device disconnect triggers error handling
    - [x] Test audio_queue does not grow unbounded (backpressure)
    - [x] Test audio_queue drains correctly on normal operation
- [x] Task: Fix audio code to make tests pass — Green phase
- [x] Task: Verify all audio tests pass
- [x] Task: Conductor - User Manual Verification 'Audio Pipeline Tests' (Protocol in workflow.md)

## Phase 5: Network Layer Tests (REQ-5)
- [x] Task: Write `tests/test_network.py` — Red phase (tests fail)
    - [x] Test WebSocket connection lifecycle (connect, disconnect, reconnect)
    - [x] Test port binding with configurable port
    - [x] Test port conflict detection raises appropriate error
    - [x] Test text message delivered to OBS in correct format
    - [x] Test message routing handles empty message gracefully
- [x] Task: Fix network code to make tests pass — Green phase
- [x] Task: Verify all network tests pass
- [x] Task: Conductor - User Manual Verification 'Network Layer Tests' (Protocol in workflow.md)

## Phase 6: Noise/False-Positive Detection Tests (REQ-6)
- [x] Task: Extend `tests/test_audio.py` and `tests/test_engine.py` — Red phase (tests fail)
    - [x] Test VAD layer: silence/noise below threshold → zero transcription
    - [x] Test blacklist layer: "bullet roulette" filtered from output
    - [x] Test blacklist layer: common gambling terms filtered from output
    - [x] Test energy layer: high noise floor segment rejected before ASR
    - [x] Test energy layer: low SNR segment rejected before ASR
    - [x] Test combined: noise + blacklist term → double protection
- [x] Task: Fix audio/engine code to make tests pass — Green phase
- [x] Task: Verify all noise detection tests pass
- [x] Task: Conductor - User Manual Verification 'Noise/False-Positive Detection Tests' (Protocol in workflow.md)

## Phase 7: Collective Review & Sign-Off
- [x] Task: Run `python -m unittest discover -s tests` — all tests must pass (AC-1)
- [x] Task: Verify each REQ has ≥3 test cases (AC-3)
    - REQ-1: 3 tests (infrastructure)
    - REQ-2: 35 tests (config validation)
    - REQ-3: 22 tests (engine logic)
    - REQ-4: 19 tests (audio pipeline)
    - REQ-5: 15 tests (network layer)
    - REQ-6: 18 tests (noise detection)
- [x] Task: Verify no test requires real hardware/GPU/network (AC-4)
- [x] Task: Auditor summons 4 agents for collective review
    - [x] Architecture: config validation, multiprocessing lifecycle, test isolation
    - [x] Performance: queue backpressure tests, VAD throughput simulation
    - [x] QA: test completeness, regression risk, manual test plan alignment
    - [x] Research: traceability (REQ → test cases), coverage audit
- [x] Task: Resolve any issues found during review
- [x] Task: Conductor - User Manual Verification 'Collective Review & Sign-Off' (Protocol in workflow.md)

## Phase 8: Presentation & Merge
- [x] Task: Auditor presents test results to user
- [x] Task: User approval for merge
- [x] Task: Merge to `master`, delete feature branch
- [x] Task: Update `conductor/tracks.md` to `[x] Completed`
- [x] Task: Conductor - User Manual Verification 'Presentation & Merge' (Protocol in workflow.md)

## Phase: Review Fixes
- [x] Task: Apply review suggestions `3e48fe0`
