# Audio Standby Sentinel & Driver Resilience (Ready as Service)

## Objective

Enable LiveAudio to run reliably and quietly as a background/headless service (e.g. when spawned by OpenCohost) when no audio input device (microphone) is connected or available. Eliminate audio driver thrashing (`sd._terminate()` / `sd._initialize()` loops), prevent system-wide audio stuttering and playback dropouts, and transition cleanly to active capture as soon as a microphone is connected.

## Problem and Root Cause

1. **Catastrophic Failure Loop on Missing Hardware:**
   In [`liveaudio/core/audio.py`](file:///E:/LiveAudio/liveaudio/core/audio.py), when no input device exists or when the configured device is absent, `sd.InputStream` raises `sd.PortAudioError`.
2. **Global Driver Teardown Thrashing:**
   The exception handler (`except sd.PortAudioError`) immediately executes:
   ```python
   sd._terminate()
   sd._initialize()
   time.sleep(3)
   ```
   On Windows, terminating and initializing PortAudio every 3 seconds drops and re-registers COM, WASAPI, MME, and DirectSound endpoints across the operating system. This forces the Windows Audio Engine to hitch and re-negotiate, producing cyclical stutter, popping, and dropouts in active audio playback (including OpenCohost's TTS voice, games, and media players).
3. **Overly Aggressive Watchdog:**
   The 2.0-second watchdog timer treats slow driver initialization or transient buffer pauses as disconnected hardware, triggering the same destructive teardown loop.
4. **Missing Standby State:**
   Hardware absence is treated as a fatal crash loop rather than a valid operational state (`STANDBY` / `WAITING_DEVICE`).

## Authorized Scope and Constraints

- **Scope:** Confined to [`liveaudio/core/audio.py`](file:///E:/LiveAudio/liveaudio/core/audio.py), [`liveaudio/core/devices.py`](file:///E:/LiveAudio/liveaudio/core/devices.py), and targeted resilience tests in `tests/`.
- **Preserve Service Invariants:** WebSocket server (`liveaudio-ws`) and ASR worker (`liveaudio-asr`) must remain 100% alive, responsive, and connected while audio capture is in standby.
- **Zero-Cost Non-Invasive Device Polling:** On Windows, probe input hardware presence using `ctypes.windll.winmm.waveInGetNumDevs()` (0.001 ms, zero CPU, zero driver impact) before touching PortAudio. Fallback gracefully on POSIX without aggressive teardown.
- **No Global Driver Nuking:** Eliminate unconditional `sd._terminate()` / `sd._initialize()` calls on loop. Re-initialization of PortAudio must occur at most once upon confirmed hardware attachment.
- **State Reporting:** Emit clear service status events (`{"type": "status", "key": "audio", "text": "Audio: en espera de micrófono", "state": "standby"}`).
- **Watchdog Hardening:** Add a startup grace window and raise the silence watchdog threshold from 2.0s to 5.0s to accommodate Windows WASAPI buffer negotiation.
- **Testing Constraints:** Tests must use mocked device probes and fake streams; no live microphones, physical hardware manipulation, or real GPU/ASR loads during unit verification.

## Execution Configuration

| Item | Value |
|---|---|
| Workflow | Organic Driven Development (ODD) |
| Task Document | `odd/tasks/audio-standby-sentinel.md` |
| TDD Mode | Strict TDD (RED -> GREEN -> REFACTOR) |
| Python Runner | `.venv/Scripts/python.exe -m pytest` |
| Primary Modules | `liveaudio/core/audio.py`, `liveaudio/core/devices.py` |
| Test Target | `tests/test_audio_standby.py`, `tests/test_audio.py` |

---

## Work Units

### B1 — Non-Invasive Hardware Presence Probe & Standby Sentinel

- [x] Add a lightweight helper `get_input_device_count()` in `liveaudio/core/devices.py`:
  - On Windows (`sys.platform == "win32"`): call `ctypes.windll.winmm.waveInGetNumDevs()`.
  - Fallback / cross-platform: inspect cached device list or safe query without teardown.
- [x] In `audio_producer` (`liveaudio/core/audio.py`), before opening `sd.InputStream`:
  - Check `get_input_device_count()`.
  - If 0 (no microphone available): enter `STANDBY` state.
  - Report status: `_status("audio", "Audio: en espera de micrófono", "standby")`.
  - Sleep passively in bounded intervals (e.g. 1.0s to 2.0s checking `shutdown_event.is_set()`).
  - DO NOT invoke `sd.InputStream`, `sd._terminate()`, or `sd._initialize()`.

**Acceptance checks:**
- When device count is 0, `audio_producer` does not call `sd.InputStream`.
- No `sd._terminate()` or `sd._initialize()` is called while in standby.
- Process remains responsive to `shutdown_event`.
- Status `standby` is emitted to `log_queue`.

### B2 — Hotplug Transition & Teardown Loop Elimination

- [x] When `get_input_device_count()` transitions from 0 to >0:
  - Perform a single-shot refresh/initialization of PortAudio.
  - Transition from `STANDBY` to `CONNECTING`.
  - Open `sd.InputStream` and transition to `ACTIVE` / `Audio: escuchando`.
- [x] Refactor the `except sd.PortAudioError` handler in `audio_producer`:
  - Stop unconditional `sd._terminate()` and `sd._initialize()`.
  - Check if the error was due to hardware disconnection (device count == 0).
  - If hardware was removed, transition back to `STANDBY` cleanly without thrashing the OS driver.
  - If it is a transient error on an existing device, apply exponential backoff (e.g. 2s, 5s, 10s) without rapid re-initialization.

**Acceptance checks:**
- Plugging in a microphone causes a single clean transition to active capture.
- Unplugging a microphone transitions cleanly back to `STANDBY` without cyclic driver stutter.
- Active system audio playback is unaffected by the standby/poll cycle.

### B3 — Watchdog Relaxation & Startup Grace Window

- [x] Separate initial stream negotiation from runtime callback stalls:
  - Introduce a startup grace period (e.g. 3.0s after `with sd.InputStream`) before the watchdog begins enforcing callback arrival.
  - Increase the watchdog timeout threshold from 2.0s to 5.0s to accommodate Windows WASAPI device latency and thread scheduling jitter.
- [x] Ensure that watchdog exceptions log informative diagnostics and transition to standby or bounded backoff instead of immediate driver destruction.

**Acceptance checks:**
- Streams that take >1.5s to start delivering callbacks do not prematurely fail with a watchdog timeout.
- True hardware disconnection triggers clean standby transition without infinite 3s terminate-init storms.

### B4 — Unit & Resilience Tests

- [x] Create `tests/test_audio_standby.py`:
  - Test `get_input_device_count()` returns expected counts on Windows and fallback platforms.
  - Test `audio_producer` enters standby and waits cleanly when device count is 0.
  - Test hotplug event: device count changes from 0 to 1, causing transition from standby to active stream.
  - Test disconnection event: stream failure with device count 0 causes transition to standby without calling `sd._terminate()` in a loop.
  - Test watchdog grace period prevents false-positive timeouts during stream startup.
- [x] Run full regression suite: `tests/test_audio.py`, `tests/test_restart_flags.py`, `tests/test_resilience_service_backend.py`.

---

## Verification Matrix

1. **Focused Tests:**
   ```powershell
   .venv/Scripts/python.exe -m pytest tests/test_audio_standby.py tests/test_audio.py tests/test_resilience_service_backend.py -q
   ```
2. **Full Test Suite:**
   ```powershell
   .venv/Scripts/python.exe -m pytest -q
   ```
3. **Lint & Code Quality:**
   ```powershell
   .venv/Scripts/ruff.exe check liveaudio tests
   git diff --check
   ```
