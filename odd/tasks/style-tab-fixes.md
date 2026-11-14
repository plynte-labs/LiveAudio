# Style Tab Fixes (manual test findings)

## Objective

Fix the Style tab defects the owner found while manually testing `feat/style-editor-mcp`, and move the 7 built-in subtitle styles into the Style tab with live save.

## Problem / Evidence (read-only diagnosis, HEAD 614149e)

1. Changes snap back: `saveSubtitleStyle` (desktop/src/app.js:1201-1213) sends `{...currentConfig, subtitle_theme: theme}`; `adoptSavedConfig` (app.js:1125) merges with `collectFormData()`, which starts from `{...currentConfig}` (app.js:1342) holding the OLD theme; `mergeSavedConfigIntoForm` (desktop/src/runtime-settings.js:32-38) treats old != sent as a user edit and keeps the OLD theme; `populateForm` -> `populateStyleControls` reverts the controls (app.js:1266). Next save then persists the old values (owner's config has `subtitle_theme: {}`). Reproduced with real modules. Backend and watcher are fine.
2. Color pickers render as an empty box with a dash: `.form-control` padding (desktop/src/styles.css:480-489) applies to `input[type=color]` (index.html:209, 215), leaving no swatch height. Values are valid `#rrggbb`.
3. Built-in styles (default, karaoke, neon, minimal, bold, rgb, typewriter) live only in `#select-subtitle-style` in the Subtítulos tab (index.html:137-138) whose change handler is empty (app.js:1582-1585); `subtitle_style` only persists via the general save, contradicting the Style tab hint "changes are sent to OBS instantly".
4. `saveSubtitleStyle` repopulates the whole general form; `refreshStyleTab` (app.js:1115-1121) overwrites style controls while the user is editing.
5. `cargo fmt --all -- --check` fails on 6 files changed by this branch.

## Decisions (owner)

- Move the built-in style selector to the Style tab (remove it from Subtítulos); selecting a built-in saves immediately and reaches OBS (new cues and the test subtitle) like the rest of the editor. `subtitle_style` becomes owned by the style save path (same ownership rule as `subtitle_theme`/`subtitle_presets`).

## Authorized Scope and Constraints

- `desktop/src` (app.js, runtime-settings.js, subtitle-style.js, index.html, styles.css, i18n.js, test_frontend.js), `desktop/src-tauri/src` (save_subtitle_style, keep_subtitle_style/ownership, config_sync), `crates/liveaudio-core` only if needed for MCP allowlist parity of `subtitle_style`, docs touched by the change (`docs/MCP.md`, `docs/WEBSOCKET_OBS.md`).
- Tests must be behavioral: drive the real save -> adopt path with stale state (not source-text assertions).
- `cargo fmt` only on files changed by this branch (`git diff --name-only 06a337d..HEAD`), in its own commit.
- Never commit `desktop/src-tauri/gen/schemas/*`. No AI attribution. No push.

## Execution Configuration

| Item | Value |
|---|---|
| Workflow | ODD |
| Worktree / Branch | `E:\LiveAudio-worktrees\mcp-config` / `feat/style-editor-mcp` |
| TDD Mode | Strict TDD (RED -> GREEN -> REFACTOR), source: global CLAUDE.md |
| Test runner | `cargo test --workspace`; `node desktop/test_frontend.js` (from desktop/) |
| RDD | off (global) |
| Loop | writer + reviewer, max 3 rounds; then owner manual retest |

## Tasks

- [x] F1 — Fix snap-back (bug 1) and over-broad repopulation (bug 4) with behavioral tests. Route: delegated writer.
- [x] F2 — Color picker CSS (bug 2). Route: delegated writer.
- [x] F3 — Built-in style selector in the Style tab with live save + OBS delivery; remove from Subtítulos; ownership + MCP parity; i18n es/en; docs. Route: delegated writer.
- [x] F4 — `cargo fmt` on branch-changed files (separate commit). Route: delegated writer.
- [x] F5 — Letter spacing control in the Style tab for `--sub-letter-spacing` (slider -2..10 px, step 0.5, 0 = normal); mapped both ways in subtitle-style.js, shown in the preview, saved through presets, i18n es/en; shared fixture cases. Route: writer (inline).
- [x] F6 — Preset buttons unstyled (`class="btn"` has no CSS rule); use the existing `btn-secondary btn-compact` classes; delete keeps the same neutral style (no danger style exists outside the modal footer); structural guard; report other unstyled Style-tab classes. Route: writer (inline).

## Acceptance Criteria

- Editing any Style control keeps the new value, persists it, and OBS receives it; edits typed during an in-flight save survive.
- Color pickers show a visible swatch.
- Choosing a built-in style in the Style tab persists immediately and the next cue/test subtitle uses it.
- `cargo test --workspace`, frontend tests and `cargo fmt --all -- --check` (for branch files) pass.

## Progress / Evidence

- F1 (snap-back, over-broad repopulation), route: writer (inline).
  - Design: the general form never merges `subtitle_theme`/`subtitle_presets` (`STYLE_CONFIG_KEYS` in runtime-settings.js). A style save adopts only those fields (`adoptSavedStyle`) into `currentConfig` and `savedConfig`. `populateForm` no longer renders style controls; `renderStyleTab` does, on initial load, reset, and external change, and skips re-rendering while `styleDirty` is set (an edit not yet sent, or made after the send).
  - RED: `node test_frontend.js` with the new section 12 (behavioral: stale client copy, edit during in-flight save, general merge) and wiring guards -> 7 failures (adoptSavedStyle and STYLE_CONFIG_KEYS missing, populateForm still repopulates style, style save still calls adoptSavedConfig).
  - GREEN: `node test_frontend.js` -> 451 passed, 0 failed (baseline 443).
  - Commit: `46d15d7` (`fix(desktop): style saves own the subtitle style fields`).
- F2 (color swatch), route: writer (inline).
  - Scoped rule `input.form-control[type="color"]` (fixed 56x36 box, padding 3px) plus swatch pseudo-element rules in desktop/src/styles.css.
  - RED: verified by the visual symptom and a test in section 12 asserting the rules exist.
  - GREEN: `node test_frontend.js` -> 452 passed, 0 failed.
  - Commit: `9a82643` (`fix(desktop): allow color inputs to display their swatch`).
- F3 (built-in selector in Style tab with live save and OBS delivery), route: writer (inline).
  - Moved `#select-subtitle-style` markup from the Subtítulos tab to the top of the Style tab; added a "Test subtitle" button (`#btn-test-subtitle`) to the Style tab that sends a 4-second preview cue in the current style.
  - `save_subtitle_style` takes `style: Option<String>` and persists it alongside theme and presets under `config_write_lock`.
  - RED: `cargo test -p liveaudio-desktop` -> `save_subtitle_style` signature mismatch.
  - GREEN: backend tests pass, frontend tests 472 passed.
  - Commits: `a3c4287` (`feat(desktop): move built-in subtitle style into the Style tab with live save`).
- F4 (`cargo fmt` on branch-changed files), route: writer (inline).
  - Formatted the 6 files reported by `cargo fmt --all -- --check`.
  - Commit: `7be7ff1` (`style: format branch-changed files with cargo fmt`).
- F5 (`--sub-letter-spacing` slider control), route: writer (inline).
  - Added range input `#input-sub-letter-spacing` (-2 to 10px, step 0.5) with `#val-sub-letter-spacing` label.
  - RED: `node test_frontend.js` -> 2 failures (slider missing in DOM and controls mapping).
  - GREEN: 499 passed.
  - Commit: `13c2e5a` (`feat(desktop): add letter spacing control to the Style tab`).
- F6 (preset buttons unstyled), route: writer (inline).
  - Cause: `#btn-sub-preset-save/load/delete` used `class="btn"`, and styles.css has no `.btn` rule, so they rendered as native buttons.
  - Fix: the three buttons use `btn-secondary btn-compact`, the classes `#btn-test-subtitle` uses in the same tab. Delete keeps the neutral style: the only danger style (`.modal-footer .btn-primary.btn-danger`) is scoped to the modal footer, so no destructive style is reused outside it.
  - RED: `node test_frontend.js` -> 3 failures (each preset button still has `btn` and no `btn-secondary btn-compact`).
  - GREEN: `node test_frontend.js` -> 502 passed, 0 failed.
  - Other unstyled Style-tab classes (report only): `checkbox-label` (the "Usar fondo personalizado" label in index.html). It has no CSS rule, so it renders with browser defaults; not changed here, to keep this task to the buttons.

## Review round 1 (independent reviewer, code trace of 614149e..HEAD)

- Findings: S1, S2, S3, S5, S6 pass. Changes required: (1) MAJOR: a general-form refresh after an external config change overwrote an unsent Style pick; (2) MINOR: a successful style save left the earlier error text on screen; (3) MINOR: reset did not clear `styleDirty` or the pending style save timer.
- Fix (1): `generalFields` (runtime-settings.js) excludes style keys from the general dirty check, so a pending style edit is not an unsaved general edit. `handleExternalConfigChange` refreshes the form with the unsent style values kept (`styleDirty ? { ...config, ...styleFields(currentConfig) } : config`), and a style-only external change goes through `refreshStyleTab`, which skips the re-render while `styleDirty` is set.
- Fix (2): `saveSubtitleStyle` success path clears `#sub-style-status`.
- Fix (3): reset clears the pending timer and `styleDirty` before `renderStyleTab(defaults, { force: true })`.
- Test quality: the section 15 scenarios run the real `app.js` through the `loadAppHarness` vm-style harness (stubbed DOM, IPC and timers), not string guards only.
- RED: before the fixes, `node test_frontend.js` failed 5 checks: the MAJOR (c) external refresh, the success status clear, the reset dirty clear, the post-reset external style render, and the form-dirty precondition (which passed once `generalFields` landed).
- Caveat: the unsent-pick scenario (d) also passes once the general refresh is fixed, so it is an exercised guard rather than a fresh RED on its own.
- GREEN: `node test_frontend.js` -> 477 passed, 0 failed. `cargo test --workspace` exit 0. `cargo fmt --all -- --check` clean.

## Review round 2 (coordinator, after round 1 fixes)

- Findings: A (MINOR, confirmed regression): a successful style save cleared the status text, so the preset "saved", "loaded" and "deleted" messages vanished. B (MINOR, plausible): reset did not wait for a style save already queued in `configSaveQueue`, so the pre-reset style could be written after the defaults. C (MINOR, test robustness): `styleSaves().at(-1).args` lookups could throw a TypeError and abort the later scenarios.
- Fix A: `showSubStyleStatus` records whether the shown message is an error (`subStyleStatusIsError`); a successful style save clears only that error text. The flag is used instead of `classList.contains`, which the harness stubs to false.
- Fix B: reset clears the pending style timer, then runs `reset_config_defaults` and the populate inside `queueConfigSave`, so any save already queued writes before the reset.
- Fix C: the two lookups use `?.`, and the general-save scenario asserts its own `save_config` payload.
- New harness scenarios (section 15): preset save, load and delete through the real handlers with their messages checked after the save resolves; reset behind a held in-flight style save (no `reset_config_defaults` until the save returns, and the defaults are the last write); a general save after picking neon sends `subtitle_style: 'neon'` and keeps it.
- Reset and the theme: reset adopts the default (empty) `subtitle_theme`, and the Style controls show the default colour. The next style save sends the control-derived default theme. The empty theme is observable from the client, so it is asserted directly.
- RED: `node test_frontend.js` before the fixes -> 480 passed, 6 failed: the preset saved, loaded and deleted messages, reset waits for the in-flight save, the defaults-are-last-write check, and the reset-adopts-empty-theme check. The general-save scenario and the preset write checks passed before the fix; they are guards, not RED.
- GREEN: `node test_frontend.js` -> 486 passed, 0 failed. No Rust touched; `cargo fmt --all -- --check` is run as a check.
- Known limits: after a failed style save `styleDirty` stays set, so external style changes are not rendered until the next successful save. This is by design (see the comment at the failed-save catch in `saveSubtitleStyle`, app.js), because the unsent edits stay on screen and must not be overwritten.
