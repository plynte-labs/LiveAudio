# Specification: Subtitle Legibility & Animation Polish (OBS Overlay)

## 1. Goal

Improve the legibility and perceived responsiveness of the OBS subtitle overlay
(`liveaudio/assets/subtitulos_obs.html`) without changing its stylistic identity
or breaking the WCAG AA contrast contract established by
`subtitle-style-system_20260509`.

Three concrete problems are addressed:

1. **A source-of-truth divergence** for `--sub-animation-duration`: the CSS
   `:root` declares `0.2s` (`subtitulos_obs.html:19`) while the JS theme schema
   `VALID_THEME_TOKENS` declares a `default` of `0.4s`
   (`subtitulos_obs.html:284`). The two disagree. The Python schema
   (`liveaudio/core/engine.py:84`) intentionally carries **no** `default` key —
   only `type/min/max` bounds — so there is nothing to reconcile on the Python
   side beyond keeping bounds valid for whatever default is chosen.

2. **Low legibility on the `minimal` preset**: `font-weight: 300`
   (`subtitulos_obs.html:138`) is thin, and its background is only 30% opaque
   (`rgba(0,0,0,0.3)`, `subtitulos_obs.html:140`), which significantly reduces
   real-world contrast on bright stream backgrounds. The small-source media
   query (`max-height: 220px`, `subtitulos_obs.html:239`) shrinks every preset to
   `clamp(24px, 4vw, 34px)` (`subtitulos_obs.html:251`), whose 24px floor can be
   too small to read.

3. **Slow, uncapped per-word reveals** on long phrases. The cumulative stagger
   grows linearly with word count and is never clamped:
   - karaoke: `index * 0.025s` (`subtitulos_obs.html:433`)
   - rgb: `index * 0.05s` (`subtitulos_obs.html:458`) — 25 words = 1.25s
   - typewriter: `index * 40ms` (`subtitulos_obs.html:477`) — 25 words = 1.0s
   On hide, all three reset to `index * 0.025s`
   (`subtitulos_obs.html:509`, `:514`, `:519`), but the DOM node is removed after
   a fixed 350ms cleanup timeout (`subtitulos_obs.html:531`), so on long phrases
   the tail words are cut off before their hide animation completes.

The work also fixes a stale comment and a false-positive test discovered during
investigation, and keeps the HTML and Python token validators in sync.

## 2. Requirements

### REQ-1: Reconcile `--sub-animation-duration` to a single source of truth (P0)
- Pick ONE default value for `--sub-animation-duration` and make the CSS `:root`
  declaration (`subtitulos_obs.html:19`) and the JS `VALID_THEME_TOKENS.default`
  field (`subtitulos_obs.html:284`) identical.
- Recommended default: `0.3s` (see Open Decision OD-1). The maintainer may pick
  `0.2s` or `0.4s` instead; whatever is chosen MUST be applied to BOTH locations.
- The chosen value MUST remain inside the validator bounds (`min: 0.1`,
  `max: 2.0`) present in both `subtitulos_obs.html:284` and
  `liveaudio/core/engine.py:84`.
- Do NOT add a `default` key to the Python schema — it deliberately carries only
  bounds. Only the CSS `:root` value is the runtime default; the JS `default`
  field is fallback documentation that the JS code never applies (`applyTheme`
  only sets tokens received over WebSocket, `subtitulos_obs.html:302-311`).

### REQ-2: Raise minimal-preset legibility (P1)
- Increase `.style-minimal` `font-weight` from `300`
  (`subtitulos_obs.html:138`) to at least `400` (recommended `500`; see OD-2).
- Increase `.style-minimal` background opacity from `rgba(0,0,0,0.3)`
  (`subtitulos_obs.html:140`) to improve real-world contrast on bright streams
  (recommended `rgba(0,0,0,0.55)`; see OD-2).
