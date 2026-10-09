# LiveAudio Migration: Packaging & Compatibility (WU8)

- **Work Unit**: WU8 (Packaging y compatibilidad)
- **Target Distribution**: LiveAudio Rust Core + Tauri 2 Desktop + Faster-Whisper Worker
- **Platforms**: Windows 10/11 x86_64 (Primary), Linux x86_64 (Headless/CLI compatible)
- **Status**: Implemented & Verified (Clean workspace compilation, 20/20 core tests passing, 7/7 ASR worker self-test phases passing, 7/7 packaging pipeline checks passing)

---

## 1. Overview of Delivered Components

Work Unit 8 establishes the complete packaging, distribution, runtime discovery, and backward compatibility pipeline for LiveAudio. It integrates the modular Rust architecture (`liveaudio-core`, `liveaudio-cli`, `liveaudio-desktop`) with Tauri 2 and the Python Faster-Whisper ASR worker into an installable and portable application bundle.

### 1.1 Architecture & Packaging Layout

The deployment artifact combines high-performance native Rust binaries with an isolated Python environment for speech-to-text inference:

```
LiveAudio-Distribution/
├── liveaudio-desktop.exe            # Tauri 2 Desktop GUI application
├── liveaudio-cli.exe                # Headless CLI service daemon & diagnostics
├── run_liveaudio.bat                # Portable launcher setting local environment
├── config.json.example              # Reference configuration
├── README.md                        # Documentation
├── LICENSE                          # MIT License
├── liveaudio/                       # Python worker package and assets
│   ├── service/
│   │   ├── asr_worker.py            # Faster-Whisper worker (Protocol v1)
│   │   └── asr_worker_protocol.py   # IPC protocol definitions & schemas
│   ├── utils/
│   │   └── dllpath.py               # Windows CUDA/cuDNN DLL resolution
│   └── assets/
│       ├── subtitulos_obs.html      # OBS Studio browser source overlay
│       └── LiveAudio-Fran.ico       # Application icon
└── packaging/
    └── vendor/
        └── uv.exe                   # Vendored fast package manager for runtime provisioning
```

### 1.2 Tauri 2 NSIS Installer Configuration

