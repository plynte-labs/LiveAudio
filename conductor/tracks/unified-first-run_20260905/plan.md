# Track unified-first-run_20260905 — Implementation Plan

> **Status:** Planning baseline; implementation is partial/interrupted. See `handoff.md` for current execution state; task checkboxes are not yet reconciled.
> **Dependency:** First complete/review `firstuse-startup-progress_20260905` integration review. Its M1–M8 manual matrix remains pending and must not be silently closed here.
> **Method:** TDD (RED → GREEN → REFACTOR), one shared-file writer at a time, no destructive cleanup, and no commit without explicit approval.

## Phase 0 — Contract and baseline verification

Goal: freeze the approved product boundary before source changes.

- [ ] Task: P0.1 — Read `spec.md`, `AGENTS.md`, `conductor/workflow.md`, product docs, and current first-use artifacts.
  - [ ] Confirm all 13 PO decisions and checklist phases 0–7 are represented.
  - [ ] Confirm dependency wording: first-use implemented/reviewed, M1–M8 pending.
- [ ] Task: P0.2 — Re-verify CodeGraph/file citations before editing.
  - [ ] Re-check `packaging/launcher.py:831-894`, `:909-947`, `:1257-1313`, `:1335-1359`, `:950-984`.
  - [ ] Re-check `liveaudio/app.py:261-340` and `liveaudio/core/audio.py:146-193` plus producer boundary around `:256-265`.
  - [ ] Record line drift in the track findings without changing unrelated documents.
- [ ] Task: P0.3 — Verify existing persisted language support and current `hf-cache` reinstall behavior.
  - [ ] Use the existing `install_location.json` atomic writer (`packaging/launcher.py:194-229`) and locked config save path (`liveaudio/utils/config.py:366-405, :530-541`).
  - [ ] Implement the concrete precedence: valid `config.json.language` from resolved home, then valid `install_location.json.language`, then OS detection; allow only `es|en`.
  - [ ] Persist launcher selection atomically to `install_location.json` immediately before handoff/Popen; on launcher-origin app startup require flag + root equality + accepted handoff before applying only language to normalized config.
  - [ ] Keep direct app precedence config → location → OS and do not mirror app language back to location.
  - [ ] Add a regression fixture for cache hashes before any reinstall code change.
- [ ] Task: Conductor — User Manual Verification `Contract and baseline verification` (Protocol in workflow.md).

## Phase 1 — RED tests and fixtures

Goal: write failing tests before changing runtime behavior. Tests must not require microphone, GPU, network, raw audio, or real user session data.

- [ ] Task: T1 — Add failing phase-vocabulary and bilingual-copy tests (REQ-1, REQ-2, REQ-3).
  - [ ] Cover all checklist phases 0–7, ES/EN labels, retry, indeterminate, error, and ready copy.
  - [ ] Acceptance: use the full T1 block in `spec.md` (Given/When/Then/Error Path/UI State/OBS Behavior).
- [ ] Task: T2 — Add failing atomic handoff tests (REQ-4, REQ-5).
  - [ ] Cover exact six-field schema, same-directory temp + flush/fsync + replace, malformed/truncated/stale data, root escape, type mismatch, attempt/version mismatch, and safe fallback.
  - [ ] Acceptance: use the full T2/T3 blocks in `spec.md`.
- [ ] Task: T3 — Add failing launcher progress/window-boundary tests (REQ-3, REQ-6, REQ-11, REQ-12, REQ-13).
  - [ ] Prove launcher global completion is not ASR readiness.
  - [ ] Prove bounded title-visible handoff, process-exit/timeout paths, no IPC, no Whisper pre-download, and no duplicate catalog.
  - [ ] Acceptance: use the full T4/T9 blocks in `spec.md`.
- [ ] Task: T4 — Add failing VAD provisioning and replacement-safety tests (REQ-7, REQ-8).
  - [ ] Cover indeterminate heartbeat, `finally` cleanup, network/TLS/cache-corrupt classification, pre-ready join-before-replace, live-stall conservative behavior, and ASR preservation.
  - [ ] Add a regression proving the VAD insecure TLS fallback is absent.
  - [ ] Acceptance: use the full T6 block in `spec.md`.
- [ ] Task: T5 — Add failing early-paint, Whisper evidence, retry, and cache-preservation tests (REQ-6, REQ-9, REQ-10).
  - [ ] Cover real bytes/tqdm-only percentages, stale-attempt rejection, indeterminate loading, prewarm retry, and `--reinstall` cache hashes.
  - [ ] Acceptance: use the full T5/T7/T8 blocks in `spec.md`.
- [ ] Task: Conductor — User Manual Verification `RED tests and fixtures` (Protocol in workflow.md).

## Phase 2 — GREEN implementation

Goal: implement the smallest design that makes the RED tests pass without changing transport ownership.

- [ ] Task: T6 — Implement the shared phase/state and bilingual copy contract (REQ-1, REQ-2, REQ-3).
  - [ ] Implement the resolved `install_location.json` language design and parity-tested ES/EN launcher vocabulary; do not add a seventh handoff field.
  - [ ] Keep launcher stdlib + tkinter and use the same human vocabulary in the app.
  - [ ] Keep the visual barrier bounded to window visibility, never ASR readiness.
- [ ] Task: T7 — Implement atomic handoff production/consumption (REQ-4, REQ-5).
  - [ ] Snapshot exactly six fields before `Popen`, phases done 0–3 only.
  - [ ] App marks phase 4 only after its own early paint and never trusts a launcher ready claim.
  - [ ] Reject malformed/stale/path-overridden input and continue safely without payload logging.
