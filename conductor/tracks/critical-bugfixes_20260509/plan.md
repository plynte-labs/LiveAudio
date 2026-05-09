# Implementation Plan: Critical Bug Fixes — Resilience & Stability

## Phase 1: ASR Freeze Recovery (REQ-1)
- [x] Task: Write `tests/test_resilience_asr.py` — Red phase (tests fail)
    - [x] Test transcribe timeout triggers after 15s
    - [x] Test timeout emits warning status to log_queue
    - [x] Test timeout continues to next audio item
    - [x] Test `torch.cuda.empty_cache()` called after transcribe on CUDA
    - [x] Test structured error event sent on failure
- [x] Task: Fix `core/engine.py` to make tests pass — Green phase
    - [x] Wrap `model.transcribe()` with timeout mechanism
    - [x] Add `torch.cuda.empty_cache()` after transcribe on CUDA
    - [x] Emit structured error events on failure
- [x] Task: Verify all ASR resilience tests pass
- [x] Task: Conductor - User Manual Verification 'ASR Freeze Recovery' (Protocol in workflow.md)

## Phase 2: Queue Backpressure Non-Blocking (REQ-2)
- [x] Task: Write tests — Red phase (tests fail)
    - [x] Test `put_nowait()` does not block VAD worker when queue full
    - [x] Test oldest phrase dropped when queue full
    - [x] Test warning logged when audio dropped
    - [x] Test status emitted to UI on backpressure
- [x] Task: Fix `core/audio.py` to make tests pass — Green phase
    - [x] Change `audio_queue.put()` to `put_nowait()` with try/except
    - [x] Implement drop-oldest strategy for speech_buffer
    - [x] Add warning log and status emission
- [x] Task: Verify all backpressure tests pass
- [x] Task: Conductor - User Manual Verification 'Queue Backpressure' (Protocol in workflow.md)

## Phase 3: Audio Producer Graceful Shutdown (REQ-3)
- [x] Task: Write tests — Red phase (tests fail)
    - [x] Test shutdown event stops outer loop
    - [x] Test `sd.InputStream` closed before exit
    - [x] Test VAD worker thread joined with timeout
    - [x] Test process exits within 3s
- [x] Task: Fix `core/audio.py` to make tests pass — Green phase
    - [x] Add `shutdown_event` for signaling
    - [x] Check event in outer loop
    - [x] Close stream explicitly on shutdown
    - [x] Join VAD worker thread with timeout
- [x] Task: Verify all shutdown tests pass
- [x] Task: Conductor - User Manual Verification 'Audio Producer Shutdown' (Protocol in workflow.md)

## Phase 4: Unicode Bidi Strip (REQ-4)
- [x] Task: Write tests — Red phase (tests fail)
    - [x] Test U+202E stripped from output
    - [x] Test U+202D stripped from output
    - [x] Test U+200E, U+200F stripped from output
    - [x] Test U+0000, U+001b stripped from output
    - [x] Test legitimate text (accents, emojis) preserved
- [x] Task: Fix `core/engine.py` and `subtitulos_obs.html` — Green phase
    - [x] Add bidi override stripping to `_sanitize_text()`
    - [x] Add same stripping to JS `safeText` sanitization
- [x] Task: Verify all Unicode tests pass
- [x] Task: Conductor - User Manual Verification 'Unicode Bidi Strip' (Protocol in workflow.md)

## Phase 5: WebSocket Port Config Wiring (REQ-5)
- [x] Task: Write tests — Red phase (tests fail)
    - [x] Test `run_ws_server()` accepts ws_port parameter
    - [x] Test WS server binds to configured port
    - [x] Test default port 8765 if not configured
- [x] Task: Fix `core/network.py`, `main.py` — Green phase
    - [x] Add `port` parameter to `run_ws_server()`
    - [x] Pass `shared_config["ws_port"]` from main.py
    - [x] Update log/status messages to use dynamic port
- [x] Task: Verify all WS port tests pass
- [x] Task: Conductor - User Manual Verification 'WebSocket Port Wiring' (Protocol in workflow.md)

