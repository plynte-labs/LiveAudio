# ONNX Runtime assets (Windows DirectML & Linux CPU)

LiveAudio loads native ONNX Runtime (Silero VAD and native Whisper ASR) from the application-owned resource directory. The binaries are **not stored in git**: fetch them with

```
node packaging/fetch_onnxruntime.mjs            # current OS
node packaging/fetch_onnxruntime.mjs windows-x64 linux-x64
```

The script downloads each official archive, aborts on any SHA-256 mismatch, extracts only the needed files into `packaging/onnxruntime/<platform>/`, and is idempotent. CI and the release workflow run it before building. Both platforms use **ONNX Runtime 1.24.4** (the last version with an official DirectML build).

## Pinned upstream artifacts

| Target | Source | Version | Files | Acceleration |
|---|---|---|---|---|
| Windows x64 | NuGet `Microsoft.ML.OnnxRuntime.DirectML` | 1.24.4 | `onnxruntime.dll`, `onnxruntime_providers_shared.dll`, `LICENSE`, `ThirdPartyNotices.txt` | DirectX 12 DirectML (NVIDIA, AMD, Intel) + CPU |
| Windows x64 | NuGet `Microsoft.AI.DirectML` | 1.15.4 | `DirectML.dll`, `DirectML-LICENSE.txt` | - |
| Linux x64 | [onnxruntime-linux-x64-1.24.4.tgz](https://github.com/microsoft/onnxruntime/releases/download/v1.24.4/onnxruntime-linux-x64-1.24.4.tgz) | 1.24.4 | `libonnxruntime.so.1.24.4`, `libonnxruntime_providers_shared.so`, `LICENSE`, `ThirdPartyNotices.txt` | Multi-threaded CPU |

SHA-256 pins live in `packaging/fetch_onnxruntime.mjs`. To bump a version, update the URL and hash there, the Linux `.so` filename in `crates/liveaudio-core/src/runtime.rs`, `crates/liveaudio-vad/src/runtime.rs`, `desktop/src-tauri/tauri.linux.conf.json`, and the CI/release assertions.

## Licenses

- ONNX Runtime: MIT.
- `DirectML.dll`: Microsoft Software License Terms (not MIT); redistributable as part of the app. The text ships as `onnxruntime/windows-x64/DirectML-LICENSE.txt` and is referenced in `THIRD_PARTY_NOTICES.md`.

## Packaging boundary

- Windows: Tauri bundles these files via `desktop/src-tauri/tauri.windows.conf.json`; the standalone CLI zip copies the directory. Release staging checks `DirectML.dll`, `onnxruntime.dll` and the DirectML license.
- Linux: AppImage/deb via `tauri.linux.conf.json` and the CLI tarball.
