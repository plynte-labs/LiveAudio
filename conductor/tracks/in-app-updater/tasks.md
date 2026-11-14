# Tasks: In-App Updater for Desktop Application (Tauri 2)

Track: `in-app-updater`  
Status: In Progress (`[~]`)  
Reference: `conductor/workflow.md`, `conductor/tracks/in-app-updater/spec.md`, `conductor/tracks/in-app-updater/plan.md`  

---

## T1 — Cryptographic Key Generation & Secret Provisioning (Human Task) · REQ-4

- [ ] [HUMAN] Product Owner generates Minisign key pair locally using `tauri signer generate`.
- [ ] [HUMAN] Product Owner adds private key as `TAURI_SIGNING_PRIVATE_KEY` repository secret in GitHub.
- [ ] Public key configured in `desktop/src-tauri/tauri.conf.json` under `plugins.updater.pubkey`.

```
Given: Product Owner in secure local environment with Tauri CLI
When: Product Owner executes key generation and provisions GitHub Actions secret
Then: Secret TAURI_SIGNING_PRIVATE_KEY is available in CI; public key is embedded in tauri.conf.json
Error Path: Private key lost or exposed -> revoke key, regenerate pair, and reissue desktop installer
UI State: N/A (build-time & security infrastructure)
OBS Behavior: Unaffected
```

---

## T2 — Configuration Field, Throttling, Clock Skew & Backoff · REQ-1, REQ-2 (Completed: 17b45ac)

- [x] Add `check_updates_enabled: bool` (default `true`) to `LiveAudioConfig` in `crates/liveaudio-core/src/config/mod.rs`. (17b45ac)
- [x] Connect dormant `last_update_check: i64` and normalize negative values to `0`. (17b45ac)
- [x] Add helper function `should_check_for_updates(last_check: i64, now: i64) -> bool`: (17b45ac)
  - 24h throttling: check permitted if `now.saturating_sub(last_check) >= 86400`.
  - Clock skew resilience: if `now < last_check` (clock rollback), treat as due immediately.
- [x] Implement failure backoff logic: on check failure, set `last_update_check = now.saturating_sub(82800)` (allowing retry in ≥1h without introducing extra config schema fields). On success, set `last_update_check = now`. (17b45ac)
- [x] Write unit tests covering serde defaults, normalization, 24h throttle, clock skew, and failure backoff. (17b45ac)

```
Given: LiveAudioConfig loaded from disk
When: config.normalize() and update checks are evaluated
Then: check_updates_enabled defaults to true, last_update_check is validated, 24h throttling/1h failure backoff enforced
Error Path: Corrupted last_update_check (negative or future clock skew) -> normalized safely, evaluated as due
UI State: Settings reflects toggle state accurately (checked by default)
OBS Behavior: Unaffected
```

---

## T3 — Tauri Updater Plugin, Stream Safety Gate, Sidecar Termination & Rust Commands · REQ-3, REQ-4, REQ-5, REQ-6, REQ-7, REQ-8, REQ-10 (Completed: 49f23d5)

- [x] Add `tauri-plugin-updater = "2"` dependency to `desktop/src-tauri/Cargo.toml` (do not add `@tauri-apps/plugin-updater` npm package). (49f23d5)
- [x] Configure `plugins.updater` in `desktop/src-tauri/tauri.conf.json`: (49f23d5)
  - `pubkey`: Minisign public key
  - `endpoints`: `["https://github.com/plynte-labs/LiveAudio/releases/latest/download/latest.json"]`
  - `windows.installMode`: `"passive"`
- [x] Register updater plugin in `desktop/src-tauri/src/lib.rs`. (49f23d5)
- [x] Maintain minimal permissions in `desktop/src-tauri/capabilities/default.json` (do NOT add `updater:*` capabilities). (49f23d5)
- [x] **Fix Stream Safety Bug:** Update `AppState::ws_clients()` in `desktop/src-tauri/src/state.rs` to inspect real client count via `state.ws_server.lock().await.as_ref().map_or(0, |ws| ws.client_count())` and remove/deprecate the dead `ws_clients_count` field; reconcile callers in `commands.rs:855`. (49f23d5)
- [x] **Define Busy State:** `is_running == true || ws_clients > 0 || foreign_service.is_some()`. (49f23d5)
- [x] Strictly block BOTH download and installation while the application is busy. (49f23d5)
- [x] Implement in-flight idempotency guards using `AtomicBool` returning `AlreadyInProgress` for concurrent check or install calls. (49f23d5)
- [x] Support compile-time endpoint override via `option_env!("LIVEAUDIO_UPDATER_ENDPOINT")`. Strictly enforce HTTPS (reject HTTP). (49f23d5)
- [x] Implement first background check scheduled **10 seconds** post-launch with ≤5s network timeout. (49f23d5)
- [x] Implement Rust commands in `desktop/src-tauri/src/commands.rs`: (49f23d5)
  - `check_for_updates(force: bool) -> Result<Option<UpdateMetadataDto>, String>`
  - `install_update_now() -> Result<(), String>` (idle only)
  - `set_install_on_close(enabled: bool) -> Result<(), String>`
  - `get_updater_status() -> Result<UpdaterStatusDto, String>`
