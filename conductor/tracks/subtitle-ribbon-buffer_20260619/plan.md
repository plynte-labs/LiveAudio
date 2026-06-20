# Implementation Plan: Vertical Ribbon Subtitle Buffer for OBS Overlay

## 1. Design Decisions

### 1.1 Mode is a CLIENT rendering branch, not a new transport
The subtitle DATA path is unchanged. The engine keeps broadcasting the same per-utterance payload (`liveaudio/core/engine.py:576-587`), and `_broadcast_msg` (`liveaudio/core/network.py:108-113`) forwards any dict unchanged. "Ribbon vs single vs adaptive" is purely HOW the client renders an arriving line. This keeps the diff small, keeps single mode byte-for-byte unchanged, and (with OD-5 deferred) means NO engine change at all. REJECTED alternative: a dedicated `{type:'mode'}` WebSocket message mirroring the theme path — rejected because the theme path is already DEAD on the Python side (`validate_theme_tokens()` at `engine.py:91-108` is never called; no Python code sends a `{type:'theme'}` message), so reusing it would require building NEW plumbing for marginal benefit.

### 1.2 Per-line timers replace module-level singletons (ribbon branch only)
Today lifetime is governed by two module-level singletons, `let hideTimeout` / `let cleanupTimeout` (`subtitulos_obs.html:263-264`), and every `showSubtitle()` call clears them at the top (`:416-417`). That is INTENTIONALLY single-line: a new line cancels the old line's expiry. For the ribbon branch, each box must carry its OWN timers so appending line B does not cancel line A. DECISION: store the timer ids ON the box element itself (`sub._hideTimer`, `sub._cleanupTimer`), and only clear those on that box's own lifecycle. Single mode keeps the existing singleton path untouched (guarded by the mode branch). Full lifecycle/transition rules in §1.12. REJECTED alternative: a global ordered list of timers — more bookkeeping, easier to leak, harder to reason about per-line eviction.

### 1.3 Bypass `processQueue` serialization in the ribbon branch; keep it in single state
`processQueue` (`subtitulos_obs.html:338-369`) is a strict serialization gate: it shows one line, sets `isShowing = true` (`:344`), and only advances on `onSubtitleComplete()` (`:372-375`) after the previous line fully cleans up. That is the OPPOSITE of a ribbon (which wants concurrent visible lines). DECISION: in the ribbon branch, `enqueueSubtitle` (`:325-336`) dequeues and renders WITHOUT waiting on `isShowing`, applying only the `DEBOUNCE_MS` pacing (OD-3) and the visible-line cap (REQ-3). In single state, the existing `isShowing`/`processQueue` flow is preserved exactly. The mode branch lives at the enqueue/process boundary, NOT deep inside `showSubtitle`.

### 1.4 Queue becomes the stack source in the ribbon branch (REQ-7)
In single mode a full queue discards the oldest pending item via `pendingQueue.shift()` (`:327-328`). In the ribbon branch the pending items SHOULD become visible lines (up to the cap) rather than be silently dropped at depth 5. DECISION: the ribbon branch does not rely on the discard-oldest pending buffer for normal flow; it renders incoming lines directly (paced), and the VISIBLE cap (`subtitle_ribbon_max_lines`) — not `MAX_PENDING_QUEUE` — is the user-facing limit. `MAX_PENDING_QUEUE` (`:314`) stays as a transport safety valve only (OD-4 RESOLVED: decouple the two).

### 1.5 URL param read at decision time, not stored-and-forgotten
`?style=` is parsed into `currentStyle` (`subtitulos_obs.html:272`) but NEVER read again — it is a dead variable; real style comes from `data.style` in the payload (`:396`). DECISION: parse `?mode=`/`?lines=` once at load into module-level state AND actually branch on that state inside `enqueueSubtitle`/`processQueue`/`showSubtitle`. A static test (AC-7) asserts the parsed mode is referenced in the render path so this trap cannot silently reappear.

### 1.6 Stacking direction = newest-on-top via `column-reverse` (OD-1 RESOLVED)
LOCKED newest-top/oldest-bottom, which contradicts the bottom-anchored layout (`:25-36`). DECISION: apply `flex-direction: column-reverse` to the ribbon container so `appendChild` (cheapest insertion) yields newest-on-TOP while the block stays anchored above the existing 50px bottom margin (`:42`). This keeps insertion code identical to today (still `appendChild`) and isolates the direction choice to one CSS rule, making a later flip to newest-bottom a one-line change with zero JS impact.

