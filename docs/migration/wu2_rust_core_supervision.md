# LiveAudio Migration: Rust Core & Process Supervision (WU2)

- **Work Unit**: WU2 (Rust Core + Supervisión)
- **Crate**: `crates/liveaudio-core`
- **Status**: Implemented & Verified (All 16 crate tests passing, 22 workspace tests passing)

---

## 1. Overview of Delivered Components

In Work Unit 2, we implemented the Rust Core supervision infrastructure and configuration engine for LiveAudio, replacing legacy process orchestration with a robust, real-time-safe, zero-zombie architecture:

### 1.1 `ProcessSupervisor` & Windows Job Object Sandbox
- **Native Kernel Isolation**: Uses `windows_sys::Win32::System::JobObjects` configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. Process IDs of spawned workers are immediately registered with `JobObject::assign_process_id(pid)`. When the parent process terminates, crashes, or is killed via Task Manager, the Windows kernel terminates all worker processes unconditionally (guaranteeing **0 orphan processes**).
- **Startup & Handshake**: Spawns worker process with piped stdin/stdout/stderr. Emits `init` command, captures cold-start pre-import beacons, and awaits `ready` event with configurable timeout.
- **Heartbeat & Watchdog**: Spawns asynchronous watchdog loop emitting `ping` at `watchdog_interval`. Enforces `watchdog_timeout` (terminating hung or stalled processes).
- **Crash Detection & Bounded Exponential Backoff**: Tracks failures with sliding 300-second window. Computes exponential backoff $t_{\text{backoff}} = \min(15.0\text{s}, 1.0\text{s} \times 2^{\text{failures} - 1})$. If crashes exceed 3 within 300 seconds, raises `SupervisorError::MaxFailuresExceeded` (fail-fast policy).
- **Idempotent Double-Shutdown**: Coordinated via atomic flags (`is_shutting_down` and `shutdown_complete`). Sending repeated shutdown calls (`supervisor.stop(...)`) is 100% idempotent and returns `Ok(())` immediately without error, deadlock, or duplicate signals.

### 1.2 Configuration Engine (`LiveAudioConfig`)
- Compatible with LiveAudio's `config.json` schema and Python validation behavior.
- Normalizes and bounds all settings (`silence_timeout`, `max_chunk_duration`, `cpu_threads`, `ws_port`, `vad_speech_pad_ms`, `vad_threshold`, `subtitle_ribbon_max_lines`, etc.).
- Transparent migration of legacy `whisper_context_prompt` to `whisper_context_prompt_es`.
- Preserves unrecognized or custom fields via `#[serde(flatten)] pub extra: serde_json::Map<String, Value>`.
- Cross-process file lock (`config.json.lock`) with automatic 30-second staleness reclamation.
- Atomic file persistence using temporary file sibling (`.config-<pid>-<nanos>.tmp`), `flush()`, `sync_all()`, and `rename()`.

### 1.3 Event Bus & Health State Machine (`liveaudio.service.event`)
- **`EventBus`**: `tokio::sync::broadcast` multi-producer, multi-consumer event distribution bus. Formatted with schema `liveaudio.service.event` (v1) and sensitive field scrubbing.
- **`HealthStateMachine`**: Tracks states (`Initializing`, `Ready`, `Running`, `Degraded`, `Stopping`, `Stopped`, `Failed`). Emits status and health events on state transitions and periodic ticks.
- **Atomic Health File**: Produces `.health-<pid>-<nanos>.tmp` sync/rename snapshots for external observers (CLI, Tauri frontend, monitoring scripts).

---

## 2. Test Verification Evidence

Execution command:
```powershell
cargo test -p liveaudio-core
```

### Test Results

```
     Running unittests src\lib.rs (target\debug\deps\liveaudio_core-1c64e8dd9da6b643.exe)

running 11 tests
test config::tests::test_audio_queue_capacity_sizing ... ok
test config::tests::test_default_config_fields ... ok
test config::tests::test_normalization_and_clamping ... ok
test tests::test_cancellation_hierarchy ... ok
test config::tests::test_legacy_whisper_prompt_migration ... ok
test event::tests::test_event_bus_broadcast ... ok
test tests::test_first_client_gate ... ok
test tests::test_respawn_policy_backoff_and_limit ... ok
test tests::test_vtt_timestamp_formatting ... ok
test config::tests::test_atomic_file_saving_and_reading ... ok
test event::tests::test_health_state_machine_transitions ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

     Running tests\supervisor_mock_worker.rs (target\debug\deps\supervisor_mock_worker-f6c8212a5fcf7a66.exe)

running 5 tests
test test_mock_worker_max_failures_exceeded_fails_fast ... ok
test test_graceful_shutdown_zero_orphans_and_idempotence ... ok
test test_mock_worker_startup_and_handshake ... ok
test test_mock_worker_bounded_respawn_and_backoff ... ok
test test_mock_worker_watchdog_timeout_and_kill ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.24s
```

All 16 tests in `liveaudio-core` passed without error or warnings.
Full workspace test suite: **22 passed, 0 failed**.
