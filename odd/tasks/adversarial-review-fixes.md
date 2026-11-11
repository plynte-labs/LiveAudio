# Adversarial review fixes (round 1)

Locator: `odd/tasks/adversarial-review-fixes.md` · Engram mirror: `odd/adversarial-review-fixes/tasks`

## Objective
Fix the seven lead-verified findings from the 2026-11-09 adversarial review (2 sonnet + 1 opus, read-only). User approved items 1-7 on 2026-11-09; items 10-18 of the report are a later round.

## Constraints
- Strict TDD (RED observed before GREEN). Runners: `cargo test -p <crate> --locked`, `cargo test --workspace --locked`, `node desktop/test_frontend.js`, `node --test tests-js/*.mjs`.
- No commits (project rule: user approval). English artifacts; i18n es/en for any new UI text. No emojis.
- Three parallel writers on disjoint files (user request: 2 haiku + 1 sonnet). Each writer touches only its listed files.
- Never touch the user's real %APPDATA%/%LOCALAPPDATA% LiveAudio dirs from tests; no GPU runs.

## Tasks
- [x] F1 — Corrupt config.json must not be overwritten: updater writes only `last_update_check` through the merge path under `config_write_lock` (updater.rs:131/151/162); on unreadable config at startup, rename to `config.json.corrupt-<ts>` and tell the UI (lib.rs:43). Route: sonnet (files: desktop/src-tauri/src/{lib.rs,updater.rs,config_sync.rs,commands.rs}, desktop/src/*).
- [x] F2 — NVIDIA extraction atomic: members to `.part` then rename (core download.rs:154); license written after extraction; skip check must not trust partial files (nvidia_runtime.rs ~199-215). Route: haiku A.
- [x] F2b — Cancel NVIDIA install on app exit; close guard warns while installing. Route: sonnet (commands.rs, app.js).
- [x] F3 — Stop flushes the in-progress utterance and waits (bounded ~3 s) for pending finals before cancelling the event task (commands.rs ~1391/1733/2320). Route: sonnet.
- [x] F4 — Model downloads: no 300 s total timeout; connect + idle timeouts; resume `.part` with Range (asr-worker download.rs:178, asr-worker-ct2 download.rs:50). Route: haiku B.
- [x] F5 — max_chunk_duration ceiling 30 s for transcription/combined (core config/mod.rs:340). Route: haiku B.
- [x] F6 — Worker stdout/stderr readers tolerate non-UTF-8 (read_until + from_utf8_lossy, bounded line) (core supervisor/process.rs:883,903). Route: haiku A.
- [x] F7 — WebSocket Origin: loopback allowed only on the bound port (plus Tauri origins / http://absolute) (network security.rs:63-73, server.rs caller). Route: haiku B.
- [ ] F8 — Lead: full workspace + frontend tests, release build (haiku).

## Progress / evidence
- F2 + F6 (haiku A, 2026-11-09): extraction writes `<name>.part` + sync_all + rename, .part removed on error; stale license removed before re-extract, new license staged in downloads/<sha16>.license and renamed after members. Worker output via bounded `read_worker_line` (fill_buf/consume, from_utf8_lossy, strip LF/CRLF, 1 MiB cap with drain). RED: zip_failed_member_leaves_no_final_or_part_file, failed_extraction_removes_stale_license_so_retry_reextracts, invalid_utf8_line_does_not_end_worker_output (["ok"] vs 3 lines), cap test -> GREEN download 8, nvidia_runtime 13, worker_output_tests 2; core lib 148/1 (the 1 = F5 RED in progress), mock_worker 12. Residual: installs made before the fix with a truncated member + matching license still pass the existence-only skip (needs member size pins; out of scope).
- F1 + F2b + F3 (sonnet, 2026-11-09): updater -> config_sync::record_update_check (config_write_lock + save_ui_changes, only last_update_check); quarantine_corrupt_config() before load_config -> config.json.corrupt-<ts> + take_startup_warning command + toast_config_corrupt_backup (es/en). cancel_nvidia_install shared by cancel command and stop_service_for_exit; close guard also for nvidiaCardState installing / styleDirty. Stop: audio task flushes utterance >= 4800 samples on cancel (send bounded 1 s); event task on its own events_cancel_token; shutdown joins audio, waits <= 3 s for pending finals (tracker moved to AppState), then cancels events. RED compile errors + 17 frontend -> GREEN desktop 116/0, frontend 849/0, tests-js 157/0, build ok. Not covered: live end-to-end Stop flush (needs real worker); exit up to +3 s (EXIT_STOP_DEADLINE 10 s kept).
- F4 + F5 + F7 (haiku B, 2026-11-09): max_chunk ceiling 30 s for transcription/combined; WS loopback Origin only on the bound port (default 80/443 when absent), non-ASCII Origin fails closed, Tauri/OBS origins unchanged (server_lifecycle step 3 now rejects localhost:1420, step 3b accepts localhost:{port}); downloads: connect 30 s + idle 60 s (ct2 blocking client: recv_timeout helper threads), Range resume (206 append, 200 restart, 416 refetch), .part = full name + ".part". RED -> GREEN core config 66, asr-worker 27, network 66; ct2 crate not compilable here (needs CUDA_TOOLKIT_ROOT_DIR), verified via scratch harness including the real download.rs (20/0). Residual: no If-Range/ETag; ct2 stall leaks a helper thread until socket close.
- Lead follow-up F5 (2026-11-09): patch.rs max_chunk_duration range 60 -> 30; index.html slider max 60 -> 30; test_frontend.js assertion RED (848/1) -> GREEN 849/0.
- F8 (lead, 2026-11-09): cargo test --workspace --locked 492 passed / 0 failed; tests-js 157/0; frontend 849/0. Release build (haiku): exit 0, liveaudio-desktop.exe 07:47:45 23,227,904 B; asr-worker 07:48:07, cli 07:48:22. ct2 CUDA worker NOT rebuilt (needs CUDA toolkit); its F4 change ships only after `build_ct2_worker.ps1 -Pin`. Pending: user manual check of Stop flush and close-during-install.
