# Tasks: Vertical Ribbon Subtitle Buffer for OBS Overlay

> Every task references a spec requirement ID. Source edits belong to the Implementation phase; this file is the approved checklist. All blocking Open Decisions are RESOLVED: OD-1 (newest-top), OD-5 (defer engine hint → T-4/T-5 DROPPED), OD-6 (default `adaptive` + threshold). OD-2/OD-3/OD-4 ship recommended values. This track touches TWO source files: `config.py` and `subtitulos_obs.html` (plus tests).

- [x] **T-1 (REQ-4):** Add `subtitle_display_mode` config key with validation (default `adaptive`)
  - Given: `liveaudio/utils/config.py` defines `VALID_SUBTITLE_STYLES` (`:55`), `VALID_BACKLOG_POLICIES` (`:56`), and `DEFAULT_CONFIG` (`:59-88`); `subtitle_backlog_policy` validation falls back to `DEFAULT_CONFIG[...]` (`:191-193`).
  - When: a `subtitle_display_mode` key (default **`"adaptive"`**, LOCKED) is added to `DEFAULT_CONFIG`, a `VALID_SUBTITLE_DISPLAY_MODES = {"single", "ribbon", "adaptive"}` set is added, and `_normalize_config` validates against it, falling back to `DEFAULT_CONFIG["subtitle_display_mode"]` (NOT a hard-coded literal).
  - Then: `single`/`ribbon`/`adaptive` pass through; any other value normalizes to the DEFAULT `"adaptive"`.
  - Error Path: missing or malformed value → fall back to the DEFAULT via `DEFAULT_CONFIG[...]`; never raise.
  - UI State: N/A (no GUI control in this track).
  - OBS Behavior: N/A (config-layer task).

- [x] **T-2 (REQ-3):** Add `subtitle_ribbon_max_lines` config key with int clamp
  - Given: `_normalize_config` coerces numeric keys via `_clamp_number(...)` (`:91-100`); `cpu_threads` uses `cast=int` at `:136`.
  - When: `subtitle_ribbon_max_lines` (default `3`) is added to `DEFAULT_CONFIG` (`:59-88`) and normalized via `_clamp_number(..., 1, 8, int)`.
  - Then: `0`/`99` clamp into `1..8`; `3.7` or non-int coerces to a valid int; valid values pass through.
  - Error Path: non-coercible value → fall back to default `3`.
  - UI State: N/A.
  - OBS Behavior: N/A.

- [x] **T-3 (REQ-4, REQ-3):** Config normalization tests
  - Given: `tests/test_config.py` covers DEFAULT_CONFIG and VALID_-set normalization patterns.
  - When: tests assert DEFAULT_CONFIG `subtitle_display_mode == "adaptive"`, unknown value → `adaptive` (AC-6), and `subtitle_ribbon_max_lines` out-of-range/non-int → clamped `1..8` (AC-5).
  - Then: tests pass.
  - Error Path: a regression in clamp/fallback fails the assertion.
  - UI State: N/A.
  - OBS Behavior: N/A.

- [ ] ~~**T-4 (REQ-6):** Add read-only `display_mode` hint to engine payload~~ — **DROPPED (OD-5 RESOLVED: defer).** No `engine.py` change in this track. Documented in spec §5 / plan §1.8. Reactivates only in a future GUI-toggle track.

- [ ] ~~**T-5 (REQ-6):** Update engine payload-shape tests for `display_mode`~~ — **DROPPED (OD-5 RESOLVED: defer).** `tests/test_engine.py` is NOT touched.

- [x] **T-6 (REQ-5):** Parse `?mode=` and `?lines=` at overlay load and READ them
  - Given: `subtitulos_obs.html` parses `?style=` via `urlParams` (`:270`) into the DEAD variable `currentStyle` (`:272`, never read again — real style comes from `data.style` at `:396`), and `?port=` in `connect()` (`:379-380`).
  - When: `mode` and `lines` are parsed at load, validated (`mode`∈{single,ribbon,adaptive}; `lines`→clamp `1..8`), stored in module-level state, AND referenced inside the enqueue/process/show path.
  - Then: `?mode=ribbon` selects ribbon; `?mode=single` selects legacy; `?lines=N` sets visible cap; absent `?mode=` → DEFAULT `adaptive`; absent `?lines=` → default 3.
  - Error Path: `?lines=abc`→default; `?lines=12`→clamp 8; unknown `?mode=`→`adaptive` (the default).
  - UI State: N/A.
  - OBS Behavior: a browser source can opt into any mode via URL with no GUI; existing `?style=`/`?port=` URLs keep working; no-param sources get adaptive (AC-7, AC-8, AC-14).