- Changing only weight is contrast-safe under the current WCAG test, which models
  minimal as pure white on `#000000` (`tests/test_wcag_contrast.py:40`, `:74-77`).
  If the background color/opacity changes such that the test's hardcoded `bg`
  pair no longer represents the CSS, update `tests/test_wcag_contrast.py:40` to
  match and keep the asserted ratio >= 3:1.

### REQ-3: Raise the small-source font floor (P1)
- Raise the lower bound of the small-source clamp from `24px` toward a more
  legible floor (recommended `clamp(28px, 4.5vw, 36px)`; see OD-3) in the
  `max-height: 220px` media query block (`subtitulos_obs.html:251`).
- The change applies uniformly to all seven style classes already listed in that
  block (`subtitulos_obs.html:244-250`).

### REQ-4: Cap cumulative per-word stagger on reveal (P0)
- For karaoke (`subtitulos_obs.html:433`), rgb (`subtitulos_obs.html:458`), and
  typewriter (`subtitulos_obs.html:477`), replace the unbounded
  `index * perWordDelay` with a clamped per-word delay so the LAST word starts no
  later than a fixed budget after the first.
- Formula: `perWordDelay = min(baseDelay, REVEAL_BUDGET_MS / max(wordCount - 1, 1))`,
  then `span.style.animationDelay = (index * perWordDelay) + 'ms'` (or the `s`
  equivalent), where `baseDelay` is the current per-style value (karaoke 25ms,
  rgb 50ms, typewriter 40ms) so short phrases are visually unchanged.
- Recommended `REVEAL_BUDGET_MS = 400` (see OD-4), defined ONCE as a named JS
  constant near the other timing constants (e.g. alongside `DEBOUNCE_MS` at
  `subtitulos_obs.html:315`) and reused by all three branches.
- A phrase of any word count MUST finish staggering within `REVEAL_BUDGET_MS`
  (last word delay <= budget), while a short phrase (few words) MUST keep its
  current spacing (budget never makes it slower).

### REQ-5: Cap cumulative per-word stagger on hide and align cleanup (P0)
- Apply the same clamped formula from REQ-4 to the hide cascade for all three
  styles (`subtitulos_obs.html:509`, `:514`, `:519`), using the SAME
  `REVEAL_BUDGET_MS` constant.
- The DOM cleanup timeout (currently fixed `350ms` at `subtitulos_obs.html:531`)
  MUST be >= the longest possible capped hide cascade plus the per-word exit
  animation duration (`rgbFadeOut`/`typeIn` are ~0.15-0.2s,
  `subtitulos_obs.html:192`, `:220`). With a 400ms budget plus ~200ms exit, set
  the cleanup timeout to a value that no longer truncates tail words
  (recommended `650ms`; see OD-5).
- No word at any index may be removed from the DOM before its hide animation
  completes.

### REQ-6: Fix the stale karaoke comment (P2)
- The comment `// Escalonamos la aparición (50ms por palabra)`
  (`subtitulos_obs.html:432`) is wrong — the code uses `0.025s` (25ms). Update
  the comment to reflect the actual (now clamped) behavior. Default to a
  neutral/professional Spanish comment to match surrounding context.

### REQ-7: Repair the false-positive typewriter-delay test (P1)
- `tests/test_html_js.py:60-64` (`test_typewriter_staggered_delay`) asserts the
  string `"80"` is present and passes only because `DEBOUNCE_MS = 80`
  (`subtitulos_obs.html:315`) happens to contain it, NOT because of any stagger
  value. After REQ-4 changes the typewriter stagger, this test still would not
  guard it. Update the test so it asserts the actual stagger mechanism
  (e.g. presence of the `REVEAL_BUDGET_MS` constant and the clamping `Math.min`
  in the typewriter branch), so it would fail if the cap were removed.

