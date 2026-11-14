# liveaudio-asr-worker-ct2

CTranslate2 (CUDA) Whisper ASR worker for NVIDIA GPUs, shipped as the sidecar
`liveaudio-asr-worker-cuda.exe`. It speaks the same JSON-lines protocol as the ONNX Runtime
worker through `liveaudio-worker-protocol`.

This crate is a **standalone workspace** (its own `[workspace]` and `Cargo.lock`). The main
workspace, CI and release never build it; it is built locally with `packaging/build_ct2_worker.ps1`.

## Behavior

- CUDA only. `device: "cpu"` (or any non-CUDA device) returns `DEVICE_UNAVAILABLE`
  (non-recoverable) so the host falls back to the ORT worker. Any CUDA or model load failure
  returns `ENGINE_LOAD_FAILED` (non-recoverable) for the same reason.
- `compute_type` defaults to `float16`; `beam_size` defaults to 1 when the host sends none.
- `--self-test` prints `{"status":"ok","engine":"liveaudio-asr-worker-ct2"}` without touching
  the GPU or loading CUDA.
- Models: `tiny|base|small|medium` map to `Systran/faster-whisper-<size>`, `large-v3` to
  `Systran/faster-whisper-large-v3`, `turbo` to `deepdml/faster-whisper-large-v3-turbo-ct2`; any
  `owner/repo` is used as given. Cache: `--cache_dir` (init `cache_dir`), then
  `LIVEAUDIO_MODEL_CACHE`, then `%LOCALAPPDATA%\LiveAudio\models\ct2\<repo with / as -->`.
  `preprocessor_config.json` comes from `openai/whisper-<size>` for the small Systran repos.
- Known gaps of the ct2rs `Whisper` wrapper: `no_speech_prob` is always `0.0` and the
  initial/context prompt is ignored. Input must be 16 kHz mono f32.

## Build

Requirements: Rust (MSVC), Visual Studio C++ workload, CMake, a CUDA 12.x toolkit with cuBLAS
dev files and cuDNN 9 headers/libs (for example a conda env with `cuda-nvcc`, `libcublas-dev`
and `cudnn`; CUDA 12.8 or newer is needed for the sm_120 architecture).

```powershell
.\packaging\build_ct2_worker.ps1 -CudaRoot <toolkit root, e.g. ...\envs\cuda\Library> [-CudnnRoot ...] [-ArchList "7.5;8.6;8.9;12.0"]
.\packaging\build_ct2_worker.ps1 -CudaRoot ... -Test      # unit tests (no GPU needed)
```

The script imports the MSVC environment, sets `RUSTFLAGS=-C target-feature=+crt-static`, a short
`CARGO_TARGET_DIR` (default `C:/t/ct2`, avoids MAX_PATH in the VS generator), `CUDA_ARCH_LIST`,
`NVCC_APPEND_FLAGS=-allow-unsupported-compiler`, and leaves `CMAKE_PARALLEL` unset (it injects a
`-j` flag MSBuild rejects).

## Release

Add `-Pin` to the build to stage the exe in `dist-ct2/` and rewrite
`packaging/ct2-worker.pin.json` (version, URL, SHA-256, size); `-PinOnly -ExePath <exe>` does only
the pin step. Commit the pin file, then attach the exe to the GitHub release with
`gh release upload`. CI never builds this crate. See `docs/PACKAGING_AND_UPDATES.md`.

## Runtime

CUDA is loaded dynamically (`cuda-dynamic-loading`): put the directory with `cublas64_12.dll`,
`cublasLt64_12.dll` and the `cudnn*_9.dll` set on `PATH` before starting the worker.
