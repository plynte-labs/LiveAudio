# Track Specification: Linux Core Support & Packaging (v1.6)

## Overview
Bring LiveAudio v1.6.0 on Linux (`x86_64`) to first-class parity with Windows for CPU-based local transcription, system audio (loopback) capture, process lifecycle safety, desktop UX polish, and release distribution (`.AppImage` + `.deb` + AppImage in-app updates). GPU/CUDA acceleration on Linux is intentionally deferred to a separate follow-up track.

## Functional Requirements

- **REQ-1 — Official Linux ONNX Runtime Fetcher**: `packaging/fetch_onnxruntime.mjs` fetches ONNX Runtime `1.24.4` for `linux-x64` from the official Microsoft NuGet package (`microsoft.ml.onnxruntime`), verified by pinned SHA-256, extracting `libonnxruntime.so.1.24.4`, `libonnxruntime_providers_shared.so`, `LICENSE`, and `ThirdPartyNotices.txt`. Sync the Linux ORT source note in `THIRD_PARTY_NOTICES.md`.
- **REQ-2 — Linux Audio Enumeration & System Loopback**:
  - Enumerate friendly input sources (microphones) and system audio monitor sources (`*.monitor` as loopback devices, including `default_loopback`) via PulseAudio/PipeWire compatibility when available, falling back gracefully to `cpal`/ALSA when PulseAudio/PipeWire is absent.
  - Filter noisy internal ALSA virtual PCMs when exposing ALSA fallback devices.
  - Support capturing from both microphone and loopback monitor devices on Linux with clean reconnection on disconnect.
- **REQ-3 — Orphan Process Containment (`PR_SET_PDEATHSIG`)**:
  - When spawning `liveaudio-asr-worker` on Linux, register `prctl(PR_SET_PDEATHSIG, SIGKILL)` via `pre_exec` (with `getppid()` race check) so the kernel terminates the worker if the parent process dies unexpectedly (`SIGKILL`, crash, OOM).
- **REQ-4 — Guard Windows-Only NVIDIA Runtime Command on Linux**:
  - `install_nvidia_runtime` IPC command must explicitly reject non-Windows platforms with a clear user-facing error instead of downloading Windows `.dll`/`.exe` binaries.
- **REQ-5 — Standalone Worker ORT Discovery on Linux**:
  - Align `liveaudio-asr-worker`'s `find_ort_dylib()` with `liveaudio-core` so it checks `../lib/LiveAudio` (standard Tauri `.deb`/AppImage resource path) in addition to exe-adjacent and dev paths, and gate Windows-only `PATH` mutation to `#[cfg(windows)]`.
- **REQ-6 — Linux Packaging (`.AppImage` + `.deb`) & In-App Updater**:
  - Configure `desktop/src-tauri/tauri.linux.conf.json` to bundle both `appimage` and `deb` targets (including `.desktop` metadata).
  - Keep custom titlebar (`decorations: false`) consistent with Windows.
  - Enable in-app updater checks/installs on Linux for AppImage builds (`updater.rs`), and extend `packaging/assemble_latest_manifest.mjs` + `.github/workflows/release.yml` to publish `linux-x86_64` alongside `windows-x86_64` in `latest.json` (showing a clear notice on `.deb` installs where self-replacement is managed by `apt`).
- **REQ-7 — Linux UI Copy, System Fonts & AppImage `xdg-open` Hygiene**:
  - Replace hardcoded Windows-only UI labels (`WASAPI Loopback`, `DirectML / CPU`) with platform-appropriate copy in English and Spanish (`i18n.js`, `index.html`, `runtime-settings.js`).
  - Enumerate installed Linux system fonts (`system_fonts.rs`, e.g., via `fc-list`) so the subtitle Style tab shows real system fonts with safe fallback to the static list.
  - Sanitize AppImage-injected library env vars (`LD_LIBRARY_PATH`, `APPIMAGE`) when spawning `xdg-open` so host browsers/file managers open reliably.

## Non-Functional Requirements
- Zero new mandatory C `-dev` build dependencies that break existing Ubuntu CI runners without explicit justification.
- Preserve strict localhost-only WebSocket binding, local-first privacy, and existing Windows behavior with zero regressions.
- Pass `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`, `cargo test --workspace --locked`, `node --test tests-js/*.mjs`, and `node desktop/test_frontend.js`.

## Acceptance Criteria
- Linux CI and local Pop!_OS build cleanly from a fresh checkout using `node packaging/fetch_onnxruntime.mjs`.
- Both microphone and system audio (loopback) devices appear with readable names and capture audio on Pop!_OS (PipeWire).
- Killing `liveaudio-desktop` with `SIGKILL` leaves zero orphaned `liveaudio-asr-worker` processes.
- Calling `install_nvidia_runtime` on Linux returns a structured unsupported-platform error and downloads nothing.
- `assemble_latest_manifest.mjs` generates a valid multi-platform `latest.json` (`windows-x86_64` + `linux-x86_64`) covered by unit tests.
- UI shows platform-appropriate labels and real system fonts on Linux.

## Out of Scope
- Linux GPU / CUDA / ROCm / OpenVINO acceleration and Linux `detect_gpu_adapters()` / `cuda_driver_version()` (deferred to a dedicated Linux GPU track).
- `.rpm`, Flatpak, or `linux/aarch64` packaging.
- Native Linux window decorations (`decorations: true`).
