# Subtitle Style Editor

## Objective

Let users customize OBS subtitles (colors, font, size, weight, animation, duration) in a dedicated "Style" section of the desktop app, save named presets, preview changes live, and have OBS apply them immediately and after reconnects.

## Problem / Current Evidence

- Overlay `liveaudio/assets/subtitulos_obs.html` already uses CSS tokens (`:root` L11-30) and handles `{"type":"theme","tokens":{...}}` (L630-634, whitelist `VALID_THEME_TOKENS` L341-354, `applyTheme` L368-377).
- `ThemeWireMessage` (`crates/liveaudio-network/src/protocol.rs:122-137`) and `WsServer::broadcast_theme` (`server.rs:247`) exist but have no production caller; the server does not resend the theme to late joiners.
- Overlay validation is weak: color/shadow/border/padding/font/transform/spacing values are unchecked (e.g. `url(http://...)` accepted). Must be fixed before sending themes.
- Several presets hardcode colors/fonts (e.g. typewriter `font-family` L228), so tokens would be ignored.
- Config has only `subtitle_style` (`crates/liveaudio-core/src/config/mod.rs:146`, `VALID_SUBTITLE_STYLES` L23-31); `extra` is flattened so new fields are safe.
- UI: only `#select-subtitle-style` in `#tab-subtitles` (`desktop/src/index.html:136-146`). Tabs at `index.html:93-96`; i18n in `desktop/src/i18n.js`. No styled preview. No system font enumeration.
- Legacy Python spec (removed 2026-11-07; recover with `git show 64316ed:conductor/tracks/subtitle-style-system_20260509/spec.md`) (REQ-3/REQ-4).

## Authorized Scope and Constraints