## Phase 6: Dependency Version Pinning (REQ-6)
- [x] Task: Update `requirements.txt` with upper bounds
    - [x] Pin `websockets>=14.0,<17.0`
    - [x] Pin `torch>=2.0.0,<2.7.0`
    - [x] Pin `faster-whisper>=1.0.0,<2.0.0`
    - [x] Pin `numpy>=1.24.0,<2.1.0`
    - [x] Pin `sounddevice>=0.4.6,<0.5.0`
    - [x] Pin `customtkinter>=5.2.0,<6.0.0`
- [x] Task: Write tests — verify all deps have upper bounds
- [x] Task: Verify all dependency tests pass
- [x] Task: Conductor - User Manual Verification 'Dependency Pinning' (Protocol in workflow.md)
- [ ] Task: Verify `pip install -r requirements.txt` resolves without conflicts
- [ ] Task: Conductor - User Manual Verification 'Dependency Pinning' (Protocol in workflow.md)

## Phase 7: VAD Download Error Handling (REQ-7)
- [x] Task: Write tests — Red phase (tests fail)
    - [x] Test VAD load failure emits error status
    - [x] Test clear log message on failure
- [x] Task: Fix `core/audio.py` — Green phase
    - [x] Wrap `torch.hub.load()` in try/except
    - [x] Emit error status "VAD: error de carga"
    - [x] Log clear message about VAD download failure
- [x] Task: Verify all VAD error tests pass
- [x] Task: Conductor - User Manual Verification 'VAD Error Handling' (Protocol in workflow.md)

## Phase 8: Output_dir Writability Check (REQ-8)
- [x] Task: Write tests — Red phase (tests fail)
    - [x] Test valid writable directory passes
    - [x] Test non-existent directory fails
    - [x] Test empty path fails
    - [x] Test file instead of directory fails
- [x] Task: Fix `main.py` — Green phase
    - [x] Add `_validate_output_dir()` function
    - [x] Call validation in `apply_pending_settings()`
    - [x] Abort with clear error if validation fails
- [x] Task: Verify all output_dir tests pass
- [x] Task: Conductor - User Manual Verification 'Output_dir Validation' (Protocol in workflow.md)

## Phase 9: GPU Auto-Detection + Runtime VRAM Monitoring (REQ-9)
- [x] Task: Write tests — Red phase (tests fail)
    - [x] Test auto-detect CUDA when available
    - [x] Test auto-detect CPU when CUDA unavailable
    - [x] Test CUDA validation with torch.zeros(1).cuda()
    - [x] Test VRAM check when CUDA available
    - [x] Test low VRAM triggers CPU fallback
- [x] Task: Fix `core/engine.py` and `main.py` — Green phase
    - [x] Add VRAM check before transcribe in `_transcribe_with_timeout()`
    - [x] Auto-fallback to CPU when VRAM < 500MB
    - [x] Emit "GPU saturada — usando CPU temporalmente" status
    - [x] Enhance `_validate_draft_config()` with torch.zeros(1).cuda() test
- [x] Task: Verify all GPU tests pass
- [x] Task: Conductor - User Manual Verification 'GPU Auto-Detection' (Protocol in workflow.md)

## Phase 10: Collective Review & Sign-Off
- [x] Task: Run `python -m unittest discover -s tests` — 169 tests pass
- [x] Task: Verify each REQ has ≥3 test cases
- [x] Task: Auditor review — all 9 REQs implemented, all 14 AC met
- [x] Task: Code review — no regressions, clean diffs
- [x] Task: Conductor - User Manual Verification 'Collective Review & Sign-Off' (Protocol in workflow.md)

## Phase 11: Presentation & Merge
- [ ] Task: Auditor presents results to user
- [ ] Task: User approval for merge
- [ ] Task: Merge to `master`, delete feature branch
- [ ] Task: Update `conductor/tracks.md` to `[x] Completed`
- [ ] Task: Conductor - User Manual Verification 'Presentation & Merge' (Protocol in workflow.md)
