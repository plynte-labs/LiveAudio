# Release v1.6.0 readiness fixes

## Objective
Close the verified findings from the v1.6.0 adversarial release review (Fable review, blind Sonnet refutation) and rebuild the Windows installer.

## Problem / Why
The review confirmed release blockers (unpinned Tauri CLI in the signing job, Linux AppImage cannot locate ONNX Runtime, CUDA worker asset risk) plus hardening, CI and documentation gaps. Shipping as-is risks a broken Linux build, a 1.2 GB download that fails at the end, and a non-reproducible signed release.

## Scope (authorized by user: "arreglar todo", then rebuild installer)
- Code, CI, packaging and docs changes for C1-C11, C13 below. C12 was refuted (no change).
- C14 (CI signing secret vs committed pubkey) cannot be fixed from the repo: requires a signed round-trip with the real secret (`docs/UPDATER_TESTING.md`). Documented in the release checklist only.
- No commits: project rule "Do not commit unless the user explicitly approves" overrides ODD work-unit commits.

## Constraints
- Minimalism decision ladder (project CLAUDE.md).
- Never commit/build artifacts into git (`dist/`, `gen/schemas/*`, ORT binaries, `*.wav`).
- Models: sonnet for code, haiku for mechanical CI/docs and the installer build.

## TDD
- Mode: strict, enabled. Source: global `~/.claude/CLAUDE.md` ("Strict TDD Mode: enabled").
- Runners: `cargo test --workspace --locked`, `node --test tests-js/*.mjs`, `node desktop/test_frontend.js`, `cargo clippy --workspace --all-targets`.

## Baseline (2026-11-13, develop @ 5f4b6cb)
cargo test green (1 ignored); clippy exit 0, 85 warnings; node tests 157/157; frontend 855/855.

## Tasks
- [x] T1 (C2, sonnet, delegated: writer trigger) Linux AppImage ONNX Runtime discovery: worker and core resolve the ORT library from the Tauri resource dir on Linux.
- [x] T2 (C3, sonnet) CUDA engine install fetches and verifies the pinned worker before the NVIDIA archives; release checklist documents the manual upload.
- [x] T3 (C4, C5, sonnet) Updater: bounded install-on-close download and visible relaunch behaviour; Linux entry (or explicit handling) in `latest.json`.
- [x] T4 (C10, C11, sonnet) Network: CWD overlay override debug-only; WebSocket connection cap and explicit message/frame limits.
- [x] T5 (C8, sonnet) Whisper model downloads pinned to a revision with integrity verification.
- [x] T6 (C9, sonnet) Webview CSP compatible with the vanilla frontend.
- [x] T7 (C13, sonnet) Uninstaller "delete app data" removes LiveAudio config, models and CUDA runtime.
- [x] T8 (C1, C6, C7, haiku) Pin Tauri CLI @2.12.1, `--locked` in release, CI runs all JS tests + frontend harness and triggers on develop; fix stale docs, installer v1.5.0 message, SECURITY.md branch, CHANGELOG 1.6.0 entries.
- [x] T9 (haiku) Rebuild sidecars and the Windows installer; report artifacts.
- [x] T10 Final full check run.

## Acceptance criteria
All runners green; clippy no new warnings; each finding addressed or explicitly documented; installer built.

