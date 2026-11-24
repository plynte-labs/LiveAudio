# Blacklist edge filter and prompt/blacklist switches

Branch: `feat/blacklist-edge-filter`, stacked on `feat/i18n-log-auto-threads` (aaea729). Worktree: `E:\LiveAudio-worktrees\ct2-pin`.

## Problem (verified)

- `is_text_blacklisted` (`desktop/src-tauri/src/commands.rs` ~3057) matches each blacklist phrase as a raw substring (`text_folded.contains(phrase)`). On any hit, the caller (~1627) drops the WHOLE transcription.
- The English preset (`desktop/src/app.js` ~276) contains `like` and `subscribe`. Any long sentence with "like", "unlikely" and so on disappears.
- The user's log showed 8 discarded utterances in one session, including 119-character ones.
- The Spanish default (`DEFAULT_BLACKLIST`, `crates/liveaudio-core/src/config/mod.rs` ~45) has `gracias` and `memos`, which also match inside real speech ("comemos").
- There is no way to turn the Whisper context prompt or the blacklist off without clearing the text.
- Parakeet ignores the context prompt: the worker's Parakeet path does not read `context_prompt`.

## Design (agreed with the user)

1. Match whole words only, on normalized text (case-folded, diacritics folded, punctuation ignored for matching).
2. If the entire utterance consists only of blacklist phrases (one or several, with punctuation and whitespace between them), drop it.
3. If a blacklist phrase sits at the START or END of the utterance, strip only that phrase, repeatedly, so stacked tails are handled. Clean up the dangling punctuation and whitespace left behind, and publish the rest.
4. If it appears in the middle, keep the text untouched.
5. Remove `like` from the English preset. Review both presets for other single common words that are unsafe even at an edge, and remove them. Keep multi-word hallucination phrases.
6. Add two switches in the Subtitles tab, next to the existing prompt and blacklist editors:
   - "Use context prompt" (`context_prompt_enabled`, default true). When off, no `context_prompt` is sent.
   - "Filter blacklisted phrases" (`blacklist_enabled`, default true). When off, no filtering happens.
   - The editors stay editable but look disabled or dimmed while their switch is off. Show a short hint that the prompt only affects Whisper models (Parakeet ignores it).

## Tasks

- [x] T1 Edge-strip blacklist filter: whole-word matching, whole-utterance drop, edge strip, middle untouched, and the preset cleanup. Apply it in the desktop path AND in the CLI path (`crates/liveaudio-cli/src/main.rs`) if the CLI filters too. Put the pure logic in one shared place (prefer `liveaudio-core`) instead of duplicating it. Route: delegated writer.
- [x] T2 The `context_prompt_enabled` and `blacklist_enabled` config fields, wiring (`asr_language_and_prompt`, the filter call sites, the CLI, the patch keys, MCP docs), the UI switches, and i18n for es and en. Route: delegated writer.

## Constraints

- TDD strict. Runners: `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets` (the warning set must equal the base), `node desktop/test_frontend.js`, `node --test tests-js/*.mjs`.
- Generated artifacts are in English. The CHANGELOG entry is in Spanish, under `[1.6.5]`.
- No AI attribution. No push. Never stage `gen/schemas`, binaries, `dist` or `*.wav`.
- Write "see git log" instead of commit hashes in this doc.
- About 400 lines per task is advisory only.

## Acceptance

- "I would like to show you something" and "hoy comemos tacos" are kept unchanged.
- "Gracias por ver." and "Subtítulos por amara.org" are dropped.
- "y eso es todo, gracias por ver el video" becomes "y eso es todo".
- With the blacklist switch off, nothing is filtered. With the prompt switch off, the worker receives no prompt.
- Log lines say whether an utterance was dropped or trimmed, without dumping the text.

## Progress / evidence

(filled per task)

### T1 (route: delegated writer, single)

- Shared filter in `crates/liveaudio-core/src/blacklist.rs` (`filter_blacklisted` -> `Keep` / `Trim` / `Drop`); desktop call sites use it. The CLI does not filter, so it needed no change.
- RED: `cargo test -p liveaudio-core blacklist` with a `Keep` stub: 5 of 7 failed (drop, trailing, leading, both edges, quotes).
- GREEN: same command, 7 passed. Workspace `cargo test --workspace --locked` all ok; clippy warning count equals base (118).
- Presets: removed `like` (EN), `gracias` and `memos` (ES). Kept multi-word phrases plus `suscríbete`/`subscribe`.
- The old `test_is_text_blacklisted_cases` was removed from the desktop tests; its cases live in the core tests.
- Commit: see git log.

### T2 (route: delegated writer, single)

- Config fields `context_prompt_enabled` / `blacklist_enabled` (default true) with `context_prompt()` and `filter_blacklist()` helpers; desktop (init payload, per-utterance prompt, filter sites) and CLI use them. Patch keys, restart-required key, `docs/MCP.md`, Advanced-tab switches (the prompt and blacklist editors live in the Advanced tab), i18n es/en, dirty-state view, types.
- RED: core test `prompt_and_blacklist_switches_default_on_and_gate_their_features` failed to compile (missing fields and methods); frontend asserts failed (6 FAIL) before the UI existed.
- GREEN: `cargo test --workspace --locked` ok, `node desktop/test_frontend.js` 952 passed, `node --test tests-js/*.mjs` 170 passed, clippy warnings 118 (same as base).
- Commit: see git log.
