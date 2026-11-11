# Implementation Plan: In-App Updater for Desktop Application (Tauri 2)

Track: `in-app-updater`  
Status: In Progress (`[~]`)  
Method: TDD (RED → GREEN → REFACTOR), Dream Team SDD Protocol, minimal invasive changes.  

---

## Phase 0: Key Generation & Security Foundation (REQ-4)

Goal: Establish the cryptographic foundation for signed updates without committing private keys to the repository.

- [ ] Task: T1 — Generate Minisign key pair and configure repository secrets (REQ-4).
    - [ ] [HUMAN TASK] Product Owner executes `tauri signer generate` locally.
    - [ ] Store private key securely outside git and add as `TAURI_SIGNING_PRIVATE_KEY` secret in GitHub repository settings.
    - [ ] Provide public key string for inclusion in `desktop/src-tauri/tauri.conf.json`.
    - [ ] Add placeholder / development key in `tauri.conf.json` until production key is supplied.
- [ ] Task: Conductor - User Manual Verification 'Phase 0: Key Generation & Security Foundation' (Protocol in workflow.md)

---

## Phase 1: Core Configuration, Throttling & Clock Resilience (REQ-1, REQ-2) [Completed: 17b45ac]

Goal: Implement configuration schema, 24-hour throttling logic, failure backoff, and clock skew resilience in `crates/liveaudio-core`.

- [x] Task: Write failing tests for config schema and throttling (RED phase). (17b45ac)
    - [x] Test `check_updates_enabled` defaults to `true` when missing in JSON.
    - [x] Test `last_update_check` defaults to `0` and normalizes negative values to `0`.
    - [x] Test roundtrip serialization and deserialization in `%APPDATA%\LiveAudio\config.json`.
    - [x] Test 24-hour throttle helper (`should_check_for_updates(last_check, now)`).
    - [x] Test failure backoff calculation (retry window of ≥1h: `now.saturating_sub(82800)`).
    - [x] Test clock skew handling (if `now < last_check`, treat as due immediately).
    - [x] Test manual check bypasses throttle window.
- [x] Task: Implement configuration fields and validation in `liveaudio-core` (GREEN phase). (17b45ac)
    - [x] Add `check_updates_enabled: bool` to `LiveAudioConfig` in `crates/liveaudio-core/src/config/mod.rs`.
    - [x] Update `LiveAudioConfig::default()` and `normalize()` with validation rules.
    - [x] Implement utility helper for update throttle, clock skew, and failure backoff.
- [x] Task: Refactor and verify all core configuration tests pass. (17b45ac)
- [ ] Task: Conductor - User Manual Verification 'Phase 1: Core Configuration, Throttling & Clock Resilience' (Protocol in workflow.md)

---

## Phase 2: Tauri Updater Backend, Stream Safety & NSIS Cleanup (REQ-3, REQ-4, REQ-5, REQ-6, REQ-7, REQ-8, REQ-10) [Completed: 49f23d5]

Goal: Integrate `tauri-plugin-updater`, fix `AppState::ws_clients()`, enforce stream safety locks, terminate sidecars in NSIS hooks, and register custom Rust commands.

- [x] Task: Write failing tests for updater commands and stream safety (RED phase). (49f23d5)
    - [x] Test `AppState::ws_clients()` returns live count from `ws_server.client_count()` instead of dead field.
    - [x] Test stream safety lock: check, download, and install operations are blocked when `is_running == true`, `ws_clients > 0`, or `foreign_service.is_some()`.
    - [x] Test in-flight idempotency: concurrent check or install calls return `AlreadyInProgress`.
    - [x] Test version monotonicity: remote versions <= current version are rejected.
    - [x] Test compile-time override: `option_env!("LIVEAUDIO_UPDATER_ENDPOINT")` is used if baked; verify HTTP is rejected.
    - [x] Test silent network failure: background offline errors return structured clean error without panic.
