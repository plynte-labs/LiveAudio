# MCP Server for Agent Configuration (opt-in)

## Objective

Let an AI agent (Claude Desktop, Claude Code, other MCP clients) read and change LiveAudio settings when the user explicitly opts in, with changes applied to the running desktop app.

## Problem / Current Evidence

- No MCP code or dependency exists. `liveaudio-cli` uses ad-hoc `args.iter().any(...)` parsing (`crates/liveaudio-cli/src/main.rs:648-725`).
- No control channel into a running instance: `crates/liveaudio-ipc` `DaemonCommand` is unused; WS ignores inbound data (`server.rs:725`).
- Desktop loads config once (`desktop/src-tauri/src/lib.rs:40`) and keeps it in `state.config`; `save_config` (`commands.rs:316`) -> `persist_config_and_apply` (`commands.rs:345`). An external edit to `config.json` is not seen and is clobbered by the next UI save.
- Config: `crates/liveaudio-core/src/config/mod.rs` (fields L127-242, `normalize()` L389-598, lock `acquire_lock` L615, atomic `save_to_path`). `output_dir` and `diagnostics_export_dir` accept any path; `extra` accepts arbitrary keys.
- Privacy policy precedent: `scrub_json_value` / `SCRUBBED_KEYS` in `crates/liveaudio-ipc/src/protocol.rs`.

## Design (decided by parent; writer may refine details, not the shape)

- Transport: stdio MCP server as a subcommand `liveaudio-cli mcp`. No network listener, no token: filesystem permissions are the trust boundary.
- Writes go to `config.json` through the existing lock + `normalize()` + atomic save, restricted by an explicit field allowlist.
- The running desktop app detects external `config.json` changes (watch or mtime poll), reloads into `state.config`, applies live settings, and notifies the frontend to refresh; settings that need a restart are reported, not silently applied.
- Opt-in: new config flag `mcp_enabled` (default false), only changeable from the desktop UI (never via MCP). When false, every tool returns a clear "disabled; enable it in LiveAudio settings" error.
- MCP SDK: prefer the official Rust SDK (`rmcp`) pinned to an exact version at least two weeks old; verify current API with context7/docs. A minimal hand-rolled JSON-RPC stdio loop is acceptable only if the SDK is unsuitable — explain why.

## Authorized Scope and Constraints

- Scope: `crates/liveaudio-core` (allowlisted patch API, `mcp_enabled`), `crates/liveaudio-cli` (`mcp` subcommand), `desktop/src-tauri/src` (external-change reload), `desktop/src` (opt-in toggle in Advanced tab, i18n es/en, frontend tests), new `docs/MCP.md`.
- Allowlist excludes: `output_dir`, `diagnostics_export_dir`, `mcp_enabled`, `extra`/unknown keys, `last_update_check`, and anything path-like. Unknown keys are rejected, not ignored.
- No tool exposes transcripts, audio, or subtitle text. Read tools return only allowlisted fields.
- Tools (minimum): `get_config`, `get_config_schema` (allowed fields, types, ranges/enums), `set_config` (partial patch; returns applied values and whether a restart is needed), `get_status` (desktop/service running via existing service lock; no content), `list_audio_devices` if cheap via existing code.
- Never commit `desktop/src-tauri/gen/schemas/*`. No AI attribution in commits. No push.

## Execution Configuration

| Item | Value |
|---|---|
| Workflow | ODD |
| Worktree / Branch | `E:\LiveAudio-worktrees\mcp-config` / `feat/mcp-config` (from `fix/ws-runtime-gaps` @ 06a337d) |
| TDD Mode | Strict TDD (RED -> GREEN -> REFACTOR), source: global CLAUDE.md |
| Test runner | `cargo test -p <crate>`, `cargo test --workspace`; frontend `node desktop/test_frontend.js` |
| RDD | off (global) |
| Delivery | work-unit commits on the branch; PR slicing decided by the owner at PR time (forecast > 400 lines) |
| Loop | writer (Fable) + reviewer (Fable), max 3 review/fix rounds |

## Tasks

