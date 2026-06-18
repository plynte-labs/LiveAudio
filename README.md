# Plynte LiveAudio

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Python 3.10+](https://img.shields.io/badge/Python-3.10%2B-blue.svg)](https://www.python.org/)
[![Platform: Windows | Linux](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux-lightgrey.svg)](https://github.com/plynte-labs/LiveAudio)

LiveAudio is a real-time automatic speech recognition (ASR) engine designed for streamers and content creators. It captures audio from your microphone or system, transcribes it locally using **Whisper** (OpenAI), and sends subtitles to **OBS Studio** via **WebSocket**.

**100% local processing — nothing is sent to the cloud.**
<img width="1918" height="987" alt="LiveAudio" src="https://github.com/user-attachments/assets/99d629a1-57ba-4fb5-b3a8-44c1b653e9dd" />

---

## Features

- **Real-time transcription** with Whisper (`tiny`, `base`, `small`, `turbo` models).
- **Voice activity detection (VAD)** with Silero VAD to cut silences automatically.
- **Flexible capture:** physical microphone or system audio (WASAPI Loopback on Windows).
- **Integrated WebSocket** to send subtitles to OBS or any HTML client.
- **OBS backlog control:** prevents bursts of old subtitles after freezes, without losing the saved transcript.
- **Hallucination filtering** via a customizable blacklist.
- **Session management:** saves transcriptions as `.jsonl` and subtitles as `.vtt`.
- **Intelligent hot-swap:** change device or model without restarting the program.
- **Robust architecture:** isolated processes (multiprocessing), audio ring buffer, and automatic reconnection on hardware disconnects.

---

## System Requirements

| Component | Recommended |
|---|---|
| **OS** | Windows 10/11 (WASAPI Loopback) or Linux x86_64 |
| **Python** | Not required for users — the installer provisions its own Python 3.11 |
| **GPU** | NVIDIA with CUDA (optional but recommended for larger models) |
| **RAM** | 8 GB minimum, 16 GB recommended |
| **Disk** | ~400 MB (CPU) / ~2.5 GB (CUDA) for the app + dependencies, plus model storage |
| **Microphone** | Any audio input device |
| **Internet** | Required on first run only (dependency + model download) |

### Running in a Virtual Machine

LiveAudio works in VMs with the following considerations:

| VM setup | Result |
|---|---|
| VM without GPU passthrough (VirtualBox, VMware default) | ✅ Works on CPU — slow but functional |
| VM with GPU passthrough (VMware vGPU, Proxmox) | ✅ Works with CUDA |
| Cloud VM without GPU (EC2, GCP) | ✅ CPU only — good for testing, not live streaming |
| VM without audio device exposed to guest | ❌ `sounddevice` won't find devices — expose the audio host adapter first |

For CPU-only VMs, run the launcher once with `--device cpu`.

---

## Installation (Users)

1. Download the latest release from [GitHub Releases](https://github.com/plynte-labs/LiveAudio/releases):
   - **Windows:** `LiveAudio-Setup-X.Y.Z.exe`
   - **Linux:** `LiveAudio-X.Y.Z-linux-x64.tar.gz` (extract, then run `./liveaudio-launcher`)
2. Run it. The **first run** downloads Python and all dependencies (**~400 MB** on CPU, **~2.5 GB** with CUDA) and **auto-detects your GPU** — no manual setup. Later runs start instantly.

> **First transcription — internet required:** LiveAudio also downloads two models on first use (unchanged from previous versions):
> - **Silero VAD** (~2 MB) — voice activity detection, from GitHub
> - **Whisper model** — size depends on your choice: `tiny` ~150 MB · `base` ~300 MB · `small` ~480 MB · `turbo` ~1.5 GB (from Hugging Face)
>
> After that, the app works fully **offline**.

> **Windows SmartScreen:** the installer is not code-signed (signing certificates are expensive for an open-source project), so Windows may show a SmartScreen warning. Click **More info → Run anyway**. You can verify the download against `SHA256SUMS.txt` published with each release.

**Options:**

- **Force a backend:** run the launcher with `--device cpu` or `--device cuda` to override GPU auto-detection. The choice is remembered for future runs.
- **Portable mode:** create an empty file named `portable.marker` next to the launcher executable. Everything (app, dependencies, config, sessions, models) is then stored in a `data/` folder next to the launcher — nothing touches your user profile. Ideal for USB drives.
- **Linux audio:** the PortAudio runtime is required for capture: `sudo apt install libportaudio2`.
- **Linux desktop entry:** `./liveaudio-launcher --install-desktop-entry` adds an applications-menu entry.
- **Updates:** the app checks GitHub Releases and offers a one-click update button; you can also run the launcher with `--update` manually. Already-downloaded PyTorch wheels are reused, so updates are small.

---

## Installation (Developers)

LiveAudio uses [uv](https://docs.astral.sh/uv/) for dependency management:

```bash
git clone https://github.com/plynte-labs/LiveAudio.git
cd LiveAudio

# CPU-only (smallest, works everywhere)
uv sync --extra cpu

# Or with NVIDIA CUDA
uv sync --extra cu121

# Run the app
uv run liveaudio

# Run the tests
uv run pytest
```

Exactly one torch extra (`cpu` or `cu121`) must be selected — they route to different PyTorch package indexes. Each release also publishes `requirements-cpu.txt` / `requirements-cu121.txt` as a pip-only escape hatch.

See [docs/PACKAGING_AND_UPDATES.md](docs/PACKAGING_AND_UPDATES.md) for the full packaging and release architecture.

---

## Project Structure

```
LiveAudio/
├── liveaudio/
│   ├── app.py            # GUI (CustomTkinter) and orchestrator
│   ├── core/
│   │   ├── audio.py      # Audio capture, VAD, and automatic reconnection
│   │   ├── engine.py     # ASR engine (Whisper) and session saving
│   │   └── network.py    # WebSocket server (broadcast)
│   ├── utils/
│   │   └── config.py     # Persistent configuration load/save
│   └── assets/
│       └── subtitulos_obs.html   # Browser Source for OBS
├── packaging/
│   └── launcher.py       # Bootstrapper frozen into the release installers
├── docs/
│   ├── GETTING_STARTED.md        # Detailed guide for new users
│   ├── WEBSOCKET_OBS.md          # OBS Studio integration
│   └── PACKAGING_AND_UPDATES.md  # Packaging / release / update architecture
├── pyproject.toml        # Project metadata and dependencies (uv)
├── uv.lock               # Locked dependency versions
└── config.json.example   # Example configuration
```

Launcher installs keep user data (config, sessions) under the install root's `data/` directory: `%LOCALAPPDATA%\LiveAudio\data` on Windows or `~/.local/share/liveaudio/data` on Linux. (Dev runs without the launcher default to `%APPDATA%\LiveAudio` on Windows or `~/.config/liveaudio` on Linux.) Override with the `LIVEAUDIO_HOME` environment variable.

---

## Main Dependencies

| Library | Version | Purpose |
|---|---|---|
| `faster-whisper` | >=1.0.0,<2.0.0 | Optimized Whisper transcription |
| `torch` | >=2.0,<2.7 (CPU) / >=2.4,<2.6 (CUDA 12.1) | Inference backend |
| `sounddevice` | >=0.4.6,<0.5.0 | Real-time audio capture |
| `numpy` | >=1.24.0,<2.1.0 | Audio buffer manipulation |
| `customtkinter` | >=5.2.0,<6.0.0 | Modern GUI |
| `Pillow` | >=10.0.0,<12.0.0 | Image processing for branding/UI |
| `websockets` | >=14.0,<17.0 | WebSocket server for OBS |

---

## Configuration

On first run, a `config.json` file is created automatically with default values in the data home (see locations above). You can modify all settings from the GUI or by editing `config.json` directly.

```json
{
    "output_dir": "<absolute_path>/sessions",
    "device": "cuda",
    "cpu_threads": 8,
    "model_size": "small (Balance CPU)",
    "blacklist": "amara.org, subtitulos por, suscribete, dale like, gracias por ver",
    "continuous_session": true,
    "subtitle_style": "default",
    "subtitle_backlog_policy": "auto",
    "subtitle_max_live_delay_sec": 10.0,
    "subtitle_catchup_interval_sec": 1.5,
    "silence_timeout": 0.8,
    "max_chunk_duration": 5.0,
    "audio_device": null,
    "selected_profile_id": "balanced",
    "ws_port": 8765,
    "obs_enabled": true,
    "asr_language": "es",
    "diagnostics_enabled": false
}
```

---

## Basic Usage

1. Start LiveAudio (the installed launcher, or `uv run liveaudio` from a checkout).
2. On the welcome screen, choose the folder where sessions will be saved.
3. In the settings panel:
   - Choose a **profile** (`Fast`, `Balanced`, `Quality`, or `Stable Streaming`) to start without manual tuning.
   - Select your **audio device** (microphone or system loopback).
   - Choose **CPU** or **CUDA** depending on your hardware.
   - Select the **model size** (`tiny`, `base`, `small`, `turbo`).
   - Press **Apply changes** to activate and save settings.
4. Press **START SYSTEM**.
5. Open `liveaudio/assets/subtitulos_obs.html` as a **Browser Source** in OBS (see [docs/WEBSOCKET_OBS.md](docs/WEBSOCKET_OBS.md)).

---

## Configuration Profiles

Profiles are built-in presets to avoid manually tuning every sensitive control.

| Profile | Recommended for |
|---|---|
| `Fast` | Lower latency and short phrases; slightly lower accuracy. |
| `Balanced` | Recommended for most sessions. |
| `Quality` | Higher accuracy; may use more VRAM and take longer. |
| `Stable Streaming` | Reduces GPU load for gaming or streaming on a busy PC. |

If you modify a built-in profile, LiveAudio treats it as `Custom`. Changes are pending until you press **Apply changes**.

---

## OBS Backlog Policy

LiveAudio always saves valid transcriptions to the session (`transcript.jsonl` and `subtitles.vtt`). The **OBS Delay** option only controls what is shown live in OBS when the ASR falls behind due to a busy GPU/CPU, full VRAM, or a temporary freeze.

| Mode | Behavior |
|---|---|
| `Auto` | Sends fresh subtitles. Short backlogs are emitted with pacing. If delay exceeds `subtitle_max_live_delay_sec`, they are saved but not shown in OBS. |
| `Live only` | Saves everything, but only shows subtitles within the configured max delay in OBS. |
| `Send all` | Sends everything to OBS even if it arrives late. Useful if you prefer full visual fidelity over avoiding bursts. |

---

## Troubleshooting

| Symptom | Possible cause | Solution |
|---|---|---|
| Nothing is transcribed | Wrong audio device | Verify the correct microphone or loopback is selected in the UI. |
| Very high latency | Model too large on CPU | Switch to `tiny` or `base`, or use GPU. |
| OBS shows no subtitles | WebSocket not connected | Make sure LiveAudio is running and the HTML points to `ws://127.0.0.1:8765`. |
| CUDA error | Outdated drivers | Update NVIDIA drivers or switch to CPU in settings. |
| Zombie processes on close | Abrupt shutdown | Always use the **STOP SYSTEM** button before closing the window. |

---

## Local Diagnostics

LiveAudio includes **local-first** diagnostics for maintenance. Nothing is sent to any external service.

### What they measure

- Visible state of `audio`, `asr`, `ws` processes
- Queue sizes visible from the UI
- Pipeline latencies and instrumented states
- Backpressure, reconnect, and timeout signals

### Configuration

| Key | Value | Description |
|---|---|---|
| `diagnostics_enabled` | `true/false` | Enables local instrumentation |
| `diagnostics_level` | `minimal` / `deep` | Controls how much local context is saved |
| `diagnostics_export_dir` | path or `null` | Preferred folder for exporting reports |

Press **Export diagnostics** in the main UI to generate a local JSON report.

---

## Contributing

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request.

- [Bug Report](https://github.com/plynte-labs/LiveAudio/issues/new?template=bug_report.yml)
- [Feature Request](https://github.com/plynte-labs/LiveAudio/issues/new?template=feature_request.yml)
- [Security Vulnerability](SECURITY.md) — report privately, do not open a public issue

Please follow our [Code of Conduct](CODE_OF_CONDUCT.md).

---

## License

Distributed under the MIT License. See [LICENSE](LICENSE) for details.

---

## Credits

- [OpenAI Whisper](https://github.com/openai/whisper)
- [Faster Whisper](https://github.com/SYSTRAN/faster-whisper)
- [Silero VAD](https://github.com/snakers4/silero-vad)
- [CustomTkinter](https://github.com/TomSchimansky/CustomTkinter)
