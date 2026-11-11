# Specification: In-App Updater for Desktop Application (Tauri 2)

Track: `in-app-updater`  
Status: Planned (`[ ]`)  
Type: Feature (Infrastructure / Desktop UX / Security)  
Target Version: `1.6.0`  

---

## 1. Overview

LiveAudio v1.5.1 introduced a high-performance Rust core and Tauri 2 desktop architecture, but updates currently require users to manually check GitHub Releases and reinstall the setup package.

This track implements a secure, stream-safe in-app updater for the desktop application using `tauri-plugin-updater` (Tauri 2). Streamers cannot afford interruptions during live broadcasts, so the updater strictly enforces stream safety, non-modal UI notifications, zero telemetry, and cryptographic artifact verification.

Existing users on version 1.5.1 must perform a one-time manual reinstall to upgrade to 1.6.0 because the 1.5.1 bundle does not include updater capabilities or the public signing key. From 1.6.0 onward, updates are handled seamlessly in-app.

---

## 2. Confirmed Product Owner Decisions

1. **Updater Engine & Packaging:** Full in-app updater built with official `tauri-plugin-updater` (version `"2"`, matching Tauri `=2.12.0`) in `desktop/src-tauri`. Windows NSIS installer configured with `installMode: "currentUser"` (no Windows UAC elevation required) and updater `installMode: "passive"`. The plugin buffers the installer payload in memory (~26 MB), which is acceptable for modern desktop environments; no custom chunk streaming is required.
2. **Release Endpoint & Build-Time Override:** Primary endpoint is `https://github.com/plynte-labs/LiveAudio/releases/latest/download/latest.json`. In release builds, runtime environment variable overrides are strictly disabled for security. QA/test builds use a compile-time override baked at build via `option_env!("LIVEAUDIO_UPDATER_ENDPOINT")`. Overrides MUST use HTTPS (the updater plugin rejects HTTP in release with `Error::InsecureTransportProtocol`).
3. **Opt-Out, Throttling & Clock Resilience:** Update checks are enabled by default with a toggle in Settings. Managed by `check_updates_enabled: bool` in `LiveAudioConfig` (`crates/liveaudio-core/src/config/mod.rs`), persisted in `%APPDATA%\LiveAudio\config.json`. The dormant field `last_update_check: i64` (line 231) is activated:
   - First background check fires **10 seconds** after startup.
   - Successful check sets `last_update_check = now` (throttled for 24h / 86,400s).
   - Failed check applies failure backoff without adding new schema fields: sets `last_update_check = now.saturating_sub(86400 - 3600)` (retry allowed in ≥1 hour).
   - Clock skew resilience: if `now < last_update_check` (system clock rolled back), the check is treated as due immediately using saturating arithmetic.
   - Manual "Check now" bypasses throttling.
4. **Zero Telemetry Guarantee:** The update check performs only a standard HTTP `GET` request for `latest.json`. No unique installation IDs, hardware identifiers, operating system metrics, or telemetry query parameters/headers are transmitted.
5. **Stream Safety & Busy State (CRITICAL):**
   - **Busy Definition:** An active pipeline is defined as `AppState::is_running() == true` OR active WebSocket clients connected (`ws_server.client_count() > 0`) OR `foreign_service.is_some()` (Monitor Mode).
   - *Verified Pre-existing Bug:* `AppState::ws_clients()` historically read `ws_clients_count`, which was never written to (dead field). The updater MUST inspect the real client count via `ws_server.client_count()` (`crates/liveaudio-network/src/server.rs:466`) and fix/deprecate the dead field in `state.rs`.
   - **Blocked Operations:** BOTH downloading update payloads and installing updates are strictly blocked while the app is busy.
   - **Install Flow:**
     - *"Install now":* Allowed ONLY when the system is completely idle (`!busy`).
     - *"Install on close":* Arms a deferred flag. Download and installation execute ONLY after `stop_service_for_exit` completes in `RunEvent::ExitRequested` (`lib.rs:79-96`), immediately before `app.exit(0)`.
   - Notifications use only a quiet, non-modal status badge and card. No modals, no popups, no audio cues, and no window focus stealing.
6. **Silent Offline & Network Bounds:** Background checks fail silently with a timeout of ≤5 seconds. No error banners or popups appear during background runs. Startup speed is never blocked.
7. **Cryptographic Security & Capabilities:**
   - Minisign Ed25519 public key stored in `desktop/src-tauri/tauri.conf.json` under `plugins.updater.pubkey`.
   - Private key generated and stored exclusively by the PO (Task T1 is human-only) and supplied to CI as `TAURI_SIGNING_PRIVATE_KEY`.
   - Only strictly greater versions (`remote > current`) are accepted.
   - HTTPS only; unverified TLS fallbacks are forbidden.
   - **Minimal Capabilities:** Do NOT grant any `updater:*` permissions in `capabilities/default.json` and do NOT install the `@tauri-apps/plugin-updater` npm package. All operations are controlled exclusively through custom Rust commands, leaving frontend CSP untouched.
