# Tasks: Subtitle Legibility & Animation Polish (OBS Overlay)

- [x] **T-1 (REQ-1):** Reconcile `--sub-animation-duration` to one source of truth
  - Given: CSS `:root` declares `0.2s` (`subtitulos_obs.html:19`) but JS `VALID_THEME_TOKENS.default` declares `0.4s` (`subtitulos_obs.html:284`)
  - When: both locations are set to the single OD-1 value (default `0.3s`)
  - Then: CSS `:root` and the JS `default` field are identical and within `[0.1, 2.0]`; Python schema (`engine.py:84`) is left with bounds only (no `default` added)
  - Error Path: if the chosen value falls outside `[0.1, 2.0]`, both validators would reject it on WebSocket apply — choose a value inside the range
  - UI State: N/A
  - OBS Behavior: entry/exit transitions run at the reconciled duration; a `theme` message overriding the token still validates against the same bounds

- [x] **T-2 (REQ-2):** Raise `.style-minimal` legibility (weight and background opacity)
  - Given: `.style-minimal` uses `font-weight: 300` (`subtitulos_obs.html:138`) on `rgba(0,0,0,0.3)` background (`:140`)
  - When: weight is raised to >= 400 (default `500`) and opacity raised per OD-2 (default `rgba(0,0,0,0.55)`)
  - Then: the minimal preset renders heavier and denser; `tests/test_wcag_contrast.py` still passes for `minimal` (>= 3:1), with `:40` updated only if the bg COLOR changed
  - Error Path: if a bg color change drops contrast below 3:1, the WCAG test fails — keep bg dark enough and update the test pair
  - UI State: N/A
  - OBS Behavior: minimal subtitles are more readable over bright stream backgrounds

- [x] **T-3 (REQ-3):** Raise the small-source font floor in the media query
  - Given: `max-height: 220px` query clamps all seven classes to `clamp(24px, 4vw, 34px)` (`subtitulos_obs.html:251`)
  - When: the clamp lower bound is raised per OD-3 (default `clamp(28px, 4.5vw, 36px)`)
  - Then: at viewport height <= 220px no preset renders below the new floor; all seven listed classes (`:244-250`) share the new clamp
  - Error Path: N/A (CSS-only; no validation path)
  - UI State: N/A
  - OBS Behavior: subtitles stay legible in short/banner-style browser sources

- [x] **T-4 (REQ-4):** Introduce `REVEAL_BUDGET_MS` and cap reveal stagger for all three animated styles
  - Given: reveal staggers are unbounded — karaoke `index*0.025s` (`:433`), rgb `index*0.05s` (`:458`), typewriter `index*40ms` (`:477`)
  - When: a single `REVEAL_BUDGET_MS` constant (default `400`, near `DEBOUNCE_MS` at `:315`) is added and each branch uses `perWordDelay = min(base, REVEAL_BUDGET_MS / max(wordCount-1, 1))`
  - Then: a 25-word phrase's last word starts <= `REVEAL_BUDGET_MS` after the first for all three styles; a 3-word phrase keeps its original per-word delay
  - Error Path: if `wordCount` is 1, `max(wordCount-1,1)` prevents divide-by-zero and the single word reveals immediately
  - UI State: N/A
  - OBS Behavior: long live subtitles appear snappy (full phrase within the budget) while short phrases look unchanged

- [x] **T-5 (REQ-5):** Cap hide stagger and align the DOM cleanup timeout
  - Given: hide cascade resets to `index*0.025s` for all three styles (`:509`, `:514`, `:519`) but the DOM node is removed after a fixed `350ms` (`:531`), truncating tail words
  - When: the hide cascade uses the same `REVEAL_BUDGET_MS` clamp and the cleanup timeout is raised to OD-5 (default `650ms`)
  - Then: on a 25-word hide, the last word's delay is <= `REVEAL_BUDGET_MS` and the cleanup timeout >= budget + per-word exit duration, so no tail word is removed mid-animation
  - Error Path: if the cleanup timeout is set below `budget + exit`, tail words are cut — size it from `budget + longest exit (~0.2s) + margin`
  - UI State: N/A
  - OBS Behavior: subtitles fade out completely (all words) before the box disappears, even on long phrases

- [x] **T-6 (REQ-6):** Fix the stale karaoke timing comment
  - Given: comment says `// Escalonamos la aparición (50ms por palabra)` (`subtitulos_obs.html:432`) but code uses 25ms (now clamped)
  - When: the comment is rewritten to describe the actual clamped behavior (neutral/professional Spanish)
  - Then: the comment matches the code; no "50ms" claim remains at that line
  - Error Path: N/A (comment-only)
  - UI State: N/A
  - OBS Behavior: N/A

- [x] **T-7 (REQ-7):** Repair the false-positive typewriter-delay test
  - Given: `test_typewriter_staggered_delay` (`tests/test_html_js.py:60-64`) passes only via the `"80"` substring from `DEBOUNCE_MS = 80` (`:315`), not via any stagger value
  - When: the test is rewritten to assert the presence of `REVEAL_BUDGET_MS` and the `Math.min` clamp in the typewriter context
  - Then: the test fails if the cap is removed and no longer depends on the `"80"` substring
  - Error Path: if the clamp symbol is renamed, the test must be updated alongside — keep the assertion in sync with the constant name
  - UI State: N/A
  - OBS Behavior: N/A (test-only)

- [x] **T-8 (REQ-8):** Confirm HTML/Python token validators stay in sync
  - Given: `VALID_THEME_TOKENS` is duplicated in `subtitulos_obs.html:275-288` (JS) and `engine.py:75-88` (Python), validated by `validateThemeToken` (`:290-300`) and `validate_theme_tokens` (`engine.py:91-108`)
  - When: this track makes no token change by default (OD-6 = No); if a `--sub-reveal-budget` token is added, it is added to BOTH dicts with matching `type/min/max` and wired through both validators
  - Then: token set and bounds are identical across both files; `tests/test_theme_engine.py` passes
  - Error Path: if a token is added to only one side, the server would strip a token the client accepts (or vice versa) — both validators MUST change together
  - UI State: N/A
  - OBS Behavior: invalid/out-of-bounds tokens continue to be rejected server-side before broadcast and re-validated client-side on receipt

- [x] **T-9 (REQ-1, REQ-4, REQ-5):** Add regression guards for reconciled duration and the cap
  - Given: no existing test pins `--sub-animation-duration` or the stagger values (verified)
  - When: a string test asserts the CSS `:root` and JS `default` duration values are equal, and a test asserts all reveal/hide branches reference `REVEAL_BUDGET_MS`
  - Then: future edits that re-diverge the duration or drop the cap fail CI
  - Error Path: if the assertion hardcodes a specific duration value, OD-1 changes would require a test update — assert EQUALITY of the two locations rather than a literal where possible
  - UI State: N/A
  - OBS Behavior: N/A (test-only)

- [x] **T-10 (all):** Run the full overlay test suite and manual OBS pass
  - Given: changes touch `subtitulos_obs.html`, possibly `tests/test_html_js.py` and `tests/test_wcag_contrast.py`
  - When: `tests/test_html_js.py`, `tests/test_wcag_contrast.py`, and `tests/test_theme_engine.py` are run, and the overlay is loaded as an OBS browser source
  - Then: all three suites pass; long phrases reveal within the budget and hide without truncation; short phrases unchanged; minimal is more legible; small-source floor holds
  - Error Path: any failing test or truncated tail-word reveal blocks the track until fixed
  - UI State: N/A
  - OBS Behavior: karaoke/rgb/typewriter reveal fast and complete their hide; out-of-range theme tokens still rejected
