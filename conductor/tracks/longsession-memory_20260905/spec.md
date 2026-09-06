# Track longsession-memory_20260905 — Specification

> Status: **exploratory proposal, PO approval pending — NOT approved, NOT implemented.**
> Source: read-only exploratory audit (Front 2). Every requirement below is PROPOSED.
> These are retained work/history paths, NOT proof of an allocator leak.

## Outcome

Long sessions (30min–2h+) degrade gracefully: no unbounded in-memory retention on the
stall/timeout paths, so a streamer never pays silent RAM growth for persistence
hiccups, repeated ASR timeouts, or enabled diagnostics.

## Quick path

1. Read the Findings table bottom-up (highest severity first).
2. Note the existing caps that MUST be cited as-is (no leak-free-total claim).
3. PO approves/rejects per-ticket Acceptance Criteria before any implementation track.

## Overview

Three unbounded-retention paths were verified with synthetic probes (no real audio or
transcripts, no files modified): `SessionWriter` retains pending records without bound
when persistence stalls — after `stop()` the writer stayed alive with 2000 pending
records in a 2s-stop synthetic probe (`liveaudio/core/engine.py:111-152`, stop at
`:147-149`). Each repeated ASR timeout leaves one live worker thread until the
underlying call completes — 5 false timeouts produced 5 live threads
(`_transcribe_with_timeout`, `liveaudio/core/engine.py`, `ASR_TRANSCRIBE_TIMEOUT_SEC`
at `:26`). `DiagnosticsStore` retains duration samples without bound when diagnostics
is enabled (50k samples ≈ 1.64MB, 100k ≈ 3.20MB traced; `liveaudio/core/diagnostics.py:74-101`,
`record_duration` at `:89-93`), but diagnostics is DISABLED by default — this path is
opt-in. Caps that already exist and must be cited as such: GUI logs/previews, IPC
queues, audio ring buffer (`RING_BUFFER_MAX_CHUNKS`, `audio.py:32,201`), WS replay,
and overlay pending queues are bounded. Do NOT assert total leak-free behavior.
Existing passing tests (17): `test_resilience_service_backend.py`,
`test_idempotent_service_backend.py`, `test_diagnostics.py`.

## Functional Requirements (PROPOSED — not approved)

- **REQ-1 (PROPOSED):** `SessionWriter`'s pending queue MUST be bounded (cap + defined
  overflow policy: drop-oldest with counter, or blocking-put with timeout — PO picks),
  and `stop()` MUST drain-or-drop deterministically within its join budget instead of
  leaving a live writer with thousands of pending records (`engine.py:111-152`).
- **REQ-2 (PROPOSED):** Repeated ASR timeouts MUST NOT accumulate one live worker
  thread per timeout; the timeout path MUST reuse/cancel/reap workers so N false
  timeouts leave no more than a bounded (ideally one) in-flight worker
  (`_transcribe_with_timeout`, `engine.py:26`ff).
- **REQ-3 (PROPOSED):** `DiagnosticsStore` duration/counter retention MUST be bounded
  when diagnostics is enabled (ring cap per metric key with drop counter), while the
  default-off posture (`diagnostics_enabled`, `diagnostics.py:28-31`) stays unchanged
  (`diagnostics.py:74-101`).
- **REQ-4 (PROPOSED):** Every new or changed bound MUST emit an observable signal
  (dropped-record counter / diagnostics counter / status event) so silent data loss
  is impossible — drops are counted and surfaced.
- **REQ-5 (PROPOSED):** The already-bounded paths (GUI logs/previews, IPC queues,
  audio ring, WS replay, overlay pending queues) MUST be documented as-is in code or
  docs comments; no behavior change, citation only.

## Non-Functional Requirements

- Bounds must be constants with safe defaults + config validation (follow existing
  `_normalize_config` clamp pattern); no unbounded default anywhere on these paths.
- Long-session validation follows the Resilience Log Analysis Protocol: PO provides
  real session logs; agents analyze failure points, memory growth, queue pressure.
- No raw audio/transcripts in probes, tests, logs, or Engram — sanitized summaries only.
- `python -m compileall` + resilience/idempotency suites stay green.

## Acceptance Criteria

### T1 — Bound SessionWriter pending retention (REQ-1, REQ-4)

- Given: persistence stalled (disk slow/full) mid-session with records still arriving
- When: the pending backlog hits the new cap, then `stop()` is called
- Then: memory stays under the documented bound; `stop()` returns within its budget; every dropped record increments a visible counter
- Error Path: stall persisting past the budget surfaces a `session-persist-stalled` warning with the drop count, never a silent hang or silent loss
- UI State: user sees a persistence-warning pill with count (localized), not a frozen save indicator
- OBS Behavior: live subtitles unaffected; only disk artifacts are at risk, and the UI says so

