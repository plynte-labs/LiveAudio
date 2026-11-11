# Subtitle latency and session export

Locator: `odd/tasks/subtitle-latency-and-export.md` · Engram mirror: `odd/subtitle-latency-and-export/tasks`

## Objective
Make saved sessions importable in DaVinci Resolve (SRT export, valid WebVTT) and reduce perceived subtitle delay (shorter phrase segmentation defaults, partial subtitles). Then continue with `odd/tasks/hybrid-asr-backends.md` H5.

## Why
- Perceived delay is dominated by phrase segmentation, not inference: phrase length (up to `max_chunk_duration` 5 s) + `silence_timeout` 0.8 s + `vad_speech_pad_ms` 0.2 s + inference (0.3-1.2 s DML).
- `crates/liveaudio-core/src/sink/webvtt.rs:86` writes a non-standard `#cue:N` block after every cue (risk for editors). No SRT output exists; SRT is the most portable subtitle format for NLEs.

## Decisions (user, 2026-11-07)
- Defaults: `silence_timeout` 0.8 -> 0.5 s, `max_chunk_duration` (subtitles purpose) 5.0 -> 3.0 s. Existing users' saved values are not migrated.
- Partial subtitles: new feature. Lead default (pending H5): opt-in config flag, off by default; H5 may enable it automatically on the NVIDIA engine.
- SRT is written next to `subtitles.vtt` whenever VTT saving is enabled (no new toggle).

## Constraints
- TDD strict (global user config). Runners: `cargo test --workspace --locked`, `node --test tests-js/*.mjs`, `node desktop/test_frontend.js`.
- No commits (AGENTS.md: explicit approval). Subagents sonnet/haiku only.
- Session audio/transcripts are personal data: never upload.

## Tasks
- [x] L1 — Session export: SRT sink (`subtitles.srt`, sequential indices 1..N, `HH:MM:SS,mmm`, resume continues numbering) and remove the `#cue:N` block from VTT while keeping resume cue-counter recovery working. Route: delegated writer (sonnet; 2+ files).
- [x] L2 — Defaults `silence_timeout` 0.5, `max_chunk_duration` 3.0 (core config, desktop UI defaults/presets, docs, tests). Route: same writer after L1.
- [x] L3 — Partial subtitles (implemented; live end-to-end not yet observed). Route: read-only mapper (haiku, done) for design, then delegated writer (sonnet) AFTER the L1/L2 writer finishes (both touch `desktop/src-tauri/src/commands.rs`; one writer at a time).
  - Design (lead, from mapper map 2026-11-07):
    - Flag `live_partial_subtitles_enabled` (default false, restart-required, template `acoustic_events_enabled`).
    - Desktop live loop only (`commands.rs`, VadTransition::None arm). The CLI loop copy ignores the flag (documented).
    - Every ~0.5 s of in-utterance audio, snapshot `utterance_samples` and send a partial transcribe via non-blocking try_send. At most one partial in flight; never stall VAD.
    - Worker protocol: `partial: bool` (serde default false) on the transcribe payload and the result. The worker skips a partial when more commands are already queued. Finals wait at most one in-flight partial decode.
    - Partials never reach the jsonl/vtt/srt sinks, `previous_transcript`, or the replay/retry buffers.
    - New typed live-only WS frame `subtitle_partial` with no `text` key (acoustic-event precedent). `SubtitleWireMessage` stays unchanged (golden test).
    - Overlay: a provisional DOM node outside pendingQueue. Each partial overwrites it, a final for the same utterance clears it, and resume resets it.
    - Desktop preview: a dedicated provisional line, not a feed item (dedupe rules).
    - Docs: `docs/WEBSOCKET_OBS.md`, `docs/MCP.md`.
- [x] L3b — Overlay: provisional line renders in the same slot the final will occupy (user decision 2026-11-07). Route: delegated writer (sonnet).
- [ ] L4 — Then start `hybrid-asr-backends` H5.

## Progress / evidence
- L1 (sonnet, 2026-11-07): generic CueFileSink (webvtt/srt constructors, shared bounded async writer) in crates/liveaudio-core/src/sink/webvtt.rs; format_srt_timestamp; VTT has no #cue block; last_cue_counter reads identifiers followed by a ' --> ' line plus legacy #cue markers; cue text drops blank lines and escapes '-->'. commands.rs writes subtitles.vtt + subtitles.srt when save_vtt_enabled. SRT numbers 1..N and resumes after the last block (may differ from VTT ids). cue_counter is a plain counter, not ms. RED: E0433 CueFileSink undeclared; GREEN: cargo test -p liveaudio-core webvtt 6 passed (lead re-ran: 6 passed). Docs: GETTING_STARTED, README.
- L2 (sonnet): defaults silence_timeout 0.5, max_chunk_duration 3.0 (core config, app.js defaults + balanced preset, index.html sliders, commands.rs balanced preset, test_frontend fixture, config.json.example, GETTING_STARTED). Balanced preset silence 0.8 -> 0.5 so a fresh config matches its default profile (lead accepted). RED: left 0.8 right 0.5 / left 5.0 right 3.0; GREEN.
- Checks L1+L2: cargo test --workspace --locked all ok; node --test tests-js 135/0; node desktop/test_frontend.js 568/0. Line endings consistent CRLF (lead checked). Clippy not run (pre-existing lints). Commit: pending approval.
- L3 (sonnet, 2026-11-07): flag live_partial_subtitles_enabled (core, patch allowlists, Advanced tab UI, i18n es/en, types); ProcessSupervisor::try_send_command; worker/IPC partial bool (finals unchanged on wire), worker skips partial when cmd queue non-empty and emits empty partial, watchdog id <id>:partial; WsServer::broadcast_subtitle_partial live-only frame {type:subtitle_partial,utterance_id,partial_text} (empty = clear); PartialSubtitleGate + partial_due (8192 samples) in commands.rs; overlay #partial-container; desktop #feed-partial-line; docs WEBSOCKET_OBS 4.6, MCP, GETTING_STARTED. CLI loop ignores the flag. RED/GREEN per behavior observed by writer. Checks: cargo test --workspace ok; tests-js 137/0; frontend 575/0; cargo build -p liveaudio-desktop ok. Lead re-ran: tests-js 137/0, subtitle_partial_broadcast 1 passed, asr-worker 30 passed. NOT run: live model end-to-end, worker skip rule (untested). ~750 authored lines (over heuristic, not split). Open: overlay provisional line position (above finals, jumps when final lands) — user decision. Commit: pending approval.
- L3b (sonnet, 2026-11-07): #partial-container removed; partial is a normal .sub-box in #subtitle-container (single: takes over slot, releases hide/cleanup timers and isShowing gate; ribbon: newest line). Final removes the partial box and lands in place, no fade flicker. RED: partialSubtitle/partialRibbon not ok; GREEN tests-js 139/0 (lead re-ran 139/0), frontend 575/0. Known: a final of an older utterance can displace a newer partial until its next update (utt_N compare dropped; ids restart on app restart); ribbon may show one extra line briefly (partial not counted in ribbonMaxLines). Live OBS not yet observed.