- [x] **T-7 (REQ-1, REQ-2, REQ-7):** Ribbon render branch — stack instead of replace, per-line timers
  - Given: `showSubtitle()` (`:409-549`) clears shared `hideTimeout`/`cleanupTimeout` (`:263-264`, `:416-417`) and removes the prior `.sub-box` (`:420-424`); the hide fires at 5000ms (`:510`), cleanup at 650ms (`:539-546`); `processQueue` (`:338-369`) serializes via `isShowing` (`:322`); `onSubtitleComplete` (`:372-375`).
  - When: `showSubtitle` takes a `renderStacked` flag — when stacked, the shared `clearTimeout` (`:416-417`) and `removeChild(existing)` block (`:420-424`) are SKIPPED, the new box is appended, each box gets its OWN `_hideTimer`/`_cleanupTimer` and `_bornAt`, the cleanup callback does NOT call `onSubtitleComplete()`, and enqueue bypasses the `isShowing` gate (paced by `DEBOUNCE_MS`, `:315`). When NOT stacked, the body is byte-for-byte today's path. The style-rendering body (`:426-505`) is UNCHANGED and shared.
  - Then: ribbon shows multiple concurrent `.sub-box` nodes; line A appended at t=0 expires ~5000ms even after line B arrives at t=2000ms (AC-2, AC-3).
  - Error Path: a render error in one line MUST NOT cancel sibling lines' timers; single mode falls back to current serialized behavior unchanged.
  - UI State: N/A.
  - OBS Behavior: stacked subtitles in the ribbon branch; identical single-line behavior when mode is `single` (AC-1, AC-11).

- [x] **T-8 (REQ-3):** Enforce visible-line cap with oldest-line eviction
  - Given: the ribbon branch renders concurrent boxes (T-7) into a `column-reverse` container; insertion is `appendChild`, so DOM child order is `[oldest … newest]` and oldest = `container.firstElementChild` (`column-reverse` only flips the VISUAL order, not DOM child indices — T-9).
  - When: appending a line would exceed `subtitle_ribbon_max_lines` (URL `?lines=` or config), the OLDEST visible line is force-evicted — clear its `_hideTimer` AND `_cleanupTimer` FIRST, then `removeChild`, then null the ids.
  - Then: live `.sub-box` count never exceeds the cap (e.g. `?lines=2` caps at 2, AC-4).
  - Error Path: eviction MUST clear the evicted box's own timers before removal (no orphaned timers, R-2).
  - UI State: N/A.
  - OBS Behavior: ribbon never grows past the configured line count.

- [x] **T-9 (REQ-8):** Stacking-direction CSS (newest-on-top, OD-1 RESOLVED)
  - Given: bottom-anchored layout — `body` flex-column `justify-content:flex-end` (`:25-36`), `#subtitle-container` 90% width / 50px bottom margin (`:38-43`); `.sub-box` entry/exit transitions (`:46-63`, Track-C-owned — DO NOT edit).
  - When: a ribbon-active container modifier class applies `flex-direction: column-reverse` plus a per-line vertical `gap`/margin, keeping width/margin; insertion stays `appendChild`.
  - Then: newest line on TOP, oldest on BOTTOM, stack grows upward above the 50px margin; all 7 presets render as stacked boxes (AC-12).
  - Error Path: a later flip to newest-bottom changes only the one `flex-direction` rule (no JS impact, R-3).
  - UI State: N/A.
  - OBS Behavior: vertical ribbon honoring the maintainer's literal "arriba el mas reciente, abajo el mas antiguo".

- [x] **T-10 (REQ-9):** Preserve normal-pacing identity under adaptive-default + single legacy
  - Given: existing single-line rendering, backpressure (`MAX_PENDING_QUEUE=5` `:314`, `DEBOUNCE_MS=80` `:315`, discard-oldest `:327-328`), and 5000ms lifetime (`:510`); DEFAULT mode is now `adaptive`.
  - When: under DEFAULT `adaptive` with spaced one-at-a-time arrivals, the SINGLE state takes the EXISTING path (no promotion); under `?mode=single`, the legacy path runs verbatim.
  - Then: normal-pacing rendering under adaptive-default is VISUALLY IDENTICAL to pre-track single behavior (AC-15); `?mode=single` is byte-for-byte legacy (AC-1, AC-11, AC-14, AC-17).
  - Error Path: any normal-pacing/single regression fails the unchanged `tests/test_backpressure_js.py`, `tests/test_obs_port.py`, `tests/test_html_js.py` URL tests.
  - UI State: N/A.
  - OBS Behavior: existing OBS browser-source URLs (`?style=`, `?port=`) work unchanged; no-param sources look the same in steady speech.

