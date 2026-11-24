# ADR-017: Linux Core Support, Audio Loopback, Process Safety, and Packaging (v1.6)

- **Status**: Accepted (2026-11-20)
- **Branch**: `feat/linux-v1.6`
- **Track**: `conductor/tracks/linux-core-packaging_20261119/`

## Context

LiveAudio v1.6.0 completed the migration from Python to a pure-Rust workspace + Tauri 2 desktop shell powered by ONNX Runtime (`ort`). While Ubuntu CI and a basic `.AppImage` bundle existed, running and shipping LiveAudio on Linux revealed several blocking and parity gaps compared to Windows:

1. **Broken Linux ONNX Runtime fetch**: `packaging/fetch_onnxruntime.mjs` targeted a GitHub Release `.tgz` for `v1.24.4` that returned `HTTP 404`, blocking fresh Linux builds and ORT-dependent Rust tests.
2. **No system audio (loopback) and noisy device lists**: `liveaudio-audio` gated loopback enumeration behind `#[cfg(target_os = "windows")]` (WASAPI), and `cpal`'s raw ALSA backend exposed internal virtual PCMs (`null`, `dsnoop`, `hw:*`) while generating ALSA/JACK stderr noise on PipeWire systems.
3. **Orphaned child processes**: Windows relies on Job Objects to terminate `liveaudio-asr-worker` if the parent crashes; Linux had a no-op stub.
4. **Windows-only IPC / UI assumptions**: `install_nvidia_runtime` lacked a non-Windows guard, standalone `liveaudio-asr-worker` did not check Tauri's `../lib/LiveAudio` resource directory, `system_fonts` returned empty on Linux, `xdg-open` inherited AppImage `LD_LIBRARY_PATH` / `APPIMAGE` variables, and UI copy hardcoded `WASAPI Loopback` / `DirectML / CPU`.
5. **Packaging & updater gaps**: Only `.AppImage` was configured (no `.deb`), `latest.json` only published `windows-x86_64`, and the desktop updater rejected Linux unconditionally.

## Decisions

### 1. Official NuGet Source for Linux ONNX Runtime 1.24.4
- Switched `linux-x64` in `packaging/fetch_onnxruntime.mjs` to Microsoft's official NuGet package (`microsoft.ml.onnxruntime` `1.24.4`, SHA-256 pinned), reusing the existing in-process ZIP extractor and extracting `libonnxruntime.so.1.24.4`, `libonnxruntime_providers_shared.so`, `LICENSE`, and `ThirdPartyNotices.txt`.
- **Trade-off**: Larger download (~125 MB multi-RID `.nupkg`) in exchange for official Microsoft availability, SHA-256 verification, and consistent NuGet sourcing with Windows.

### 2. PipeWire / PulseAudio Compatibility via `pactl` + `parec` with ALSA Fallback
- Implemented `crates/liveaudio-audio/src/linux_pulse.rs` using `LC_ALL=C pactl info` / `pactl list sources` (bounded by a 3s timeout, executed via `tokio::task::spawn_blocking` in Tauri commands) to enumerate friendly microphones and `*.monitor` system loopback sources, and `parec --raw --format=float32le` for capture.
- When `pactl`/`parec` is absent, falls back cleanly to `cpal`/ALSA with a noise filter that preserves `default`, `pipewire`, and `pulse`.
- **Trade-off**: Zero new C `-dev` build dependencies (`libpulse-dev` is not required in CI or local builds) and out-of-the-box compatibility with PipeWire (`pipewire-pulse`) and PulseAudio across Pop!_OS, Ubuntu, Fedora, and Arch. Requires `pulseaudio-utils` (`pactl`/`parec`) at runtime for loopback; pure-ALSA hosts fall back to microphone-only capture.

### 3. Kernel-Enforced Orphan Containment (`PR_SET_PDEATHSIG`)
- Added async-signal-safe `pre_exec` hooks calling `libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL)` followed by a `libc::getppid() != parent_pid` race check when spawning both `liveaudio-asr-worker` (`supervisor/process.rs`) and `parec` (`linux_pulse.rs`).
- Gated `CpalAudioSource::start()`'s temporary-thread `with_clean_audio_thread` retry (a Windows COM STA workaround) to `#[cfg(windows)]` because Linux `PR_SET_PDEATHSIG` tracks the *spawning thread* rather than only the process.

### 4. Multi-Format Linux Packaging (`.AppImage` + `.deb`) & AppImage In-App Updater
- Enabled `["appimage", "deb"]` in `desktop/src-tauri/tauri.linux.conf.json` with Handlebars `{{exec}}` / `{{icon}}` in `packaging/liveaudio.desktop`.
- Extended `packaging/assemble_latest_manifest.mjs` and `.github/workflows/release.yml` so `latest.json` is assembled in the `publish` job with both `windows-x86_64` (`.exe` + `.sig`) and `linux-x86_64` (`.AppImage` + `.AppImage.sig`).
- Enabled in-app updater on Linux when `APPIMAGE` is present; `.deb` installs display a localized notice directing users to GitHub Releases instead of a misleading offline error.

### 5. Linux Desktop Hygiene & Platform-Neutral UI
- Guarded `install_nvidia_runtime` on non-Windows in both `liveaudio-core` and Tauri IPC.
- Aligned `liveaudio-asr-worker` ORT discovery with `../lib/LiveAudio` and gated Windows `PATH` mutation to `#[cfg(windows)]`.
- Implemented Linux system font discovery via `fc-list : family`, sanitized AppImage library env vars before spawning `xdg-open` via `tokio::process::Command` (preventing zombie processes), and updated EN/ES UI labels to platform-neutral copy.

## Verification

- Automated: `cargo test --workspace --locked` (527+ tests passing), `cargo clippy --workspace --all-targets` (0 warnings in modified files), `node --test tests-js/*.mjs` (169/169 passing), `node desktop/test_frontend.js` (876/876 passing).
- Manual (Pop!_OS 24.04 LTS, PipeWire 1.5.85, Intel UHD Graphics 620 CPU-only): verified clean app startup without ALSA/JACK spam, friendly PipeWire microphone + system loopback enumeration (`liveaudio-cli devices` and UI dropdown), system font enumeration, `.deb` updater notice, and live transcription end-to-end.

## Follow-Up Roadmap (Out of Scope for This PR)

1. **Linux GPU Acceleration — CUDA & Non-CUDA GPUs (AMD / Intel Arc)**:
   - Implement Linux GPU adapter & driver detection in `crates/liveaudio-core/src/gpu.rs` (`/sys/bus/pci/devices`, `libcuda.so.1`, Vulkan/ROCm/OpenVINO probes).
   - Add Linux CUDA execution provider support (ONNX Runtime CUDA / CTranslate2 Linux worker) for NVIDIA GPUs.
   - Evaluate and implement non-CUDA Linux GPU execution providers for **AMD GPUs** (ROCm / MIGraphX / Vulkan) and **Intel Arc / integrated GPUs** (OpenVINO / WebGPU / Vulkan) so Linux users without NVIDIA hardware can offload Whisper inference from CPU.
2. **Intensive Linux Latency & Long-Session Benchmarking**:
   - Benchmark release-profile (`--release`) end-to-end latency, VAD-to-subtitle timing, and 30–120 minute session stability on higher-spec Linux desktop hardware (multi-core modern CPU + discrete GPU), since initial verification ran on a low-power CPU-only laptop (Intel UHD 620, debug build).