### T2 — No per-timeout worker accumulation (REQ-2, REQ-4)

- Given: ASR backend slower than `ASR_TRANSCRIBE_TIMEOUT_SEC` for 5 consecutive calls that later complete
- When: each call times out from the caller's perspective
- Then: at most a bounded number of in-flight workers exist (target: 1 reused/reaped); thread count returns to baseline after completion
- Error Path: underlying call raising (not just slow) still reaps the worker and surfaces the original error, never a leaked thread plus a timeout
- UI State: timeout warning shows retry/backoff state, not a freeze
- OBS Behavior: no duplicate subtitle emission when the slow call finally completes after its timeout

### T3 — Bound DiagnosticsStore retention, keep default-off (REQ-3, REQ-4)

- Given: diagnostics enabled (`minimal` or `deep`) over a long session
- When: duration samples accumulate past the per-key ring cap
- Then: oldest samples roll off, a rollover counter records the loss, snapshot shape is unchanged, and default-off behavior is untouched
- Error Path: cap exhaustion never raises, never blocks the recording call path
- UI State: diagnostics export notes the rollover count where visible
- OBS Behavior: unchanged (diagnostics path is out-of-band)

### T4 — Cite existing caps; regression probes (REQ-5)

- Given: the already-bounded paths (GUI logs/previews, IPC queues, audio ring, WS replay, overlay pending queues)
- When: reviewed under this track
- Then: each bound is cited with file:line in a code comment or docs section; new stdlib-first probes lock the three fixed paths (writer cap, worker reuse, diagnostics ring)
- Error Path: any probe failing on current code is filed as a blocking defect against T1–T3, not silently skipped
- UI State: unchanged
- OBS Behavior: unchanged

## Out of Scope (explicitly NOT proven by the audit — do not claim)

- Any allocator-level leak (nothing here proves or disproves one; these are retained work/history paths).
- Total "leak-free" certification of the app.
- Changing the diagnostics default-off posture.
- Real-audio soak tests in this track (those belong to a PO-log-driven resilience pass).
- Disk-full recovery UX beyond the stall warning.

## Findings

| File:line | Evidence | Severity | Blocking? |
|---|---|---|---|
| `liveaudio/core/engine.py:111-152` (stop at `:147-149`) | `SessionWriter`: unbounded `queue.Queue()`; `stop()` puts a sentinel + `join(timeout=2.0)` — synthetic probe left the writer alive with 2000 pending records after a 2s stop. Verified verbatim on `develop @ e004ae3` (`stop` at `:147-149`). | High | Yes (for T1) |
| `liveaudio/core/engine.py:26` + `_transcribe_with_timeout` | One live worker per repeated ASR timeout until the underlying call completes (5 false timeouts → 5 live threads). Timeout constant `ASR_TRANSCRIBE_TIMEOUT_SEC = 15.0` at `:26` verified verbatim. | High | Yes (for T2) |
| `liveaudio/core/diagnostics.py:74-101` (`record_duration :89-93`, gate `:28-31`) | `durations` dict-of-lists appends without bound when enabled (50k→~1.64MB, 100k→~3.20MB traced); `diagnostics_enabled` defaults to False (opt-in). Verified verbatim. | Medium | No (opt-in; T3 covers) |
| Existing caps (cite as-is) | GUI logs/previews, IPC queues, audio ring (`RING_BUFFER_MAX_CHUNKS`, `audio.py:32,201`), WS replay, overlay pending queues are bounded. No change; citation only. | Info | No (T4 covers) |
| Passing suites (17 tests) | `tests/test_resilience_service_backend.py`, `tests/test_idempotent_service_backend.py`, `tests/test_diagnostics.py` passed in audit (filenames verified present on disk). | Info | No |

> Drift note: `engine.py:111-152`, `:26`, `diagnostics.py:74-101`, `audio.py:32,201`
> verified against the current checkout during track writing. `_transcribe_with_timeout`
> exact line span and the `utils/config.py` defaults / `network.py` / overlay HTML queue
> lines are carried from the verified audit context — re-confirm before editing.

## Traceability

- Engram observation **#6430** + session **#6436** (Front 2: long-session memory audit).
- Workflow: Resilience Log Analysis Protocol (`conductor/workflow.md`) governs any
  follow-up soak validation with PO-provided real session logs.