- [x] **T-11 (REQ-7):** Define ribbon vs backpressure interaction (queue as stack)
  - Given: single mode discards oldest pending via `pendingQueue.shift()` (`:327-328`) under the `isShowing` gate.
  - When: the ribbon branch surfaces pending lines AS visible stack entries (up to the cap) instead of silently discarding at depth 5; `MAX_PENDING_QUEUE` (`:314`) stays as a transport safety valve only (OD-4 RESOLVED: decouple).
  - Then: ribbon visible depth is governed by `subtitle_ribbon_max_lines`, not `MAX_PENDING_QUEUE` (AC-4); single mode discard behavior is untouched (AC-11).
  - Error Path: conflating the two knobs causes silent drops — guarded by AC-4 + AC-11.
  - UI State: N/A.
  - OBS Behavior: ribbon shows the backlog as a stack; single mode unchanged.

- [ ] ~~**T-12 (REQ-6):** URL precedence over payload `display_mode` hint~~ — **DROPPED (OD-5 RESOLVED: defer).** No payload hint ships this track, so there is nothing for the URL to take precedence over. URL/config alone resolve the mode deterministically (NFR-3). Reactivates with T-4/T-5 in a future GUI-toggle track (AC-10).

- [x] **T-13 (REQ-1, REQ-2, REQ-3, REQ-5, REQ-10):** Ribbon + adaptive JS/DOM test suite
  - Given: new `tests/test_ribbon_js.py`.
  - When: tests assert `?mode=single` removeChild-before-append (AC-1), ribbon concurrent nodes (AC-2), per-line independent expiry (AC-3), cap eviction (AC-4), `?mode=`/`?lines=` parsed AND referenced in render path (AC-7, AC-8), default-resolves-to-`adaptive`, adaptive no-stack-under-normal-flow (AC-15), adaptive promote-on-burst/replay then clean demote (AC-16), and 7 presets stack (AC-12).
  - Then: all assertions pass.
  - Error Path: dead-variable reintroduction (parsed-but-unused mode) fails AC-7.
  - UI State: N/A.
  - OBS Behavior: N/A (static/DOM test layer).

- [x] **T-14 (NFR-1, REQ-2):** Ribbon + adaptive-transition memory-leak test
  - Given: `tests/test_memory_leak.py` asserts single-mode `removeChild` + `existing=null`.
  - When: a ribbon variant asserts per-line `removeChild` + timer-id nullify and return-to-0 `.sub-box` nodes within ~5650ms (5000ms hide `:510` + 650ms cleanup `:539-546`) of the last subtitle, AND the promote-then-demote adaptive path leaves no orphaned per-line timers (R-10, AC-13).
  - Then: no orphaned nodes or timers after a ribbon/adaptive burst.
  - Error Path: leaked node/timer fails the assertion.
  - UI State: N/A.
  - OBS Behavior: N/A.

- [x] **T-15 (REQ-5, REQ-9):** Extend URL-param / port / backpressure tests
  - Given: `tests/test_html_js.py` `TestValidStylesAndURLParams`, `tests/test_obs_port.py`, `tests/test_backpressure_js.py` `TestClientBackpressure`.
  - When: `?mode=`/`?lines=` coverage is added to the URL-param and port tests; single-mode backpressure assertions are preserved and ribbon/adaptive counterparts added.
  - Then: existing single-mode tests still pass (AC-11, AC-14); new params asserted present.
  - Error Path: single-mode regression fails the preserved assertions.
  - UI State: N/A.
  - OBS Behavior: N/A.

