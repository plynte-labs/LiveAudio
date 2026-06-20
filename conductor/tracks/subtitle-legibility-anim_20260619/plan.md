# Implementation Plan: Subtitle Legibility & Animation Polish (OBS Overlay)

## 1. Design Decisions

### 1.1 Single source of truth for animation duration (REQ-1)
The CSS `:root` value (`subtitulos_obs.html:19`) is the ONLY value the runtime
actually applies — `applyTheme` (`subtitulos_obs.html:302-311`) only sets tokens
that arrive over WebSocket, so the JS `VALID_THEME_TOKENS.default` field
(`subtitulos_obs.html:284`) is dead documentation in practice. The divergence is
therefore a documentation bug, not a runtime bug. We fix it by making the doc
field match the CSS `:root`, and we pick a single agreed value for both (OD-1).
We deliberately do NOT add a `default` to the Python schema
(`liveaudio/core/engine.py:84`): per the verified investigation, the Python schema
intentionally carries only `type/min/max` (server-side validation strips
out-of-bounds tokens before broadcast, `engine.py:91-108`), so adding a default
there would invent state the server never uses.

**Rejected**: deleting the JS `default` field entirely. Rejected because the field
documents intended defaults for the whole token set consistently
(`subtitulos_obs.html:276-287`); removing just one would make the schema
inconsistent and surprise future readers. Keeping it and correcting the value is
the least-surprise fix.

### 1.2 Clamp via a named budget constant, not magic numbers (REQ-4, REQ-5)
Rather than sprinkle three new literals, we define ONE constant
`REVEAL_BUDGET_MS` (default 400, OD-4) near the existing timing constants
(`DEBOUNCE_MS` at `subtitulos_obs.html:315`) and compute
`perWordDelay = min(baseDelay, REVEAL_BUDGET_MS / max(wordCount - 1, 1))` in each
branch. Using `min(baseDelay, ...)` guarantees short phrases are byte-for-byte
unchanged (the base wins) and only long phrases get compressed (the budget wins).
This keeps each style's identity intact while bounding total reveal time, and it
gives the test a single, assertable symbol.

**Rejected**: a fixed `perWordDelay = budget / wordCount` for all lengths.
Rejected because it would SLOW DOWN short phrases (e.g. 2 words would spread to
200ms each) — the opposite of the goal. The `min(...)` form is strictly better.

**Rejected**: capping by truncating delays at a ceiling per word
(`min(index * base, budget)`). Rejected because it bunches all tail words at the
exact same delay, producing a visible "snap" of many words at once instead of a
smooth-but-fast cascade.

### 1.3 Align cleanup timeout to the longest capped cascade (REQ-5)
With the cascade now bounded by `REVEAL_BUDGET_MS`, the fixed 350ms cleanup
(`subtitulos_obs.html:531`) is no longer guaranteed to outlast a long hide
cascade plus the per-word exit animation (~0.15-0.2s,
`subtitulos_obs.html:192`, `:220`). We raise the cleanup timeout to
`budget + exit + margin` (default 650ms, OD-5) so tail words always finish.
Because the budget is now bounded, this number is a single safe constant rather
than something that must scale with word count.

