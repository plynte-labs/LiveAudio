# Track unified-first-run_20260905 — Tasks

> **Status:** Planning baseline; implementation is partial/interrupted. See `handoff.md` for current execution state; task checkboxes are not yet reconciled.
> **Dependency:** `firstuse-startup-progress_20260905` is implemented and reviewed, but M1–M8 remain pending manual evidence. Do not mark them complete from this track.
> **Acceptance:** Every implementation ticket links to a complete Given/When/Then/Error Path/UI State/OBS Behavior block in `spec.md`.

## Phase 0 — Contract and baseline

- [ ] **T0.1 — Confirm scope and PO decisions** (REQ-1–REQ-13)
  - [ ] Confirm D1–D13, checklist phases 0–7, Option C, and no-IPC boundary.
  - [ ] Confirm first-use dependency and M1–M8 pending status.
  - [ ] Acceptance: `spec.md` scope and decision table match the approved PO input exactly.
- [ ] **T0.2 — Re-verify implementation anchors** (REQ-3, REQ-6, REQ-7)
  - [ ] Re-check `packaging/launcher.py:831-894`, `:909-947`, `:1257-1313`, `:1335-1359`, `:950-984`.
  - [ ] Re-check `liveaudio/app.py:261-340` and `liveaudio/core/audio.py:146-193`, producer boundary `:256-265`.
  - [ ] Acceptance: drift is recorded before source edits.
- [ ] **T0.3 — Verify language and cache baselines** (REQ-2, REQ-10)
  - [ ] Use existing `install_location.json` atomic persistence and locked config save; allow only `es|en`.
  - [ ] Verify precedence: valid config language from resolved home, then valid location language, then OS detection; launcher selection persists location immediately before handoff/Popen.
  - [ ] Verify launcher-origin app application requires existing flag, root equality, and accepted handoff; only language may change in normalized config.
  - [ ] Verify direct app precedence config → location → OS and no app-to-location mirror.
  - [ ] Record `hf-cache` file hashes before any reinstall implementation.
  - [ ] Add a test for existing ES config plus launcher EN: app starts EN without clobbering other preferences; invalid location language falls back without mutation.
  - [ ] Acceptance: language and cache behavior are backed by the existing atomic paths, not guessed.

## Phase 1 — RED tests and fixtures

- [ ] **T1 — Phase vocabulary and bilingual copy tests** (REQ-1, REQ-2, REQ-3)
  - [ ] Test all phases 0–7 and ES/EN indeterminate/retry/error/ready copy.
  - [ ] Test window-visible versus ASR-ready semantics.
  - [ ] Acceptance: see `spec.md` T1.
- [ ] **T2 — Atomic handoff schema tests** (REQ-4, REQ-5)
  - [ ] Test exactly six fields: `hf_home`, `install_root`, `extra`, `app_version`, `attempt`, `launcher_phases_done`.
  - [ ] Test same-directory temp/fsync/replace behavior and safe fallback for malformed/truncated/stale/root-escape input.
  - [ ] Acceptance: see `spec.md` T2 and T3.
- [ ] **T3 — Launcher progress/window-boundary tests** (REQ-3, REQ-6, REQ-11–REQ-13)
  - [ ] Test no fake uv percentage, no global-100%-means-ready claim, bounded title barrier, process exit, and timeout.
  - [ ] Test no IPC, no launcher Whisper download, no duplicate catalog, and explicit exclusions.
  - [ ] Acceptance: see `spec.md` T4 and T9.
- [ ] **T4 — VAD provisioning and lifecycle tests** (REQ-7, REQ-8)
  - [ ] Test heartbeat, indeterminate state, `network/tls/cache-corrupt`, retry-only-VAD, `finally` cleanup, and no insecure TLS fallback.
  - [ ] Test pre-ready stop/join/prove-no-writes before replacement, preservation of the same queues and ASR, and live-stall conservative reporting.
  - [ ] Acceptance: see `spec.md` T6.
- [ ] **T5 — Early paint, Whisper, retry, and cache tests** (REQ-6, REQ-9, REQ-10)
  - [ ] Test early UI before heavy imports, real bytes/tqdm-only percentage, stale attempt rejection, indeterminate loading, and prewarm retry.
  - [ ] Test reinstall preserves `hf-cache` hashes unconditionally; no purge UI or command is added in this MVP.
  - [ ] Acceptance: see `spec.md` T5, T7, and T8.

## Phase 2 — GREEN implementation

- [ ] **T6 — Implement shared phase model and bilingual copy** (REQ-1, REQ-2, REQ-3)
  - [ ] Implement allowlisted `install_location.json.language` persistence and parity-tested ES/EN launcher vocabulary; do not add a seventh handoff field.
  - [ ] Keep launcher stdlib + tkinter and preserve the bounded visual barrier only.
  - [ ] Acceptance: see `spec.md` T1 and T10.
- [ ] **T7 — Implement atomic handoff producer/consumer** (REQ-4, REQ-5)
  - [ ] Snapshot phases 0–3 before `Popen`; app marks phase 4 after its own paint.
  - [ ] Validate exact schema, roots, attempt/version, stale state, and safe fallback without payload logging.
  - [ ] Acceptance: see `spec.md` T2 and T3.