- [x] T1 — Core: `mcp_enabled` flag + allowlisted, validated config patch API (lock + normalize + atomic save) + schema description. Route: delegated writer.
- [x] T2 — Desktop: detect external `config.json` changes, reload `state.config`, apply live settings, emit frontend refresh; no clobber. Route: delegated writer.
- [x] T3 — CLI: `liveaudio-cli mcp` stdio server with the tools above, gated by `mcp_enabled`. Route: delegated writer.
- [x] T4 — UI: opt-in toggle (Advanced tab) with short explanation, es/en i18n, frontend tests. Route: delegated writer.
- [x] T5 — Docs: `docs/MCP.md` (enable, client config snippets for Claude Desktop / Claude Code pointing at the installed `liveaudio-cli`, tool list, security model). Route: delegated writer.

## Acceptance Criteria

- With `mcp_enabled=false`, all tools refuse; the flag cannot be changed via MCP.
- `set_config` with a disallowed or unknown key is rejected; out-of-range values are clamped/rejected by `normalize()`.
- A change made via MCP appears in the running desktop app without restart (live settings) and is not overwritten by the next UI save.
- An MCP client can complete `initialize` + `tools/list` + `tools/call` over stdio (integration test).
- `cargo test --workspace` and `node desktop/test_frontend.js` pass; no new clippy lints in changed lines.

## Progress / Evidence

- Route: delegated writer (single writer, Fable). Strict TDD.
- T1 RED 1: `cargo test -p liveaudio-core patch` -> `error[E0609]: no field 'mcp_enabled' on type 'config::LiveAudioConfig'`.
- T1 RED 2 (stub `apply_patch`): 8 failed, panic `not implemented` at config/patch.rs (stub body).
- T1 RED 3 (schema test, stub `config_schema`): `schema_describes_every_patchable_key_and_no_restricted_one` panicked.
- T1 GREEN: `cargo test -p liveaudio-core` -> 66 + 1 + 8 + 12 + 1 passed, 0 failed. Patch module: 11 tests.
- Allowlist decision (not in the doc, inferred): `audio_device`, `selected_profile_id`, `profile_mode` excluded (device object and UI profile state). Reported as open gap.
- SDK decision (T3): `rmcp` 3.5.1 is 10 days old (fails the two-week rule); 3.5.0 (2026-09-28) is the newest eligible. Its 3.x `ServerHandler` surface targets the new 2026-07-28 draft (MRTR `CallToolResponse`, tasks, discover) and the published crate ships no examples, so the three-method surface would depend on churning APIs. Fallback chosen: hand-rolled JSON-RPC 2.0 stdio loop (`crates/liveaudio-cli/src/mcp.rs`), zero new crates. Only the tokio `io-std` feature was added. Negotiates `2025-11-25` / `2025-06-18`.
- T3 RED: `cargo test -p liveaudio-cli mcp` -> 8 failed (stub `handle_message` returned None; panics at mcp.rs unwrap sites).
- T3 GREEN: `cargo test -p liveaudio-cli mcp` -> 8 passed. Integration `cargo test -p liveaudio-cli --test mcp_stdio` -> 2 passed (initialize, tools/list, get_config, set_config allowed + rejected key, disabled refusal, config bytes unchanged). Honesty note: the integration test was written after the unit GREEN, so it was not observed RED.
- T2 design: the UI sends its form baseline as `base` on every `save_config`; the backend merges three-way (`save_ui_changes`: fields the form changed win, all others keep the on-disk value) under the config lock and the runtime lock. Desktop watcher polls `config.json` mtime each second, parses on change under the runtime lock, adopts only real differences (own saves produce no event), and emits `config_changed` with changed keys and `restart_required`. Frontend refreshes a clean form and keeps dirty edits (`externalConfigUpdate`).
- T2 RED/GREEN (core): `save_ui_changes` 2 tests RED on stub (2 failed, `not implemented`), GREEN 13 patch tests pass.
- T2 RED/GREEN (desktop): `cargo test -p liveaudio-desktop config_sync` RED 4 failed (stub panics), GREEN 4 passed; full `cargo test -p liveaudio-desktop` -> 72 passed, 0 failed.
- T2 RED/GREEN (frontend): `node desktop/test_frontend.js` RED = module load failure (`externalConfigUpdate` not exported); GREEN 350 passed, 0 failed (345 before + 5 new).
- T2 gap: the watcher loop itself (tokio interval + Tauri emit) has no automated test; covered by the pure `adopt_disk_config` and the save path. Needs a manual run against the real app.
- T4 RED: `node desktop/test_frontend.js` -> 3 FAIL (MCP switch id, i18n keys, form read/populate), 350 passed.
- T4 GREEN: `node desktop/test_frontend.js` -> 353 passed, 0 failed. Switch `#check-mcp-enabled` in Advanced tab, es/en `lbl_mcp_enabled` / `hint_mcp_enabled`; saved through the normal Save button (three-way merge path).
- T5: `docs/MCP.md` written (enable, Claude Desktop and Claude Code snippets, tools, allowlist, security model, troubleshooting). Check: every `MCP_PATCHABLE_KEYS` entry appears in the doc (31/31, scripted). Docs carry no test; accuracy checked against the code.
- Restart classification reuses the desktop `RUNTIME_SETTINGS` list (desktop/src/runtime-settings.js) as `RESTART_REQUIRED_KEYS`.