### 1.4 Legibility changes are contrast-aware (REQ-2, REQ-3)
`tests/test_wcag_contrast.py` models presets with HARDCODED fg/bg pairs
(`:36-44`), and minimal is white on `#000000` (`:40`). A font-weight bump is
invisible to that test (weight doesn't affect the ratio), so weight-only is the
zero-risk path. If we also raise the minimal background opacity (OD-2), the real
CSS still resolves darker than the test's `#000000` model is generous about, so
contrast only improves; we update the test pair only if we change the bg COLOR
(not just opacity) so the model stays representative. Font-size/clamp changes
(REQ-3) do not affect contrast ratios at all.

### 1.5 Fix the false-positive test rather than delete it (REQ-7)
`test_typewriter_staggered_delay` (`tests/test_html_js.py:60-64`) is a false
positive: it passes on the `"80"` substring from `DEBOUNCE_MS = 80`
(`subtitulos_obs.html:315`), not on any stagger value. We rewrite it to assert the
real mechanism (the `REVEAL_BUDGET_MS` constant and the `Math.min` clamp in the
typewriter branch) so it actually guards the cap. This turns a meaningless test
into a regression guard for REQ-4.

## 2. Implementation Steps

> All overlay edits are in a single file; group them into one coherent commit per
> work unit to keep the diff reviewable. Land this whole track before Track B
> (`subtitle-ribbon-buffer_20260619`) starts touching the same file.

### Step 1 — Reconcile animation duration (REQ-1, AC-1)
- Edit `liveaudio/assets/subtitulos_obs.html:19` — set `--sub-animation-duration`
  to the OD-1 value (default `0.3s`).
- Edit `liveaudio/assets/subtitulos_obs.html:284` — set the JS
  `VALID_THEME_TOKENS['--sub-animation-duration'].default` to the SAME value.
- Confirm `liveaudio/core/engine.py:84` still has only `type/min/max` (no edit;
  verify the chosen value is within `[0.1, 2.0]`).

### Step 2 — Minimal-preset legibility (REQ-2, AC-2)
- Edit `liveaudio/assets/subtitulos_obs.html:138` — raise `.style-minimal`
  `font-weight` per OD-2 (default `500`).
- Edit `liveaudio/assets/subtitulos_obs.html:140` — raise `.style-minimal`
  background opacity per OD-2 (default `rgba(0,0,0,0.55)`).
- If the bg COLOR (not just opacity) changes, edit
  `tests/test_wcag_contrast.py:40` to match and keep ratio >= 3:1.

### Step 3 — Small-source font floor (REQ-3, AC-3)
- Edit `liveaudio/assets/subtitulos_obs.html:251` — change the clamp to the OD-3
  value (default `clamp(28px, 4.5vw, 36px)`) inside the `max-height: 220px` media
  query block that already lists all seven classes
  (`subtitulos_obs.html:244-250`).

### Step 4 — Introduce `REVEAL_BUDGET_MS` and cap reveal stagger (REQ-4, AC-4)
- Edit `liveaudio/assets/subtitulos_obs.html` near `:315` — add
  `const REVEAL_BUDGET_MS = 400;` (OD-4) beside `DEBOUNCE_MS`.
- Edit the karaoke reveal branch `subtitulos_obs.html:433` — replace
  `index * 0.025` with the clamped `min(baseDelay, REVEAL_BUDGET_MS / max(n-1,1))`
  form (baseDelay = 25ms).
- Edit the rgb reveal branch `subtitulos_obs.html:458` — same form, baseDelay = 50ms.
- Edit the typewriter reveal branch `subtitulos_obs.html:477` — same form,
  baseDelay = 40ms.
- Keep units consistent per branch (karaoke/rgb use `s`, typewriter uses `ms`);
  compute in ms then convert, or keep each branch in its existing unit.

### Step 5 — Cap hide stagger and align cleanup (REQ-5, AC-5)
- Edit the hide cascade for karaoke/rgb/typewriter
  (`subtitulos_obs.html:509`, `:514`, `:519`) — apply the SAME clamped formula and
  `REVEAL_BUDGET_MS` constant.
- Edit `liveaudio/assets/subtitulos_obs.html:531` — raise the cleanup timeout from
  `350` to the OD-5 value (default `650`).

### Step 6 — Fix stale comment (REQ-6, AC-6)
- Edit `liveaudio/assets/subtitulos_obs.html:432` — replace the incorrect
  "50ms por palabra" comment with an accurate description of the clamped behavior
  (neutral/professional Spanish to match context).

### Step 7 — Repair the typewriter-delay test (REQ-7, AC-7)
- Edit `tests/test_html_js.py:60-64` — assert presence of `REVEAL_BUDGET_MS` and
  the `Math.min` clamp in the typewriter context instead of the `"80"` substring.

### Step 8 — Validator sync confirmation (REQ-8, AC-8)
- No code change by default. Confirm HTML `VALID_THEME_TOKENS`
  (`subtitulos_obs.html:275-288`) and Python `VALID_THEME_TOKENS`
  (`liveaudio/core/engine.py:75-88`) remain identical in token set and bounds.
- ONLY if OD-6 is accepted: add `--sub-reveal-budget` to BOTH dicts with matching
  `type/min/max`, wire it through `validateThemeToken`
  (`subtitulos_obs.html:290-300`) and `validate_theme_tokens`
  (`engine.py:91-108`), and add a corresponding test in
  `tests/test_theme_engine.py`.

## 3. Risks & Mitigations

- **Risk: changing minimal background color breaks WCAG test silently.**
  Mitigation: only opacity is changed by default (test models `#000000`, which is
  the darkest case, so contrast can only improve). If the COLOR changes, Step 2
  updates `tests/test_wcag_contrast.py:40` in the same commit.
- **Risk: the `"80"` substring test masked the typewriter timing for a long time;
  other timing values (0.025s, 0.05s, 40ms, 5000ms, 38px) are pinned by NO test**
  (verified). Mitigation: Step 7 converts the false positive into a real guard for
  the cap; document in the test docstring that timing literals are otherwise
  unguarded so future edits add assertions.
- **Risk: cleanup timeout still too short on a custom (large) animation duration.**
  Mitigation: OD-5 sizes the timeout from `budget + exit + margin`; the exit
  animations (`rgbFadeOut` 0.2s `:192`, `typeIn` 0.15s `:220`) are fixed and not
  driven by `--sub-animation-duration`, so 650ms is safe for the default budget.
- **Risk: Track B merge conflict on the same stagger/cleanup code.**
  Mitigation: land Track C first (spec section 7); Track B rebases on the cleaned
  base. Communicate the OD-1 value so Track B does not reintroduce the divergence.
- **Risk: unit mismatch when converting karaoke/rgb (`s`) vs typewriter (`ms`).**
  Mitigation: compute `perWordDelay` in ms in all three branches and convert at
  assignment, or keep each branch's existing unit but derive the budget term in the
  matching unit; cover with the Step 7 assertion plus manual OBS check.

## 4. Test Strategy

### Automated (extend existing suite)
- `tests/test_html_js.py` — rewrite `test_typewriter_staggered_delay`
  (`:60-64`) to assert `REVEAL_BUDGET_MS` and the `Math.min` clamp (REQ-7/AC-7).
  Add a new test asserting all three reveal branches and all three hide branches
  reference `REVEAL_BUDGET_MS` (guards REQ-4/REQ-5 against future removal).
- `tests/test_wcag_contrast.py` — run unchanged for weight-only minimal edits;
  update the `minimal` pair (`:40`) only if the bg COLOR changes (REQ-2/AC-2).
- `tests/test_theme_engine.py` — run unchanged (`:53-62` is keyword-presence only,
  `:85-114` asserts class presence by string; both safe). Add a token test ONLY if
  OD-6 is accepted.
- Add a focused string test (in `tests/test_html_js.py`) asserting the CSS `:root`
  `--sub-animation-duration` and the JS `default` carry the SAME value (guards
  REQ-1/AC-1 against future divergence).

### Manual verification in OBS / stream
- Load `liveaudio/assets/subtitulos_obs.html` as an OBS browser source.
- For each of karaoke, rgb, typewriter: feed a long (>= 25 word) subtitle and
  confirm the full phrase appears within ~`REVEAL_BUDGET_MS` and, on auto-hide
  (5000ms), the tail words complete their fade before the box disappears (no
  abrupt truncation).
- Feed a short (2-3 word) subtitle and confirm the cascade spacing is visually
  identical to current behavior (cap not engaged).
- Switch to `minimal` over a bright background scene and confirm improved
  readability (heavier weight / denser background).
- Resize the browser source below 220px height and confirm text no longer shrinks
  below the new floor.
- Send a WebSocket `theme` message with an out-of-range `--sub-animation-duration`
  (e.g. `5s`) and confirm it is still rejected by both validators (no regression).
