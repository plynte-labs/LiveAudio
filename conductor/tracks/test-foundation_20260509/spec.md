# Track: Test Foundation — Red-First TDD

## Overview
Establish the `tests/` directory with a comprehensive unittest suite covering config validation, subtitle engine logic, audio pipeline, network layer, and noise/false-positive detection. Tests are written to fail first (red phase), then code is fixed to make them pass (green phase).

## Functional Requirements

### REQ-1: Test Infrastructure
- `tests/` directory created with `__init__.py`.
- Test discovery works with `python -m unittest discover -s tests`.
- Tests are organized by module: `test_config.py`, `test_engine.py`, `test_audio.py`, `test_network.py`.

### REQ-2: Config Validation Tests
- `_clamp_number()` rejects values outside min/max bounds.
- `_normalize_config()` produces consistent output for equivalent inputs.
- `_sanitize_text()` strips dangerous characters, handles None/empty input.
- Config validation rejects unsafe values (negative durations, ports outside range, empty required fields).

### REQ-3: Subtitle Engine Logic Tests
- `_obs_emit_decision()` correctly implements backlog policies (`auto`, `live_only`, `send_all`).
- Text formatting produces valid output (no empty lines, proper encoding).
- VTT output includes proper WebVTT cue format with timestamps.

### REQ-4: Audio Pipeline Tests
- VAD threshold enforcement: silence/noise below threshold produces no transcription.
- Audio energy threshold: low-quality segments rejected before ASR.
- Device disconnect detection triggers appropriate error handling.
- Queue backpressure: audio_queue does not grow unbounded.

### REQ-5: Network Layer Tests
- WebSocket connection lifecycle: connect, disconnect, reconnect.
- Port binding: configurable port, conflict detection.
- Message routing: text messages delivered to OBS in correct format.

### REQ-6: Noise/False-Positive Detection (3 layers)
- **VAD layer**: Silence/noise below VAD threshold produces zero transcription output.
- **Blacklist layer**: Known gambling terms ("bullet roulette", etc.) are filtered from output even if ASR produces them.
- **Energy layer**: Low-quality audio segments (high noise floor, low SNR) are rejected before reaching ASR.

## Non-Functional Requirements
- All tests use `unittest` (no external test dependencies beyond stdlib).
- Tests run in under 30 seconds total.
- Tests are deterministic (no flaky tests).
- Tests do not require GPU, audio hardware, or network access (mock external dependencies).

## Acceptance Criteria
- **AC-1**: `python -m unittest discover -s tests` runs with zero errors.
- **AC-2**: All tests pass (green) after code fixes are applied.
- **AC-3**: Each REQ (1-6) has at least 3 test cases covering happy path, edge case, and failure mode.
- **AC-4**: No test requires real audio hardware, GPU, or live WebSocket connection.
- **AC-5**: Test coverage on tested modules is measurable (via `python -m coverage` if available, or manual audit).

## Out of Scope
- Integration tests requiring real OBS connection.
- Long-session resilience tests (handled by log analysis protocol).
- Performance benchmarks (handled by separate track).
- UI/GUI testing.
