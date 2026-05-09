# Implementation Plan: Critical Bug Fixes — Resilience & Stability

## Phase 1: ASR Freeze Recovery (REQ-1)
- [ ] Task: Write `tests/test_resilience_asr.py` — Red phase (tests fail)
    - [ ] Test transcribe timeout triggers after 15s
    - [ ] Test timeout emits warning status to log_queue
    - [ ] Test timeout continues to next audio item
    - [ ] Test `torch.cuda.empty_cache()` called after transcribe on CUDA
    - [ ] Test structured error event sent on failure
- [ ] Task: Fix `core/engine.py` to make tests pass — Green phase
    - [ ] Wrap `model.transcribe()` with timeout mechanism
    - [ ] Add `torch.cuda.empty_cache()` after transcribe on CUDA
    - [ ] Emit structured error events on failure
- [ ] Task: Verify all ASR resilience tests pass
- [ ] Task: Conductor - User Manual Verification 'ASR Freeze Recovery' (Protocol in workflow.md)

## Phase 2: Queue Backpressure Non-Blocking (REQ-2)
- [ ] Task: Write tests — Red phase (tests fail)
    - [ ] Test `put_nowait()` does not block VAD worker when queue full
    - [ ] Test oldest phrase dropped when queue full
    - [ ] Test warning logged when audio dropped
    - [ ] Test status emitted to UI on backpressure
- [ ] Task: Fix `core/audio.py` to make tests pass — Green phase
    - [ ] Change `audio_queue.put()` to `put_nowait()` with try/except
    - [ ] Implement drop-oldest strategy for speech_buffer
    - [ ] Add warning log and status emission
- [ ] Task: Verify all backpressure tests pass
- [ ] Task: Conductor - User Manual Verification 'Queue Backpressure' (Protocol in workflow.md)

## Phase 3: Audio Producer Graceful Shutdown (REQ-3)
- [ ] Task: Write tests — Red phase (tests fail)
    - [ ] Test shutdown event stops outer loop
    - [ ] Test `sd.InputStream` closed before exit
    - [ ] Test VAD worker thread joined with timeout
    - [ ] Test process exits within 3s
- [ ] Task: Fix `core/audio.py` to make tests pass — Green phase
    - [ ] Add `multiprocessing.Event` for shutdown signaling
    - [ ] Check event in outer loop before each iteration
    - [ ] Close `sd.InputStream` explicitly before exit
    - [ ] Join VAD worker thread with timeout
- [ ] Task: Verify all shutdown tests pass
- [ ] Task: Conductor - User Manual Verification 'Audio Producer Shutdown' (Protocol in workflow.md)

## Phase 4: Unicode Bidi Strip (REQ-4)
- [ ] Task: Write tests — Red phase (tests fail)
    - [ ] Test U+202E stripped from output
    - [ ] Test U+202D stripped from output
    - [ ] Test U+200E, U+200F stripped from output
    - [ ] Test U+0000, U+001b stripped from output
    - [ ] Test legitimate text (accents, emojis) preserved
- [ ] Task: Fix `core/engine.py` and `subtitulos_obs.html` — Green phase
    - [ ] Add bidi override stripping to `_sanitize_text()`
    - [ ] Add same stripping to JS `safeText` sanitization
- [ ] Task: Verify all Unicode tests pass
- [ ] Task: Conductor - User Manual Verification 'Unicode Bidi Strip' (Protocol in workflow.md)

## Phase 5: WebSocket Port Config Wiring (REQ-5)
- [ ] Task: Write tests — Red phase (tests fail)
    - [ ] Test `run_ws_server()` accepts ws_port parameter
    - [ ] Test WS server binds to configured port
    - [ ] Test OBS HTML reads port from query parameter
    - [ ] Test default port 8765 if not configured
- [ ] Task: Fix `core/network.py`, `main.py`, `subtitulos_obs.html` — Green phase
    - [ ] Add `ws_port` parameter to `run_ws_server()`
    - [ ] Pass `shared_config["ws_port"]` from main.py
    - [ ] Update OBS HTML to read `?port=XXXX` from URL
- [ ] Task: Verify all WS port tests pass
- [ ] Task: Conductor - User Manual Verification 'WebSocket Port Wiring' (Protocol in workflow.md)