### 1.7 Config-first validation, client clamp as defense (REQ-4, NFR-5)
Authoritative validation for `subtitle_display_mode` and `subtitle_ribbon_max_lines` lives in `config.py` `_normalize_config`, following the exact patterns already there: a `VALID_` set check for the mode that falls back to `DEFAULT_CONFIG["subtitle_display_mode"]` (like `subtitle_backlog_policy` at `:191-193`), and an int clamp for the line count via `_clamp_number(..., cast=int)` (`:91-100`, mirroring `cpu_threads` at `:136`). Because the mode fallback reads `DEFAULT_CONFIG[...]`, the default is the single source of truth — the locked `adaptive` default flows through automatically. The client `?lines=` clamp to `1..8` is a secondary guard for URL-driven sources that never touch config.

### 1.8 Server payload hint DEFERRED (OD-5 RESOLVED)
With `adaptive` the LOCKED default, the config default + URL `?mode=` cover every shipping case, so the `display_mode` payload hint adds no value and would force `tests/test_engine.py` payload-shape churn. DECISION: defer. NO `engine.py` change in this track. A future GUI-toggle track can add `"display_mode": shared_config.get("subtitle_display_mode", "adaptive")` at the payload site (`engine.py:576-587`, alongside the `style` read at `:572-574`), importing `VALID_SUBTITLE_DISPLAY_MODES` from `config.py` (NFR-6). URL precedence would apply then.

### 1.9 Adaptive engagement keys off signals that already exist end-to-end (REQ-10)
Adaptive is NOT a third rendering engine — it is single state that PROMOTES itself to the ribbon render branch (1.3/1.4) only while subtitles are accumulating, then DEMOTES back. DECISION: the primary engagement signal is the existing pending-queue depth (`pendingQueue.length`, `subtitulos_obs.html:320`, `:325-369`) — more than one item waiting while one is visible means ASR is outrunning the one-at-a-time pace, so promote to stacked. The secondary signal is the per-utterance payload `is_replay` / `catchup_interval_sec` / `total_delay` (`liveaudio/core/engine.py:583,585-586`) which the overlay currently IGNORES; while `is_replay` is true the overlay stays stacked so a catch-up burst fills the ribbon instead of flashing past. Demotion happens when the queue drains (≤ 1 pending) and only one live line remains. This reuses signals already present end-to-end, so adaptive adds NO new transport and NO new server state. REJECTED alternative: a fixed JS timer/rate sampler — redundant with the queue depth the client already tracks. Full state machine + threshold in §1.10–1.11 (OD-6 RESOLVED).

### 1.10 Adaptive engagement/disengagement STATE MACHINE (REQ-10)

Adaptive is a TWO-STATE finite machine layered over the existing enqueue/process path. It does NOT replace `single` or `ribbon` — those modes hard-pin one state. The machine only runs when the resolved mode is `adaptive`.

**States:**
- **`SINGLE`** — the legacy serialized path runs verbatim: `enqueueSubtitle`→`processQueue` (`:325-336`, `:338-369`) gated by `isShowing` (`:322`), `showSubtitle` clears the shared `hideTimeout`/`cleanupTimeout` (`:416-417`) and does removeChild-before-append (`:420-424`). At most one live `.sub-box`. This is the steady-state for normal one-at-a-time speech.
- **`RIBBON`** — the ribbon branch runs: no `isShowing` gate, no removeChild-before-append, per-line `_hideTimer`/`_cleanupTimer` (§1.2), visible cap eviction (REQ-3), `column-reverse` container (§1.6). Multiple concurrent `.sub-box` up to `subtitle_ribbon_max_lines`.

A single module-level variable holds the current state, e.g. `let adaptiveState = 'SINGLE'` (only meaningful when `resolvedMode === 'adaptive'`). The container's ribbon-active CSS modifier class is added on entering RIBBON and removed on entering SINGLE.

**Where the branch lives (CRITICAL — at the enqueue/process boundary, not inside `showSubtitle`):**
The mode/state decision is evaluated ONCE per arriving line, at the TOP of `enqueueSubtitle` (`:325`), BEFORE the item is pushed and BEFORE `processQueue` runs. `showSubtitle` receives only a small flag (e.g. `renderStacked`) telling it whether to take the per-line-timer/append path or the shared-timer/replace path. This keeps `showSubtitle`'s style-rendering body (`:426-505`, the karaoke/rgb/typewriter/default span logic) UNTOUCHED and shared across states.