- [ ] Task: T8 — Implement truthful launcher and app progress (REQ-6, REQ-9, REQ-11, REQ-13).
  - [ ] Replace heuristic `uv sync` percentages and post-download percentages with indeterminate states unless byte/tqdm evidence exists.
  - [ ] Paint the app before heavy imports and continue phases 5–7 locally.
  - [ ] Do not add snapshot_download, mirrors, custom resume, IPC, launcher Whisper download, or duplicate HF/catalog code.
- [ ] Task: T9 — Implement VAD provisioning, heartbeat, retry, and safe lifecycle (REQ-7, REQ-8).
  - [ ] Add VAD `provision-*` states and localized remediation.
  - [ ] Remove the VAD insecure TLS bypass and surface TLS failures honestly.
  - [ ] For a pre-ready failed producer: cooperative stop, join, prove no remaining writes, then replace while preserving the same queue objects and ASR process.
  - [ ] For a live stalled producer: report stalled; do not force-terminate silently; use bounded cooperative exit or explicit full stop/restart with visible state.
- [ ] Task: T10 — Lock reinstall cache preservation (REQ-10).
  - [ ] Preserve `hf-cache` unconditionally; do not add a purge UI or command in the MVP.
  - [ ] Verify before/after hashes and leave unrelated install data untouched.
- [ ] Task: Conductor — User Manual Verification `GREEN implementation` (Protocol in workflow.md).

## Phase 3 — REFACTOR, integration, and documentation

Goal: clean up implementation, preserve compatibility, and make the behavior explainable to users and reviewers.

- [ ] Task: T11 — Refactor for idempotency/resilience without changing the approved contract.
  - [ ] Ensure duplicate handoff reads, repeated retries, stale events, and repeated language selection converge to the same state.
  - [ ] Ensure no unsafe shared-queue reuse and no OBS subtitle burst during recovery.
  - [ ] Add/maintain `test_resilience_*.py` and `test_idempotent_*.py` coverage.
- [ ] Task: T12 — Update user-facing documentation.
  - [ ] Update `README.md`, `docs/GETTING_STARTED.md`, `HISTORIAL_CAMBIOS.md`, and `docs/PACKAGING_AND_UPDATES.md` with phase vocabulary, handoff/window boundary, truthful progress, VAD retry/errors, and reinstall cache semantics.
  - [ ] Document excluded Option A/B and transport features without implying implementation.
- [ ] Task: T13 — Add manual evidence instructions and preserve historical distinction.
  - [ ] Publish E2E-1…E2E-10 instructions with preconditions/actions/expected/evidence.
  - [ ] Keep first-use M1–M8 explicitly pending until real evidence exists.
- [ ] Task: Conductor — User Manual Verification `REFACTOR, integration, and documentation` (Protocol in workflow.md).

## Phase 4 — Verification and collective review

Goal: prove the exact candidate and stop if any gate lacks evidence.

- [ ] Task: V1 — Run focused TDD tests and resilience/idempotency suites.
  - [ ] Record results; historical first-use counts (30 focused + 66 resilience/idempotency) are baseline context, not rerun evidence.
- [ ] Task: V2 — Run `python -m compileall liveaudio packaging/launcher.py tests` using the current package layout, plus the exact relevant test commands determined by the implementer, and check-only static validation.
- [ ] Task: V3 — Perform collective review by Architecture, Performance, QA, and Research roles.
  - [ ] Require structured PASS/FAIL/PASS-with-notes sign-offs.
  - [ ] Resolve blockers before manual closure; no reviewer may infer E2E evidence.
- [ ] Task: V4 — Rebuild VM v2 and verify clean-machine behavior.
  - [ ] Keep `build_artifacts/vm-test/` ignored and unversioned.
- [ ] Task: V5 — Execute E2E-1…E2E-10 and attach evidence.
  - [ ] Mark CUDA E2E-2 not run only with a recorded capability reason.
  - [ ] Do not mark any manual row or M1–M8 complete without real evidence.
- [ ] Task: Conductor — User Manual Verification `Verification and collective review` (Protocol in workflow.md).

## Phase 5 — Auditor presentation and approval gate

- [ ] Task: F1 — Present changed paths, tests, compileall, review sign-offs, VM v2 result, and E2E evidence.
- [ ] Task: F2 — Obtain explicit approval before any stage, commit, push, merge, or revert.
- [ ] Task: F3 — Update track status only after the auditor confirms all gates and evidence.
- [ ] Task: Conductor — User Manual Verification `Auditor presentation and approval gate` (Protocol in workflow.md).

## Test and acceptance mapping

| Ticket | Requirements | Acceptance block |
|---|---|---|
| T1 | REQ-1–REQ-3 | `spec.md` T1 |
| T2 | REQ-4–REQ-5 | `spec.md` T2–T3 |
| T3 | REQ-3, REQ-6, REQ-11–REQ-13 | `spec.md` T4, T9 |
| T4 | REQ-7–REQ-8 | `spec.md` T6 |
| T5 | REQ-6, REQ-9–REQ-10 | `spec.md` T5, T7–T8 |
| T6 | REQ-1–REQ-3 | `spec.md` T1 |
| T7 | REQ-4–REQ-5 | `spec.md` T2–T3 |
| T8 | REQ-6, REQ-9, REQ-11, REQ-13 | `spec.md` T4–T5, T9 |
| T9 | REQ-7–REQ-8 | `spec.md` T6 |
| T10 | REQ-10 | `spec.md` T8 |
| T11–T13 | All applicable requirements | `spec.md` closure gates and E2E matrix |
