# Packaging, Releases and Updates

Canonical maintainer documentation for how LiveAudio is built, distributed,
installed and updated. User-facing instructions live in the
[README](../README.md); this document explains how the machinery works.

---

## 1. Architecture Overview

LiveAudio is distributed as a **small bootstrapper launcher** (a frozen
PyInstaller executable of `packaging/launcher.py`, stdlib + tkinter only)
rather than a multi-GB bundle. The launcher provisions everything else on
first run:

```
LiveAudio-Setup-X.Y.Z.exe / liveaudio-launcher
        │
        ├─ 1. Resolve install root (portable.marker / default per-user dir)
        ├─ 2. Detect hardware → torch extra: cpu | cu121
        ├─ 3. Download liveaudio-src-X.Y.Z.zip from the GitHub release
        │     (URL + SHA256 baked into the exe via _release_meta.py)
        ├─ 4. Ensure uv (bundled > PATH > previously downloaded)
        ├─ 5. uv sync --locked --extra <cpu|cu121> --python 3.11
        │     (uv also downloads a managed CPython into <root>/python)
        ├─ 6. Write installed.json (atomic; marks the install as complete)
        └─ 7. Launch <root>/app/.venv/.../liveaudio (gui-script entry point)
```

On subsequent runs, `installed.json` matches the launcher's target version,
so the launcher takes the **fast path**: no UI, straight to the app.

### Install layout

| Location | Content |
|---|---|
| `<root>/app/` | Application source (`pyproject.toml`, `uv.lock`, `liveaudio/`) |
| `<root>/app/.venv/` | Virtual environment created by `uv sync` |
| `<root>/python/` | uv-managed CPython 3.11 (`UV_PYTHON_INSTALL_DIR`) |
| `<root>/uv/` | Downloaded uv binary (only when not bundled / on PATH) |
| `<root>/installed.json` | `{app_version, extra, python, uv_version}` |
| `<root>/bootstrap.log` | Full launcher + uv log of every run |
| `<root>/data/` | App data home (`LIVEAUDIO_HOME`): `config.json`, `sessions/` |

Install root resolution (in `resolve_install_root`):

1. `LIVEAUDIO_INSTALL_ROOT` env var (testing/automation override)
2. **Portable mode**: a `portable.marker` file next to the launcher →
   `<launcher dir>/data` becomes the root; `HF_HOME` is also redirected so
   Whisper models stay inside the portable folder
3. Default: `%LOCALAPPDATA%\LiveAudio` (Windows) /
   `~/.local/share/liveaudio` (Linux)

The app's data home is `<root>/data` unless `LIVEAUDIO_HOME` is set
explicitly. When the app runs without the launcher it defaults to
`%APPDATA%\LiveAudio` on Windows; a legacy `config.json` in the current
working directory is migrated automatically on first load.

### Hardware detection ladder

`detect_device()` decides the torch extra, first match wins:

1. `--device cpu|cuda` CLI flag (persisted in `installed.json`)
2. Persisted preference (`extra` in `installed.json`)
3. NVML present (nvml.dll / libnvidia-ml) **and** `nvidia-smi` reports
   driver >= 525 and VRAM >= 4096 MiB → `cu121`
4. Anything else (no NVML, smi failure, weak GPU) → `cpu`, with a
   user-facing note when a GPU was found but below requirements

### Launcher CLI

`--device cpu|cuda`, `--update [vX.Y.Z]`, `--self-test`, `--reinstall`,
`--install-desktop-entry` (Linux), `--headless`, `--src-dir PATH`
(dev mode: bootstrap from a local checkout instead of a release zip).

---

## 2. Release Process

1. Bump `__version__` in `liveaudio/__init__.py` (single source of truth;
   hatchling reads it for the wheel version too).