- [x] Task: Implement Tauri 2 updater plugin, stream safety, and Rust commands (GREEN phase). (49f23d5)
    - [x] Add `tauri-plugin-updater = "2"` dependency to `desktop/src-tauri/Cargo.toml` (do not add `@tauri-apps/plugin-updater` npm package).
    - [x] Configure `plugins.updater` in `desktop/src-tauri/tauri.conf.json` with endpoint, pubkey, and passive NSIS mode.
    - [x] Register updater plugin in `desktop/src-tauri/src/lib.rs`.
    - [x] Keep `desktop/src-tauri/capabilities/default.json` minimal (do NOT add `updater:*` permissions; expose custom commands only).
    - [x] Fix `AppState::ws_clients()` in `desktop/src-tauri/src/state.rs` and reconcile callers in `commands.rs`.
    - [x] Update `desktop/src-tauri/hooks.nsh` `NSIS_HOOK_PREINSTALL` to terminate `liveaudio-cli.exe` and `liveaudio-cli-x86_64-pc-windows-msvc.exe`.
    - [x] Implement IPC commands in `desktop/src-tauri/src/commands.rs`:
        - [x] `check_for_updates(force: bool)` (includes 10s startup delay, ≤5s timeout, in-flight guard)
        - [x] `install_update_now()` (requires idle: `!running && ws_clients == 0 && !foreign_service`, in-flight guard)
        - [x] `set_install_on_close(enabled: bool)`
        - [x] `get_updater_status()`
    - [x] Hook into `RunEvent::ExitRequested` in `lib.rs`: if "Install on close" is armed, execute download and passive installer AFTER `stop_service_for_exit` completes, right before `app.exit(0)`.
- [x] Task: Refactor and verify all updater backend tests pass. (49f23d5)
- [ ] Task: Conductor - User Manual Verification 'Phase 2: Tauri Updater Backend, Stream Safety & NSIS Cleanup' (Protocol in workflow.md)

---

## Phase 3: Frontend UI, Settings Placement & Bilingual i18n (REQ-8, REQ-9) [Completed]

Goal: Expose updater toggles, manual check trigger, non-modal status badges, release notes card, and bilingual copy in `tab-advanced`.

- [x] Task: Add bilingual translation keys in `desktop/src/i18n.js` (ES / EN).
    - [x] `lbl_check_updates_auto` / `hint_check_updates_auto` (with 24h & privacy disclosure).
    - [x] `btn_check_now`, `btn_install_now`, `btn_install_on_close`.
    - [x] Statuses: `status_checking_updates`, `status_up_to_date`, `status_update_available`, `err_update_offline`, `status_update_deferred_busy`.
- [x] Task: Update HTML layout in `desktop/src/index.html`.
    - [x] Place `#group-updates` in `tab-advanced` adjacent to `#group-runtime-maintenance`.
    - [x] Add auto-update switch with privacy disclosure hint.
    - [x] Add "Check now" button and inline status text container.
    - [x] Add non-modal status badge in application header.
    - [x] Add non-modal expandable release notes card (NEVER a modal).
- [x] Task: Implement UI logic in `desktop/src/app.js`.
    - [x] Bind config toggle to `save_config` IPC.
    - [x] Bind "Check now" button to invoke `check_for_updates(force: true)` and render inline status.
    - [x] Show subtle update badge when an update is available; render release notes card inline.
    - [x] Disable "Install now" when transcription or streaming is active.
    - [x] Bind "Install on close" toggle/button and track pending install state.
- [ ] Task: Conductor - User Manual Verification 'Phase 3: Frontend UI, Settings Placement & Bilingual i18n' (Protocol in workflow.md)

---

## Phase 4: CI/CD Packaging, Signing & Manifest Assembly (REQ-11) [Completed: 136df05]

Goal: Automate Tauri NSIS installer build, code signing, manifest generation, and updater artifact publishing in GitHub Actions.