## Final verification (writer)

- `cargo test --workspace`: 306 passed, 0 failed.
- `node desktop/test_frontend.js`: 353 passed, 0 failed.
- `cargo clippy -p liveaudio-core|liveaudio-cli|liveaudio-desktop --all-targets -- -A clippy::not_unsafe_ptr_arg_deref` (the pre-existing deny-level error in `job_object/mod.rs:73` blocks clippy otherwise): exit 0; reported warnings are on lines not touched by this feature.
- Commits: fee4f38 (T1 core), 7d9fcca (T3 CLI, committed before T2), cf1c0f9 (T2 desktop reload), b4f7a55 (T4 UI toggle), 73d5cb0 (T5 docs).

## Open gaps

- Allowlist beyond the doc's explicit exclusions: `audio_device`, `selected_profile_id`, `profile_mode` are excluded (inferred, not specified). Everything else in the UI-visible schema is allowed.
- Restart classification reuses the frontend `RUNTIME_SETTINGS` list (duplicated in `RESTART_REQUIRED_KEYS`); the two must be kept in sync.
- The desktop watcher loop has no automated test; needs a manual check in the running app.
- Follow-up (after merge with feat/subtitle-style-editor): `subtitle_theme`, `subtitle_presets` still to add.

## Next Step

Review by the owner; no push performed.

## Follow-up (after merge with feat/subtitle-style-editor)

- Add the new style fields (`subtitle_theme`, `subtitle_presets`) to the MCP allowlist with the same validation.

## Review round 1

Coordinator review returned CHANGES REQUIRED with four items. Same rules: strict TDD, explicit-path staging, no `gen/schemas`, no attribution, no push.

- Item 1 (MAJOR, save path): `save_config` returns the saved config and the three save handlers repopulate the form from it, so the baseline is no longer taken from form values.
  - RED: `node desktop/test_frontend.js` -> 3 FAIL (blacklist/prompt repopulate, no baseline from form, runtime save uses the backend config), 353 passed. The old assertion on `savedConfig = structuredClone(newConfig)` was replaced by the `populateForm(saved)` check.
  - GREEN: 357 passed, 0 failed. `cargo test -p liveaudio-desktop config_sync` -> 4 passed (the saved value after an external edit equals the disk value).
  - Honesty note: the Rust signature change (`Result<LiveAudioConfig, String>`) was not RED-observed on its own; it is a type-level change covered by the config_sync tests.
  - Docs: `docs/MCP.md` section 5 states that form saves keep agent edits and that the form shows the saved values.
  - Commit: fd426d0.
- Item 2 (stdin bounding): lines over 1 MiB are discarded up to their newline and answered with an invalid request error; invalid UTF-8 gets a parse error; the server keeps reading.
  - RED: `cargo test -p liveaudio-cli --test mcp_stdio` on the old reader -> `oversized_and_invalid_utf8_lines_do_not_stop_the_server` panicked (BrokenPipe).
  - GREEN: `cargo test -p liveaudio-cli` -> 25 unit + 3 integration passed. Honesty note: the two unit tests (`bounded_reader_splits_lines_and_discards_oversized_ones`, `invalid_utf8_gets_a_parse_error_and_the_server_keeps_going`) were not observed RED separately.
  - Docs: 4096-character text cap and 1 MiB line limit documented in `docs/MCP.md` section 3.
  - Commits: 295e2e6 (CLI fix), 34544be (docs).