**Transition table (evaluated in `enqueueSubtitle`, after the new item is conceptually counted):**

| Current | Condition | Next | Action |
|---|---|---|---|
| SINGLE | `mode==='adaptive'` AND (`pendingQueue.length > 1` OR incoming payload `is_replay===true`) | RIBBON | add ribbon-active class; the currently-visible single box (if any) is ADOPTED into the ribbon (see §1.12); render incoming line via ribbon branch |
| SINGLE | otherwise | SINGLE | legacy serialized path |
| RIBBON | new line arrives | RIBBON | render via ribbon branch (append + per-line timer + cap eviction) |
| RIBBON | settle check fires AND `pendingQueue.length <= 1` AND live `.sub-box` count ≤ 1 AND not currently in `is_replay` | SINGLE | remove ribbon-active class; the one surviving box (if any) is RE-ADOPTED into the single path (see §1.12) |

`single` mode = permanently SINGLE (the machine is bypassed). `ribbon` mode = permanently RIBBON.

**Signal sources, all already present:**
- `pendingQueue.length` (`:320`) — primary client signal, read synchronously in `enqueueSubtitle`.
- payload `is_replay` (`engine.py:585`) — secondary signal, available on `data.is_replay` in the `onmessage` handler (`:386-401`); thread it into `enqueueSubtitle(text, style, isReplay)`.
- payload `catchup_interval_sec` (`engine.py:586`) — informational; not needed for the binary promote/demote but available if pacing tuning is later wanted.

### 1.11 Hysteresis / anti-flap (REQ-10)

The danger is oscillating SINGLE⇄RIBBON when the arrival rate sits right at the one-at-a-time pace. Three mechanisms make the machine flicker-free:

1. **Asymmetric (edge-triggered) thresholds.** Promote the instant `pendingQueue.length > 1` (or `is_replay`). Demote ONLY when the queue is FULLY drained (`<= 1` pending) AND only one live box remains. The promote condition (`>1` waiting) and the demote condition (`<=1` waiting + 1 live) cannot both be momentarily true, so a single arriving line cannot bounce the state.
2. **Settle window on demotion only.** Demotion is not evaluated synchronously on every line; it is checked after a short settle delay reusing the existing `DEBOUNCE_MS = 80` (`:315`) — schedule a one-shot `setTimeout` when the drain condition first looks satisfied, and only commit the demotion if the condition STILL holds when it fires (a new line arriving in that window cancels the pending demotion and keeps RIBBON). Promotion has NO delay (instant, so bursts never flash a single line first).
3. **`is_replay` latch.** While `is_replay` keeps arriving the machine STAYS in RIBBON regardless of momentary queue drain, because a catch-up burst can have brief gaps between replayed lines. Demotion requires `is_replay` to have stopped (the most recent payload had `is_replay===false`).

**Flicker-free for surviving lines:** the transition NEVER re-renders or re-times existing boxes (§1.12). Toggling the container's `flex-direction` between `column` (implicit, single) and `column-reverse` (ribbon) with a SINGLE live box does not move it visually because a one-element flex column and column-reverse render identically — so the demotion of the last line is visually a no-op.

REJECTED alternative: a hysteresis band on queue length (promote at >2, demote at <1). Rejected — the edge-trigger + settle-window already prevents flap with simpler, more legible conditions and no magic second threshold to tune.

### 1.12 Per-line timer lifecycle ACROSS transitions (REQ-2)

Today there is ONE module-level `hideTimeout`/`cleanupTimeout` pair (`:263-264`), cleared at the top of every `showSubtitle` (`:416-417`). The ribbon branch needs per-line ownership; the hard part is what happens to IN-FLIGHT boxes when the state flips.

**Per-line ownership (RIBBON state).** When `showSubtitle` renders a box in the ribbon branch, it stores the timer ids on the node: `sub._hideTimer = setTimeout(...)` (mirrors `:510`) and inside it `sub._cleanupTimer = setTimeout(...)` (mirrors `:539-546`). The cleanup callback does the per-line GC: `if (sub.parentNode) sub.parentNode.removeChild(sub)` then `sub._hideTimer = sub._cleanupTimer = null`. It MUST NOT call `onSubtitleComplete()` in the ribbon branch (that would re-pump the serialized queue / toggle `isShowing`); the ribbon branch is not gated by `isShowing`.