- [x] Task: Update `.github/workflows/release.yml`. (136df05)
    - [x] Compile `liveaudio-cli` release binary. (136df05)
    - [x] Stage `liveaudio-cli-x86_64-pc-windows-msvc.exe` sidecar into `desktop/src-tauri/binaries/`. (136df05)
    - [x] Execute `cargo tauri build` generating NSIS bundle and `.sig` artifact (`bundle.createUpdaterArtifacts = true`). (136df05)
    - [x] Sign installer with `TAURI_SIGNING_PRIVATE_KEY`. (136df05)
    - [x] Add dedicated script step to assemble `latest.json` from built installer and `.sig`. (136df05)
    - [x] Collect generated assets: `LiveAudio-Setup-<version>.exe`, `LiveAudio-Setup-<version>.exe.sig`, and `latest.json`. (136df05)
    - [x] Include updater assets in the `publish` draft release step. (136df05)
- [ ] Task: Conductor - User Manual Verification 'Phase 4: CI/CD Packaging, Signing & Manifest Assembly' (Protocol in workflow.md)

---

## Phase 5: Documentation, Privacy Statement & Testing Guide (REQ-12) [Completed: 364adca]

Goal: Accurately document network behavior, privacy guarantees, upgrade guidance, and create the VM testing recipe.

- [x] Task: Create `docs/UPDATER_TESTING.md` (required to close T3/T4). (364adca)
    - [x] Document VM test recipe using QA build with compile-time `LIVEAUDIO_UPDATER_ENDPOINT="https://localhost:8443/latest.json"`.
    - [x] Document `mkcert` local CA installation inside the test VM.
    - [x] Provide lightweight Node static HTTPS server setup (`serve_updates.mjs`).
    - [x] Document signing a 1.6.1 test installer with the PO's private key.
    - [x] Provide sample `latest.json` format (`version`, `notes`, `pub_date`, `platforms.windows-x86_64`).
    - [x] Provide instructions for simulating signature tampering (M8) and older version rejection (M9).
- [x] Task: Update `README.md` and `README.es.md`. (364adca)
    - [x] Replace "nothing sent / zero phone home" with accurate copy: *"Audio and transcripts never leave your machine. LiveAudio only checks GitHub for new versions (can be disabled in Settings)."*
    - [x] Document in-app updater feature.
- [x] Task: Update `SECURITY.md`. (364adca)
    - [x] Align network scope section with updater endpoint verification and Minisign validation.
- [x] Task: Update `HISTORIAL_CAMBIOS.md`. (364adca)
    - [x] Add entry for v1.6.0 documenting the in-app updater, stream safety design, and one-time manual upgrade note for v1.5.1 users.
- [x] Task: Track out-of-repo website privacy policy update (`/privacy`). (364adca)
- [ ] Task: Conductor - User Manual Verification 'Phase 5: Documentation, Privacy Statement & Testing Guide' (Protocol in workflow.md)

---

## Phase 6: Verification, Manual Matrix & Collective Review

Goal: Execute end-to-end verification and obtain Dream Team sign-off.

- [ ] Task: Execute Manual Test Matrix (M1 through M9) on Windows test environment following `docs/UPDATER_TESTING.md`.
- [ ] Task: Run full automated test suite (`cargo test --workspace`).
- [ ] Task: Convene Dream Team collective review:
    - [ ] Architecture / Security (`[Agent Arquitecto]`): Stream safety gate, client count fix, Minisign signature verification, compile-time override HTTPS enforcement.
    - [ ] Performance / Resilience (`[Agent Performance]`): 10s startup delay, ≤5s timeout, failure backoff (1h), clock skew saturating math, in-flight idempotency.
    - [ ] QA / UX (`[Agent QA]`): Advanced tab placement, non-modal card, inline status, bilingual copy, `UPDATER_TESTING.md` recipe validation.
    - [ ] Research / Documentation (`[Agent Research]`): Privacy copy accuracy, v1.5.1 migration note, manifest format alignment.
- [ ] Task: Auditor presentation and closure of track.
- [ ] Task: Conductor - User Manual Verification 'Phase 6: Verification, Manual Matrix & Collective Review' (Protocol in workflow.md)
