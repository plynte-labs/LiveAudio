# Track firstuse-startup-progress_20260905 — Plan

> APROBADO por PO (7 decisiones, 2026-09-05, Engram #6439). IMPLEMENTADO en rama `feature/firstuse-startup-progress` (sin commit); REQ-6 decidido opción a heartbeat pre-import (PO 2026-09-05, Engram #6444).

## Phase 1 — Evidence re-verification (read-only)

Goal: confirm every Findings row still holds on the implementation base commit.

- [ ] Task: Re-read `liveaudio/service/supervisor.py:238-244` and `:248-283`; paste current verbatim mapping. (REQ-1, REQ-3)
  - [ ] Confirm collapse of active ASR states to `loading` still present.
  - [ ] Confirm absence of a startup/stall deadline.
- [ ] Task: Re-read `liveaudio/core/engine.py:339-369` tqdm parse (corrected location; consumer-only `liveaudio/app.py:1720-1728`) and the cache-exception label `engine.py:471-472`. (REQ-2, REQ-4, REQ-5)
  - [ ] Record any line drift in `spec.md` drift note.
- [ ] Task: Re-read `liveaudio/core/engine.py:434-438` TLS override vs `liveaudio/core/audio.py:157-179` save/restore pattern; document current scope. (REQ-7)
  - [ ] Record any line drift in `spec.md` drift note.
- [ ] Task: Conductor - User Manual Verification 'Evidence re-verification' (Protocol in workflow.md).

## Phase 2 — Approved implementation (per PO decisions)

Goal: implement the approved design (wire format + state machine + catalog + watchdog + TLS + prewarm copy).

- [ ] Task: Honest `asr_state` + legacy mirror in supervisor (`downloading/loading/transcribing/ready/stalled/failed`; mirror collapses to `loading/ready/failed`). (REQ-1, REQ-2)
  - [ ] Define wire format of progress events `{phase,percent,attempt,code}` with monotonicity rule (reset to 0 once per attempt, clamp 0–100).
- [ ] Task: Harden tqdm parse in `engine.py:339-369` (float percent, monotonic clamp, stale-progress signal, indeterminate fallback); GUI pill with `%`, no bar. (REQ-5)
- [ ] Task: Conservative supervisor watchdog (stall 120–180s → `stalled` + manual retry; slow progress → warning only; never kill with recent progress) + informative absolute deadline + retry-as-new-attempt. (REQ-3)
  - [ ] Leave REQ-6 open: heartbeat pre-import vs accepted-gap note — both options documented, no silent pick.
- [ ] Task: Provisioning error-code catalog with ES/EN copy keys (reserve `model-not-found` for real absence); cross-check string strategy with `i18n-completeness_20260905`. (REQ-4)
- [ ] Task: TLS scoping (copy `audio.py:157-179` save/restore or per-request context) + `provision-tls` code; no HTTP-stack migration. Prewarm=true kept + toggle copy ES/EN. (REQ-7, REQ-8)
- [ ] Task: Conductor - User Manual Verification 'Approved implementation' (Protocol in workflow.md).

## Phase 3 — Approved tests (no code yet beyond probes)

Goal: define how T1–T4 will be proven, stdlib-first.

- [ ] Task: Define stdlib-only AST-extracted probes for structured progress preservation (T1) and conservative deadline firing (T2). (REQ-1, REQ-2, REQ-3, REQ-5)
  - [ ] Specify Given/When/Then per ticket; no service/GPU/mic/network.
- [ ] Task: Define error-code matrix test (each failure class → exact code + copy key) for T3. (REQ-4)
- [ ] Task: Define TLS-scope + prewarm-copy review checklist for T4. (REQ-7, REQ-8)
- [ ] Task: Conductor - User Manual Verification 'Approved tests' (Protocol in workflow.md).

## Phase 4 — Docs impact

Goal: scope user-facing fallout of the approved states/codes.

- [ ] Task: List docs to update (`README.md`, `docs/GETTING_STARTED.md`, `HISTORIAL_CAMBIOS.md`) for new states/codes. (REQ-1, REQ-4, REQ-8)
  - [ ] Note changelog entry wording (bilingual).
- [ ] Task: Conductor - User Manual Verification 'Docs impact' (Protocol in workflow.md).
