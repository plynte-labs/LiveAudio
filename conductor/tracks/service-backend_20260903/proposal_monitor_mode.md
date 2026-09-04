# Proposal & Design — GUI Monitor Mode & Safe Config Persistence

Track: `service-backend_20260903` · Status: draft · Type: architecture-proposal

## 1. Context & Problem Statement

When OpenCohost starts LiveAudio as an external headless microservice (`liveaudio --service --parent-pid <OPENCOHOST_PID>`), it becomes the sole process owner. The service acquires `service.lock` (`InstanceLock`), starts a WebSocket server on loopback `127.0.0.1:8765`, and claims the audio microphone and VRAM resources for ASR transcription.

If the operator subsequently launches the CustomTkinter desktop GUI (`liveaudio` / `main.py`):
1. Currently, `app.py` has no awareness of `service.lock` or active background services.
2. The GUI initializes with `is_running = False` ("INICIAR SISTEMA"), appearing idle.
3. If the user clicks "INICIAR SISTEMA", port fallback engages (`8766`), and the GUI spawns a second full audio capture pipeline and a second Whisper model instance in VRAM.

Furthermore, if the user modifies settings in the GUI and requests a restart:
- Attempting to kill and respawn the service from the GUI results in **Ownership Hijacking**: the new service would have `--parent-pid <GUI_PID>`. As soon as the GUI is closed, the service terminates, breaking OpenCohost PTT.
- Attempting to kill the service from the GUI triggers a **Supervisor Race Condition**: OpenCohost supervisor watchdog immediately detects child termination and attempts to respawn at the exact same instant as the GUI, racing on `service.lock` and port binding.

## 2. Architectural Decisions & Guiding Principles

1. **Owner-Bound Lifecycle (Single Responsibility)**:
   - The process parent (`--parent-pid`) is the sole lifecycle supervisor.
   - Closing the LiveAudio GUI must NEVER stop or disrupt the service spawned by OpenCohost.
2. **No Ownership Usurpation**:
   - The GUI must never kill or respawn a service owned by another parent process.
3. **Hardware Resource Protection**:
   - Only one Whisper / VAD pipeline may run on the host system to prevent VRAM exhaustion and microphone capture contention.
4. **Passive Config Persistence**:
   - Modifications in GUI are safely saved to `config.json`.
   - Applying changes to an active service owned by OpenCohost is deferred to the next natural restart of OpenCohost, or triggered through OpenCohost own supervisor.

## 3. Detailed Design

### 3.1 Startup Detection in GUI (`liveaudio/app.py`)

On application startup, before initializing UI controls:
1. Instantiate `InstanceLock`:
   ```python
   from liveaudio.service.lock import InstanceLock
   lock = InstanceLock()
   service_active = lock._holder_alive()
   ```
2. If `service_active` is `True`:
   - Enter **Monitor Mode** (`self.service_managed = True`).

### 3.2 Monitor Mode UI State

When in Monitor Mode:
1. **Power Button**:
   - Text: `"Servicio activo (OpenCohost)"`.
   - Disabled for starting a duplicate engine.
   - Distinct color (e.g., `#2E7D32` dark green, non-clickable or status-only).
2. **Status Chips**:
   - Status indicators reflect that the service is running in background.
   - Optional: GUI opens a read-only WS connection to `127.0.0.1:8765` to display real-time live transcription in the console/preview pane without creating an audio capture thread.
3. **Settings & Persistence**:
   - The user can adjust settings (VAD threshold, language, model size, OBS styles).
   - Clicking "Guardar / Aplicar":
     - Persists normalized settings to `config.json`.
     - Displays informational banner / alert:
       `"Configuración guardada en disco. Se aplicará la próxima vez que inicies OpenCohost o reinicies el servicio."`
     - Does NOT attempt to terminate or restart the running service.
4. **Window Close Behavior**:
   - Closing the GUI (X button) closes only the GUI window.
   - The background service and OpenCohost connection remain completely unaffected.

## 4. Acceptance Criteria

- **AC-M1**: If `service.lock` is held by an active PID, opening the CTK GUI shows Monitor Mode without error.
- **AC-M2**: Clicking "INICIAR SISTEMA" while service is active is prevented; no duplicate Whisper model or second WS server is spawned.
- **AC-M3**: Changes saved in GUI are written to `config.json` without killing or restarting the service.
- **AC-M4**: Closing the CTK GUI leaves the headless service running under its original parent PID.