### REQ-8: Keep HTML and Python token validators in sync (P0)
- This track does NOT add a new theme token (see OD-6). If the maintainer elects
  to add a `--sub-reveal-budget` token (optional, OD-6), it MUST be added to
  BOTH `VALID_THEME_TOKENS` in `subtitulos_obs.html:275-288` AND in
  `liveaudio/core/engine.py:75-88`, with matching `type`/`min`/`max` bounds, and
  validated by both `validateThemeToken` (`subtitulos_obs.html:290-300`) and
  `validate_theme_tokens` (`liveaudio/core/engine.py:91-108`).
- The default implementation keeps the reveal budget as a JS-internal constant
  (not a WebSocket-controllable token), so no validator change is required.

## 3. Non-Functional Requirements

- **Contrast**: All seven presets MUST continue to pass WCAG AA large-text
  contrast (>= 3:1) as enforced by `tests/test_wcag_contrast.py`. Any color/opacity
  change must be reflected in that test's `PRESETS` pairs.
- **Stylistic identity**: Karaoke (yellow pop), rgb (random HSL), typewriter
  (monospace + blink cursor) must remain visually recognizable; clamping affects
  only timing, not the animation shape.
- **No regression on short phrases**: Phrases up to the point where the cap
  engages must reveal with exactly the current spacing.
- **Determinism for tests**: The reveal budget and capped delays must be derivable
  by string inspection of the HTML (the test suite reads the file as text, e.g.
  `tests/test_html_js.py:70-73`); avoid runtime-only values that cannot be asserted.
- **Backward compatibility**: Existing `?style=` URLs and WebSocket theme messages
  continue to work unchanged; `VALID_STYLES` (`subtitulos_obs.html:266`) is not
  modified.

## 4. Acceptance Criteria

- **AC-1 (REQ-1)**: `--sub-animation-duration` declares the identical value in
  the CSS `:root` (`subtitulos_obs.html:19`) and in `VALID_THEME_TOKENS.default`
  (`subtitulos_obs.html:284`); the value is within `[0.1, 2.0]`. The Python schema
  still has no `default` key.
- **AC-2 (REQ-2)**: `.style-minimal` `font-weight` is >= 400, and
  `tests/test_wcag_contrast.py` still passes for the `minimal` preset with the
  asserted ratio >= 3:1 (test pair updated if the bg color changed).
- **AC-3 (REQ-3)**: The small-source media query clamp lower bound is >= 28px and
  all seven listed classes use the new clamp; no preset shrinks below the floor at
  `max-height <= 220px`.
- **AC-4 (REQ-4)**: For a 25-word phrase, the last word's `animationDelay` on
  reveal is <= `REVEAL_BUDGET_MS` for karaoke, rgb, and typewriter; for a 3-word
  phrase, per-word delay equals the original base delay (no slow-down).
- **AC-5 (REQ-5)**: On hide of a 25-word phrase, the last word's `animationDelay`
  is <= `REVEAL_BUDGET_MS`, and the DOM cleanup timeout
  (`subtitulos_obs.html:531`) is >= `REVEAL_BUDGET_MS + per-word exit duration`,
  so no tail word is removed mid-animation.
- **AC-6 (REQ-6)**: The karaoke timing comment (`subtitulos_obs.html:432`) no
  longer says "50ms por palabra" and accurately describes the clamped behavior.
- **AC-7 (REQ-7)**: `tests/test_html_js.py::test_typewriter_staggered_delay`
  asserts the real clamping mechanism (budget constant + `Math.min`) and would
  fail if the cap were removed; it no longer relies on the `"80"` substring.
- **AC-8 (REQ-8)**: HTML and Python `VALID_THEME_TOKENS` remain byte-for-byte
  consistent in token set and bounds; if OD-6 adds a token, both validators accept
  it and `tests/test_theme_engine.py` still passes.
- **AC-9 (all)**: The full existing test suite (`tests/test_html_js.py`,
  `tests/test_wcag_contrast.py`, `tests/test_theme_engine.py`) passes.

## 5. Out of Scope

