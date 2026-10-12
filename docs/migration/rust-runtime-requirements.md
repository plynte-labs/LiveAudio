# Rust Migration: ASR Runtime Requirements

## Runtime boundary

The Rust application supervises the existing Python Faster-Whisper worker; Rust does not replace Python inference. Service startup needs a usable Python 3.11 interpreter, the `liveaudio` worker package, and successful imports of `faster_whisper`, `numpy`, `sounddevice`, `websockets`, and `torch`. Startup checks those imports with a bounded probe and does not load a model or select/probe a GPU.

The preflight imports run before worker startup, and the worker repeats its own imports; the incremental startup cost of this duplicate work has not been measured.

Existing development environments remain usable. `LIVEAUDIO_PYTHON` selects an explicit interpreter when it exists, and `LIVEAUDIO_WORKER_PATH` selects an explicit worker script when it exists. Otherwise the app checks its managed runtime and existing bundled/development Python locations before falling back to `python` on `PATH`. A missing dependency blocks service startup with an explicit setup instruction; startup does not invoke uv or download packages.

## Explicit setup

Run the setup command yourself and choose exactly one backend:

```text
liveaudio-cli setup-runtime --backend cpu
liveaudio-cli setup-runtime --backend cu121
```

`cpu` selects the project CPU extra. `cu121` selects the project's NVIDIA CUDA 12.1 PyTorch extra. The application does not infer a backend from detected hardware and does not silently switch extras. The command requires `uv` to be available from the app's bundled `uv.exe` locations or `PATH`.

Setup writes the embedded, lockfile-pinned `pyproject.toml` and `uv.lock`, plus the staged `liveaudio` worker source, under the app data home at `asr-runtime`. It then runs:

```text
uv sync --project <managed-runtime-root> --locked --no-install-project --no-dev --extra <cpu|cu121> --python 3.11
```

Provisioning may download uv-managed Python and Python packages from configured package indexes. It is an explicit, potentially network-using operation; an existing local cache may reduce downloads but offline completion is not guaranteed. Setup does not download ASR model weights. Models are fetched later by the Python worker according to its existing behavior.

On Windows, the default data home is `%APPDATA%\LiveAudio`; `LIVEAUDIO_HOME` is an explicit data-home override. The managed interpreter is `.venv\Scripts\python.exe` on Windows and `.venv/bin/python` on other platforms.

## Verification boundaries

Automated tests use a local fake uv executable to verify manifest/package materialization and the exact selected-backend sync arguments. They do not run uv sync, access package indexes, install dependencies, fetch models, or verify a real CUDA driver. No clean-VM or networked provisioning test has been performed for this migration correction. Before distribution, manually validate both the expected CPU path and any advertised CUDA path on clean supported machines, then confirm the packaged uv/runtime discovery layout and worker imports. Do not claim self-contained, offline, clean-VM, or CUDA-ready distribution until those checks have been completed on the actual artifacts.
