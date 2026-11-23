# Implementation Plan: Linux Core Support & Packaging (v1.6)

## Phase 1: Runtime Hygiene, Process Safety & IPC Guards (REQ-1, REQ-3, REQ-4, REQ-5)
- [x] Task: Switch Linux ONNX Runtime 1.24.4 fetch to official NuGet package with SHA-256 verification (REQ-1)
    - [x] Update `packaging/fetch_onnxruntime.mjs` and `packaging/onnxruntime/README.md`
    - [x] Sync `THIRD_PARTY_NOTICES.md` Linux ORT source note
- [x] Task: Add Linux orphan-process containment via `PR_SET_PDEATHSIG` (REQ-3)
    - [x] Write integration test verifying spawned Linux child receives termination signal when parent exits abruptly
    - [x] Implement `pre_exec` `libc::prctl(PR_SET_PDEATHSIG, SIGKILL)` (with `getppid()` race check) in `crates/liveaudio-core/src/supervisor/process.rs`
- [x] Task: Guard `install_nvidia_runtime` on non-Windows platforms (REQ-4)
    - [x] Write unit test verifying `install_nvidia_runtime` returns an unsupported-platform error off Windows without network/filesystem writes
    - [x] Add `#[cfg(not(target_os = "windows"))]` guard in `desktop/src-tauri/src/commands.rs`
- [x] Task: Align standalone `liveaudio-asr-worker` ORT discovery on Linux (REQ-5)
    - [x] Write unit test covering `../lib/LiveAudio/onnxruntime/linux-x64/libonnxruntime.so.1.24.4` lookup
    - [x] Update `crates/liveaudio-asr-worker/src/model.rs` `find_ort_dylib()` and gate Windows `PATH` mutation to `#[cfg(windows)]`
- [x] Task: Conductor - User Manual Verification 'Phase 1: Runtime Hygiene, Process Safety & IPC Guards' (Protocol in workflow.md)

## Phase 2: Linux Audio Enumeration & System Loopback (REQ-2)
- [x] Task: Design and test PulseAudio/PipeWire friendly device & monitor enumeration with ALSA fallback (REQ-2)
    - [x] Write unit tests for PulseAudio/PipeWire source + `*.monitor` parsing, `default_loopback` resolution, and ALSA noise filtering (`null`, `lavrate`, `dsnoop`, etc.)
    - [x] Implement Linux source & monitor enumeration in `crates/liveaudio-audio/src/device.rs` (and wire `default_loopback` in `desktop/src-tauri/src/commands.rs`)
- [x] Task: Implement Linux system audio (loopback) capture & reconnect resilience (REQ-2)
    - [x] Write unit/integration tests for loopback device resolution, graceful fallback/error path when PulseAudio/PipeWire is unavailable, and clean shutdown/reconnect
    - [x] Implement Linux monitor capture backend in `crates/liveaudio-audio/src/cpal_source.rs` (or sibling Linux capture module) with clean ALSA fallback
- [x] Task: Conductor - User Manual Verification 'Phase 2: Linux Audio Enumeration & System Loopback' (Protocol in workflow.md)

## Phase 3: Linux Packaging (`.deb` + `.AppImage`) & In-App Updater (REQ-6)
- [x] Task: Enable `.deb` alongside `.AppImage` in Tauri Linux bundle config and release workflow (REQ-6)
    - [x] Update `desktop/src-tauri/tauri.linux.conf.json` (`targets: ["appimage", "deb"]` + desktop metadata) and `.github/workflows/release.yml`
    - [x] Verify `.deb` and `.AppImage` bundle metadata locally or via Tauri build dry check
- [x] Task: Extend updater manifest and desktop updater for Linux AppImage (REQ-6)
    - [x] Write/update unit tests in `tests-js/test_updater_manifest.mjs` and Rust `updater.rs` tests for `linux-x86_64` AppImage updates and `.deb` non-AppImage notice
    - [x] Update `packaging/assemble_latest_manifest.mjs`, `.github/workflows/release.yml`, and `desktop/src-tauri/src/updater.rs`
- [ ] Task: Conductor - User Manual Verification 'Phase 3: Linux Packaging & In-App Updater' (Protocol in workflow.md)

## Phase 4: Linux UI Copy, System Fonts, AppImage `xdg-open` Hygiene & Final Review (REQ-7)
- [x] Task: Enumerate Linux system fonts and sanitize `xdg-open` environment (REQ-7)
    - [x] Write unit tests for `fc-list` font parsing/deduplication and `xdg-open` env sanitization (`LD_LIBRARY_PATH`, `APPIMAGE`)
    - [x] Implement Linux font discovery in `crates/liveaudio-core/src/system_fonts.rs` and env cleanup in `desktop/src-tauri/src/commands.rs`
- [x] Task: Platform-appropriate UI copy for audio loopback & execution providers (REQ-7)
    - [x] Update `desktop/src/i18n.js`, `desktop/src/index.html`, `desktop/src/runtime-settings.js`, and `desktop/test_frontend.js`
- [x] Task: Docs, Collective Review & Full Validation Gate
    - [x] Update `README.md`, `README.es.md`, `docs/PACKAGING_AND_UPDATES.md` (if needed), and `CHANGELOG.md`
    - [x] Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`, `cargo test --workspace --locked`, `node --test tests-js/*.mjs`, and `node desktop/test_frontend.js`
    - [x] Run collective specialist review (Architect/Security, Performance, QA, Research) before merge
- [x] Task: Conductor - User Manual Verification 'Phase 4: Linux UI Polish & Final Review' (Protocol in workflow.md)