- Track B ribbon/buffer overlay behavior (`subtitle-ribbon-buffer_20260619`).
- Adding new presets or removing existing ones (`VALID_STYLES` unchanged,
  `subtitulos_obs.html:266`).
- Changing the 5000ms per-line auto-hide duration (`subtitulos_obs.html:533`) —
  noted as a candidate but deferred; this track only ensures the hide CASCADE and
  cleanup are not truncated, not the dwell time.
- Reworking the rgb random-HSL color generation (`subtitulos_obs.html:454-457`)
  for per-word contrast; the WCAG test models rgb as fixed white
  (`tests/test_wcag_contrast.py:42`) and per-word color is out of scope here.
- Any GUI/CustomTkinter changes — this track is overlay-only.
- Server-side broadcast/backpressure logic in `engine.py` beyond keeping the
  token schema in sync.

## 6. Open Decisions

- **OD-1 — Reconciled `--sub-animation-duration` default.**
  Default recommendation: **`0.3s`** (midpoint between the two diverging values;
  snappier than 0.4s, less abrupt than 0.2s, well inside `[0.1, 2.0]`).
  Alternatives: `0.2s` (current CSS runtime value — zero visible change, just
  fixes the JS doc) or `0.4s` (current JS doc value). Maintainer must confirm.
- **OD-2 — Minimal weight and background opacity.**
  Default recommendation: `font-weight: 500` and `background-color: rgba(0,0,0,0.55)`.
  Rationale: 500 reads cleanly without losing the "minimal" lightness; 0.55 opacity
  meaningfully lifts real-world contrast on bright streams. If the maintainer
  prefers to keep the near-transparent aesthetic, raise weight only (>= 400) and
  leave opacity at 0.3 — that path needs no WCAG test edit.
- **OD-3 — Small-source clamp floor.**
  Default recommendation: `clamp(28px, 4.5vw, 36px)`. Alternative: keep the 4vw
  scaling but only raise the floor to 28px (`clamp(28px, 4vw, 34px)`). Maintainer
  picks the floor and whether to also lift the ceiling.
- **OD-4 — Reveal budget (`REVEAL_BUDGET_MS`).**
  Default recommendation: **`400`** (matches the seed's <=400ms target). Lower
  (e.g. 300) feels snappier but compresses the per-word effect on long phrases;
  higher (e.g. 500) preserves more stagger but feels slower. Maintainer confirms.
- **OD-5 — Cleanup timeout value.**
  Default recommendation: **`650ms`** = 400ms budget + ~200ms per-word exit
  animation + margin. If OD-4 changes the budget, recompute as
  `budget + longest exit animation + ~50ms margin`.
- **OD-6 — Expose reveal speed as a WebSocket token?**
  Default recommendation: **No** — keep `REVEAL_BUDGET_MS` as a JS-internal
  constant. Exposing a `--sub-reveal-budget` token would require adding it to BOTH
  validators (`subtitulos_obs.html:275-288` and `engine.py:75-88`) with matching
  bounds and new tests. Defer unless the maintainer wants runtime control.

## 7. Track Coordination

- **Shared file**: `liveaudio/assets/subtitulos_obs.html` is also the primary file
  for sibling Track B `subtitle-ribbon-buffer_20260619`.
- **Required sequencing**: Track C (this track) SHOULD land FIRST. It is an
  isolated CSS/JS polish pass (timing constants, weights, a clamp, a comment, and
  a test fix) with low blast radius and no new tokens. Landing it first gives Track
  B a cleaned-up, consistent base (single animation-duration source of truth,
  capped cascades, aligned cleanup timeout) on which to build the ribbon buffer,
  avoiding a later merge that re-touches the same stagger/cleanup code paths.
- **Sync obligation**: Any change to the token set/bounds must touch BOTH
  `subtitulos_obs.html:275-300` and `liveaudio/core/engine.py:75-108`. Track B must
  honor the same rule. Communicate the chosen `--sub-animation-duration` default
  (OD-1) to Track B so the ribbon does not reintroduce a divergence.
