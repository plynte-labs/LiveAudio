# Track i18n-completeness_20260905 — Tasks

> All boxes are `[ ]`: nothing is implemented. Each ticket references its REQ-ID(s).
> Full acceptance template per ticket (Given/When/Then/Error Path/UI State/OBS Behavior)
> lives in `spec.md` under Acceptance Criteria.

- [ ] Task: T1 — stable_streaming applies in ES and EN (REQ-1, REQ-2, REQ-7)
  - [ ] Re-verify `app.py:1331-1353` + `i18n.py:160-168,:338-344`; PO picks rename direction.
  - [ ] Fix keys or pid so dispatch + refresh + both locales agree; add CI key-consistency probe.
  - [ ] Acceptance: see `spec.md` T1 block (Given/When/Then/Error Path/UI State/OBS Behavior).

- [ ] Task: T2 — No Spanish-only remainders (REQ-3)
  - [ ] Key model descriptions, folder dialog, validation errors, anti-hallucination label in both locales.
  - [ ] Acceptance: see `spec.md` T2 block (Given/When/Then/Error Path/UI State/OBS Behavior).

- [ ] Task: T3 — No English-only remainders + dynamic events keyed (REQ-4, REQ-5)
  - [ ] Key diagnostics UI copy; replace reverse-matched dynamic strings with parameterized keys on audited paths.
  - [ ] Acceptance: see `spec.md` T3 block (Given/When/Then/Error Path/UI State/OBS Behavior).

- [ ] Task: T4 — Selector callback honesty + UX decisions signed (REQ-6)
  - [ ] Honor `save_config=False`; PO signs welcome-only placement + hidden auto-detect copy.
  - [ ] Acceptance: see `spec.md` T4 block (Given/When/Then/Error Path/UI State/OBS Behavior).
