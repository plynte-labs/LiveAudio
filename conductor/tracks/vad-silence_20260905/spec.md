# Track vad-silence_20260905 — Specification

> Status: **exploratory proposal, PO approval pending — NOT approved, NOT implemented.**
> Source: read-only exploratory audit (Front 4). Every requirement below is PROPOSED.
> This track MUST NOT contradict `conductor/tracks/vad-onset-grace_20260619/spec.md`,
> which explicitly excludes hysteresis/adaptive gates from its scope.

## Outcome

The VAD/silence path is honest about what it does: synthetic-zero segmentation is
documented as segmentation (not real-silence classification), VAD thread death is
surfaced and recoverable, insecure acquisition is pinned down to a policy decision,
and the noisiest tests assert production behavior instead of invented math.

## Quick path

1. Read the Findings table — segmentation-vs-classification first (it constrains every REQ).
2. Check the hard boundary with `vad-onset-grace_20260619` before scoping any gate work.
3. PO approves/rejects per-ticket Acceptance Criteria before any implementation track.

## Overview

A synthetic probe of the real `vad_worker` (AST-extracted, mocked model) proved
SEGMENTATION, not real-silence classification: all-silence enqueues nothing; exact
threshold equality (0.5) enqueues nothing (strict `>` verified at
`liveaudio/core/audio.py:300`); one mocked 0.51 frame after 7 silent frames plus 26
trailing silent frames enqueues 1.088s of synthetic zeros. This says nothing about
whether Silero misclassifies real silence, nor does it reproduce Whisper
hallucinations — and there is no minimum voiced-duration/density gate. ASR disables
the secondary `vad_filter` but keeps `no_speech_prob > 0.6` + blacklist. VAD inference
exceptions escape the daemon thread; the monitor logs a dead thread without
recovering it; reconnect clears only the ring, retaining utterance + recurrent state.
Acquisition is unpinned and insecure (`torch.hub.load` of remote `hubconf`,
`urllib urlopen` region cited by audit) while audio acquisition disables TLS
verification (`audio.py:150-193`, unverified-context block at `:157-179` verified
verbatim); ASR additionally mutates the global SSL context. This is a CONFIRMED
insecure-acquisition policy, NOT evidence of compromise, and NOT proof every Hugging
Face transport inherits it. Installed: faster-whisper 1.2.1, torch 2.6.0+cpu
(`uv.lock`, `pyproject.toml`). Several noise/threshold tests duplicate logic or
assert invented energy math rather than production rejection
(`tests/test_audio.py:219-239`, `tests/test_noise_detection.py:98-135`). Hard
boundary: the prior `vad-onset-grace` track EXCLUDES hysteresis/adaptive gates —
this track may only propose what does not contradict that contract.

## Functional Requirements (PROPOSED — not approved)

- **REQ-1 (PROPOSED):** The segmentation behavior proven by the probe (silence→nothing,
  exact-threshold→nothing, single-hot-frame→synthetic-zero enqueue) MUST be locked by
  a regression test that asserts EXACTLY that and labels itself segmentation-only —
  never framed as real-silence classification (`audio.py:239-346`, threshold `:300`).
- **REQ-2 (PROPOSED):** Any proposed minimum-voiced-duration/density gate MUST first
  pass a contract check against `vad-onset-grace_20260619/spec.md` (hysteresis/adaptive
  gates excluded there); if excluded, the gate ships as a documented accepted-gap OR a
  contract amendment proposal — never a silent contradiction.
- **REQ-3 (PROPOSED):** VAD inference exceptions MUST NOT escape the daemon thread
  silently: catch, count, surface a `vad-worker-fault` status, and specify
  recover-vs-abort semantics; the dead-thread monitor log MUST become a recovery or an
  explicit abort decision (`audio.py:400-436`, `engine.py:247,434-438,566-570`).
- **REQ-4 (PROPOSED):** Reconnect MUST define retained-vs-cleared state explicitly
  (ring vs utterance vs recurrent VAD state); retaining utterance + recurrent state
  while clearing only the ring MUST be either justified in-spec or changed.
- **REQ-5 (PROPOSED):** Acquisition MUST be a PO-signed policy: pinned source
  (commit/tag or vendored `hubconf`) + TLS verification restored or narrowly scoped
  with an expiry/review date; the global SSL-context mutation MUST be scoped or
  removed. No claim about compromise; no claim about HF transport inheritance
  (`audio.py:150-193`, `uv.lock`, `pyproject.toml`).
- **REQ-6 (PROPOSED):** The duplicated/invented-math tests (`test_audio.py:219-239`,
  `test_noise_detection.py:98-135`) MUST be routed to assert production rejection
  (strict `>`, real `vad_worker` boundary) via a shared helper — matching the
  deferred collateral note already on `tracks.md` — or be deleted with PO sign-off.

## Non-Functional Requirements

- Probe/test pattern stays synthetic + stdlib-first (mocked model frames); no mic, no GPU, no network.
- Any new gate must prove no onset-clipping regression against the onset-grace contract (soft onsets still recovered via `vad_speech_pad_ms`).
- Dependency pins (`uv.lock`/`pyproject.toml`) change only with a recorded reason.
- Privacy: sanitized technical summaries only.

## Acceptance Criteria

### T1 — Lock segmentation behavior as segmentation-only (REQ-1)