## Phase 6: Dependency Version Pinning (REQ-6)
- [ ] Task: Update `requirements.txt` with upper bounds
    - [ ] Pin `websockets>=14.0,<17.0`
    - [ ] Pin `torch>=2.0.0,<2.7.0`
    - [ ] Pin `faster-whisper>=1.0.0,<2.0.0`
    - [ ] Pin `numpy>=1.24.0,<2.1.0`
    - [ ] Pin `sounddevice>=0.4.4,<0.5.0`
- [ ] Task: Verify `pip install -r requirements.txt` resolves without conflicts
- [ ] Task: Conductor - User Manual Verification 'Dependency Pinning' (Protocol in workflow.md)

## Phase 7: VAD Download Error Handling (REQ-7)
- [ ] Task: Write tests — Red phase (tests fail)
    - [ ] Test VAD load failure emits clear error message
    - [ ] Test status "VAD: error de carga" emitted to UI
    - [ ] Test audio producer handles VAD failure gracefully
- [ ] Task: Fix `core/audio.py` to make tests pass — Green phase
    - [ ] Wrap `torch.hub.load()` in try/except
    - [ ] Add clear log message on failure
    - [ ] Emit status to UI
- [ ] Task: Verify all VAD error tests pass
- [ ] Task: Conductor - User Manual Verification 'VAD Error Handling' (Protocol in workflow.md)

## Phase 8: Output_dir Writability Check (REQ-8)
- [ ] Task: Write tests — Red phase (tests fail)
    - [ ] Test non-writable path rejected before start
    - [ ] Test non-existent drive rejected
    - [ ] Test clear error message shown to user
    - [ ] Test system start aborted on validation failure
- [ ] Task: Fix `main.py` to make tests pass — Green phase
    - [ ] Add writability check in `apply_pending_settings()`
    - [ ] Show clear error message on failure
    - [ ] Abort system start if validation fails
- [ ] Task: Verify all output_dir tests pass
- [ ] Task: Conductor - User Manual Verification 'Output_dir Validation' (Protocol in workflow.md)

## Phase 9: GPU Auto-Detection + Runtime VRAM Monitoring (REQ-9)
- [ ] Task: Write tests — Red phase (tests fail)
    - [ ] Test CUDA detection sets default device correctly at startup
    - [ ] Test CPU-only machine defaults to "cpu" without crash
    - [ ] Test CUDA change rejected when unavailable (torch.cuda.is_available() = False)
    - [ ] Test CUDA test (`torch.zeros(1).cuda()`) fails gracefully
    - [ ] Test config rolls back on CUDA validation failure
    - [ ] Test VRAM monitoring detects low memory before transcribe
    - [ ] Test auto-fallback to CPU when VRAM < 500MB
    - [ ] Test automatic GPU recovery when VRAM frees up
    - [ ] Test user warning emitted on persistent VRAM pressure
    - [ ] Test CUDA validation completes in under 2s
- [ ] Task: Fix `utils/config.py`, `core/engine.py`, `main.py` — Green phase
    - [ ] Add `torch.cuda.is_available()` check at startup
    - [ ] Add validation + test on device change to "cuda"
    - [ ] Add VRAM check before each transcribe (`torch.cuda.mem_get_info()`)
    - [ ] Implement auto-fallback to CPU when VRAM < 500MB
    - [ ] Emit status warning on persistent VRAM pressure
    - [ ] Show GPU model name and VRAM in settings UI
- [ ] Task: Verify all GPU detection tests pass
- [ ] Task: Conductor - User Manual Verification 'GPU Auto-Detection' (Protocol in workflow.md)

## Phase 10: Collective Review & Sign-Off
- [ ] Task: Run `python -m unittest discover -s tests` — all tests must pass
- [ ] Task: Verify each REQ has ≥3 test cases
- [ ] Task: Auditor summons 4 agents for collective review
    - [ ] Architecture: shutdown safety, CUDA validation, config rollback
    - [ ] Performance: backpressure behavior, timeout overhead, VRAM management
    - [ ] QA: error messages clarity, UX feedback, regression risk
    - [ ] Research: dependency compatibility, traceability, docs updates
- [ ] Task: Resolve any issues found during review
- [ ] Task: Conductor - User Manual Verification 'Collective Review & Sign-Off' (Protocol in workflow.md)

## Phase 11: Presentation & Merge
- [ ] Task: Auditor presents results to user
- [ ] Task: User approval for merge
- [ ] Task: Merge to `master`, delete feature branch
- [ ] Task: Update `conductor/tracks.md` to `[x] Completed`
- [ ] Task: Conductor - User Manual Verification 'Presentation & Merge' (Protocol in workflow.md)