- [ ] **T8 — Implement truthful launcher/app progress and early paint** (REQ-6, REQ-9, REQ-11, REQ-13)
  - [x] Remove fixed bootstrap/window percentages; retain determinate progress only for real byte evidence. (Validation: `validation.md`, latest focused run.)
  - [ ] Paint app before heavy imports; continue phases 5–7 locally.
  - [ ] Keep Option A rejected and Option B deferred.
  - [ ] Acceptance: see `spec.md` T4, T5, T7, and T9.
- [ ] **T9 — Implement VAD provisioning and safe retry** (REQ-7, REQ-8)
  - [ ] Add VAD `provision-*` state, non-blocking heartbeat with `finally` cleanup, localized errors, and phase retry.
  - [ ] Remove VAD insecure TLS fallback; preserve truthful TLS failures.
  - [ ] Report a live stalled producer; do not force-kill it or silently restart ASR. Use cooperative exit or an explicit full stop/restart when necessary.
  - [ ] For pre-ready replacement, stop/join and prove no remaining writes, then preserve the same queue objects and ASR process.
  - [ ] Acceptance: see `spec.md` T6.
- [ ] **T10 — Lock reinstall cache preservation** (REQ-10)
  - [ ] Preserve `hf-cache` unconditionally; do not add a purge UI or command in the MVP.
  - [ ] Verify before/after hashes and safe failure behavior.
  - [ ] Acceptance: see `spec.md` T8.

## Phase 3 — REFACTOR, docs, and compatibility

- [ ] **T11 — Resilience and idempotency refactor** (REQ-4–REQ-10)
  - [ ] Add/maintain `test_resilience_*.py` and `test_idempotent_*.py` tests for duplicate reads, retries, stale events, and safe recovery.
  - [ ] Preserve OpenCohost legacy state and OBS backlog/burst behavior.
  - [ ] Acceptance: all linked blocks plus E2E-5, E2E-6, E2E-10.
- [ ] **T12 — Update user-facing docs** (REQ-1, REQ-2, REQ-6–REQ-10)
  - [ ] Update `README.md`, `docs/GETTING_STARTED.md`, `HISTORIAL_CAMBIOS.md`, and `docs/PACKAGING_AND_UPDATES.md`.
  - [ ] Explain window-visible versus ready, honest indeterminate phases, VAD errors/retry, and cache-preserving reinstall.
  - [ ] Acceptance: docs trace to the requirements and do not claim M1–M8 completion.
- [ ] **T13 — Manual E2E evidence package** (REQ-1–REQ-13)
  - [ ] Publish steps and evidence slots for E2E-1…E2E-10.
  - [ ] Keep CUDA E2E-2 explicitly not run if capability is absent.
  - [ ] Acceptance: no row is marked complete without real evidence.

## Phase 4 — Verification and review

- [ ] **T14 — Focused automated validation**
  - [ ] Run focused tests, resilience/idempotency suites, and current-layout compileall.
  - [ ] Record historical first-use baseline (30 focused + 66 resilience/idempotency) as context only; do not claim rerun.
- [ ] **T15 — Four-role collective review**
  - [ ] Architecture/security/privacy review.
  - [ ] Performance/resilience review.
  - [ ] QA/product/manual-closure review.
  - [ ] Research/API/traceability review.
- [ ] **T16 — VM v2 rebuild and manual execution**
  - [ ] Rebuild VM v2; keep `build_artifacts/vm-test/` ignored/unversioned.
  - [ ] Execute E2E-1…E2E-10 and attach evidence.
  - [ ] Do not mark first-use M1–M8 complete without their own evidence.

## Acceptance template references

Each ticket above must satisfy a full acceptance block in `spec.md`:

| Ticket group | Spec blocks |
|---|---|
| T1, T6 | T1 and T10 |
| T2, T7 | T2 and T3 |
| T3, T8 | T4, T5, T7, and T9 |
| T4, T9 | T6 |
| T5, T10 | T5, T7, and T8 |
| T11–T16 | Closure gates and E2E matrix |

## Manual E2E checklist

- [ ] E2E-1 — Clean ES CPU install.
- [ ] E2E-2 — Clean EN CUDA install; record not-run reason if CUDA is unavailable.
- [ ] E2E-3 — Valid/malformed/stale handoff, early paint, window timeout, and process exit.
- [ ] E2E-4 — Slow/silent uv sync remains truthful and indeterminate.
- [ ] E2E-5 — VAD network/TLS/corrupt-cache errors, heartbeat, retry, and lifecycle safety.
- [ ] E2E-6 — Whisper real progress, retry, and stale-event rejection.
- [ ] E2E-7 — Warm, partial, and offline cache behavior.
- [ ] E2E-8 — Reinstall preserves hf-cache hashes unconditionally; no purge action exists in the MVP.
- [ ] E2E-9 — Post-download loading, prewarm switch, retry, and localization.
- [ ] E2E-10 — OBS no-burst, legacy compatibility, start/stop, and hot-swap regression.

## Delivery guardrails

- [ ] No implementation before spec/plan review confirms exact PO alignment.
- [ ] No edit to unrelated dirty files or protected skill/registry/config changes.
- [ ] No stage, commit, push, merge, or revert without explicit approval.
- [ ] No manual PASS inferred from automated tests or screenshots without provenance.