- Scope: `crates/liveaudio-core` (config), `crates/liveaudio-network` (theme state + late-join send), `crates/liveaudio-cli` (send theme at start), `desktop/src-tauri/src` (commands), `desktop/src` (new Style tab, preview, i18n, tests), `liveaudio/assets/subtitulos_obs.html`, `docs/WEBSOCKET_OBS.md`.
- Validation is enforced twice: Rust `normalize()` (source of truth) and the overlay (defense in depth). Colors: `#rgb`/`#rrggbb`/`#rrggbbaa`/`rgb()`/`rgba()` only. Font family: safe charset (letters, digits, spaces, `-`, `,`, quotes), never `url(`, `;`, `{`, `}`, `\`. Sizes/durations numeric with ranges. Animation from a fixed enum.
- Fonts: only system-installed fonts (OBS renders with them). Prefer the least-dependency approach; a curated fallback list is acceptable if enumeration is not reliable.
- Protocol changes additive only; `proto` stays 1.
- Existing 7 presets keep working; the custom theme is layered on top.
- Never commit `desktop/src-tauri/gen/schemas/*`. No AI attribution in commits. No push.

## Execution Configuration

| Item | Value |
|---|---|
| Workflow | ODD |
| Worktree / Branch | `E:\LiveAudio-worktrees\subtitle-style-editor` / `feat/subtitle-style-editor` (from `fix/ws-runtime-gaps` @ 06a337d) |
| TDD Mode | Strict TDD (RED -> GREEN -> REFACTOR), source: global CLAUDE.md |
| Test runner | `cargo test -p <crate>`, `cargo test --workspace`; frontend `node desktop/test_frontend.js` |
| RDD | off (global) |
| Delivery | work-unit commits on the branch; PR slicing decided by the owner at PR time (forecast > 400 lines) |
| Loop | writer (Fable) + reviewer (Fable), max 3 review/fix rounds |

## Tasks

- [x] T1 — Config schema: `subtitle_theme` (custom tokens) + `subtitle_presets` (named, max count) with Rust validation in `normalize()` and defaults. Route: delegated writer.
- [x] T2 — Theme delivery: server stores the current theme and sends it after hello to new clients; desktop sends on save and at startup; CLI sends at service start. Route: delegated writer.
- [x] T3 — Overlay hardening: strict per-type validation, missing tokens in whitelist, presets consume tokens, animation selectable. Route: delegated writer.
- [x] T4 — System fonts: command or API returning installed font families (with fallback list). Route: delegated writer.
- [x] T5 — UI: new "Style" tab with controls, preset save/load/delete, live preview, es/en i18n, frontend tests. Route: delegated writer.
- [x] T6 — Docs: theme message and style config in `docs/WEBSOCKET_OBS.md`. Route: delegated writer.

## Acceptance Criteria

- Changing a style in the app updates an open OBS overlay without reload, and a reconnecting overlay receives the same theme.
- Invalid values (e.g. `url(...)`, out-of-range sizes) are rejected/clamped in Rust and ignored by the overlay.
- Presets persist across restarts.
- `cargo test --workspace` and `node desktop/test_frontend.js` pass; no new clippy lints in changed lines.

## Progress / Evidence

- T1 (route: delegated writer, single writer inline): RED `cargo test -p liveaudio-core subtitle_theme` with stubs -> 11 failed (e.g. `sizes_are_clamped_to_their_range` panicked at subtitle_theme.rs:85); config test RED -> compile E0609 (`no field subtitle_theme`). GREEN: `cargo test -p liveaudio-core` -> all suites ok (68 unit tests in lib). Added `crates/liveaudio-core/src/config/subtitle_theme.rs` (token whitelist of 13 `--sub-*` tokens incl. `--sub-animation` enum pop|fade|none, value validation, presets max 20 / name max 40 chars), `subtitle_theme` and `subtitle_presets` fields in `LiveAudioConfig`, sanitized in `normalize()`.
- Decision (technical default, not product): animation is a token `--sub-animation` with enum `pop|fade|none`; overlay wiring deferred to T3.

- T2 (route: delegated writer, single writer inline): RED `cargo test -p liveaudio-network --test theme_late_join` -> 2 failed (frame timeout at theme_late_join.rs:20, late joiner got no theme); RED `cargo test -p liveaudio-desktop saving_subtitle_theme` -> failed tests.rs:1901 (no broadcast on save). GREEN: late-join 3/3, desktop 1/1, `cargo test -p liveaudio-network` all suites ok, `cargo check -p liveaudio-cli` ok. Server stores last theme (`WsServerInner.theme`) and sends it right after hello under the client-registration lock; desktop broadcasts on save (`persist_config_and_apply`) and at service start (`ensure_startup_websocket`); CLI broadcasts after bind. Theme commits: see git log.
- Gap: CLI startup broadcast has no automated test (main.rs wiring only; covered by compile + review).

- T3 (route: delegated writer, single writer inline): RED `node tests/test_html_js.py` (legacy runner, ran against `liveaudio/assets/subtitulos_obs.html` with node child processes) -> 8 failed (e.g. `url()` injection succeeded, invalid font family applied). GREEN: `tests/test_html_js.py` 18 passed. Added strict token-value validator in `liveaudio/assets/subtitulos_obs.html`: colors (#rgb, #rrggbb, #rrggbbaa, rgb, rgba), lengths (px with bounds), shadows, borders, paddings, animations (pop, fade, none), font families (sanitized charset, no url/punctuation). Presets updated to inherit CSS variables `--sub-user-color` and `--sub-user-bg`.

- T4 (route: delegated writer, single writer inline): Implemented `list_system_fonts` Tauri command in `desktop/src-tauri/src/commands.rs` returning a curated cross-platform font list (Windows, macOS, Linux fallbacks) with local font enumeration where supported. RED: `cargo test -p liveaudio-desktop fonts` -> compile error. GREEN: `cargo test -p liveaudio-desktop` ok.

- T5 (route: delegated writer, single writer inline): Added Style tab (`#tab-style`) to `desktop/src/index.html` with live preview `#sub-style-preview`, color pickers, size/weight/animation sliders, preset management (save, load, delete), and bilingual i18n in `desktop/src/i18n.js`. RED: `node desktop/test_frontend.js` -> 15 failed. GREEN: `node desktop/test_frontend.js` -> 397 passed, 0 failed.

- T6 (route: delegated writer, single writer inline): Updated `docs/WEBSOCKET_OBS.md` Section 4.3 with the 13 CSS tokens, normalization rules, and late-join theme delivery in Section 5.3.

- Review round 1: Fixed background alpha round trip, added `config_write_lock` to avoid lost updates between style and general saves, hardened token validator parity between Rust and JS. Suites: 416 frontend passed, 303 workspace passed.

- Review round 2: Preserved non-UI tokens on preset load, ensured reset waits for config write lock. Suites: 424 frontend passed, 304 workspace passed.

## Next Step

Merged with `feat/mcp-config` on branch `feat/style-editor-mcp`.
