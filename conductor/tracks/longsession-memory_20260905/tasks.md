# Track longsession-memory_20260905 — Tasks

> All boxes are `[ ]`: nothing is implemented. Each ticket references its REQ-ID(s).
> Full acceptance template per ticket (Given/When/Then/Error Path/UI State/OBS Behavior)
> lives in `spec.md` under Acceptance Criteria.

- [ ] Task: T1 — Bound SessionWriter pending retention (REQ-1, REQ-4)
  - [ ] Re-verify `engine.py:111-152` before designing; PO picks drop-oldest vs blocking-put.
  - [ ] Specify cap, drop counter, `stop()` drain/drop semantics, stall warning.
  - [ ] Acceptance: see `spec.md` T1 block (Given/When/Then/Error Path/UI State/OBS Behavior).

- [ ] Task: T2 — No per-timeout worker accumulation (REQ-2, REQ-4)
  - [ ] Re-verify `_transcribe_with_timeout` + `engine.py:26` before designing.
  - [ ] Specify worker reuse/cancel/reap lifecycle + late-completion duplicate guard.
  - [ ] Acceptance: see `spec.md` T2 block (Given/When/Then/Error Path/UI State/OBS Behavior).

- [ ] Task: T3 — Bound DiagnosticsStore retention, keep default-off (REQ-3, REQ-4)
  - [ ] Re-verify `diagnostics.py:74-101` and default-off gate `:28-31`.
  - [ ] Specify per-key ring cap + rollover counter; snapshot shape unchanged.
  - [ ] Acceptance: see `spec.md` T3 block (Given/When/Then/Error Path/UI State/OBS Behavior).

- [ ] Task: T4 — Cite existing caps; regression probes (REQ-5)
  - [ ] Cite each existing bound with file:line (GUI logs/previews, IPC queues, audio ring, WS replay, overlay queues).
  - [ ] Define stdlib-first regression probes locking T1–T3 behavior.
  - [ ] Acceptance: see `spec.md` T4 block (Given/When/Then/Error Path/UI State/OBS Behavior).
