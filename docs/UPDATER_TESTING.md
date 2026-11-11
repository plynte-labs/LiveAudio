# LiveAudio In-App Updater Testing Guide & VM Recipe

This document provides a complete, reproducible testing recipe for validating the LiveAudio Tauri 2 in-app updater in a clean local environment or virtual machine (Windows 11 VM).

---

## 1. Prerequisites & Environment Setup

Testing an in-app updater requires:
1. A clean target environment (e.g. Windows 11 Sandbox or Hyper-V/VirtualBox VM).
2. A valid local HTTPS server because the updater strictly requires HTTPS (`Error::InsecureTransportProtocol` on HTTP).
3. A local trusted root Certificate Authority (CA) using [`mkcert`](https://github.com/FiloSottile/mkcert).
4. A QA build compiled with compile-time override `LIVEAUDIO_UPDATER_ENDPOINT`.

### 1.1 Local CA & TLS Certificate Setup with `mkcert`

On your test host or inside the VM:

1. Install `mkcert` (via Chocolatey or Scoop):
   ```powershell
   choco install mkcert
   # or
   scoop install mkcert
   ```

2. Generate and trust the local CA:
   ```powershell
   mkcert -install
   ```

3. Generate a certificate for `localhost`:
   ```powershell
   mkdir C:\updater-test\certs -Force
   cd C:\updater-test\certs
   mkcert localhost 127.0.0.1 ::1
   # Generates: localhost+2.pem (cert) and localhost+2-key.pem (key)
   ```

---

## 2. Minimal Local HTTPS Server

Create a lightweight Node 22 HTTPS server (`serve_updates.mjs`) to serve `latest.json` and update installers over port `8443`:

```javascript
// C:\updater-test\serve_updates.mjs
import https from "node:https";
import fs from "node:fs";
import path from "node:path";

const DIRECTORY = "C:\\updater-test\\dist";
const CERT_FILE = "C:\\updater-test\\certs\\localhost+2.pem";
const KEY_FILE = "C:\\updater-test\\certs\\localhost+2-key.pem";
const PORT = 8443;

fs.mkdirSync(DIRECTORY, { recursive: true });

const server = https.createServer(
  {
    key: fs.readFileSync(KEY_FILE),
    cert: fs.readFileSync(CERT_FILE),
  },
  (req, res) => {
    const urlPath = new URL(req.url, `https://${req.headers.host}`).pathname;
    const safePath = path.normalize(urlPath).replace(/^(\.\.[/\\])+/, "");
    const filePath = path.join(DIRECTORY, safePath);

    if (fs.existsSync(filePath) && fs.statSync(filePath).isFile()) {
      res.setHeader("Cache-Control", "no-cache, no-store, must-revalidate");
      const ext = path.extname(filePath).toLowerCase();
      if (ext === ".json") res.setHeader("Content-Type", "application/json");
      else if (ext === ".sig") res.setHeader("Content-Type", "text/plain");
      else res.setHeader("Content-Type", "application/octet-stream");
      fs.createReadStream(filePath).pipe(res);
    } else {
      res.writeHead(404);
      res.end("Not Found");
    }
  }
);

server.listen(PORT, "0.0.0.0", () => {
  console.log(`Serving updates over HTTPS on https://localhost:${PORT}/ from ${DIRECTORY}...`);
});
```

Run the server in a dedicated terminal with Node 22:
```powershell
node C:\updater-test\serve_updates.mjs
```

---

## 3. Compiling the QA Build (v1.6.0)

In release builds, the endpoint defaults to GitHub Releases. To point the application to the test server, compile with the compile-time environment variable `LIVEAUDIO_UPDATER_ENDPOINT`:

```powershell
# 1. Build CLI sidecar
cargo build --release -p liveaudio-cli
mkdir desktop\src-tauri\binaries -Force
copy target\release\liveaudio-cli.exe desktop\src-tauri\binaries\liveaudio-cli-x86_64-pc-windows-msvc.exe

# 2. Build Tauri desktop package with QA endpoint baked in
$env:LIVEAUDIO_UPDATER_ENDPOINT = "https://localhost:8443/latest.json"
cd desktop
npx @tauri-apps/cli build --config src-tauri/tauri.release.conf.json
```

The resulting installer (`LiveAudio_1.6.0_x64-setup.exe` or `LiveAudio-Setup-1.6.0.exe`) in `target\release\bundle\nsis\` will query `https://localhost:8443/latest.json`.

---

## 4. Signing a Test Update Package (v1.6.1)

To test the upgrade path, generate a fake or real update package for version `1.6.1`:

1. Update version to `"1.6.1"` temporarily in `desktop/src-tauri/tauri.conf.json`.
2. Build the package:
   ```powershell
   cd desktop
   npx @tauri-apps/cli build --config src-tauri/tauri.release.conf.json
   ```
3. Sign the test installer using the Minisign private key:
   ```powershell
   npx @tauri-apps/cli signer sign --private-key-path <path_to_private_key> target\release\bundle\nsis\LiveAudio_1.6.1_x64-setup.exe
   ```
   This generates `target\release\bundle\nsis\LiveAudio_1.6.1_x64-setup.exe.sig`.

