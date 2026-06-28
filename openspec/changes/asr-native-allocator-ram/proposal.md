# Proposal: Investigate & bound ASR-worker native RAM (secondary growth suspect)

## Intent

After fixing the primary session RAM leak (ADR-015, Silero VAD under `inference_mode`), confirm whether any residual growth remains in the ASR worker's native ctranslate2/torch allocator, and bound it if real. This is a measure-first change.

## Scope

### In Scope
- Per-PID RSS + tracemalloc instrumentation to attribute residual growth.
- A mitigation (only if confirmed): periodic respawn of the ASR child and/or a max-utterance-duration cap.

### Out of Scope
- The VAD leak (already fixed, ADR-015).
- CUDA VRAM accounting (ctranslate2 CUDA buffers live in VRAM, not system RSS).

## Capabilities

### Modified Capabilities
- ASR worker lifecycle gains an optional bounded-memory recycling policy.

## Approach

The diagnosis ranked this as *uncertain*: `model.transcribe(beam_size=5)` runs once per utterance; ctranslate2 4.x owns its allocator and `torch.cuda.empty_cache()` does not free CT2 buffers (and is CUDA-only / conditional). The expectation is that CT2 pools converge to a high-water-mark (plateau), not linear growth — so this likely does NOT need a fix. Confirm before touching:

1. Run a long session with the per-PID RSS logger + tracemalloc in `asr_consumer`.
2. If `asr_consumer` RSS rises **while tracemalloc stays flat** → native allocator. If it plateaus → benign high-water-mark (no action).
3. Reproduce with a fixed 5s synthetic-audio loop: if RSS keeps climbing with constant input → real native growth.

Mitigation ONLY if confirmed CPU-int8 + linear growth: respawn the ASR child every N utterances (`terminate()` returns all native RSS to the OS, the same mechanism `hot_swap_engine` already uses) and cap maximum utterance duration.

## Affected Areas

| Area | Impact | Description |
|------|--------|-------------|
| `core/engine.py` | Investigated / maybe modified | `asr_consumer` loop; optional utterance-duration cap |
| `app.py` | Maybe modified | Optional periodic ASR-worker respawn policy |
| `tools/` (new, optional) | Added | Reusable RSS-per-PID + tracemalloc probes |

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Respawn drops in-flight audio | Medium | Respawn only at utterance boundaries; drain queues first (as hot_swap does) |
| Chasing a non-issue (it plateaus) | Medium | Measure-first gate; do nothing if RSS plateaus |
| Utterance cap truncates long speech | Low | Make the cap generous and configurable |

## Rollback Plan

If mitigation is added and proves unnecessary or harmful, remove the respawn/cap policy; the ASR worker reverts to single-load lifetime.

## Dependencies

- ADR-015 must be applied and verified first (VAD leak removed) so this is measured in isolation.

## Success Criteria

- [ ] A measured verdict: is there residual linear RSS growth in `asr_consumer` after the VAD fix? (yes/no, with numbers).
- [ ] If yes and CPU: RSS stays bounded across a long session with the recycling policy.
- [ ] If no: documented as a non-issue (plateau), no code change shipped.