**Single→Ribbon (promotion) — ADOPT the live box.** At promotion there may be exactly one live box created by the SINGLE path, whose expiry currently rides the SHARED `hideTimeout`/`cleanupTimeout`. To avoid orphaning it (and to avoid the next `showSubtitle` clearing those shared timers and silently killing its expiry), promotion RE-HOMES that box: cancel the shared `hideTimeout`/`cleanupTimeout` (`clearTimeout`), then re-arm equivalent per-line `sub._hideTimer`/`sub._cleanupTimer` on the existing node for its REMAINING lifetime. Simplest correct approach: store each box's append timestamp (`sub._bornAt = Date.now()`) when created in EITHER path, and on adoption arm `sub._hideTimer` for `max(0, 5000 - (Date.now() - sub._bornAt))`. The box is now a normal ribbon citizen; no double-removal because the shared timers were cleared before re-arming.

**Ribbon→Single (demotion).** Demotion only fires when ≤1 live box remains. That surviving box already owns per-line timers — those are LEFT RUNNING (do not touch them). New lines after demotion take the single path and clear the SHARED timers (`:416-417`), which are unrelated to the survivor's per-line timers, so the survivor expires correctly on its own schedule and the next single-path line removes it via the normal removeChild-before-append (`:420-424`) only if it is still present. Edge case: if a single-path line arrives while the survivor's per-line `_hideTimer` is still pending, the removeChild-before-append path removes the survivor node; its per-line timers must be cleared at that point to avoid a stale callback touching a detached node. DECISION: the single-path removeChild block (`:420-424`) is extended to also clear any `_hideTimer`/`_cleanupTimer` on the box it removes (harmless no-op for genuine single-mode boxes that never set them). This one guard makes single-path removal safe regardless of which path created the box.

**Cap eviction (RIBBON state, REQ-3).** When appending would exceed `subtitle_ribbon_max_lines`, the OLDEST box is force-evicted: clear its `_hideTimer` AND `_cleanupTimer` FIRST, then `removeChild`, then null the ids. Clearing before removal is what prevents the orphaned-timer leak (R-2). Insertion is `appendChild`, so DOM child order is `[oldest … newest]` and the OLDEST box is `container.firstElementChild`. `flex-direction: column-reverse` only flips the VISUAL order, NOT the DOM child indices — so eviction MUST target `container.firstElementChild`, not `lastElementChild` (targeting the last child would evict the NEWEST box). Track insertion order in a small array if an even more unambiguous source is wanted.

**GC discipline preserved per line.** Every removal path (per-line expiry, cap eviction, single-path replace, demotion edge case) does `removeChild` + nullify the timer ids, mirroring the predecessor `subtitle-style-system_20260509` memory-leak fix and the current single-path discipline (`:420-424`, `:539-546`). NFR-1 / AC-13 pin return-to-0 nodes and no live timers after a burst.

## 2. Implementation Steps

> NOTE: These are the concrete file touchpoints. Source edits happen in the Implementation phase, NOT in this design. All blocking Open Decisions are now RESOLVED (OD-1 newest-top, OD-5 defer engine, OD-6 default=adaptive + threshold). OD-2/OD-3/OD-4 ship the recommended values and remain easy post-hoc tweaks. With OD-5 deferred, this track touches only TWO files: `config.py` and `subtitulos_obs.html` (plus tests).

### Step 1 — Config keys + validation (Python)
- File: `liveaudio/utils/config.py`
  - Add `VALID_SUBTITLE_DISPLAY_MODES = {"single", "ribbon", "adaptive"}` near `VALID_SUBTITLE_STYLES`/`VALID_BACKLOG_POLICIES` (`:55-56`).
  - Add `"subtitle_display_mode": "adaptive"` (LOCKED default) and `"subtitle_ribbon_max_lines": 3` to `DEFAULT_CONFIG` (`:59-88`).
  - In `_normalize_config` (`:103+`): validate `subtitle_display_mode` against the set, falling back to `DEFAULT_CONFIG["subtitle_display_mode"]` (mirror `subtitle_backlog_policy` at `:191-193` — do NOT hard-code `"single"`); coerce `subtitle_ribbon_max_lines` via `_clamp_number(..., 1, 8, int)` (`:91-100`, mirror `cpu_threads` at `:136`).