## Progress / evidence
- Route: delegated direct, one sonnet writer for T1-T7 (writer trigger). TDD strict; runner cargo/node. RED observed before each fix.
- T1: core `find_ort_dylib_near` probes `../lib/LiveAudio` (Tauri Linux resource_dir); test `ort_dylib_is_found_in_tauri_linux_resource_dir`. Parent correction: writer used crate name `liveaudio-desktop`, but tauri-codegen sets `package_info.name` from `productName` ("LiveAudio"); fixed and re-tested green.
- T2: pinned worker downloaded/verified before NVIDIA archives; test `missing_worker_asset_fails_before_large_archives`; release checklist items 8-9 in docs/PACKAGING_AND_UPDATES.md.
- T3: install-on-close bounded at 120 s (`run_bounded_install`); updater disabled off Windows (`updater_supported_on`), latest.json stays windows-only (Linux updates are manual). Second-instance focus no-op left as is (now bounded).
- T4: CWD overlay probes debug-only; `max_connections` (64) semaphore and `max_inbound_message_bytes` (64 KiB); tests in server_lifecycle.rs.
- T5: 5 built-in repos pinned to commit SHAs; Hub size+LFS sha256 verified before commit; cached files need matching size. ct2 crate untouched (multi-repo, not trivial).
- T6: strict CSP in tauri.conf.json; harness assertions in desktop/test_frontend.js.
- T7: hooks.nsh POSTUNINSTALL removes %APPDATA%\LiveAudio and %LOCALAPPDATA%\LiveAudio only if delete-app-data ticked and not update; harness assertions. NSIS not compiled (template fetched from tauri repo for variable names).
- T8: release.yml and PACKAGING doc pin `@tauri-apps/cli@2.12.1` and use `cargo build --release --locked`; ci.yml runs on master+develop, JS job runs `node --test tests-js/*.mjs` and `node desktop/test_frontend.js` (pure JS, all inputs tracked, no ONNX needed); hooks.nsh uses `${VERSION}` (`!define VERSION` verified upstream in Tauri installer.nsi); docs, SECURITY, CHANGELOG 1.6.0 dated 2026-11-13 (new text English inside Spanish files, translation pending). Observed: node 157/157, frontend 866 passed 0 failed. No new test (config/docs). Not committed.
- T9: `target/release/bundle/nsis/LiveAudio_1.6.0_x64-setup.exe` (23959009 bytes, sha256 `9c0b0e33b94648a13387374479fffec1a3f85a48742403f6811bdcd7b041f0de`); built with `createUpdaterArtifacts=false` (no local signing key, no `.sig`); sidecars built `--locked` and staged as `*-x86_64-pc-windows-msvc.exe`; Tauri CLI added `features = []` to `desktop/src-tauri/Cargo.toml` (uncommitted, review before commit).

- T8 follow-up (parent): new CHANGELOG and GETTING_STARTED lines translated to Spanish to match those files.
- T10 (parent, 2026-11-13): cargo test --workspace --locked: all ok; clippy: exit 0, 79 warnings (baseline 85); node --test tests-js: 157/157; desktop/test_frontend.js: 866/866. Reverted Tauri CLI no-op rewrite of desktop/src-tauri/Cargo.toml.

- T11 (parent, inline, 2026-11-13): upgrade over v1.5.1 failed ("Error launching installer" / "Unable to uninstall!"). Cause: f3d6fe5 changed bundle.publisher "franguh" -> "Plynte Labs / FranGuh"; NSIS reads previous install dir from HKCUSoftware<publisher>iveaudio, v1.5.x wrote softwareranguhiveaudio. fix: restore "franguh" (no custom template needed, 1.6.0 unpublished). red: harness 866/1 failed; green: 867/0. would have broken every 1.5.x -> 1.6.0 auto-update.
- T12 (delegated, 2026-11-13): style preset bleed. Cause: applyStyleChoice built-in branch left subtitle_theme and controls from the previous saved style. Fix: built-in branch sets subtitle_theme {}, populateStyleControls(controlsFromTheme({})), clears preset name input. red: harness 868 passed/2 failed; green: 870/0; tests-js 157/0.

## pending (outside the repo / manual)
- Upload the exact pinned `liveaudio-asr-worker-cuda.exe` to the v1.6.0 release (C3).
- Signed-update round trip with the real CI secret (C14, docs/UPDATER_TESTING.md).
- Manual smoke test: app under new CSP, uninstall with "delete app data", Linux AppImage transcription.
- Not done: ct2 crate model integrity; second-instance focus during install-on-close.
- No commits made (project rule); awaiting user approval.

## Next step
User review and commit approval.

- T13 (delegated, 2026-11-13): clean-VM blocker, VCRUNTIME140.dll missing for worker/CLI/ORT. Fix 1: `fetch_onnxruntime.mjs` stages vcruntime140/vcruntime140_1/msvcp140/msvcp140_1 from the VS Redist (vswhere, not hash-pinned) into packaging/onnxruntime/windows-x64 (gitignored); listed in tauri.windows.conf.json (install root), release.yml CLI zip, ci.yml closure check; notices + packaging doc. Fix 2: `suppress_loader_error_dialogs()` (SetErrorMode SEM_FAILCRITICALERRORS, windows-sys feature Win32_System_Diagnostics_Debug) called at desktop `run()` and CLI `main()`; exit 0xC0000135 maps to "Microsoft Visual C++ runtime DLL missing; reinstall LiveAudio". red: js 4 failed, cargo compile errors; green: tests-js 161/0, harness 870/0, cargo tests-core new tests. Not verified: real clean-VM install (needs rebuilt installer).