Configured in [`desktop/src-tauri/tauri.conf.json`](file:///E:/LiveAudio/desktop/src-tauri/tauri.conf.json):
- **Bundle Targets**: `["nsis"]` generating single-file Windows installer.
- **Install Mode**: `currentUser` (installs to `%LOCALAPPDATA%\Programs\LiveAudio` without requiring administrative UAC elevation).
- **Multilingual Support**: English (`en-US`) and Spanish (`es-ES`).
- **Resource Bundling**: Recursive glob pattern `../../liveaudio/**/*` embeds all ASR worker modules, protocol schemas, and OBS HTML/CSS assets.
- **Application Metadata**: Product name, version, icons (32x32 to 512x512, ICO), category, and copyright.

---

## 2. Dynamic Worker Discovery Engine

Implemented in [`crates/liveaudio-core/src/supervisor/process.rs`](file:///E:/LiveAudio/crates/liveaudio-core/src/supervisor/process.rs) via `WorkerProcessConfig::discover_asr_worker()`:

### 2.1 Python Executable Resolution Hierarchy
When launching the supervised worker, the system discovers the Python runtime in order of preference:
1. **Explicit Environment Variable**: `LIVEAUDIO_PYTHON` (allows testing or external runtime injection).
2. **Adjacent to Binary (Installed / Portable)**:
   - `<exe_dir>\python.exe`
   - `<exe_dir>\python\python.exe`
   - `<exe_dir>\runtime\python.exe`
   - `<exe_dir>\.venv\Scripts\python.exe`
3. **Parent Hierarchy (Development / Workspace)**:
   - `<exe_dir>\..\.venv\Scripts\python.exe`
   - `<exe_dir>\..\..\.venv\Scripts\python.exe`
   - `<exe_dir>\..\..\..\.venv\Scripts\python.exe`
4. **Current Working Directory**:
   - `.venv\Scripts\python.exe`
5. **System Fallback**: Standard `python` executable on system `PATH`.

### 2.2 Worker Script Resolution Hierarchy
Discovers `asr_worker.py`:
1. **Explicit Environment Variable**: `LIVEAUDIO_WORKER_PATH`.
2. **Bundled Tauri Resource Paths**:
   - `<exe_dir>\liveaudio\service\asr_worker.py`
   - `<exe_dir>\resources\liveaudio\service\asr_worker.py`
   - `<exe_dir>\_up_\liveaudio\service\asr_worker.py`
3. **Development Paths**:
   - `<exe_dir>\..\..\..\liveaudio\service\asr_worker.py`
   - `liveaudio\service\asr_worker.py` relative to current directory.

### 2.3 Package Root & Environment Configuration
- Automatically resolves the repository/bundle root and injects it into `PYTHONPATH` so `import liveaudio...` executes without dependency on working directory.
- Configures standard unbuffered and UTF-8 flags:
  - `PYTHONUNBUFFERED=1`
  - `PYTHONUTF8=1`
  - `PYTHONIOENCODING=utf-8`
- Directs model caching to application data folders:
  - `HF_HOME`: `%APPDATA%\LiveAudio\models\hf`
  - `TORCH_HOME`: `%APPDATA%\LiveAudio\models\torch`

---

## 3. Hardware Acceleration & Dynamic Fallback Matrix

LiveAudio provides automated CUDA hardware detection with seamless, zero-crash CPU fallback:

| Component | Target / Version | CUDA Acceleration | CPU Fallback |
| :--- | :--- | :--- | :--- |
| **Python Runtime** | CPython 3.11.x (or 3.10-3.12) | Supported | Supported |
| **CTranslate2** | 4.8.0 | `float16`, `bfloat16`, `int8_float16` | `int8`, `float32` |
| **PyTorch / Audio** | 2.5.1+cu121 | NVIDIA CUDA 12.1+ / cuDNN 9 | MKL / OpenMP CPU backend |
| **Silero VAD** | ONNX Runtime 1.26.0 | DirectML / CUDA provider | CPU provider |
| **Whisper Models** | Faster-Whisper 1.2.1 | `tiny`, `base`, `small`, `turbo` | `tiny`, `base`, `small` |

### CUDA / cuDNN DLL Resolution on Windows
- `liveaudio.utils.dllpath.ensure_torch_dlls()` registers PyTorch's bundled CUDA/cuDNN DLL directory (`torch/lib`) with `os.add_dll_directory()` and prepends it to `PATH`.
- Resolves `cublas64_12.dll`, `cudnn_cnn_infer64_9.dll`, and `cudnn_ops_infer64_9.dll` dynamically before `ctranslate2` initializes.
- **Automatic Fallback on OOM or Missing Driver**: If CUDA initialization fails or raises `torch.cuda.OutOfMemoryError` / `RuntimeError`, the worker emits an informational event and seamlessly switches to CPU execution using `int8` quantization without terminating the application.

---

## 4. Path Safety & Unicode Compatibility

- **Path Handling**: All configuration file loading, saving, model directories, and session storage use native `PathBuf` on Windows without lossy ASCII conversions.
- **Spaces & Accents**: Verified in paths containing spaces and extended Unicode characters:
  - `%APPDATA%\LiveAudio\test spaces & accénts 🎙️`
- **Atomic Persistence**: Configuration updates write to `.config-<pid>-<nanos>.tmp`, call OS `FlushFileBuffers` (`sync_all()`), and perform atomic replace (`rename()`), preventing corruption during sudden system shutdowns.

---

## 5. Configuration Migration (`config.json` v1.2.7)

The Rust Core configuration engine ([`crates/liveaudio-core/src/config/mod.rs`](file:///E:/LiveAudio/crates/liveaudio-core/src/config/mod.rs)) maintains 100% backward compatibility with configurations generated by LiveAudio v1.2.7 and earlier:

1. **Context Prompt Field Migration**:
   - Legacy key: `"whisper_context_prompt"`
   - Modern keys: `"whisper_context_prompt_es"` (primary Spanish) and `"whisper_context_prompt_en"` (English)
   - Automatically migrates existing strings and clears the obsolete key.
2. **Out-of-Bounds Parameter Clamping**:
   - `silence_timeout`: Clamped to $[0.3, 2.0]$ seconds (default: 0.8s).
   - `max_chunk_duration`: Clamped to $[1.0, 15.0]$ seconds for subtitle mode (up to 60.0s for batch transcription).
   - `cpu_threads`: Clamped to $[1, \text{available\_parallelism}]$.
   - `vad_speech_pad_ms`: Clamped to $[0, 500]$ ms (defaults to 200 ms).
   - `vad_threshold`: Clamped to $[0.1, 0.9]$ (defaults to 0.5).
3. **Preservation of Extra / Custom Keys**:
   - Unknown settings and third-party plugin attributes are retained through `#[serde(flatten)] pub extra: serde_json::Map<String, Value>`.

---

## 6. Verification & Validation Evidence

### 6.1 Cargo Workspace Clean Compilation
```powershell
cargo build --workspace
```
Output:
```
   Compiling liveaudio-core v0.1.0 (E:\LiveAudio\crates\liveaudio-core)
   Compiling liveaudio-desktop v0.1.0 (E:\LiveAudio\desktop\src-tauri)
   Compiling liveaudio-cli v0.1.0 (E:\LiveAudio\crates\liveaudio-cli)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 8.37s
```
- `liveaudio-cli.exe`: 1,364,992 bytes (Exit Code: 0)
- `liveaudio-desktop.exe`: 13,893,120 bytes (Exit Code: 0)

### 6.2 Core Unit and Integration Tests
```powershell
cargo test -p liveaudio-core
```
Output:
```
running 15 tests
test config::tests::test_default_config_fields ... ok
test config::tests::test_audio_queue_capacity_sizing ... ok
test config::tests::test_normalization_and_clamping ... ok
test config::tests::test_legacy_whisper_prompt_migration ... ok
test tests::test_first_client_gate ... ok
test tests::test_cancellation_hierarchy ... ok
test config::tests::test_config_v1_2_7_comprehensive_migration ... ok
test tests::test_respawn_policy_backoff_and_limit ... ok
test tests::test_vtt_timestamp_formatting ... ok
test event::tests::test_event_bus_broadcast ... ok
test supervisor::process::tests::test_worker_process_config_discovery ... ok
test supervisor::process::tests::test_worker_process_config_env_override ... ok
test config::tests::test_atomic_file_saving_and_reading ... ok
test config::tests::test_unicode_and_spaces_path_handling ... ok
test event::tests::test_health_state_machine_transitions ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; finished in 0.02s

     Running tests\supervisor_mock_worker.rs
test result: ok. 5 passed; 0 failed; 0 ignored; finished in 2.23s
```
**Total**: 20 tests passed, 0 failed.

### 6.3 Headless CLI Diagnostics (`liveaudio-cli doctor`)
```powershell
target\debug\liveaudio-cli.exe doctor
```
Output:
```
=== LiveAudio Doctor ===
OS: windows
Architecture: x86_64
Windows Job Object: AVAILABLE (0 zombie child guarantees)
Python Runtime: \\?\E:\LiveAudio\.venv\Scripts\python.exe
ASR Worker Script: DISCOVERED (\\?\E:\LiveAudio\liveaudio\service\asr_worker.py)
App Data Home: C:\Users\tavo_\AppData\Roaming\LiveAudio
HuggingFace Cache (HF_HOME): C:\Users\tavo_\AppData\Roaming\LiveAudio\models\hf
Unicode Path Support: VERIFIED (C:\Users\tavo_\AppData\Roaming\LiveAudio\test spaces & accénts 🎙️)
WebSocket Port candidates: [8765, 8766, 8767, 8768, 8769, 8770, 8771, 8772, 8773, 8774]
Diagnostics complete.
```

### 6.4 Comprehensive Packaging Verification Pipeline
Executed via [`packaging/package_tauri_release.py --self-test`](file:///E:/LiveAudio/packaging/package_tauri_release.py):

```
==============================================================================
               LiveAudio Packaging & Release Verification (WU8)               
==============================================================================

>>> [1. Verifying Python Worker Runtime & Core Dependencies]
  [INFO] Using Python runtime: E:\LiveAudio\.venv\Scripts\python.exe
  [PASS] ctranslate2 v4.8.0 available (CUDA devices: 1)
  [PASS] faster-whisper v1.2.1 available
  [PASS] torch v2.5.1+cu121 available (CUDA: True)
  [PASS] onnxruntime v1.26.0 available (Silero VAD backend)
  [PASS] ASR Worker script present: E:\LiveAudio\liveaudio\service\asr_worker.py
>>> [2. Verifying CUDA / cuDNN DLL Inclusion & Automatic CPU Fallback]
  [INFO] Executing ASR Worker self-test suite (covers CUDA inference + CPU fallback)...
  [PASS] ASR Worker self-test passed all 7/7 phases (CUDA float16, hot swap, watchdog, CPU int8 fallback)
  [INFO] Testing forced CPU int8 fallback run...
  [PASS] Forced CPU int8 execution verified successfully (hardware-agnostic fallback ready)
>>> [3. Verifying Unicode & Spaces in Installation Paths (APPDATA, HF_HOME)]
  [INFO] Testing in Unicode path with spaces: C:\Users\tavo_\AppData\Local\Temp\LiveAudio Test José Ñandú 🎙️ xcjc8dgb
  [PASS] Unicode and spaces in configuration filesystem paths verified
  [PASS] liveaudio-cli doctor validated Unicode and spaces paths successfully
>>> [4. Verifying Configuration Migration from config.json v1.2.7]
  [PASS] config.json v1.2.7 schema migration logic verified (whisper_context_prompt -> whisper_context_prompt_es)
  [PASS] Out-of-range parameter bounding and custom field preservation verified
>>> [5. Verifying Cargo Workspace Build (dev profile)]
  [INFO] Running command: cargo build --workspace
  [PASS] Workspace built cleanly in 8.64s
  [PASS] liveaudio-cli.exe size: 1,364,992 bytes
  [PASS] liveaudio-desktop.exe size: 13,893,120 bytes
>>> [6. Verifying Tauri 2 NSIS Bundle Configuration]
  [PASS] Bundle active is set to true
  [PASS] Bundle targets configured: ['nsis'] (NSIS included)
  [PASS] Bundle resources configured: ['../../liveaudio/**/*']
  [PASS] NSIS installMode configured to 'currentUser' (no elevation required)
>>> [7. Assembling Portable Release Distribution]
  [PASS] Portable distribution created: E:\LiveAudio\dist\LiveAudio-0.1.0-windows-x86_64-portable.zip
  [PASS] Archive size: 28,882,193 bytes
  [PASS] SHA256: f785f1e6966b89a1503fa0878fc70ec4973c1447f28fbab8e469f48749fcb764
  [PASS] Checksum file written: E:\LiveAudio\dist\LiveAudio-0.1.0-windows-x86_64-portable.zip.sha256

==============================================================================
                             Verification Summary                             
==============================================================================

>>> ALL WORK UNIT 8 PACKAGING & COMPATIBILITY CHECKS PASSED <<<
```

---

## 7. Deployment Matrix Summary

| Distribution Format | Target Audience | Installer / Packaging Output | Elevation (UAC) |
| :--- | :--- | :--- | :--- |
| **NSIS Installer** | Standard Windows Users | `LiveAudio_0.1.0_x64-setup.exe` | Not required (`currentUser`) |
| **Portable ZIP** | Advanced Users / USB Drives | `LiveAudio-0.1.0-windows-x86_64-portable.zip` | None (standalone) |
| **Headless Daemon** | Servers / OBS Studio rigs | `liveaudio-cli.exe serve` / `run` | None |
