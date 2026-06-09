# Tasks: Local Observability and Diagnostics

## Review Workload Forecast

| Field | Value |
|-------|-------|
| Estimated changed lines | 320-520 |
| 400-line budget risk | High |
| Chained PRs recommended | Yes |
| Suggested split | PR 1 diagnostics core -> PR 2 runtime hooks -> PR 3 test/docs |
| Delivery strategy | ask-on-risk |
| Chain strategy | feature-branch-chain |

Decision needed before apply: Yes
Chained PRs recommended: Yes
Chain strategy: feature-branch-chain
400-line budget risk: High

### Suggested Work Units

| Unit | Goal | Likely PR | Notes |
|------|------|-----------|-------|
| 1 | Create diagnostics core and config gates | PR 1 | Base slice with no UI dependency |
| 2 | Instrument audio/engine/network runtime health | PR 2 | Depends on PR 1; verify bounded overhead |
| 3 | Add test-health summaries, docs, and UX/export | PR 3 | Depends on PR 2; closes maintainer flow |

## Phase 1: Diagnostics Foundation

- [x] 1.1 RED: add tests for diagnostics sanitization, level gating, and report schema.
- [x] 1.2 GREEN: create `core/diagnostics.py` with counters, durations, states, and report builders.
- [x] 1.3 GREEN: extend `utils/config.py` with validated diagnostics settings and safe defaults.

## Phase 2: Runtime Instrumentation

- [x] 2.1 RED: add focused tests for audio queue/callback/worker health snapshots.
- [x] 2.2 GREEN: instrument `core/audio.py` for ring-buffer depth, callback freshness, and worker lifecycle.
- [x] 2.3 RED: add focused tests for ASR/backlog/shutdown diagnostics.
- [x] 2.4 GREEN: instrument `core/engine.py` for model-load time, ASR latency/timeouts, backlog, and shutdown duration.
- [x] 2.5 RED: add focused tests for client-count/backpressure reporting.
- [x] 2.6 GREEN: instrument `core/network.py` for client count, retry buffer health, and queue-drain timing.

## Phase 3: Test-Health and Exposure

- [x] 3.1 RED: add teardown diagnostics tests for lingering queues/processes/threads.
- [x] 3.2 GREEN: add reusable pytest helpers in `tests/helpers.py` or `tests/conftest.py` for local health summaries.
- [x] 3.3 GREEN: expose diagnostics snapshot/export entry points in `main.py` with local-only messaging.

## Phase 4: Documentation and Maintainer Flow

- [x] 4.1 Update `README.md` and `docs/` with privacy boundary, enablement, and troubleshooting steps.
- [x] 4.2 Document how to use diagnostics to investigate hangs like non-exiting `pytest` runs.

## Phase 5: Verification

- [x] 5.1 Run focused diagnostics tests and confirm sanitized reports.
- [x] 5.2 Reproduce at least one known failure mode and verify diagnostics identify the affected subsystem.
- [x] 5.3 Verify docs and config match the shipped behavior.