- Item 3 (guard, MINOR): `desktop/test_frontend.js` asserts that `RESTART_REQUIRED_KEYS` (patch.rs) equals `RUNTIME_SETTINGS` (runtime-settings.js) minus `audio_device` and `output_dir`.
  - Passes on arrival (357 passed). RED demonstrated by removing `'device'` from the JS list: `[FAIL] RESTART_REQUIRED_KEYS mirrors RUNTIME_SETTINGS (only in Rust: [device], only in JS: [])`. The file was restored from git.
  - Commit: 465bcab.
- Item 4 (MINOR, docs): `docs/MCP.md` section 3 now separates serde type rejection (`ws_port: 70000`, negative or fractional `vad_speech_pad_ms`, negative `cpu_threads`) from in-type range clamping (`silence_timeout: 9.0` stored as `2.0`). Docs only, no test.
  - Commit: 5a64307.
- Schema language `null`: the RED for `schema_language_admits_null_like_the_patch_api` was masked by a compile error in the same round; the fix (commit 8ad6739) was verified GREEN in the core test run.
- Watcher loop: still manual-only (no automated test); needs a check in the running app.

Verification after round 1:
- `cargo test --workspace`: 311 passed, 0 failed (was 306).
- `node desktop/test_frontend.js`: 357 passed, 0 failed (was 353).

## Review round 2

Coordinator review: APPROVE, all four round-1 findings resolved. One MINOR (A) in the final round.

- A (regression from fd426d0): the blacklist and prompt saves repopulated the whole form after the IPC round trip, which can wait up to the config lock timeout. Keystrokes typed in other fields during that wait were overwritten.
  - Fix: `mergeSavedConfigIntoForm(sent, formNow, saved)` (desktop/src/runtime-settings.js) takes the backend value only for fields the user has not changed since the save was sent. `adoptSavedConfig` in app.js applies it and sets the saved baseline. The blacklist, prompt, and runtime save paths all use it.
  - RED: `node desktop/test_frontend.js` -> 5 FAIL (helper not exported; source assertions for `adoptSavedConfig` not met; `populateForm(saved)` still present), 354 passed.
  - GREEN: 361 passed, 0 failed. The behavioral check covers an in-flight edit (kept) next to an untouched field (backend value), and the no-edit case (exact backend config).
  - Commits: 12d49a2 (fix), e901ba2 (docs/MCP.md section 5).

Verification after round 2:
- `cargo test --workspace`: 311 passed, 0 failed.
- `node test_frontend.js` (from desktop/): 361 passed, 0 failed.
- git status --short: only desktop/src-tauri/gen/schemas/{capabilities,desktop-schema,windows-schema}.json modified, not staged.

## Integration (feat/style-editor-mcp: feat/mcp-config merged with feat/subtitle-style-editor)

- Merge: `75c30a1` (`merge: integrate subtitle style editor with MCP config`). Conflicts were in `desktop/src-tauri/src/commands.rs` and `desktop/src/app.js`. Both sides were kept: the MCP save path (three-way merge, returned config) and the style branch's `config_write_lock`, `keep_subtitle_style`, and style helpers. Verified at merge time: `cargo test --workspace` 336 passed, `node test_frontend.js` 440 passed.
- Item 1, ownership and lock: the settings form never diffs or overwrites `subtitle_theme` / `subtitle_presets`. `save_config` takes `config_write_lock` before the base/disk read.
  - RED: `cargo test -p liveaudio-desktop form_save_never_changes_the_saved_subtitle_style` -> `left: {"--sub-color": "#00ff00"}`, `right: {"--sub-color": "#ff0000"}`.
  - GREEN: `config_sync` 5 passed. Commit `7f11cea`.
- Item 2, style save adopts the returned config: `save_subtitle_style` already returned the saved config. The frontend now calls `adoptSavedConfig` on it.
  - RED: `node test_frontend.js` -> 2 FAIL (style save adoption, external refresh of the Style tab).
  - GREEN: 442 passed. Commit `4c1ccad`.
