# Track vad-silence_20260905 — Plan

> Proposal only. No implementation authorized. PO approval required per phase gate.
> Hard boundary throughout: do not contradict `vad-onset-grace_20260619/spec.md`.

## Phase 1 — Evidence re-verification (read-only)

Goal: confirm every Findings row still holds on the implementation base commit.

- [ ] Task: Re-read `vad_worker` (`audio.py:239-346`), strict `>` (`:300`), inference guard (`:296-298`), unverified block (`:150-193`), reconnect/monitor (`:400-436`).
  - [ ] Re-run the synthetic segmentation probe (mocked frames, stdlib-only, no mic/GPU) if cheap; confirm nothing / nothing / 1.088s-zeros.
- [ ] Task: Re-read `engine.py:247,434-438,566-570` (monitor, reconnect, ASR SSL mutation) + `uv.lock`/`pyproject.toml` pins.
  - [ ] Record any line drift in `spec.md` drift note.
- [ ] Task: Re-read `tests/test_audio.py:219-239` + `tests/test_noise_detection.py:98-135`; confirm duplicated/invented-math characterization.
- [ ] Task: Re-read `vad-onset-grace_20260619/spec.md` exclusion clause; write down the exact contract boundary for T2.
- [ ] Task: Conductor - User Manual Verification 'Evidence re-verification' (Protocol in workflow.md).

## Phase 2 — Future specification (proposal hardening)

Goal: turn PROPOSED REQs into an approvable spec inside the contract boundary.

- [ ] Task: Specify the segmentation-only regression test (sequences, assertions, naming/docstring rule).
- [ ] Task: Run the T2 contract check: gate proposal vs onset-grace exclusion → one of compatible-with-reasoning / amendment-proposal / accepted-gap (PO picks in writing).
- [ ] Task: Specify `vad-worker-fault` status payload + recover-vs-abort semantics + reconnect retained-vs-cleared table (PO decides retained state).
- [ ] Task: Specify acquisition policy (pin source, TLS scope, review date) + SSL-mutation removal scope.
- [ ] Task: Conductor - User Manual Verification 'Future specification' (Protocol in workflow.md).

## Phase 3 — Future tests (no code yet)

Goal: define how T1–T4 will be proven, synthetic-first.

- [ ] Task: Define mocked-frame probe matrices (silence sweeps, threshold boundary 0.5±ε, single-hot-frame) + shared strict-`>` helper replacing duplicated test logic.
- [ ] Task: Define fault-injection specs (inference raise → fault status + recovery; double-fault escalation; reconnect state table test).
- [ ] Task: Define acquisition-policy review checklist (pin proof, TLS scope proof, no-compromise language check).
- [ ] Task: Conductor - User Manual Verification 'Future tests' (Protocol in workflow.md).

## Phase 4 — Docs impact

Goal: scope user-facing fallout before any code.

- [ ] Task: List docs to update (`README.md`, `docs/GETTING_STARTED.md`, `HISTORIAL_CAMBIOS.md`) for fault UX + acquisition policy notes.
  - [ ] Note changelog entry wording (bilingual).
- [ ] Task: Conductor - User Manual Verification 'Docs impact' (Protocol in workflow.md).