### Step 2 — Engine payload hint: DEFERRED (OD-5 RESOLVED)
- NO `engine.py` change in this track. The `display_mode` payload hint is deferred (spec §5, decision §1.8). `liveaudio/core/engine.py` and `tests/test_engine.py` are NOT touched. This is an intentional scope reduction enabled by the adaptive-default decision.

### Step 3 — Overlay: parse `?mode=` / `?lines=` at load (client JS)
- File: `liveaudio/assets/subtitulos_obs.html`
  - In the URL-param block (`:269-272`), parse `mode` and `lines` via the existing `urlParams` instance (`:270`). Validate `mode` against `{single, ribbon, adaptive}`, falling back to `'adaptive'` (the locked default — NOT `single`). Store in module-level state that is ACTUALLY read later (avoid the `currentStyle` dead-variable trap at `:272`). Clamp `lines` to `1..8`; fall back to default `3`.
  - No `onmessage` change for mode (OD-5 deferred); the handler (`:386-401`) only gains threading `data.is_replay` into the enqueue call for adaptive (Step 4).

### Step 4 — Overlay: mode-aware enqueue / state machine / render branch (client JS)
- File: `liveaudio/assets/subtitulos_obs.html`
  - `enqueueSubtitle`/`processQueue` (`:325-369`): add the thin mode/state branch at the TOP of `enqueueSubtitle` (§1.10). `single` → existing serialized path untouched. `ribbon` → render incoming lines directly (paced by `DEBOUNCE_MS`), bypassing the `isShowing` gate. `adaptive` → run the SINGLE/RIBBON state machine (§1.10) keyed off `pendingQueue.length > 1` OR `data.is_replay`, with the demotion settle window (§1.11). Thread `is_replay` from `onmessage` (`:386-401`) into `enqueueSubtitle(text, style, isReplay)`.
  - `showSubtitle` (`:409-549`): take a `renderStacked` flag. When stacked, SKIP the shared `clearTimeout` (`:416-417`) and the `removeChild(existing)` block (`:420-424`); append the new box; set per-line `sub._hideTimer`/`sub._cleanupTimer` and `sub._bornAt`; enforce the visible cap (evict oldest, clearing its timers first, REQ-3). When NOT stacked, the body is byte-for-byte today's path. Extend the single-path removeChild block (`:420-424`) to also clear any `_hideTimer`/`_cleanupTimer` on the removed node (§1.12 demotion edge case — harmless for genuine single boxes).
  - Per-line cleanup callback (stacked) does `removeChild` + null the per-line ids and MUST NOT call `onSubtitleComplete()` (`:372-375`) (that re-pumps the serialized queue). The single path keeps calling `onSubtitleComplete()` exactly as today.
  - Adaptive adoption/re-home of the live box on promotion and the survivor on demotion per §1.12 (`_bornAt`-based remaining-lifetime re-arm).

### Step 5 — Overlay: stacking-direction CSS (client CSS, OD-1 RESOLVED newest-top)
- File: `liveaudio/assets/subtitulos_obs.html`
  - Add a ribbon-active container modifier class carrying `flex-direction: column-reverse` (newest-on-top) plus a per-line vertical `gap`/margin, keeping the existing 90% width / 50px bottom margin (`:38-43`). Do NOT touch the per-box `.sub-box` entry/exit transitions (`:46-63`, Track-C-owned, NFR-2 / shared-issue guard). Ensure all 7 `.style-*` presets still render as stacked boxes (REQ-9 / AC-12).

### Step 6 — Tests (see section 4)
- Add/extend the test files enumerated in section 4. New tests cover ribbon DOM behavior, per-line timers, the cap/clamp, `?mode=`/`?lines=` parsing-and-use, config normalization (default now `adaptive`), the adaptive state machine (no-stack-under-normal-flow + promote-on-burst/replay + clean demote), and `?mode=single` legacy restore. NO `tests/test_engine.py` change (OD-5 deferred).

## 3. Risks & Mitigations

