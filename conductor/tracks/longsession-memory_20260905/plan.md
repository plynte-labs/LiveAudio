# Track longsession-memory_20260905 — Plan

> Proposal only. No implementation authorized. PO approval required per phase gate.

## Phase 1 — Evidence re-verification (read-only)

Goal: confirm every Findings row still holds on the implementation base commit.

- [ ] Task: Re-read `SessionWriter` (`engine.py:111-152`) and re-run the stdlib-only synthetic stall probe mentally or as a script (no audio, no service).
  - [ ] Confirm unbounded queue + 2s-stop behavior; record drift if changed.
- [ ] Task: Re-read `_transcribe_with_timeout` + `ASR_TRANSCRIBE_TIMEOUT_SEC` (`engine.py:26`).
  - [ ] Confirm one-worker-per-timeout accumulation; record drift.
- [ ] Task: Re-read `DiagnosticsStore` (`diagnostics.py:74-101`) and the default-off gate (`:28-31`); confirm opt-in posture.
  - [ ] Confirm existing caps cited in REQ-5 (ring buffer, IPC queues, overlay queues) with exact lines.
- [ ] Task: Conductor - User Manual Verification 'Evidence re-verification' (Protocol in workflow.md).

## Phase 2 — Future specification (proposal hardening)

Goal: turn PROPOSED REQs into an approvable spec with exact caps + policies.

- [ ] Task: Specify SessionWriter cap value, overflow policy (drop-oldest vs blocking-put), drop counter name, and `stop()` drain/drop semantics (PO picks policy).
  - [ ] Specify the `session-persist-stalled` warning payload.
- [ ] Task: Specify timeout-worker lifecycle (reuse/cancel/reap), max in-flight bound, and duplicate-emission guard for late completions.
- [ ] Task: Specify diagnostics per-key ring cap + rollover counter; confirm default-off unchanged.
- [ ] Task: Conductor - User Manual Verification 'Future specification' (Protocol in workflow.md).

## Phase 3 — Future tests (no code yet)

Goal: define how T1–T4 will be proven, stdlib-first, no real audio.

- [ ] Task: Define synthetic probes: stalled-persistence writer cap; 5-false-timeout thread census; diagnostics ring rollover at small cap.
  - [ ] Name the new `test_resilience_*` / `test_idempotent_*` files per workflow gates.
- [ ] Task: Define PO-log-driven soak analysis steps per the Resilience Log Analysis Protocol (memory growth, queue pressure events).
- [ ] Task: Conductor - User Manual Verification 'Future tests' (Protocol in workflow.md).

## Phase 4 — Docs impact

Goal: scope user-facing fallout before any code.

- [ ] Task: List docs to update (`README.md`, `docs/GETTING_STARTED.md`, `HISTORIAL_CAMBIOS.md`) for new warnings/counters.
  - [ ] Note changelog entry wording (bilingual).
- [ ] Task: Conductor - User Manual Verification 'Docs impact' (Protocol in workflow.md).
