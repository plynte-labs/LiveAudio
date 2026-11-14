# About, support and usage statistics

Locator: `odd/tasks/about-support-stats.md` · Engram mirror: `odd/about-support-stats/tasks`

## Objective
Add a header button that opens a designed "About" modal with local usage statistics and support actions: report a problem (GitHub issues), star the repo (https://github.com/plynte-labs/LiveAudio), sponsor (https://github.com/sponsors/FranGuh/). No emojis; icons from a lightweight library (Lucide, ISC, vendored as inline SVG — no bundler/npm in desktop/).

## Decisions (user, 2026-11-08)
- Placement: header icon button next to the version badge, opening a modal (not a new tab).
- Visual design required; no emojis; lightweight icon library.

## Constraints (lead)
- Stats are local only: integers and dates in `<data home>\stats.json` (LIVEAUDIO_HOME aware), never transcript text, no network. Atomic write (temp + rename). Counted at the accepted-final point (commands.rs ~1533) so they work with saving off; flush on stop and app exit.
- External links: one fixed-URL allowlist in Rust (pattern: open_author_profile / is_nvidia_license_url); no generic open_url.
- Issue link: `issues/new/choose` or `issues/new?template=bug_report.yml`; copy safe environment info (version, OS/arch, ASR backend, GPU name — no paths, no username) to the clipboard. Body prefill unverified, not used.
- Lucide ISC notice in THIRD_PARTY_NOTICES.md. Version badge hard-coded v1.6.0 stays as is.
- TDD strict: `cargo test --workspace --locked`, `node desktop/test_frontend.js`, `node --test tests-js/*.mjs`. No commits. i18n es + en.

## Tasks
- [x] A1 — Backend: usage stats store + `get_usage_stats` command + fixed-URL support link command + safe environment summary. Route: delegated writer (sonnet; 2+ non-trivial files).
- [x] A2 — Frontend: header button, About modal (stats + support actions + version/license line), Lucide icons, i18n, a11y (focus, Esc, aria), tests. Route: same writer after A1.

- [x] A3 — Replace pre-existing emojis (user, 2026-11-08): confirm-modal icons -> whitelisted Lucide keys (SVG built from hardcoded markup, never interpolated); drop emojis from tracing/eprintln log strings (commands.rs, logging.rs, liveaudio-cli daemon lines) and the browser mock log fixture. Keep the U+1F399 unicode-path test dir in liveaudio-cli (it tests unicode paths). Route: haiku explorer (done) then haiku writer; then haiku release build.

## Progress / evidence
- A1+A2 (sonnet, 2026-11-08): liveaudio-core usage_stats.rs (UsageStats load/zero-fallback, UsageTracker throttled 60 s atomic write, flush skips unchanged); desktop commands open_support_link (allowlist issues/repo/sponsor), get_usage_stats, get_support_environment (version, OS/arch, backend, GPU names; no paths); record_subtitle after log_transcription_metadata, record_session at session setup, flush in shutdown_service_resources (stop + exit). Header #btn-about + About modal (house modal pattern, focus trap, Esc/backdrop), Lucide info/bug/star/heart from lucide-static 0.468.0, ISC notice, i18n es/en. RED: E0433 UsageStats; desktop --no-run 5 errors; frontend missing About keys. GREEN: workspace tests 0 failed (desktop 107, core 137); frontend 709/0 (lead re-ran 709/0); tests-js 153/0; cargo build -p liveaudio-desktop ok. Lead verified allowlist (commands.rs:2485) and no emoji added by this feature. Not verified: real-window layout, Tab trap, rundll32 URL opening, clipboard, stats flush via Tauri stop path, contrast. Pre-existing emojis remain in app.js toasts/modals (225-226, 1459, 2296, 2345, 2808) and index.html:649.
- A3 (haiku, 2026-11-08): app.js MODAL_ICONS (warning/refresh/download -> hardcoded Lucide 0.468.0 triangle-alert/refresh-cw/download; innerHTML only from the map, Object.hasOwn fallback to warning); callers restart/reset/NVIDIA use keys; index.html #modal-icon placeholder SVG; .modal-icon svg 22px (inherits text color). Emojis removed from 15 tracing strings in commands.rs, 2 in logging.rs, 9 in liveaudio-cli daemon lines, mock log fixture; cli unicode test dir kept. Notices list the 7 Lucide icons. RED 6 failed -> GREEN frontend 716/0 (lead re-ran 716/0), tests-js 153/0, workspace tests ok. Lead checked: EOL clean (i/lf w/crlf, no CR churn). Not verified visually.