- Item 3, watcher and OBS: `apply_external_config` loads and adopts `config.json` under `config_write_lock`. A changed subtitle theme is broadcast to the running OBS server. The frontend refreshes the Style tab on `subtitle_theme` / `subtitle_presets` changes, even with unsaved general edits.
  - RED: `external_adoption_waits_for_the_write_lock` -> the adoption finished while the lock was held (`config_sync.rs`, the `!adopt.is_finished()` assertion). `external_theme_change_is_pushed_to_the_running_obs_server` -> `left: 0`, `right: 1`.
  - GREEN: `config_sync` 7 passed. Rust in `e71db15`, frontend in `4c1ccad`.
- Item 4, MCP allowlist: `subtitle_theme` and `subtitle_presets` are patchable through `set_config`. They use the same type check and `normalize()` path, and neither requires a restart. `get_config_schema` names the 13 tokens and the preset limits (20 presets, 40-character names).
  - RED: `cargo test -p liveaudio-core patch` -> 3 failed (`subtitle_style_is_patchable_without_restart_and_described_in_the_schema`, `patch_stores_the_subtitle_style_after_validation`, `patch_rejects_a_subtitle_theme_of_the_wrong_json_type_without_writing`).
  - GREEN: `cargo test -p liveaudio-core` all suites passed. Commit `630c1af`, with `docs/MCP.md` updated.
- Open: the watcher's OBS push and the Style-tab refresh have no end-to-end test in a running app (manual check still needed).

Final verification (integration): `cargo test --workspace` 342 passed, 0 failed. `node test_frontend.js` (from desktop/) 442 passed, 0 failed.

## Integration review round 1

Reviewer: CHANGES REQUIRED (MAJOR 1, MAJOR 2, MINOR 3, MINOR 4). The reviewer's frontend count of 345 came from running in the wrong directory; the parent re-ran it at 442/0.

- MAJOR 1, form save adopted a changed theme without broadcasting. Fix: `push_theme_if_changed` in `config_sync.rs` is the single place that pushes a subtitle theme to OBS. `replace_runtime_config` and the watcher call it, so form saves, style saves, reset, and external edits all go through it.
  - RED: `form_save_pushes_the_theme_it_adopts_from_disk_to_obs` -> `left: 0`, `right: 1` (no broadcast).
  - GREEN: with the shared helper, the test passes, and the watcher and style tests still pass. Commit `549e251`.
- MAJOR 2, style save and reset overwrote the file from stale memory. Fix: `save_style_fields` merges the style into the file on disk (base = stored, next = stored with the new theme and presets), so only the theme and presets change. Reset stays a full overwrite on purpose, documented in `commands.rs`, and goes through `replace_runtime_config`.
  - RED: `style_save_keeps_general_changes_made_on_disk` -> `left: 9000`, `right: 9100` (the disk change was lost).
  - GREEN: the test passes. Commit `549e251`.
- MINOR 3, tests for out-of-band disk changes: the two RED tests above (`form_save_pushes_the_theme_it_adopts_from_disk_to_obs`, `style_save_keeps_general_changes_made_on_disk`).
- MINOR 4, Save button bypassed the queue. Fix: the click handler is wrapped in `queueConfigSave`.
  - RED: `node test_frontend.js` -> `[FAIL] Save button routes its save through queueConfigSave`, 442 passed.
  - GREEN: 443 passed. Commit `1fbfeeb`.

Verification (integration review round 1):
- `cargo test --workspace`: 344 passed, 0 failed.
- `node test_frontend.js` (from desktop/): 443 passed, 0 failed.

## Integration review round 2

Reviewer: APPROVE, with three items in this final round.

- Item 1, explicit style save within the watcher window: accepted by the parent as "explicit user action wins". No code change. Documented in `docs/MCP.md` section 5.
- Item 2, flaky fixed test port: the OBS test servers now bind an OS-assigned free port (`running_obs` in `config_sync.rs`). The external-change test uses the same helper.
- Item 3, test gap:
  - Dedup: `saving_an_unchanged_theme_does_not_broadcast_again` passes with the check in place. RED by disabling the check (`if false && ...`): `left: 1, right: 0`. The check was restored. Commit `890526a`.
  - Reset: `reset_pushes_the_cleared_theme_to_obs_once` passes on arrival (the reset path already sent a changed theme once). It also checks that a second reset with an empty theme sends nothing.

Verification (integration review round 2):
- `cargo test --workspace`: 346 passed, 0 failed.
- `node test_frontend.js` (from desktop/): 443 passed, 0 failed.