- [x] Hook into `RunEvent::ExitRequested` in `lib.rs`: if "Install on close" is armed, execute download and passive installer AFTER `stop_service_for_exit` finishes, immediately before `app.exit(0)`. (49f23d5)
- [x] Update `desktop/src-tauri/hooks.nsh` (`NSIS_HOOK_PREINSTALL`) to terminate sidecars `liveaudio-cli.exe` and `liveaudio-cli-x86_64-pc-windows-msvc.exe`. (49f23d5)
- [x] Add unit tests for `AppState::ws_clients()`, stream safety guard, and monotonicity rejection. (49f23d5)

```
Given: LiveAudio desktop running with active transcription, connected OBS clients, or foreign service
When: Background task or user initiates update download or install
Then: Operation is blocked with stream-safety reason; downloads and restarts deferred until completely idle
Error Path: Network offline or timeout (>5s) -> fails silently in background, logs at debug level
UI State: "Install now" button is disabled with explanatory tooltip while streaming
OBS Behavior: Subtitle broadcast continues without interruption, jitter, or frame drops
```

---

## T4 — Frontend UI Placement, Toggle Hint, Non-Modal Card & Bilingual i18n · REQ-8, REQ-9 (Completed)

- [x] Add bilingual translation strings in `desktop/src/i18n.js` (ES / EN):
  - `lbl_check_updates_auto`: "Buscar actualizaciones automáticamente" / "Check for updates automatically"
  - `hint_check_updates_auto`: "LiveAudio busca actualizaciones en GitHub Releases cada 24 h. El audio y las transcripciones nunca salen de tu ordenador." / "LiveAudio checks GitHub Releases for updates every 24h. Audio and transcripts never leave your computer."
  - `btn_check_now`: "Buscar ahora" / "Check now"
  - `status_checking_updates`: "Buscando actualizaciones…" / "Checking for updates…"
  - `status_up_to_date`: "LiveAudio está actualizado" / "LiveAudio is up to date"
  - `status_update_available`: "Nueva versión disponible: v{version}" / "New version available: v{version}"
  - `err_update_offline`: "No se pudo comprobar la versión" / "Unable to check for updates"
  - `btn_install_now`: "Instalar y reiniciar" / "Install and restart"
  - `btn_install_on_close`: "Instalar al cerrar" / "Install on close"
  - `status_update_deferred_busy`: "Instalación en espera (transmisión activa)" / "Update deferred (stream active)"
- [x] Update HTML layout in `desktop/src/index.html`:
  - [x] Place `#group-updates` inside `tab-advanced` directly adjacent to `#group-runtime-maintenance`.
  - [x] Add auto-update switch with explicit disclosure hint.
  - [x] Add "Check now" button and inline status container.
  - [x] Add non-modal status badge/pill in the application header.
  - [x] Add non-modal expandable release notes card (NEVER a modal dialog).
- [x] Implement UI interactions in `desktop/src/app.js`:
  - [x] Sync toggle state with `save_config`.
  - [x] Wire "Check now" button to invoke `check_for_updates(force: true)` and render inline status text.
  - [x] Background checks stay silent (no inline status unless an update is found).
  - [x] Show subtle update badge when an update is available; render release notes card inline.
  - [x] Disable "Install now" when transcription or streaming is active.
  - [x] Wire "Install on close" button and track state.

```
Given: Application is in Advanced tab and an update is available
When: User clicks "Check now" (or background check completes)
Then: Inline text displays "New version available"; non-modal card reveals release notes; Install options offered
Error Path: Endpoint unreachable -> inline text displays "Unable to check for updates"; background checks stay silent
UI State: Non-modal badge and card appear without stealing focus, opening popups, or playing sound
OBS Behavior: Zero overlay disturbance
```

---

## T5 — CI/CD Packaging, Signing & Manifest Assembly Automation · REQ-11 (Completed: 136df05)

- [x] Update `.github/workflows/release.yml`: (136df05)
  - [x] Compile `liveaudio-cli` release binary. (136df05)
  - [x] Stage `desktop/src-tauri/binaries/liveaudio-cli-x86_64-pc-windows-msvc.exe`. (136df05)
  - [x] Execute `cargo tauri build` to build NSIS installer and generate `.sig` artifact (`bundle.createUpdaterArtifacts = true`). (136df05)
  - [x] Sign installer using `TAURI_SIGNING_PRIVATE_KEY`. (136df05)
  - [x] Add dedicated CI script step to assemble `latest.json` from the generated installer and `.sig` file. (136df05)
  - [x] Collect and upload `LiveAudio-Setup-<version>.exe`, `LiveAudio-Setup-<version>.exe.sig`, and `latest.json` to the GitHub release draft. (136df05)