2. Update `CHANGELOG.md`, commit, and tag: `git tag vX.Y.Z && git push --tags`.
3. The `release.yml` workflow runs on the tag:
   - verifies the tag matches `liveaudio.__version__` (fails the build if not)
   - builds the wheel and `liveaudio-src-X.Y.Z.zip` (pyproject + uv.lock +
     package, no caches)
   - exports `requirements-cpu.txt` / `requirements-cu121.txt` from the
     lockfile (pip-only escape hatch)
   - freezes the launcher on Windows and Linux with PyInstaller, embedding
     the source-zip URL + SHA256 (`packaging/build_release_meta.py`) and a
     vendored pinned uv binary
   - produces `LiveAudio-Setup-X.Y.Z.exe` and
     `LiveAudio-X.Y.Z-linux-x64.tar.gz`, generates `SHA256SUMS.txt`, and
     creates a **draft** GitHub release with all assets
4. A human reviews the draft (check assets, smoke-test the installer) and
   **publishes** it. Nothing ships without that manual step.

---

## 3. Update Flow

1. The app checks the GitHub Releases API in the background and shows an
   update button when a newer version exists.
2. Clicking it spawns the launcher (found via the `LIVEAUDIO_LAUNCHER` env
   var the launcher exports at app start) detached with `--update vX.Y.Z`,
   then the app exits.
3. The launcher fetches the release metadata, verifies the source zip
   against the release's checksum asset, replaces `<root>/app/` (the
   `.venv` is preserved), and re-runs `uv sync --locked`.
4. uv only downloads what changed — torch wheels are cached, so a typical
   update transfers a few MB, not gigabytes.
5. `installed.json` is rewritten last and the new version launches.

`--update` without a tag means "latest". If the installed version already
matches, the launcher reports "already up to date" and exits.

---

## 4. CUDA Constraints

- **Index routing:** the `cu121` extra resolves torch/torchaudio from
  `https://download.pytorch.org/whl/cu121`; `cpu` uses the `/whl/cpu`
  index. The extras are declared as conflicting in `[tool.uv]` — exactly
  one must be selected.
- **Version window:** `torch>=2.4,<2.6` on cu121. The cu121 index publishes
  no 2.6 wheels, and >=2.4 is required because it ships **cuDNN 9**, which
  `ctranslate2>=4.5` (faster-whisper's backend) links against.
- **DLL resolution (`liveaudio/utils/dllpath.py`):** on Windows,
  ctranslate2 loads `cublas64_12.dll` / cuDNN 9 DLLs via `LoadLibrary`,
  which searches `PATH` — not Python's DLL directories. `ensure_torch_dlls()`
  registers `torch/lib` with `os.add_dll_directory()` **and** prepends it to
  `PATH` before the model loads. Without this, CUDA inference fails with
  "cublas64_12.dll not found" even though torch bundles the DLLs.
- Driver floor is 525 (first driver branch with full CUDA 12.x support);
  below that the launcher falls back to CPU rather than installing a broken
  CUDA stack.

---

## 5. Troubleshooting

| Problem | What to do |
|---|---|
| Interrupted download of the big CUDA install | Just re-run the launcher. `uv` caches per-wheel, so completed wheels are not re-downloaded; the source zip is re-fetched and checksum-verified. |
| Corrupted / half-broken install | Run the launcher with `--reinstall` — wipes `app/` and the `.venv`, keeps the device preference, bootstraps fresh. |
| "SHA256 mismatch" | The download was corrupted or tampered with. The launcher retries automatically (3 attempts with backoff); persistent failures mean a proxy/AV is rewriting downloads. |
| Need to see what happened | Read `bootstrap.log` in the install root (`%LOCALAPPDATA%\LiveAudio` on Windows, `~/.local/share/liveaudio` on Linux, `data/` next to the launcher in portable mode). The error dialog has an "Open log" button. |
| Wrong backend installed | Re-run with `--device cpu` or `--device cuda`; the launcher re-syncs and persists the choice. |
| No GUI environment (servers, CI) | Use `--headless` for plain stdout progress. The launcher also falls back to headless automatically when tkinter is unavailable. |
| Diagnose paths/detection without installing | `--self-test` prints the detected device, resolved paths and uv status, then exits. |
