# Track: Critical Bug Fixes — Resilience & Stability

## Overview
Fix 8 critical/high-severity bugs identified in the Dream Team audit, plus add GPU auto-detection with robust CUDA validation and runtime VRAM monitoring. Focus on ASR freeze recovery, queue backpressure, graceful shutdown, Unicode sanitization, WebSocket port wiring, dependency pinning, VAD error handling, output_dir validation, and safe GPU/CPU switching with auto-fallback.

## Functional Requirements

### REQ-1: ASR Freeze Recovery (C1)
- `model.transcribe()` wrapped with timeout (15s for 5s audio chunk).
- On timeout: log warning, emit status, continue to next item.
- `torch.cuda.empty_cache()` called after each transcribe on CUDA.
- Structured error event sent to log_queue on failure.

### REQ-2: Queue Backpressure Non-Blocking (C2)
- `audio_queue.put()` changed to `put_nowait()` with try/except `queue.Full`.
- On full: drop oldest phrase from speech_buffer, log warning, emit status.
- No blocking of VAD worker thread or C audio callback.

### REQ-3: Audio Producer Graceful Shutdown (C3)
- `multiprocessing.Event` added for shutdown signaling.
- Outer loop checks event before each iteration.
- `sd.InputStream` closed explicitly before exit.
- VAD worker thread joined with timeout before process exit.

### REQ-4: Unicode Bidi Strip (C4)
- `_sanitize_text()` strips U+202E, U+202D, U+200E, U+200F, U+0000, U+001b.
- JS `safeText` in `subtitulos_obs.html` strips same characters.
- Tests verify bidi override characters are removed.

### REQ-5: WebSocket Port Config Wiring (H1)
- `run_ws_server()` accepts `ws_port` parameter.
- `main.py` passes `shared_config["ws_port"]` to WS server process.
- `subtitulos_obs.html` reads port from query parameter (`?port=XXXX`).
- Default port 8765 preserved if not configured.

### REQ-6: Dependency Version Pinning (H9/H10)
- `requirements.txt` pinned with upper bounds:
  - `websockets>=14.0,<17.0`
  - `torch>=2.0.0,<2.7.0`
  - `faster-whisper>=1.0.0,<2.0.0`
  - `numpy>=1.24.0,<2.1.0`
  - `sounddevice>=0.4.4,<0.5.0`

### REQ-7: VAD Download Error Handling (H11)
- `torch.hub.load()` wrapped in try/except.
- Clear log message on failure: "Error loading VAD model. Check internet connection or manually download Silero VAD."
- Status emitted to UI: "VAD: error de carga".

### REQ-8: Output_dir Writability Check (H12)
- `apply_pending_settings()` validates `output_dir` is writable before starting.
- Clear error message if path is read-only, non-existent drive, or disconnected network path.
- System start aborted if validation fails.

### REQ-9: GPU Auto-Detection + Runtime VRAM Monitoring (H4)
- **Startup**: Detect CUDA via `torch.cuda.is_available()`. If available → default `"cuda"`, else → default `"cpu"` with info log.
- **Device change validation**: When user changes device to `"cuda"`:
  1. Check `torch.cuda.is_available()` — if False, reject with clear error.
  2. Run small test: `torch.zeros(1).cuda()` — if fails, reject with error.
  3. Only if both pass, accept the change.
- **Runtime VRAM monitoring**: Before each transcribe, check available VRAM via `torch.cuda.mem_get_info()`.
- **Auto-fallback**: If VRAM < 500MB, use CPU for that chunk automatically.
- **User warning**: If persistent VRAM pressure, emit status "GPU saturada — usando CPU temporalmente".
- **Recovery**: When VRAM frees up, automatically resume GPU usage.
- **Fallback**: If CUDA fails at any point, config rolls back to previous value.

## Non-Functional Requirements
- All fixes maintain backward compatibility with existing config files.
- No breaking changes to WebSocket protocol or OBS HTML API.
- Shutdown completes within 3 seconds on Windows.
- ASR timeout does not exceed 3x the audio chunk duration.
- Unicode sanitization does not alter legitimate text (accents, emojis, etc.).
- CUDA validation completes in under 2 seconds.

## Acceptance Criteria
- **AC-1**: All 109 existing tests pass after changes.
- **AC-2**: ASR transcribe timeout triggers correctly within 15s.
- **AC-3**: Queue full does not block VAD worker thread (verified by test).
- **AC-4**: Audio producer exits within 3s on shutdown signal.
- **AC-5**: Bidi override characters stripped from subtitle output.
- **AC-6**: WS server binds to configured port (not hardcoded).
- **AC-7**: `pip install -r requirements.txt` installs compatible versions.
- **AC-8**: VAD failure shows clear error message in UI.
- **AC-9**: Non-writable output_dir prevents system start with clear error.
- **AC-10**: App starts on CPU-only machine without crash.
- **AC-11**: Changing device to CUDA on incompatible GPU is rejected with clear error, config rolls back.
- **AC-12**: CUDA validation test completes in under 2s.
- **AC-13**: Auto-fallback to CPU triggers when VRAM < 500MB.
- **AC-14**: GPU recovery automatic when VRAM frees up.

## Out of Scope
- Language selector (language="es" remains hardcoded for now).
- Resilience/idempotency test files (separate track).
- OBS connection status indicator (separate track).
- Config file locking (separate track).
- Replay buffer cap (separate track).