```
Given: A new release tag (e.g., v1.6.0) is pushed to the repository
When: GitHub Actions release workflow executes on windows-latest
Then: Produces signed installer, .sig, and correctly formatted latest.json attached to draft release
Error Path: Missing TAURI_SIGNING_PRIVATE_KEY or manifest assembly failure -> workflow fails with clear actionable error
UI State: N/A (CI pipeline)
OBS Behavior: Unaffected
```

---

## T6 — Documentation, Privacy Policy & VM Testing Recipe · REQ-12 (Completed: 364adca)

- [x] **Create `docs/UPDATER_TESTING.md` (required to close T3 and T4):** (364adca)
  - Document VM test recipe using QA build with compile-time `LIVEAUDIO_UPDATER_ENDPOINT="https://localhost:8443/latest.json"`.
  - Provide instructions for installing `mkcert` local CA inside the VM.
  - Provide instructions for running a tiny static HTTPS server (`serve_updates.mjs`).
  - Document how to sign a 1.6.1 test installer with the PO's private key.
  - Provide sample `latest.json` schema (`version`, `notes`, `pub_date`, `platforms.windows-x86_64.url` + `signature` = raw `.sig` contents).
  - Document how to tamper with the signature (M8) and serve an older version (M9).
- [x] Update `README.md` and `README.es.md`: (364adca)
  - Replace "zero phone home / nothing sent" claims with accurate statement:
    *"Audio and transcripts never leave your machine. LiveAudio only checks GitHub for new versions (can be disabled in Settings)."*
  - Document the in-app updater feature.
- [x] Update `SECURITY.md` scope section to reflect release endpoint checks and Minisign verification. (364adca)
- [x] Update `HISTORIAL_CAMBIOS.md` documenting v1.6.0 in-app updater, stream safety, and one-time manual upgrade requirement for v1.5.1 users. (364adca)
- [x] Create tracking note for out-of-repo website privacy policy update (`/privacy`). (364adca)

```
Given: Documentation review for v1.6.0 public release
When: Auditor inspects README.md, README.es.md, SECURITY.md, HISTORIAL_CAMBIOS.md, and docs/UPDATER_TESTING.md
Then: Privacy statement accurately reflects release checks; testing recipe enables full local validation
Error Path: Missing docs/UPDATER_TESTING.md -> T3 and T4 cannot be closed
UI State: N/A (documentation)
OBS Behavior: Unaffected
```

---

## Manual Test Matrix (Pre-Release Quality Gate)

All tests must be executed and recorded following `docs/UPDATER_TESTING.md`:

| ID | Test Scenario | Preconditions | Action | Expected Outcome | Pass/Fail |
|---|---|---|---|---|---|
| **M1** | Clean VM Install (1.6.0) | Clean Windows 11 VM without prior LiveAudio install | Run `LiveAudio-Setup-1.6.0.exe` | Installs cleanly to `%LOCALAPPDATA%`, creates shortcuts, launches without UAC prompt | [ ] |
| **M2** | Happy Path In-App Update | LiveAudio 1.6.0 installed; test endpoint serves 1.6.1 + valid `.sig` | Trigger "Check now", click "Install now" | Downloads update, installs in passive mode, restarts as v1.6.1 | [ ] |
| **M3** | Offline Startup | Network adapter disconnected | Launch application | Starts instantly (<2s), no blocking, no error dialogs, logs silent offline status | [ ] |
| **M4** | Toggle OFF Zero-Request Verification | Set `check_updates_enabled = false` in Settings | Monitor network with Wireshark / Fiddler for 24h & app restarts | Zero HTTP/HTTPS requests sent to GitHub or update endpoint | [ ] |
| **M5** | Stream Safety Lock (Active Session) | Transcription service running (`is_running = true`), OBS connected, or monitor mode | Trigger update check while streaming | Update badge appears, but "Install now" is disabled; downloads and installations blocked | [ ] |
| **M6** | Install on Close | Update 1.6.1 available; select "Install on close" | Close application window | Service flushes and terminates cleanly, download and passive install execute immediately before exit | [ ] |
| **M7** | SmartScreen / Smart App Control Observation | Fresh Windows 11 with SmartScreen enabled | Run in-app update | Document SmartScreen behavior during passive installer execution | [ ] |
| **M8** | Cryptographic Signature Tamper Rejection | Test endpoint serves valid 1.6.1 installer but invalid/tampered `.sig` | Click "Install now" | Updater aborts download, rejects binary, shows signature error, no execution | [ ] |
| **M9** | Version Monotonicity (Older/Equal Rejection) | App version is 1.6.0; endpoint manifests 1.6.0 or 1.5.1 | Trigger "Check now" | Manifest parsed, version determined not greater, reported as "Up to date" | [ ] |
