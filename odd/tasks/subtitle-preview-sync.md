# Subtitle editor preview sync

Locator: `odd/tasks/subtitle-preview-sync.md` · Engram mirror: `odd/subtitle-preview-sync/tasks`

## Objective
The desktop subtitle style editor preview must render exactly what OBS renders for every style and setting (user report 2026-11-08: after choosing a style, changing color or spacing applies in OBS but not in the preview; the editor misleads the user).

## Context
- OBS overlay: `assets/subtitulos_obs.html` (served at `/overlay`). Desktop preview uses its own CSS copy (`desktop/src/styles.css` ~:934) and JS in `desktop/src/app.js` (known drift source since the 2026-11-07 OBS spacing review).

## Constraints
- TDD strict (`node desktop/test_frontend.js`, `node --test tests-js/*.mjs`). No commits. English artifacts.

## Tasks
- [x] P1 — Map every style token/setting applied by the overlay vs the preview, find why style presets override user color/spacing in the preview, and make the preview use the same rules (prefer a single shared source over duplicated CSS). Route: delegated writer (sonnet).

## Progress / evidence
- P1 (sonnet, 2026-11-08): root cause = preview had its own drifted copy of style CSS (no --sub-user-* overrides, hardcoded preset colors/bg/padding, no data-anim/data-font, double background). Fix: overlay is single source; SUBTITLE-STYLE:BEGIN/END regions in assets/subtitulos_obs.html; packaging/sync_overlay_preview_css.mjs generates desktop/src/overlay-preview.css scoped under .sub-style-preview (--check mode); subtitle-style.js previewProperties(theme) mirrors overlay applyTheme (verified in vm for 10 themes); app.js renderSubtitlePreview uses sub-box visible style-X; styles.css copies removed. RED 14 failed -> GREEN 14/14; frontend 662/0; tests-js 153/0 (lead re-ran + --check ok); network tests ok. Gaps: visual check by user pending; preview has no per-word stagger/ribbon/exit; words replay entrance animation on each control change.
