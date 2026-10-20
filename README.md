# Plynte LiveAudio

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE) [![Rust 2021](https://img.shields.io/badge/Rust-2021%20Edition-orange.svg)](https://www.rust-lang.org/) [![Tauri v2](https://img.shields.io/badge/Tauri-v2-blue.svg)](https://tauri.app/) [![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux-lightgrey.svg)](#system-requirements)

**Real-time, 100% local subtitles for streamers.** LiveAudio captures your mic or system audio, transcribes it on your own GPU or CPU with Whisper, and shows it live in OBS. Nothing is sent to the cloud.

<img width="1917" height="1020" alt="LiveAudio v1.5.1" src="https://github.com/user-attachments/assets/7b4c167c-9473-4009-836a-c5cb3592b29f" />

> **[⬇ Download the latest release](https://github.com/plynte-labs/LiveAudio/releases/latest)** · [Website](https://liveaudio.opencohost.com) · [OBS setup guide](docs/WEBSOCKET_OBS.md) · [Versión en Español](README.es.md)

---

## Why it's built this way

v1.2 was pure Python and used almost 1 GB of RAM sitting idle. In v1.5 I moved everything latency-critical to a **Rust workspace** and kept only the Whisper inference in Python, as a supervised child process:

```
Mic / loopback ──► Rust (CPAL capture) ──► Rust (Silero VAD, ONNX) ──► Python worker (Faster-Whisper)
                                                                              │
                         OBS overlay ◄── Rust (Tokio WebSocket) ◄─────────────┘
```

- **Rust core:** audio capture, voice detection, WebSocket broadcast, config, and process supervision (Win32 Job Objects, so zero zombie processes).
- **Python worker:** Faster-Whisper only, isolated and auto-restarted if it crashes.
- **Typed IPC** between both sides (JSON lines).

## Results (v1.5 Rust vs v1.2 Python)

| Metric                              | Python v1.2.7 | Rust v1.5.0                 | Improvement |
| ----------------------------------- | ------------- | --------------------------- | ----------- |
| Idle memory                         | ~959 MB       | ~73 MB (core + idle worker) | ~13.1× less RAM |
| Cold start                          | ~2,812 ms     | ~17 ms (CLI)                | ~166× faster |
| VAD per 32 ms frame                 | 0.358 ms      | 0.114 ms                    | ~3.1× faster |
| Clean shutdown                      | ~3,401 ms     | ~7 ms                       | ~485× faster |
| ASR speed (`small`, RTX 3060, fp16) | —             | ~25.7× real-time            | Ultra-low latency |

Measured on: Windows 11, RTX 3060, AMD Ryzen 7 3700G, 32 GB RAM. Full methodology: [benchmark report](docs/migration/wu9_benchmark_comparison_report.md).

---

## Quick Start (Streamers & End Users)

### 1. Download & Launch
1. Download **`LiveAudio-Setup-1.5.1.exe`** (or the portable `.zip`) from [GitHub Releases](https://github.com/plynte-labs/LiveAudio/releases/latest).
2. Launch the application. On first start, LiveAudio provisions its isolated Python 3.11 runtime and ONNX Silero VAD automatically. No manual Python or CUDA installation is required.

### 2. Configure in 3 Clicks
In the **Setup (Inicio)** tab:
1. **Audio Device:** Select your physical microphone or **WASAPI Loopback** (to transcribe desktop / game audio).
2. **Spoken Language:** Choose `es` (Spanish) or `en` (English).
3. **Hardware Profile:** Select `Balanced` (default) or any profile that fits your hardware.
4. Click **Start Transcription (Iniciar Transcripción)**.

### 3. Add Subtitles to OBS Studio
1. In OBS, click **+** under **Sources** and add a **Browser Source**.
2. Check **Local file** and browse to:
   ```
   liveaudio/assets/subtitulos_obs.html
   ```
3. Set **Width: 1920**, **Height: 200**, and **FPS: 60**.
4. Uncheck *"Shutdown source when not visible"* and uncheck *"Refresh browser when scene becomes active"* to keep the WebSocket connection uninterrupted.
5. Speak into your mic — live subtitles appear instantly on stream!

---

## OBS Studio Overlay & Customization

The bundled browser source overlay connects to LiveAudio's local WebSocket broadcast server (`ws://127.0.0.1:8765`) over loopback.

### Overlay Themes
Choose your style on the **Subtitles** tab without restarting the app:

| Theme | Visual Style |
|---|---|
| `default` | Clean white text with soft shadow and semi-transparent dark background. |
| `karaoke` | Dynamic word-by-word `popIn` highlight in bright golden yellow with black stroke. |
| `neon` | Cyberpunk retro aesthetic with bright cyan glow, uppercase tracking, and dark backplate. |
| `minimal` | Pure typography with no background box; blends seamlessly into gameplay. |
| `bold` | High-contrast heavyweight font with rapid fade animations for fast-paced streams. |
| `rgb` | Smooth rainbow gradient across active words. |
| `typewriter` | Monospaced typewriter letter-by-letter rendering with a blinking cursor. |

### Display Modes
- **Adaptive:** Intelligently balances subtitle duration and layout based on sentence length.
- **Single:** Displays only the latest sentence, perfect for minimalistic overlays.
- **Ribbon:** Stacks multi-line vertical subtitle feeds (configurable from 1 to 8 lines).

### Dynamic Port Discovery
LiveAudio defaults to port `8765`. If another program occupies this port, LiveAudio automatically binds to the next free port in the range `8765..8774`. To specify a custom port in OBS, pass the URL query parameter:
```
file:///path/to/liveaudio/assets/subtitulos_obs.html?port=8766
```

---

## Hardware Profiles & Presets

Hardware profiles let you tune latency vs. GPU/CPU load with zero guesswork:

| Profile | Device | Model Size | Silence Timeout | Backlog Policy | Best For |
|---|---|---|---|---|---|
| **Fast** | CPU | `base` | `0.4s` | `live_only` (5s max) | Ultra-low latency, short phrases, low CPU overhead. |
| **Balanced** | CUDA / GPU | `small` | `0.8s` | `auto` (10s max) | **Recommended for most streams.** Great accuracy with sub-second response. |
| **Quality** | CUDA / GPU | `turbo` | `1.0s` | `auto` (15s max) | Maximum precision for podcasting, interviews, and multi-speaker talks. |
| **Stable Streaming** | CPU | `small` | `0.6s` | `live_only` (6s max) | Offloads the GPU completely so AAA games get 100% of GPU resources. |

---

## Key Highlights of v1.5.1

### 1. Tauri 2 Bilingual Desktop Interface
- Fully bilingual UI with an instant switch between **Spanish (🇪🇸)** and **English (🇺🇸)**.
- Frameless custom window with drag header, minimize, maximize, and responsive layout.
- Integrated **Live Subtitle Preview** panel and a **Probar (Test)** button to verify OBS overlay appearance in one click.

### 2. Stream-Safe Close Guard
- Closing the window while a transcription session is actively running triggers a confirmation dialog.
- Prevents accidentally ending live stream captions. Confirming the dialog performs a graceful shutdown (`ExitRequested`); canceling preserves the session.

### 3. PTT Replay Fix (Push-to-Talk)
- For clients like OpenCohost that open a new WebSocket connection per PTT press, late-join replay bursts are tagged with `is_replay: true`.
- Eliminates duplicate utterances and ensures only newly spoken words are broadcast.

### 4. Anti-Burst Backlog & Pacing Protection
- **`auto`**: Delivers live speech immediately. Mild queue delays are smoothed with human-paced delivery (`catchup_interval_sec = 1.5s`). Delays older than `subtitle_max_live_delay_sec` are saved to disk but skipped in OBS to avoid visual clutter.
- **`live_only`**: Strictly displays real-time subtitles, dropping stale queues during sudden system freezes.
- **`send_all`**: Guaranteed delivery with bounded replay queues (drop-oldest at 256 messages) so the live edge is preserved.

### 5. Win32 Job Objects (Zero Zombie Processes)
- Built on Rust's native `crates/liveaudio-core/src/job_object/mod.rs` with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
- If the desktop app or launcher crashes or is terminated by the user, the Windows kernel terminates all Python ASR worker processes immediately. No orphaned processes eating GPU VRAM or CPU.

### 6. Honest Model Provisioning & Transparent Errors
- Real monotonic download progress percentages (0–100%) and KiB/MiB counters without fake spinning loaders.
- Typed error codes with one-line actionable remediation (`provision-network`, `provision-disk-full`, `provision-tls`, `provision-cache-corrupt`).

### 7. Dual Session Recording
- Every session automatically records clean `.jsonl` transcripts and synchronized `.vtt` WebVTT subtitle files into your configured `sessions/` folder.

---

## System Requirements

| Component | Minimum | Recommended |
|---|---|---|
| **Operating System** | Windows 10/11 x86_64, Linux x86_64 | Windows 11 (WASAPI Loopback support) |
| **GPU** | None (CPU mode works out of the box) | NVIDIA GTX 1660 / RTX 2060+ with CUDA 12.1+ |
| **VRAM** | 2 GB (for `tiny` / `base`) | 6 GB+ (for `small` / `turbo`) |
| **RAM** | 8 GB | 16 GB |
| **Storage** | 500 MB (app) + 1 GB per model cache | SSD storage |
| **Microphone** | Any USB / 3.5mm microphone or virtual cable | Dedicated cardioid mic or loopback source |

### Virtual Machine Support
- **Without GPU passthrough:** Operates smoothly on CPU multi-threading (1 to 16 threads).
- **With vGPU / PCIe passthrough:** Full CUDA acceleration enabled.
- **Audio adapter:** Ensure an audio host device or virtual adapter is exposed to the guest OS.

---

## Developer Guide & Building from Source

### Prerequisites
- **Rust:** `1.80+` (2021 edition)
- **Node.js:** `18+` (LTS) & `npm`
- **Python:** `3.11` with [uv](https://docs.astral.sh/uv/)

### Development Setup

```bash
# 1. Clone repository
git clone https://github.com/plynte-labs/LiveAudio.git
cd LiveAudio

# 2. Sync Python worker environment with uv
# For CUDA acceleration (NVIDIA GPU):
uv sync --extra cu121

# Or for CPU-only development:
uv sync --extra cpu

# 3. Run Tauri 2 desktop app in development
cargo run -p liveaudio-desktop

# 4. Run headless CLI service
cargo run -p liveaudio-cli -- --help
```

### Running Test Suites

```bash
# Rust workspace unit and integration tests (core, network, audio, vad, ipc)
cargo test --workspace

# Frontend DOM, i18n, and close-guard tests
cd desktop && node test_frontend.js

# Python ASR worker IPC integration tests
.venv\Scripts\pytest tests/test_asr_worker_ipc.py
```

### Production Release Build

```bash
# 1. Build the release CLI sidecar binary
cargo build --release -p liveaudio-cli

# 2. Build the Tauri 2 desktop package
cd desktop
npx @tauri-apps/cli@2.12.1 build --config src-tauri/tauri.release.conf.json
```

---

## Project Structure

```
LiveAudio/
├── crates/
│   ├── liveaudio-core/       # App lifecycle, state machine, job objects & provisioning
│   ├── liveaudio-audio/      # CPAL audio acquisition (Mic & WASAPI loopback)
│   ├── liveaudio-vad/        # Silero VAD v6 (ONNX Runtime engine)
│   ├── liveaudio-network/    # Tokio-Tungstenite WebSocket server & OBS protocol
│   ├── liveaudio-ipc/        # Typed JSON Lines IPC protocol between Rust & Python
│   └── liveaudio-cli/        # Headless CLI service & Tauri sidecar binary
├── desktop/
│   ├── src-tauri/            # Tauri 2 backend (Rust commands & window management)
│   └── src/                  # Vanilla frontend (HTML, CSS, JS, i18n, live preview)
├── liveaudio/
│   ├── service/              # Faster-Whisper Python worker runtime
│   └── assets/               # subtitulos_obs.html browser source overlay
└── docs/                     # WebSocket protocol specs, migration reports & benchmarks
```

---

## Troubleshooting

| Issue | Likely Cause | Solution |
|---|---|---|
| **No audio captured / blank subtitles** | Incorrect audio device selected | Check the **Setup** tab and confirm your mic or WASAPI loopback is selected. Verify permissions. |
| **High latency (>3s) on CPU** | Model size too large for CPU | Switch to `tiny` or `base`, or select the `Fast` profile. |
| **OBS overlay shows disconnected** | LiveAudio not running or wrong port | Verify LiveAudio is started (**Iniciar Transcripción**). Check status pill for effective port (e.g., `8765`). If changed, use `?port=XXXX` in the OBS browser URL. |
| **CUDA out of memory error** | VRAM exhausted by games/apps | Switch to `small` or CPU mode. LiveAudio auto-clears CUDA cache when VRAM falls below 500 MB. |
| **Model download stalled** | Network timeout or TLS interruption | Click **Reparar entorno / Retry**. LiveAudio resumes downloads without restarting from scratch. |

---

## License & Credits

Distributed under the **MIT License**. See [LICENSE](LICENSE) for details.

### Core Open Source Technologies
- [Tauri 2](https://tauri.app/) — Lightweight, secure native desktop framework.
- [Faster-Whisper](https://github.com/SYSTRAN/faster-whisper) & [CTranslate2](https://github.com/OpenNMT/CTranslate2) — High-throughput Whisper inference.
- [Silero VAD](https://github.com/snakers4/silero-vad) & [ONNX Runtime](https://onnxruntime.ai/) — Fast native voice activity detection.
- [CPAL](https://github.com/RustAudio/cpal) — Cross-platform low-latency audio capture.
- [Tokio](https://tokio.rs/) — Asynchronous Rust runtime & WebSocket server.