4. Copy both the installer and `.sig` into `C:\updater-test\dist\`:
   ```powershell
   copy target\release\bundle\nsis\LiveAudio_1.6.1_x64-setup.exe C:\updater-test\dist\
   copy target\release\bundle\nsis\LiveAudio_1.6.1_x64-setup.exe.sig C:\updater-test\dist\
   ```

5. Assemble `C:\updater-test\dist\latest.json` using `packaging/assemble_latest_manifest.mjs` or create manually:
   ```json
   {
     "version": "1.6.1",
     "notes": "LiveAudio v1.6.1 - Test update for VM validation",
     "pub_date": "2026-10-29T18:00:00Z",
     "platforms": {
       "windows-x86_64": {
         "signature": "<RAW_CONTENTS_OF_LiveAudio_1.6.1_x64-setup.exe.sig>",
         "url": "https://localhost:8443/LiveAudio_1.6.1_x64-setup.exe"
       }
     }
   }
   ```

---

## 5. Manual Test Execution Matrix (M1 – M9)

### M1: Clean VM Install (1.6.0)
- **Preconditions:** Clean Windows 11 VM without prior LiveAudio install.
- **Action:** Run `LiveAudio-Setup-1.6.0.exe`.
- **Expected Outcome:** Installs cleanly to `%LOCALAPPDATA%\Programs\LiveAudio`, creates desktop and Start menu shortcuts, and launches without Windows UAC elevation prompts (`installMode: "currentUser"`).

### M2: Happy Path In-App Update (1.6.0 → 1.6.1)
- **Preconditions:** LiveAudio 1.6.0 running in VM; HTTPS test server running serving 1.6.1 installer + valid `.sig`.
- **Action:**
  1. Open **Settings > Advanced**.
  2. Click **Check now (Buscar ahora)**.
  3. Verify status indicates `New version available: v1.6.1` and release notes card expands.
  4. Click **Install and restart (Instalar y reiniciar)**.
- **Expected Outcome:** Update downloads (~26 MB buffered), launches passive NSIS installer (no dialog wizard), closes the 1.6.0 app, terminates any running sidecars, and relaunches as v1.6.1. Header reflects v1.6.1.

### M3: Offline Startup
- **Preconditions:** Disconnect network adapter in VM (or stop the test server).
- **Action:** Launch LiveAudio.
- **Expected Outcome:** Application opens within <2 seconds. No modal dialogs, blocking alerts, or UI errors appear. Background check fails silently.

### M4: Toggle OFF Zero-Request Verification
- **Preconditions:** Set `check_updates_enabled = false` in **Settings > Advanced**.
- **Action:**
  1. Start Wireshark or Fiddler inside the VM.
  2. Restart the app, wait 15 seconds, and simulate 24 hours of operation (or system clock advance).
- **Expected Outcome:** Exactly 0 HTTP/HTTPS requests are transmitted to the update endpoint.

### M5: Stream Safety Lock (Active Session)
- **Preconditions:** Active session running (`AppState::is_running() == true`), active OBS WebSocket client connected (`ws_server.client_count() > 0`), or foreign service monitor mode.
- **Action:** Trigger update check or observe background check when an update is available.
- **Expected Outcome:**
  - Non-modal notification pill appears.
  - The **"Install and restart"** button is **disabled** with a stream-safety reason: *"Update deferred (stream active)" / "Instalación en espera (transmisión activa)"*.
  - No popups steal focus. Subtitle transmission to OBS continues without any jitter, frame drops, or audio disruption.

### M6: Install on Close
- **Preconditions:** Update 1.6.1 is available while streaming or idle.
- **Action:**
  1. Click **Install on close (Instalar al cerrar)**.
  2. Close the application window via header close button or Alt+F4.
- **Expected Outcome:**
  - App initiates clean shutdown: flushes transcripts to disk, signals ASR worker process to terminate cleanly, closes WebSocket server.
  - Immediately before `app.exit(0)`, the passive installer executes.
  - The next application launch boots into v1.6.1.

### M7: SmartScreen / Smart App Control Observation
- **Preconditions:** Fresh Windows 11 installation with Microsoft Defender SmartScreen enabled.
- **Action:** Execute the in-app update.
- **Expected Outcome:** Document SmartScreen prompts. Passive installation runs under current user privileges. Note any warnings if the binary lacks an EV Authenticode code-signing certificate (Minisign protects the integrity of the binary, but Windows SmartScreen tracks reputation separately).

### M8: Cryptographic Signature Tamper Rejection
- **Preconditions:** Test endpoint serves valid 1.6.1 installer, but `latest.json` has a tampered or corrupted signature string (e.g. modify one base64 character in `signature`).
- **Action:** Click **Check now** and click **Install and restart**.
- **Expected Outcome:**
  - Updater downloads payload, verifies Minisign signature against public key, and detects mismatch.
  - Installer execution is **aborted immediately**.
  - App remains on v1.6.0. Status reports signature verification failure. No untrusted binary is executed.

### M9: Version Monotonicity (Older/Equal Rejection)
- **Preconditions:** Application is on version 1.6.0. Server serves `latest.json` with `version: "1.6.0"` or `version: "1.5.1"`.
- **Action:** Click **Check now**.
- **Expected Outcome:** Manifest is parsed, but version is not strictly greater than current version (`1.6.0`). UI reports *"LiveAudio is up to date (LiveAudio está actualizado)"*. No download or install is offered.

---

## 6. Out-of-Repository Follow-Up

- **Landing Page Privacy Policy Update (`/privacy`):**
  The public website at `https://liveaudio.opencohost.com/privacy` resides in an external repository (`liveaudio-web`). As an out-of-repo follow-up to this release, update the website's privacy disclosure to reflect that LiveAudio checks GitHub Releases for new versions every 24h, audio/transcripts never leave the machine, and checks can be toggled off in settings.
