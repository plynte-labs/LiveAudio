# Track i18n-completeness_20260905 — Plan

> Proposal only. No implementation authorized. PO approval required per phase gate.

## Phase 1 — Evidence re-verification (read-only)

Goal: confirm every Findings row still holds on the implementation base commit.

- [ ] Task: Re-read `on_profile_select` (`app.py:1331-1353`) + both locale blocks (`i18n.py:160-168`, `:338-344`); confirm the `stable` vs `stable_streaming` mismatch persists.
  - [ ] Re-run the AST-extracted mocked reproduction (Fast applies / stable no-op in ES+EN) if cheap.
- [ ] Task: Re-read `refresh_profile_status` (`app.py:1305-1329`) and the gap sites (`app.py:665-684,:835-847,:920,:1093,:1502-1508,:1693-1718`; `i18n.py:387-394`; `engine.py:367,539`; `network.py:227,243`).
  - [ ] Record any line drift in `spec.md` drift note.
- [ ] Task: Confirm `tests/test_asr_language.py` current coverage boundaries (what it covers, what it does not).
- [ ] Task: Conductor - User Manual Verification 'Evidence re-verification' (Protocol in workflow.md).

## Phase 2 — Future specification (proposal hardening)

Goal: turn PROPOSED REQs into an approvable spec with exact key decisions.

- [ ] Task: PO picks mismatch-fix direction (rename locale keys vs rename dispatch pid) + migration note for existing configs referencing the preset id.
- [ ] Task: Specify keyed replacements for every reverse-matched dynamic string (parameterized keys, placeholder contracts ES/EN).
- [ ] Task: Specify selector `save_config=False` contract + PO-signed copy for welcome-only placement and hidden auto-detect relationship.
- [ ] Task: Conductor - User Manual Verification 'Future specification' (Protocol in workflow.md).

## Phase 3 — Future tests (no code yet)

Goal: define how T1–T4 will be proven, GUI-free where possible.

- [ ] Task: Define locale-key consistency probe (constructed preset keys exist both locales; no reverse-matched literals on audited paths; placeholder parity) as a CI test.
- [ ] Task: Define AST-extracted mocked selector tests (all four presets × ES × EN) + callback `save_config=False` no-write test.
- [ ] Task: Define EN-surface and ES-surface spot-check lists (T2/T3) as manual QA scripts per the QA Decision Rule.
- [ ] Task: Conductor - User Manual Verification 'Future tests' (Protocol in workflow.md).

## Phase 4 — Docs impact

Goal: scope user-facing fallout before any code.

- [ ] Task: List docs to update (`README.md`, `docs/GETTING_STARTED.md`, `HISTORIAL_CAMBIOS.md`) for preset rename direction + language behavior notes.
  - [ ] Note changelog entry wording (bilingual).
- [ ] Task: Conductor - User Manual Verification 'Docs impact' (Protocol in workflow.md).
