# Design: Local Observability and Diagnostics

## Technical Approach

Add a small diagnostics module that records structured counters, durations, and lifecycle snapshots across the existing multiprocessing/threading boundaries. Instrumentation stays local, avoids transcript/audio payload capture, and supports two levels: `minimal` and `deep`.

## Architecture Decisions

| Decision | Choice | Alternatives | Rationale |
|----------|--------|-------------|-----------|
| Data ownership | In-process event sink + snapshot helpers | External agent, remote collector | Fits LiveAudio's local app model and avoids privacy/network expansion |
| Integration style | Explicit hooks at queue/process/thread boundaries | Generic logging everywhere | Targets the real failure modes: backlog, hangs, and teardown leaks |
| Exposure model | Local report/export and optional UI panel | Background upload/dashboard | Keeps diagnostics useful without changing trust boundaries |
| Control plane | Config-backed diagnostic level in `utils/config.py` | Hard-coded always-on mode | Lets maintainers turn depth up only when needed |

## Data Flow

`core/audio.py` -> emit callback/queue/worker signals  
`core/engine.py` -> emit model-load/ASR/backlog/shutdown signals  
`core/network.py` -> emit client/backpressure/drain signals  
`tests/*` -> emit teardown/resource summaries  
All signals -> `core/diagnostics.py` (new) -> local snapshot/report -> optional UI/export in `main.py`

## File Changes

| File | Action | Description |
|------|--------|-------------|
| `core/diagnostics.py` | Create | Shared counters, timers, resource snapshot helpers, sanitized report builder |
| `core/audio.py` | Modify | Track ring-buffer depth, callback freshness, worker/thread lifecycle |
| `core/engine.py` | Modify | Track model load, ASR latency/timeouts, backlog decisions, shutdown duration |
| `core/network.py` | Modify | Track client count, retry/backpressure state, queue-drain timings |
| `utils/config.py` | Modify | Add validated diagnostics toggles/levels/export path |
| `main.py` | Modify | Add UI/report entry points and safe visibility of current health |
| `tests/conftest.py` or `tests/helpers.py` | Modify/Create | Add reusable teardown diagnostics helpers |
| `tests/test_*.py` | Modify | Add coverage for diagnostics summaries and leak detection |
| `README.md`, `docs/` | Modify | Document privacy boundary and maintainer workflow |

## Interfaces / Contracts

- `record_counter(name, delta=1, tags=None)`
- `record_duration(name, seconds, tags=None)`
- `record_state(name, value, tags=None)`
- `snapshot_runtime_health()` -> sanitized dict
- `snapshot_test_health()` -> sanitized dict
- `build_diagnostics_report()` -> text/json-ready local artifact

## Testing Strategy

| Layer | What to Test | Approach |
|-------|-------------|----------|
| Unit | Sanitization, level gating, report formatting | Deterministic tests for diagnostics helpers |
| Integration | Audio/engine/network hooks emit bounded signals | Targeted tests with mocks/fakes around queues and workers |
| E2E | Known hang/leak scenarios produce useful evidence | Focused pytest runs and local report assertions |

## Migration / Rollout

No migration required. Ship disabled or minimal by default, then document how maintainers enable deeper diagnostics only during investigation.

## Open Questions

- [ ] Whether diagnostics export should default to JSON, text, or both in the first slice.
- [ ] Whether UI exposure should be limited to an export button in the first implementation slice.
