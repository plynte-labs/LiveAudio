# LiveAudio

by Plynte Labs / FranGuh

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE) [![Rust 2021](https://img.shields.io/badge/Rust-2021%20Edition-orange.svg)](https://www.rust-lang.org/) [![Tauri v2](https://img.shields.io/badge/Tauri-v2-blue.svg)](https://tauri.app/) [![DirectML](https://img.shields.io/badge/DirectML-DX12%20GPU-purple.svg)](https://learn.microsoft.com/en-us/windows/ai/directml/dml) [![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux-lightgrey.svg)](#system-requirements)

**Real-time, 100% local subtitles for streamers.** LiveAudio captures your mic or system audio, transcribes it locally on your GPU or CPU using native Whisper on ONNX Runtime DirectML or CPU (plus an optional NVIDIA CUDA engine), and streams live subtitles to OBS Studio with zero cloud reliance. Audio and transcripts never leave your machine; the application only queries GitHub Releases for updates every 24 hours (can be toggled off in Settings > Advanced).

<img width="1917" height="1020" alt="LiveAudio v1.6.0" src="https://github.com/user-attachments/assets/7b4c167c-9473-4009-836a-c5cb3592b29f" />

> **[⬇ Download the latest release](https://github.com/plynte-labs/LiveAudio/releases/latest)** · [Website](https://liveaudio.opencohost.com) · [OBS setup guide](docs/WEBSOCKET_OBS.md) · [Versión en Español](README.es.md)

---

## Why it's built this way

Early versions (v1.2) were written in Python and consumed nearly 1 GB of RAM sitting idle. In v1.5 performance-critical pipelines were moved to Rust, and now LiveAudio is **100% Native Rust + Tauri 2** with **ZERO Python dependencies**:

```
Mic / Loopback ──► Rust (CPAL Capture) ──► Rust (Silero VAD, ONNX) ──► Rust ASR (Whisper, ONNX Runtime DirectML/CPU)
                                                                                │
                                   OBS Overlay ◄── Rust (Tokio WebSocket / HTTP) ◄──────┘
```

- **100% Native Rust Core & Pipeline:** Audio capture (CPAL), Voice Activity Detection (Silero VAD in ONNX Runtime), Whisper ASR engine (in native Rust with ONNX Runtime DirectML for DirectX 12 hardware acceleration on NVIDIA, AMD, and Intel GPUs, plus CPU multi-thread fallback on Windows and Linux, and an optional NVIDIA CUDA engine on CTranslate2), WebSocket broadcast, and configuration management.
- **Zero Python Requirement:** No Python installation, virtual environments (`.venv`), or CUDA toolkit installation required. Windows distributions bundle `DirectML.dll` and `onnxruntime.dll` (`1.24.4-directml`) for plug-and-play DirectX 12 GPU acceleration.
- **Process Supervision & Safety:** Win32 Job Objects guarantee clean shutdown with 0 orphan or zombie processes.
- **High-Speed Architecture:** Audio frames, VAD triggers, ASR chunks, and WebSocket broadcasts flow through lock-free channels with minimal latency.

## Results (Native Rust ORT vs Legacy Python)

| Metric | Legacy Python v1.2.7 | Native Rust v1.6.0 | Improvement |
|---|---|---|---|
| Idle memory | ~959 MB | ~65 MB (complete native stack) | ~14.7× less RAM |
| Cold start | ~2,812 ms | ~17 ms (CLI) | ~166× faster |
| VAD per 32 ms frame | 0.358 ms | 0.114 ms | ~3.1× faster |
| Clean shutdown | ~3,401 ms | ~7 ms (Job Object teardown) | ~485× faster |
| Hardware acceleration | CUDA only (NVIDIA) | DirectML (NVIDIA, AMD, Intel), CPU & optional CUDA (NVIDIA) | Universal DX12 GPU support |

Measured on: Windows 11, RTX 3060, AMD Ryzen 7 3700G, 32 GB RAM.

---

## Quick Start (Streamers & End Users)

### 1. Download & Launch
1. Download **`LiveAudio-Setup-1.6.0.exe`** from [GitHub Releases](https://github.com/plynte-labs/LiveAudio/releases/latest). The portable `.zip` (`liveaudio-cli-*-windows-x64.zip`) contains only the CLI and ASR worker, not the desktop app.
2. Launch the application. LiveAudio runs out of the box with its embedded ONNX Runtime DirectML engine and Silero VAD. No external Python or CUDA drivers are required.

> **Updating from v1.5.1 or older:** Users on v1.5.1 or older must install v1.6.0 manually once. Versions 1.5.1 and earlier do not include the in-app updater engine or public signing keys. Subsequent updates from 1.6.0 onward update seamlessly in-app. The in-app updater is Windows-only; Linux users download new releases manually.

### 2. Configure in 3 Clicks
In the **Setup (Inicio)** tab:
1. **Audio Device:** Select your physical microphone or **WASAPI Loopback** (to transcribe desktop / game audio).
2. **Spoken Language:** Choose `es` (Spanish) or `en` (English).
3. **Hardware Profile:** Select `Balanced` (default) or any profile that fits your hardware.
4. Click **Start Transcription (Iniciar Transcripción)**.

### 3. Add Subtitles to OBS Studio
1. In OBS, click **+** under **Sources** and add a **Browser Source**.
2. Set **URL** to:
   ```
   http://127.0.0.1:8765/overlay
   ```
   *(Or check **Local file** and select `assets/subtitulos_obs.html` from the app folder).*
3. Set **Width: 1920**, **Height: 200**, and **FPS: 60**.
4. Uncheck *"Shutdown source when not visible"* and uncheck *"Refresh browser when scene becomes active"* to keep the WebSocket connection uninterrupted.
5. Speak into your mic — live subtitles appear instantly on stream!

---

## OBS Studio Overlay & Customization

The bundled browser source overlay connects to LiveAudio's local WebSocket broadcast server (`ws://127.0.0.1:8765`) over loopback, or can be loaded directly from the built-in HTTP server at `http://127.0.0.1:8765/overlay`.

### Overlay Themes
Choose your style on the **Subtitles** tab without restarting the app:

| Theme | Visual Style |
|---|---|
| `default` | Clean white text with soft shadow and semi-transparent dark background. |
| `karaoke` | Dynamic word-by-word `popIn` highlight in bright golden yellow with black stroke. |
| `neon` | Cyberpunk aesthetic with cyan borders, neon glow, and letter spacing. |
| `minimal` | Unobtrusive, borderless subtitles with subtle background blending. |
| `bold` | Heavy typography with high-contrast text for fast-paced gameplay. |
| `rgb` | Colorful, cycling per-word rainbow palette. |
| `typewriter` | Monospace font with vintage terminal character-by-character typing. |

---

## Key Highlights of v1.6.0

### 0. Stream-Safe In-App Updater (v1.6.0+)
- **Stream Safety Gate:** LiveAudio automatically checks GitHub Releases for updates every 24h. Downloads and installations are strictly blocked while live transcription is running, OBS clients are connected, or monitor mode is active.
- **Minisign Cryptographic Verification:** All update bundles and manifests are verified with Minisign Ed25519 digital signatures before execution.
- **Privacy & Non-Modal UI:** Audio and transcripts never leave your computer. Update checks contain zero telemetry and can be disabled at any time in **Settings > Advanced**. Choose between installing immediately when idle or "Install on close" when exiting.

### 1. 100% Native Rust + ONNX Runtime DirectML Stack
- Complete removal of Python dependencies and runtime provisioning.
- Universal DirectX 12 hardware acceleration via DirectML for NVIDIA GeForce, AMD Radeon, and Intel Arc/Iris GPUs, plus multi-threaded CPU fallback.
- Optional NVIDIA CUDA engine (CTranslate2) for NVIDIA GPUs. It is opt-in: the app downloads cuBLAS and cuDNN from NVIDIA on request, after you accept their license terms.

### 2. Tauri 2 Bilingual Desktop Interface
- Fully bilingual UI with an instant switch between **Spanish (🇪🇸)** and **English (🇺🇸)**.
- Settings organized into **Setup**, **Subtitles**, **Advanced**, and **Files**.

### 3. Native Audio Capture & WASAPI Loopback
- Low-latency physical microphone capture and Windows WASAPI Loopback via CPAL.
- Dynamic resampler handling sample rates from 8 kHz to 192 kHz down to Whisper's native 16 kHz mono.

### 4. Silero VAD v6 Voice Activity Detection
- High-speed native voice activity detection powered by ONNX Runtime. Evaluates 32 ms audio frames in ~0.11 ms with zero memory leaks.

### 5. Multi-Source Subtitle Broadcasting & HTTP Overlay
- Broadcasts JSON subtitle cues to local OBS Browser Sources over WebSocket (`127.0.0.1:8765`).
- Loopback HTTP server serves `http://127.0.0.1:<port>/overlay` directly with no filesystem path configuration needed.

### 6. Dual Session Recording
- Every session automatically records clean `.jsonl` transcripts and synchronized `.vtt` (WebVTT) and `.srt` (SubRip, importable in DaVinci Resolve) subtitle files into your configured `sessions/` folder.

---

## System Requirements

| Component | Minimum | Recommended |
|---|---|---|
| **Operating System** | Windows 10/11 x86_64, Linux x86_64 | Windows 11 (WASAPI Loopback support) |
| **Python** | **Not required** (100% Native Rust stack) | None |
| **GPU** | None (CPU mode works out of the box) | DirectX 12 compatible GPU (NVIDIA, AMD, Intel) |
| **VRAM** | 2 GB (for `tiny` / `base`) | 4 GB+ (for `small` / `turbo`) |
| **RAM** | 4 GB | 8 GB+ |
| **Storage** | 250 MB (app) + 500 MB per model cache | SSD storage |
| **Microphone** | Any USB / 3.5mm microphone or virtual cable | Dedicated cardioid mic or loopback source |

### Virtual Machine Support
- **Without GPU passthrough:** Operates smoothly on CPU multi-threading (1 to 16 threads).
- **With vGPU / PCIe passthrough:** DirectML / GPU acceleration enabled.
- **Audio adapter:** Ensure an audio host device or virtual adapter is exposed to the guest OS.

---

## Developer Guide & Building from Source

### Prerequisites
- **Rust:** `1.80+` (2021 edition)
- **Node.js:** `22+` & `npm`
- **Python:** **Zero Python required**

### Development Setup

```bash
# 1. Clone repository
git clone https://github.com/plynte-labs/LiveAudio.git
cd LiveAudio

# 2. Run Tauri 2 desktop app in development
cargo run -p liveaudio-desktop

# 3. Run headless CLI service
cargo run -p liveaudio-cli -- --help
```

### Running Test Suites

```bash
# Rust workspace unit and integration tests (core, network, audio, vad, asr)
cargo test --workspace

# Frontend DOM, i18n, and close-guard tests
cd desktop && node test_frontend.js

# Updater manifest tests
node --test tests-js/test_updater_manifest.mjs
```

### Production Release Build

```bash
# 1. Build the release CLI sidecar binary
cargo build --release -p liveaudio-cli

# 2. Build the Tauri 2 desktop package (installer & updater bundle)
cd desktop
npx @tauri-apps/cli@2.12.1 build --config src-tauri/tauri.release.conf.json
```

---

## Project Structure

```
LiveAudio/
├── crates/
│   ├── liveaudio-core/       # App lifecycle, state machine, job objects & configuration
│   ├── liveaudio-audio/      # CPAL audio acquisition (Mic & WASAPI loopback)
│   ├── liveaudio-vad/        # Silero VAD v6 (ONNX Runtime engine)
│   ├── liveaudio-asr-worker/ # Native Whisper ASR worker (ONNX Runtime DirectML/CPU)
│   ├── liveaudio-asr-worker-ct2/ # Optional NVIDIA CUDA Whisper worker (CTranslate2, standalone build)
│   ├── liveaudio-network/    # Tokio-Tungstenite WebSocket server & embedded HTTP overlay
│   ├── liveaudio-ipc/        # Typed IPC protocol definitions
│   └── liveaudio-cli/        # Headless CLI service & Tauri sidecar binary
├── desktop/
│   ├── src-tauri/            # Tauri 2 backend (Rust commands, updater & window management)
│   └── src/                  # Vanilla frontend (HTML, CSS, JS, i18n, live preview)
├── assets/                   # subtitulos_obs.html browser source overlay
├── packaging/
│   ├── onnxruntime/          # Bundled DirectML (Windows) & CPU (Linux) ORT binaries
│   └── assemble_latest_manifest.mjs # Signed updater manifest assembly script (Node 22)
├── tests-js/                 # Node 22 test suites for updater and frontend
└── docs/                     # WebSocket protocol specs, MCP docs & VM testing guides
```

---

## Troubleshooting

| Issue | Likely Cause | Solution |
|---|---|---|
| **No audio captured / blank subtitles** | Incorrect audio device selected | Check the **Setup** tab and confirm your mic or WASAPI loopback is selected. Verify permissions. |
| **High latency (>3s) on CPU** | Model size too large for CPU | Switch to `tiny` or `base`, or select the `Fast` profile. |
| **OBS overlay shows disconnected** | LiveAudio not running or wrong port | Verify LiveAudio is started (**Start Transcription**). Check status pill for effective port (e.g., `8765`). If changed, use `?port=XXXX` in the OBS browser URL or use `http://127.0.0.1:<port>/overlay`. |
| **DirectML / GPU initialization failure** | Outdated graphics drivers | Update your GPU drivers (NVIDIA, AMD, or Intel) or switch to CPU mode in settings. |
| **Model download stalled** | Network timeout or TLS interruption | Click **Retry**. LiveAudio resumes downloads without restarting from scratch. |

---

## License & Credits

Distributed under the **MIT License**. See [LICENSE](LICENSE) for details.

### Core Open Source Technologies
- [Tauri 2](https://tauri.app/) — Lightweight, secure native desktop framework.
- [ONNX Runtime](https://onnxruntime.ai/) & [DirectML](https://learn.microsoft.com/en-us/windows/ai/directml/dml) — High-performance cross-vendor hardware-accelerated machine learning.
- [Silero VAD](https://github.com/snakers4/silero-vad) — Fast native voice activity detection.
- [CPAL](https://github.com/RustAudio/cpal) — Cross-platform low-latency audio capture.
- [Tokio](https://tokio.rs/) — Asynchronous Rust runtime & WebSocket server.
