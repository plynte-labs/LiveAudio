# Track vad-silence_20260905 — Tasks

> All boxes are `[ ]`: nothing is implemented. Each ticket references its REQ-ID(s).
> Full acceptance template per ticket (Given/When/Then/Error Path/UI State/OBS Behavior)
> lives in `spec.md` under Acceptance Criteria.

- [ ] Task: T1 — Lock segmentation behavior as segmentation-only (REQ-1)
  - [ ] Re-verify `audio.py:239-346` + `:300` before designing; keep probe synthetic/mocked.
  - [ ] Specify sequences, assertions, segmentation-only naming rule.
  - [ ] Acceptance: see `spec.md` T1 block (Given/When/Then/Error Path/UI State/OBS Behavior).

- [ ] Task: T2 — Gate proposal with contract check (REQ-2)
  - [ ] Quote the `vad-onset-grace` exclusion; PO picks compatible / amendment / accepted-gap in writing.
  - [ ] Prove no onset-clipping regression if any gate proceeds.
  - [ ] Acceptance: see `spec.md` T2 block (Given/When/Then/Error Path/UI State/OBS Behavior).

- [ ] Task: T3 — VAD thread fault surfacing + reconnect state contract (REQ-3, REQ-4)
  - [ ] Re-verify `audio.py:400-436` + `engine.py:247,434-438,566-570` before designing.
  - [ ] Specify fault status, recovery/abort semantics, escalation, retained-vs-cleared table.
  - [ ] Acceptance: see `spec.md` T3 block (Given/When/Then/Error Path/UI State/OBS Behavior).

- [ ] Task: T4 — Acquisition policy + test honesty (REQ-5, REQ-6)
  - [ ] Re-verify `audio.py:150-193` + pins; PO signs pin/TLS-scope/review-date.
  - [ ] Route duplicated tests through a shared strict-`>` helper or PO-delete them.
  - [ ] Acceptance: see `spec.md` T4 block (Given/When/Then/Error Path/UI State/OBS Behavior).
