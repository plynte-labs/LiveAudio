# UI polish and close guard — v1.5.1

Locator: `odd/tasks/ui-polish-1-5-1.md` · Engram mirror: `odd/ui-polish-1-5-1/tasks`
Branch: `develop` · TDD: strict (global config) · Runners: `node test_frontend.js` (in `desktop/`), `cargo test --workspace` (repo root)

## Objective
Fix the live-feed "Probar" button, polish visual/i18n defects seen in the desktop UI, ask before closing while a session runs, and ship as 1.5.1.

## Problem (observed, screenshot 2026-10-14 20:45)
- **Probar adds every test cue twice.** `send_test_subtitle` (`commands.rs:2136`) emits `subtitle_cue` to the feed listener, and `app.js:1423` also appends the returned cue.
- **Probar sends unrelated text.** It picks a random hardcoded marketing phrase (about "CUDA" or "sin retraso") instead of an obvious test line.
- **Probar never reaches OBS.** It does not use the WebSocket server.
- **Untranslated text in the Spanish UI.** The status pill shows "loading", and the startup status line reads "Starting the ASR worker and waiting for it to become ready...".
- **Closing the window does not ask** while an owned session is running.

## Scope decisions
- **Probar:** one localized, clearly labeled test cue per click. It shows once in the feed and goes to OBS when the owned WS server is running.
- **Close guard:** only for a session this window owns. Closing in Monitor Mode does not stop the foreign service, so it does not prompt.
- **Close modal:** reuse the existing modal design and component. No new visual language.
- **Version:** 1.5.1, in every place the version is declared.

## Tasks (route: delegated direct, one Fable writer — 2+ non-trivial files)
- [x] T1 Probar: one cue per click, localized test text, broadcast to OBS when the server runs. Tests first.
- [x] T2 Close confirmation modal when an owned session is running (reuse the existing modal). Tests first.
- [x] T3 Visual and i18n defects: localize the status pill and startup status line. Fix other defects found in the screenshot and code audit.
- [x] T4 Bump version to 1.5.1 everywhere it is declared, including the header badge.

## Checks
`node test_frontend.js` (desktop) · `cargo test --workspace` · `cargo fmt` on touched files only.

## Progress / evidence
- **T1 Probar** — commit `54ae5f6` — files: `desktop/src/app.js`, `desktop/src/i18n.js`, `desktop/src-tauri/src/commands.rs`, `desktop/src-tauri/src/tests.rs`, `desktop/test_frontend.js`.
  - RED: `node test_frontend.js` 5 failed (single-append, localized text, es/en keys); `cargo test -p liveaudio-desktop test_subtitle_cue_broadcasts` failed to compile (`test_subtitle_wire_message` missing).
  - GREEN: frontend 0 failed; new Rust test passes. The command emits `subtitle_cue` (the only feed path) and broadcasts through `WsServer::broadcast_subtitle` when the server runs.
  - Decision: the test line has no time suffix; the feed item timestamp already distinguishes clicks.
- **T2 Close confirmation** — commit `bd2e2d1` — files: `desktop/src/app.js`, `desktop/src/i18n.js`, `desktop/test_frontend.js`.
  - RED: 6 failed (onCloseRequested guard, Monitor Mode exclusion, re-entry, copy). GREEN: 0 failed.
  - Design: frontend `onCloseRequested` reuses `showConfirmModal`. Confirm re-enters `appWindow.close()`, so the existing `ExitRequested` stop runs once. No new capability was needed.
- **T3 Visual and i18n** — commit `be05433` — files: `desktop/src/app.js`, `desktop/src/i18n.js`, `desktop/src/index.html`, `desktop/src/styles.css`, `desktop/test_frontend.js`.
  - RED: 11 failed. GREEN: 0 failed.
  - Fixes: pill `loading` label (styled as starting); startup line localized by `starting_worker` phase; badge drops "Tauri 2"; blocked feed items dim only time and text, so the kebab keeps its opacity; startup progress block gets hint sizing and a full-width accent meter with a `[hidden]` override; the OBS URL field gets a full-URL tooltip.
- **T4 Version 1.5.1** — commit `7a583ec` — files: `Cargo.toml`, `Cargo.lock`, `README.md` (release title only), `desktop/src-tauri/tauri.conf.json`, `desktop/src/index.html`, `desktop/src/i18n.js`, `desktop/test_frontend.js`.
  - RED: 4 failed. GREEN: 0 failed. `cargo check` refreshed the 7 workspace crate versions in `Cargo.lock`.
  - Left as is: `desktop/src-tauri/hooks.nsh` "v1.5.0+" (legacy-install detection marker, not a declaration); `desktop/package.json` (`0.1.0`, not tied to the app version); README benchmark and feature-history lines.

### Checks
- `node test_frontend.js` (desktop): 342 passed, 0 failed.
- `cargo test --workspace`: 275 passed, 0 failed (baseline 274 plus the T1 test).
- `git status --short`: only the `gen/schemas` noise remains uncommitted.
- rustfmt: not applied to whole files. `commands.rs` and `tests.rs` carry pre-existing drift outside the touched hunks, and the new code was checked to be clean.

### Open questions
- The startup line now comes from the frontend phase mapping. The backend still emits its English text, which the UI no longer shows.
- `desktop/package.json` version (0.1.0) and the README header were left unchanged. Confirm whether the package version should follow the app.