- Given: the AST-extracted `vad_worker` with a mocked model probability source
- When: fed all-silence / exact-0.5 / single-0.51-then-silence frame sequences
- Then: nothing / nothing / one ≈1.088s synthetic-zero phrase enqueued; the test name and docstring say "segmentation, not classification"
- Error Path: any deviation fails the test with the frame sequence in the message, never a silent pass
- UI State: unchanged (no user-visible change; test-only ticket)
- OBS Behavior: unchanged

### T2 — Gate proposal with contract check (REQ-2)

- Given: the `vad-onset-grace` exclusion of hysteresis/adaptive gates
- When: a minimum-voiced-duration/density gate is proposed
- Then: EITHER the gate is shown contract-compatible with written reasoning, OR it ships as a contract-amendment proposal / accepted-gap note — PO picks one of the three, in writing
- Error Path: a gate that contradicts the contract and ships silently is a blocking defect
- UI State: unchanged unless the PO-approved gate adds a tunable (then it follows the existing slider pattern)
- OBS Behavior: no new clipping of soft onsets; onset-grace behavior preserved

### T3 — VAD thread fault surfacing + reconnect state contract (REQ-3, REQ-4)

- Given: a VAD inference exception mid-session, and separately a reconnect event
- When: the exception escapes / the reconnect fires
- Then: a `vad-worker-fault` status with counter is emitted and recovery-or-abort runs per spec; reconnect follows the retained-vs-cleared table (ring cleared; utterance + recurrent state per PO decision)
- Error Path: a second consecutive fault escalates (backoff/abort), never an infinite silent respawn loop
- UI State: user sees a VAD-fault warning pill (localized), not a frozen "listening" indicator
- OBS Behavior: no synthetic-zero phrase burst on recovery; backlog policy governs catch-up

### T4 — Acquisition policy + test honesty (REQ-5, REQ-6)

- Given: current unpinned hub acquisition + disabled TLS verification + global SSL mutation
- When: reviewed under this track
- Then: source is pinned (or vendored) AND TLS verification is restored or narrowly scoped with a review date; the duplicated/invented-math tests assert the production strict-`>` boundary via a shared helper or are PO-deleted
- Error Path: acquisition failure surfaces a distinct acquisition/TLS code, never a generic VAD error
- UI State: first-run acquisition shows honest progress/failure copy (keyed strings, cross-ref `i18n-completeness_20260905`)
- OBS Behavior: unchanged

## Out of Scope (explicitly NOT proven by the audit — do not claim)

- That Silero misclassifies real silence (unproven — the probe used synthetic zeros + mocked probabilities).
- Reproducing or fixing Whisper hallucinations (not reproduced here).
- Hysteresis/adaptive gates beyond the onset-grace contract (excluded unless the contract is amended).
- That any transport compromise occurred (no evidence of compromise).
- That all Hugging Face traffic inherits the unverified context (unproven — cross-ref only).
- Energy-math redesign of VAD scoring.

## Findings

| File:line | Evidence | Severity | Blocking? |
|---|---|---|---|
| `liveaudio/core/audio.py:239-346` (threshold `:300`, guard `:296-298`) | Real `vad_worker`, AST-extracted with mocked model: all-silence→nothing; exact 0.5→nothing (strict `>` at `:300` verified verbatim); one 0.51 frame + trailing silence→1.088s synthetic zeros. Segmentation proven; classification NOT proven. | High (as constraint) | Yes (for T1/T2) |
| No minimum voiced gate | No minimum voiced-duration/density gate exists on the probed path. Auditor-verified observation. | Medium | No (T2 covers) |
| `liveaudio/core/audio.py:400-436`; `liveaudio/core/engine.py:247,434-438,566-570` | VAD inference exceptions escape the daemon thread; monitor logs dead thread without recovery; reconnect clears ring only, retains utterance + recurrent state. Auditor-verified; re-verify exact lines before implementation. | Medium | No (T3 covers) |
| `liveaudio/core/audio.py:150-193` (unverified block `:157-179` verified verbatim) | `ssl._create_default_https_context = ssl._create_unverified_context` around acquisition; unpinned `torch.hub.load` of remote `hubconf`. Confirmed insecure-acquisition policy. | High | Yes (for T4) |
| Global SSL mutation (ASR) | ASR additionally changes the global SSL context; cross-ref `firstuse-startup-progress_20260905` REQ-7. Auditor-verified observation. | Medium | No (T4 covers) |
| `tests/test_audio.py:219-239`; `tests/test_noise_detection.py:98-135` | Tests duplicate logic or assert invented energy math, not production rejection. Consistent with the deferred collateral note on `tracks.md`. | Low | No (T4 covers) |
| `uv.lock`, `pyproject.toml` | Installed: faster-whisper 1.2.1, torch 2.6.0+cpu. Auditor-verified. | Info | No |
| `conductor/tracks/vad-onset-grace_20260619/spec.md` | Prior contract EXCLUDES hysteresis/adaptive gates — hard boundary for REQ-2/T2. | High (as constraint) | Yes (for T2) |

> Drift note: `audio.py:239` (worker), `:300` (strict `>`), `:157-179` (unverified
> block), `:201` (ring `maxlen`) verified byte-for-byte on `develop @ e004ae3` during
> track writing. All other `audio.py` / `engine.py` / `tests/` line numbers are carried
> from the verified audit context — re-confirm before editing and record drift here.

## Traceability

- Engram observations **#6429** + **#6434** + session **#6435** (Front 4: VAD/silence audit).
- Contract to preserve: `conductor/tracks/vad-onset-grace_20260619/spec.md`.
- Cross-refs: `firstuse-startup-progress_20260905` (TLS scoping REQ-7),
  `i18n-completeness_20260905` (keyed acquisition progress/failure copy).