- **R-1 — Build-on-top of Track C (RESOLVED to sequential, low residual risk).** Track C's changes are ALREADY in the working tree; Track B builds on the cleaned animation baseline. RESIDUAL RISK: re-deriving or fighting Track-C constants (`--sub-animation-duration: 0.3s` `:19`, `REVEAL_BUDGET_MS` `:316-319`, per-line stagger bases, 5000/650ms lifetime). MITIGATION: REUSE all Track-C values verbatim; the ribbon only ADDS a container `flex-direction` modifier + per-line gap and swaps the timer OWNER, never the timings; the single render path is preserved (spec §7).
- **R-2 — Memory leak from per-line timers.** Moving from one shared timer to N per-line timers risks orphaned timers / detached nodes if a box is evicted early (cap exceeded), or if a box created in RIBBON is later removed by the single path after demotion. MITIGATION: eviction clears `_hideTimer`+`_cleanupTimer` before `removeChild`; the single-path removeChild block (`:420-424`) is extended to clear any per-line timers on the node it removes (§1.12); AC-13 / extended `tests/test_memory_leak.py` enforces 0 nodes and no live timers post-burst.
- **R-3 — Direction reversal late.** If the maintainer later flips to newest-bottom, layout must change. MITIGATION: direction is isolated to ONE CSS rule (`flex-direction: column-reverse`) + `appendChild` insertion unchanged, so the flip is a one-line change with no JS impact (OD-1 resolved newest-top by default).
- **R-4 — Single-mode / normal-pacing regression under adaptive-default.** Because the DEFAULT is now `adaptive`, a bug in the state machine could make NORMAL speech stack (the exact thing the maintainer wants to avoid) or could flicker. MITIGATION: SINGLE state takes the EXISTING code path verbatim (the `renderStacked` flag is false in SINGLE); promotion is edge-triggered (`>1` waiting) so it cannot fire on spaced one-at-a-time arrivals; AC-15 explicitly pins ONE box under normal flow with default/no-params; AC-17 pins `?mode=single` byte-for-byte legacy. The unchanged `tests/test_backpressure_js.py`, `tests/test_obs_port.py`, `tests/test_html_js.py` URL tests pin the single path.
- **R-5 — Dead-variable trap repeat (`?mode=` parsed but never read).** MITIGATION: AC-7 static test asserts the parsed mode is referenced inside the render path, not just declared (mirrors the `currentStyle` dead-variable defect at `:272`).
- **R-6 — Backpressure semantics confusion.** Conflating `MAX_PENDING_QUEUE` (transport, `:314`) with `subtitle_ribbon_max_lines` (visible) could cause silent drops. MITIGATION: OD-4 RESOLVED decouples them; spec REQ-7 + AC-4/AC-11 pin both independently. Note the transport-level discard during extreme >5 bursts is accepted (OD-4 note).
- **R-7 — Engine payload-shape churn (RETIRED).** OD-5 is RESOLVED to DEFER the payload hint, so `engine.py`/`tests/test_engine.py` are NOT touched and there is no payload-shape risk in this track.
- **R-8 — `?lines=` abuse covering the frame.** A huge `?lines=` could occlude the stream. MITIGATION: hard clamp `1..8` client-side AND in config normalization (AC-8, AC-5).
- **R-9 — Adaptive flapping at the engage/disengage boundary.** If the accumulation threshold sits right at the steady-state pace, the overlay could oscillate SINGLE↔RIBBON and flicker. MITIGATION: hysteresis (§1.11) — promote edge-triggered at `pendingQueue.length > 1`; demote only after full drain (≤1 pending) + one live line + a settle window reusing `DEBOUNCE_MS` (`:315`) + `is_replay` latch; the transition never re-renders surviving lines; AC-15/AC-16 pin no-stack-under-normal-flow and clean collapse.
- **R-10 — Orphaned/double-removed box at a state transition.** Promoting while a single-path box is live, or demoting with a survivor, could orphan the box's expiry or double-remove it. MITIGATION: §1.12 adoption protocol — on promotion, clear shared timers and re-arm per-line timers on the existing node using `_bornAt`-based remaining lifetime; on demotion, leave the survivor's per-line timers running and rely on the timer-clearing removeChild guard for the single path. AC-16 exercises promote-then-demote; AC-13 pins no leaked timers.
- **R-11 — `is_replay` never arrives under `live_only` backlog policy.** `engine.py` `_obs_emit_decision` returns `is_replay=False` and suppresses emission past `max_delay` under `live_only` (`:208-209`), so adaptive's secondary signal is dead under that policy. MITIGATION: adaptive's PRIMARY signal is the client `pendingQueue.length`, which still works under `live_only`; the secondary `is_replay` is purely additive. Documented in spec REQ-10 so the maintainer knows `live_only` makes the ribbon rely on client queue depth alone (and may suppress the very backlog that would feed it — by design of `live_only`).

