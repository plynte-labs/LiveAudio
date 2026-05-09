# Implementation Plan: Test Foundation — Red-First TDD

## Phase 1: Test Infrastructure Setup
- [ ] Task: Create `tests/` directory with `__init__.py`
- [ ] Task: Verify `python -m unittest discover -s tests` runs (empty suite)
- [ ] Task: Create test helper module `tests/helpers.py` with common mocks (audio device, WebSocket, ASR)
- [ ] Task: Conductor - User Manual Verification 'Test Infrastructure Setup' (Protocol in workflow.md)

## Phase 2: Config Validation Tests (REQ-2)
- [ ] Task: Write `tests/test_config.py` — Red phase (tests fail)
    - [ ] Test `_clamp_number()` rejects values below min
    - [ ] Test `_clamp_number()` rejects values above max
    - [ ] Test `_clamp_number()` accepts values within range
    - [ ] Test `_normalize_config()` produces consistent output for equivalent inputs
    - [ ] Test `_sanitize_text()` strips dangerous characters
    - [ ] Test `_sanitize_text()` handles None input
    - [ ] Test `_sanitize_text()` handles empty string
    - [ ] Test config validation rejects negative durations
    - [ ] Test config validation rejects ports outside 1-65535
    - [ ] Test config validation rejects empty required fields
- [ ] Task: Fix config code to make tests pass — Green phase
- [ ] Task: Verify all config tests pass
- [ ] Task: Conductor - User Manual Verification 'Config Validation Tests' (Protocol in workflow.md)

## Phase 3: Subtitle Engine Logic Tests (REQ-3)
- [ ] Task: Write `tests/test_engine.py` — Red phase (tests fail)
    - [ ] Test `_obs_emit_decision()` with `live_only` policy drops old text
    - [ ] Test `_obs_emit_decision()` with `send_all` policy queues all text
    - [ ] Test `_obs_emit_decision()` with `auto` policy applies default behavior
    - [ ] Test text formatting produces no empty lines
    - [ ] Test VTT output includes proper `WEBVTT` header
    - [ ] Test VTT output includes valid cue timestamps (`HH:MM:SS.mmm --> HH:MM:SS.mmm`)
    - [ ] Test VTT output includes cue index numbers
- [ ] Task: Fix engine code to make tests pass — Green phase
- [ ] Task: Verify all engine tests pass
- [ ] Task: Conductor - User Manual Verification 'Subtitle Engine Logic Tests' (Protocol in workflow.md)

## Phase 4: Audio Pipeline Tests (REQ-4)
- [ ] Task: Write `tests/test_audio.py` — Red phase (tests fail)
    - [ ] Test VAD threshold: silence produces no transcription output
    - [ ] Test VAD threshold: noise below threshold produces no output
    - [ ] Test audio energy threshold: low-quality segment rejected before ASR
    - [ ] Test device disconnect triggers error handling
    - [ ] Test audio_queue does not grow unbounded (backpressure)
    - [ ] Test audio_queue drains correctly on normal operation
- [ ] Task: Fix audio code to make tests pass — Green phase
- [ ] Task: Verify all audio tests pass
- [ ] Task: Conductor - User Manual Verification 'Audio Pipeline Tests' (Protocol in workflow.md)

## Phase 5: Network Layer Tests (REQ-5)
- [ ] Task: Write `tests/test_network.py` — Red phase (tests fail)
    - [ ] Test WebSocket connection lifecycle (connect, disconnect, reconnect)
    - [ ] Test port binding with configurable port
    - [ ] Test port conflict detection raises appropriate error
    - [ ] Test text message delivered to OBS in correct format
    - [ ] Test message routing handles empty message gracefully
- [ ] Task: Fix network code to make tests pass — Green phase
- [ ] Task: Verify all network tests pass
- [ ] Task: Conductor - User Manual Verification 'Network Layer Tests' (Protocol in workflow.md)

## Phase 6: Noise/False-Positive Detection Tests (REQ-6)
- [ ] Task: Extend `tests/test_audio.py` and `tests/test_engine.py` — Red phase (tests fail)
    - [ ] Test VAD layer: silence/noise below threshold → zero transcription
    - [ ] Test blacklist layer: "bullet roulette" filtered from output
    - [ ] Test blacklist layer: common gambling terms filtered from output
    - [ ] Test energy layer: high noise floor segment rejected before ASR
    - [ ] Test energy layer: low SNR segment rejected before ASR
    - [ ] Test combined: noise + blacklist term → double protection
- [ ] Task: Fix audio/engine code to make tests pass — Green phase
- [ ] Task: Verify all noise detection tests pass
- [ ] Task: Conductor - User Manual Verification 'Noise/False-Positive Detection Tests' (Protocol in workflow.md)

## Phase 7: Collective Review & Sign-Off
- [ ] Task: Run `python -m unittest discover -s tests` — all tests must pass (AC-1)
- [ ] Task: Verify each REQ has ≥3 test cases (AC-3)
- [ ] Task: Verify no test requires real hardware/GPU/network (AC-4)
- [ ] Task: Auditor summons 4 agents for collective review
    - [ ] Architecture: config validation, multiprocessing lifecycle, test isolation
    - [ ] Performance: queue backpressure tests, VAD throughput simulation
    - [ ] QA: test completeness, regression risk, manual test plan alignment
    - [ ] Research: traceability (REQ → test cases), coverage audit
- [ ] Task: Resolve any issues found during review
- [ ] Task: Conductor - User Manual Verification 'Collective Review & Sign-Off' (Protocol in workflow.md)

## Phase 8: Presentation & Merge
- [ ] Task: Auditor presents test results to user
- [ ] Task: User approval for merge
- [ ] Task: Merge to `master`, delete feature branch
- [ ] Task: Update `conductor/tracks.md` to `[x] Completed`
- [ ] Task: Conductor - User Manual Verification 'Presentation & Merge' (Protocol in workflow.md)
