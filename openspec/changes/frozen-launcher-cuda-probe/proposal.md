# Proposal: Frozen-launcher-safe CUDA probe

## Intent

Make the out-of-process CUDA probe work under the packaged/frozen launcher. Today it relies on `sys.executable` being a bare Python interpreter; in a frozen build that silently breaks and forces every CUDA machine to CPU.

## Scope

### In Scope
- Make `cuda_is_available()` (utils/cuda.py) robust when running frozen.
- Keep the GUI process free of torch/ctranslate2 (no regression of ADR-014).

### Out of Scope
- Changing the ASR/VAD device-selection logic itself.
- The packaging/launcher build pipeline (tracked separately in `portable-packaging-resilience`).

## Capabilities

### Modified Capabilities
- CUDA availability detection must remain correct and torch-free in both dev (python-on-PATH) and frozen (`.exe`) runtimes.

## Approach

`utils/cuda.py` currently does `subprocess.run([sys.executable, "-c", code])`. In a frozen build `sys.executable` is the app `.exe`, so `-c "..."` relaunches the GUI instead of running the snippet → the child never returns 0/1 → the `except` returns `False` → CUDA users silently fall back to CPU.

Gate the probe target:
- When `not getattr(sys, "frozen", False)`: keep the current `sys.executable -c` path.
- When frozen: either (a) invoke a dedicated console entry point / `multiprocessing` spawn helper that runs the ctranslate2 query, or (b) detect CUDA natively via `ctypes` against `nvcuda.dll` (`cuInit` + `cuDeviceGetCount`), which pulls neither torch nor ctranslate2 and needs no subprocess.

Option (b) is the most robust and also removes the subprocess cost; option (a) reuses the existing ctranslate2 truth. Recommend prototyping (b) and falling back to (a) if driver-API edge cases appear.

## Affected Areas

| Area | Impact | Description |
|------|--------|-------------|
| `utils/cuda.py` | Modified | Branch the probe on `sys.frozen`; add native or entry-point path |
| `packaging/launcher.spec` / entry points | Modified (if option a) | Register a console helper the frozen build can invoke |
| `tests/test_lazy_imports.py` | Modified | Add a frozen-path simulation (monkeypatch `sys.frozen`) asserting the probe still returns without importing torch into the caller |

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| `nvcuda.dll` API differences across driver versions | Low | Wrap in try/except; fall back to the entry-point/subprocess path |
| Frozen entry point recursion | Medium | Guard with an explicit argv flag and `multiprocessing.freeze_support()` |
| False CPU fallback persists silently | Medium | Log the probe path taken + result; surface in diagnostics |

## Rollback Plan

Revert `utils/cuda.py` to the dev-only subprocess probe. CUDA detection still works in dev; only frozen CUDA builds regress (current state).

## Dependencies

- ADR-014 (torch-free GUI) — must not be regressed.
- Coordinates with the packaging overhaul (`portable-packaging-resilience`).

## Success Criteria

- [ ] In a frozen build on a CUDA machine, `cuda_is_available()` returns `True` and the ASR worker loads on GPU.
- [ ] In dev and frozen, importing the GUI / calling `load_config()` on a cuda config does not import torch into the GUI process.
- [ ] A test simulates the frozen runtime and asserts the probe path is correct and torch-free.