## 4. Test Strategy

### Tests to ADD
- `tests/test_ribbon_js.py` (NEW): static/DOM assertions on `subtitulos_obs.html` —
  - `?mode=single`: prior `.sub-box` removed before append (AC-1); full legacy-restore (AC-17).
  - `?mode=ribbon`: two concurrent `.sub-box` nodes (AC-2); per-line independent expiry (AC-3); visible cap eviction (AC-4).
  - `?mode=`/`?lines=` parsed AND referenced in the render path, guarding the dead-variable trap (AC-7, AC-8); absent `?mode=` resolves to `adaptive`.
  - Adaptive (default): spaced arrivals yield exactly one box (AC-15); a burst (`pendingQueue.length > 1`) OR an `is_replay:true` payload promotes to stacked, then demotes cleanly without flicker (AC-16).
  - All 7 presets render as stacked boxes (AC-12).
- `tests/test_memory_leak.py` (EXTEND): ribbon + adaptive-transition variant asserting per-line `removeChild` + nullify and return-to-0 nodes after a burst (AC-13), including the promote-then-demote path (R-10). Existing single-mode assertions (`removeChild` + `existing=null`) stay valid for single mode.

### Tests to EXTEND
- `tests/test_config.py`: normalization + clamp for `subtitle_display_mode` (unknown → DEFAULT `adaptive`, AC-6; assert DEFAULT_CONFIG value is `"adaptive"`) and `subtitle_ribbon_max_lines` (out-of-range / non-int → clamped `1..8`, AC-5), following existing DEFAULT_CONFIG/VALID_-set test patterns.
- `tests/test_html_js.py`: `TestValidStylesAndURLParams` gains `?mode=`/`?lines=` coverage mirroring the existing `get('style')` assertions.
- `tests/test_obs_port.py`: confirm `?mode=`/`?lines=` params do not break existing `URLSearchParams`/port assertions; add presence assertions.
- `tests/test_backpressure_js.py`: `TestClientBackpressure` — keep single-mode assertions (`isShowing`, `pendingQueue`, debounce, MAX_QUEUE) intact (AC-11); add ribbon/adaptive counterparts reflecting concurrent-visible semantics.

### Tests explicitly NOT touched here
- `tests/test_engine.py`: NOT touched (OD-5 deferred — no `display_mode` payload key, no `engine.py` change). AC-9/AC-10 are deferred with the hint.
- `tests/test_html_js.py` `TestEntryExitAnimationMirroring`: brittle false-positive owned by Track C (animation CSS). Track B must not edit `.sub-box.hiding` exit CSS (`:59-63`) in a way that masks/breaks it; the ribbon only adds a container `flex-direction` modifier + per-line gap.
- `tests/test_theme_engine.py` `TestThemeValidationPython`: unrelated; no mode uses the theme path. (The dead `validate_theme_tokens()` at `engine.py:91-108` is noted but not in scope.)

### Manual verification in OBS / stream
1. NO params (now resolves to `adaptive`) → speak normally (one utterance at a time, with gaps) → confirm VISUALLY IDENTICAL to before: exactly one line, same 5s lifetime, no stacking (REQ-9 / AC-15).
2. NO params (adaptive) → speak in a rapid burst so lines accumulate → confirm the ribbon engages (up to 3 lines, newest on top), then collapses to one line when you slow down, with NO flicker of the surviving line (AC-16, NFR-2).
3. `?mode=single` → confirm full legacy single-line-replace behavior is restored (AC-17).
4. `?mode=ribbon&lines=3` → confirm always-stacked up to 3, newest on top, each aging out independently ~5s after IT appeared.
5. `?mode=ribbon&lines=1` → confirm one visible line via the per-line path.
6. Catch-up replay: trigger a backlog catch-up (`is_replay`/`catchup_interval_sec` payloads, `engine.py:585-586`) under `auto`/`send_all` → confirm the ribbon fills (paced by 80ms) and no flicker; under `live_only`, confirm adaptive still engages on raw client queue depth (R-11).
7. Reload the browser source mid-stream → confirm deterministic MODE resolution (NFR-3) and no leaked DOM growth over a multi-minute session (NFR-1).
8. Test each of the 7 styles with `?mode=ribbon&style=<name>` combos (real style comes from payload; verify against the configured `subtitle_style`).
