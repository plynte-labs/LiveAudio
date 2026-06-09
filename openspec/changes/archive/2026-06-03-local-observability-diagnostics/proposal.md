# Proposal: Local Observability and Diagnostics

## Intent

Stop diagnosing LiveAudio by suspicion alone. Add local-first observability and diagnostics so runtime stalls, queue leaks, shutdown hangs, and unhealthy tests can be identified from evidence.

## Scope

### In Scope
- Add a local diagnostics capability for runtime and test-health signals.
- Define sanitized local reports/exports for troubleshooting.
- Add config and docs for opt-in diagnostic levels.

### Out of Scope
- Remote telemetry, SaaS dashboards, or user analytics.
- Fixing every current suspect as part of this planning change.
- Always-on collection of transcript/audio contents.

## Capabilities

### New Capabilities
- `local-observability`: Collect, surface, and export sanitized local runtime and test diagnostics.

### Modified Capabilities
- None.

## Approach

Introduce a lightweight diagnostics layer with counters, timings, queue/process/thread snapshots, and explicit teardown reporting. Wire it into the audio, ASR, WebSocket, config, and test boundaries. Keep it local-only, bounded, and disabled or minimal by default.

## Affected Areas

| Area | Impact | Description |
|------|--------|-------------|
| `core/audio.py` | Modified | Emit queue depth, callback health, and worker lifecycle signals |
| `core/engine.py` | Modified | Emit model-load, ASR latency, backlog, and shutdown timings |
| `core/network.py` | Modified | Emit client-count, backpressure, and queue-drain diagnostics |
| `utils/config.py` | Modified | Add validated diagnostics configuration |
| `main.py` | Modified | Surface local diagnostics controls and export entry points |
| `tests/` | Modified | Add test-health checks and teardown diagnostics |
| `README.md`, `docs/` | Modified | Document privacy, controls, and troubleshooting flow |

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Runtime overhead | Medium | Sample/coarsen metrics and keep default mode minimal |
| Sensitive data leakage | Medium | Sanitize paths, envs, and content; never export raw audio/transcripts |
| Metric sprawl | Medium | Start with a bounded schema tied to concrete failure modes |

## Rollback Plan

Disable diagnostics via config, remove UI/report hooks, and revert instrumentation files without changing core audio/ASR behavior.

## Dependencies

- Existing strict TDD workflow.
- No external service dependency.

## Success Criteria

- [ ] A local report can identify long-lived queues/processes/threads after a run.
- [ ] Runtime diagnostics expose stage latency, backlog, and shutdown health without remote telemetry.
- [ ] Docs define what is collected, what is never collected, and how to enable it.