8. **NSIS Sidecar Cleanup:** `desktop/src-tauri/hooks.nsh` (`NSIS_HOOK_PREINSTALL`) must terminate running sidecar processes (`liveaudio-cli.exe` and `liveaudio-cli-x86_64-pc-windows-msvc.exe`) in addition to legacy `liveaudio.exe` to prevent file locks during passive updates.
9. **CI Build & Manifest Assembly:** `.github/workflows/release.yml` builds the sidecar, invokes `cargo tauri build` with `bundle.createUpdaterArtifacts = true`, and runs a dedicated script step to assemble `latest.json` (as `tauri build` does not generate the manifest file automatically). Uploads installer, `.sig`, and `latest.json` to the release draft.
10. **UI Placement & Copy:** Toggle placed in `#group-updates` inside `tab-advanced` (next to `#group-runtime-maintenance`). Toggle hint explicitly discloses 24h release checks and privacy. Release notes appear as an inline expandable non-modal card (never a modal). Manual check provides inline text feedback; background checks remain silent.
11. **Testing Documentation:** Dedicated test recipe document `docs/UPDATER_TESTING.md` created to support offline/local VM testing with `mkcert`, static HTTPS server, and signing walkthrough.

---

## 3. Functional Requirements

### REQ-1: Configuration, Throttling & Clock Skew
- Add `check_updates_enabled: bool` to `LiveAudioConfig` with `#[serde(default = "default_true")]`.
- Reuse dormant `last_update_check: i64` (epoch timestamp in seconds).
- `LiveAudioConfig::normalize()` normalizes `check_updates_enabled` and ensures `last_update_check >= 0`.
- Throttling rules:
  - Startup check runs 10 seconds post-launch if enabled and due.
  - Interval: 24h (86,400s).
  - Clock skew: if `now < last_update_check`, treat as due immediately (`now.saturating_duration_since(last_update_check)`).
  - Failure backoff: upon check failure, set `last_update_check = now.saturating_sub(82800)` so retry occurs in 1h (3,600s).
  - Success: set `last_update_check = now`.

### REQ-2: Zero Telemetry & Privacy
- HTTP `GET` requests to the updater endpoint must contain zero query parameters, tracking cookies, machine GUIDs, or telemetry headers.
- User-Agent header must not leak hardware specs, device IDs, or personal info.

### REQ-3: Endpoint Configuration & Compile-Time Override
- Default endpoint: `https://github.com/plynte-labs/LiveAudio/releases/latest/download/latest.json`.
- Compile-time override via `option_env!("LIVEAUDIO_UPDATER_ENDPOINT")` for QA/test builds. Runtime environment variables are ignored in release builds.
- Endpoint MUST use HTTPS protocol; plain HTTP is rejected.

### REQ-4: Cryptographic Verification & Minisign Security
- Manifest and binary signatures verified using Minisign Ed25519 public key.
- Unverified, tampered, or mismatched binaries fail with signature verification error before execution.
- No insecure TLS fallback.

### REQ-5: Monotonic Version Enforcement
- Evaluates versions via strict SemVer: `semver::Version::parse(remote) > semver::Version::parse(current)`.
- Reinstalling equal versions or downgrades is rejected.

### REQ-6: Stream Safety, Real Client Count & Idempotency
- Updater checks:
  1. `AppState::is_running() == false`
  2. Real client count `state.ws_server.lock().await.as_ref().map_or(0, |ws| ws.client_count()) == 0`
  3. `state.foreign_service.read().await.is_none()`
- `AppState::ws_clients()` fixed to return the real count from `ws_server.client_count()`.
- If busy, payload downloads and installations are strictly rejected with a stream-safety reason.
- Idempotency: `AtomicBool` guards prevent duplicate concurrent checks or installations; calls return `AlreadyInProgress`.

### REQ-7: Silent Offline & Network Timeout
- Background checks fail silently without UI banners or popups if network fails.
- Network request timeout strictly capped at ≤5 seconds.

### REQ-8: Non-Modal UI & Dual Installation Flow
- Non-modal update indicator (badge in header and card in `tab-advanced`).
- "Install now" button enabled only when idle.
- "Install on close" queues download and passive install to execute during `RunEvent::ExitRequested` after `stop_service_for_exit` finishes.
- Release notes presented as an expandable non-modal card (never a modal dialog).

### REQ-9: Bilingual UI Controls & i18n
- UI controls in `desktop/src` provide exact bilingual translations for:
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

### REQ-10: NSIS Sidecar Termination
- Update `desktop/src-tauri/hooks.nsh` to terminate `liveaudio-cli.exe` and `liveaudio-cli-x86_64-pc-windows-msvc.exe` during `NSIS_HOOK_PREINSTALL`.

### REQ-11: CI/CD Packaging & Manifest Assembly
- GitHub Actions workflow `.github/workflows/release.yml`:
  1. Compiles sidecar `liveaudio-cli`.
  2. Runs `cargo tauri build` producing NSIS bundle and `.sig` artifact.
  3. Executes a script step assembling `latest.json` (extracting version, notes, pub_date, and platform URL/signature).
  4. Signs installer with `TAURI_SIGNING_PRIVATE_KEY`.
  5. Uploads `LiveAudio-Setup-<version>.exe`, `.sig`, and `latest.json` to the release draft.

### REQ-12: Documentation, Privacy Notice & VM Testing Recipe
- Update `README.md`, `README.es.md`, `SECURITY.md`, and `HISTORIAL_CAMBIOS.md` with accurate privacy copy.
- Add `docs/UPDATER_TESTING.md` detailing the VM testing recipe, `mkcert` setup, static HTTPS server, and signing steps.

---

## 4. Technical Debt & Out-of-Scope Items

- **Pre-existing Security Debt:** `desktop/src-tauri/tauri.conf.json` currently has `"app": { "security": { "csp": null } }`. Retained as pre-existing debt without modification in this track; updater introduces zero new capabilities to the webview.
- **Payload Buffering:** `tauri-plugin-updater` buffers the ~26 MB payload into memory. Acceptable for desktop; streaming download is out of scope.
- **External Web Page:** Public website `/privacy` update tracked as an out-of-repo follow-up.
