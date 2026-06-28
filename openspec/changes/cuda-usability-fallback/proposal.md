# Proposal: CUDA usability fallback & non-destructive probe failure

## Intent

Harden two low-severity gaps introduced when the CUDA probe moved off torch (ADR-014): (1) a present-but-unusable GPU now fails late in the ASR worker with no CPU fallback, and (2) a transient probe failure can silently persist a cuda→cpu downgrade to the user's saved config.

## Scope

### In Scope
- A cuda→cpu fallback in the ASR worker's model-load path.
- Distinguish "probe reported 0 devices" from "probe raised" before downgrading, and avoid persisting the downgrade on probe failure.

### Out of Scope
- The frozen-launcher probe path (see `frozen-launcher-cuda-probe`).
- Re-adding the old `torch.zeros(1).cuda()` allocation smoke-test (it tested torch's allocator, not ctranslate2's — wrong backend).

## Capabilities

### Modified Capabilities
- Device selection degrades gracefully and never silently corrupts the saved device preference.

## Approach

**Fallback (engine.py):** `_validate_draft_config` now only checks CUDA *presence* via `cuda_is_available()`; a GPU that is present but cannot allocate (OOM, driver wedged, TCC mode) passes validation and then `WhisperModel(device="cuda")` throws at `engine.py:435`, caught at `engine.py:656` which only logs and shuts the worker down — no fallback. Add: on model-load failure with `device == "cuda"`, retry once with `device="cpu"` and notify the GUI (status pill + log) instead of leaving ASR dead.

**Non-destructive downgrade (config.py):** `load_config` sets `device="cpu"` and persists it when `cuda_is_available()` is falsy — but the probe swallows all exceptions, so a transient ctranslate2/DLL hiccup returns `False` and overwrites the user's saved `cuda` preference permanently. Change: only downgrade when the probe affirmatively reports zero devices; on probe *error*, apply cpu to the in-memory runtime config only, leaving the saved preference intact.

## Affected Areas

| Area | Impact | Description |
|------|--------|-------------|
| `core/engine.py` | Modified | cuda→cpu retry on model-load failure + GUI notification |
| `utils/cuda.py` | Modified | Distinguish "0 devices" from "probe raised" (tri-state or explicit error signal) |
| `utils/config.py` | Modified | Do not persist the downgrade on probe error; runtime-only fallback |
| `tests/` | Modified | Cover: probe raises → preference preserved; model-load fails on cuda → falls back to cpu |

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Silent CPU run when user expected GPU | Medium | Make the fallback loud (status pill + log line), not silent |
| Tri-state probe complicates callers | Low | Keep a simple bool API plus an optional "reason"/error out-param |

## Rollback Plan

Revert to the current present-only check and always-persist downgrade. Behavior returns to the known low-severity gaps.

## Dependencies

- Builds on ADR-014's `cuda_is_available()`. Best sequenced after `frozen-launcher-cuda-probe` so both touch `utils/cuda.py` once.

## Success Criteria

- [ ] A present-but-unusable GPU degrades to CPU with a visible notification; ASR keeps working.
- [ ] A transient probe failure does not rewrite the user's saved `device=cuda` preference.
- [ ] Tests cover both the fallback and the preserved-preference paths.