- [x] **T-16 (Track Coordination):** Build on the landed Track C baseline (sequencing resolved)
  - Given: Track C's changes are ALREADY in the working tree on `subtitulos_obs.html`; Track B is the sequential successor on the same module (no parallel branch). Track C established `--sub-animation-duration: 0.3s` (`:19`), `REVEAL_BUDGET_MS` (`:316-319`), per-line stagger bases, 5000/650ms lifetime (`:510`, `:539-546`), and a single render path.
  - When: Track B reuses all Track-C constants verbatim and only ADDS a container `flex-direction` modifier + per-line gap and swaps the timer OWNER, preserving the single `showSubtitle` render path.
  - Then: no re-derivation of Track-C timings; no edit to the `.sub-box.hiding` exit CSS (`:59-63`); the brittle `TestEntryExitAnimationMirroring` (`tests/test_html_js.py:67-86`) is neither masked nor broken.
  - Error Path: editing the exit-animation CSS or re-deriving Track-C constants regresses Track C — block.
  - UI State: N/A.
  - OBS Behavior: N/A.

- [x] **T-17 (REQ-10):** Adaptive state machine — promote on accumulation, demote on drain (DEFAULT mode)
  - Given: the ribbon render branch (T-7), the existing pending queue (`pendingQueue`, `:320`, `:325-369`), `isShowing` (`:322`), `DEBOUNCE_MS=80` (`:315`); the payload carries currently-ignored `is_replay`/`catchup_interval_sec`/`total_delay` hints (`liveaudio/core/engine.py:583,585-586`), threaded into `enqueueSubtitle(text, style, isReplay)` from `onmessage` (`:386-401`).
  - When: mode is `adaptive` (DEFAULT) — a `let adaptiveState` runs the SINGLE⇄RIBBON machine (plan §1.10): PROMOTE SINGLE→RIBBON at the top of `enqueueSubtitle` when `pendingQueue.length > 1` OR `isReplay===true` (edge-triggered, no delay); DEMOTE RIBBON→SINGLE only after full drain (≤1 pending) + one live `.sub-box` + an `is_replay`-stopped latch + a settle window reusing `DEBOUNCE_MS` (plan §1.11). The container ribbon-active class toggles with the state.
  - Then: steady one-at-a-time speech shows exactly one line (AC-15); a burst or replay stacks then collapses without flicker (AC-16).
  - Error Path: flapping MUST NOT flicker surviving lines — edge-trigger + settle window + `is_replay` latch (R-9); the demotion timer is cancelled if a new line arrives in the settle window.
  - UI State: N/A.
  - OBS Behavior: no oversized text block in normal flow; the ribbon appears ONLY during accumulation, honoring the "solo si empiezan a pisarse/encolarse" intent — now the shipped DEFAULT.

- [x] **T-18 (REQ-4, REQ-9):** `?mode=single` legacy-restore guarantee under adaptive-default
  - Given: the DEFAULT is now `adaptive`; the legacy single path still exists (shared-timer clear `:416-417`, removeChild-before-append `:420-424`, serialized discard-oldest `:327-328`).
  - When: `?mode=single` (or config `subtitle_display_mode="single"`) resolves; the state machine is bypassed and the legacy path runs verbatim.
  - Then: byte-for-byte legacy single-line-replace behavior is restored (AC-17); a static/DOM test asserts the single path is reachable and behaviorally equal to pre-track single mode.
  - Error Path: any drift from legacy single behavior fails AC-17.
  - UI State: N/A.
  - OBS Behavior: purists fully opt out of stacking with `?mode=single`.

- [x] **T-19 (REQ-2, REQ-10):** Per-line timer lifecycle across SINGLE⇄RIBBON transitions
  - Given: SINGLE-path boxes ride the shared `hideTimeout`/`cleanupTimeout` (`:263-264`, `:416-417`); RIBBON boxes own `_hideTimer`/`_cleanupTimer`/`_bornAt` (T-7); promotion/demotion can leave an in-flight box (plan §1.12).
  - When: PROMOTION adopts the live single box — cancel the shared timers, re-arm per-line `_hideTimer`/`_cleanupTimer` for the REMAINING lifetime via `max(0, 5000 - (Date.now() - sub._bornAt))`. DEMOTION leaves the survivor's per-line timers running. The single-path removeChild block (`:420-424`) is extended to clear any `_hideTimer`/`_cleanupTimer` on the node it removes (harmless for genuine single boxes).
  - Then: no box is orphaned or double-removed across transitions; each box expires once on its own schedule (R-10).
  - Error Path: a stale per-line timer firing on a detached node, or a survivor losing its expiry, fails AC-13/AC-16.
  - UI State: N/A.
  - OBS Behavior: transitions are invisible to the viewer — surviving lines do not flicker, re-time, or duplicate.
