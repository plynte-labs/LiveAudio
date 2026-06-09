# Specification: Dynamic Internationalization (i18n) & Settings Reorganization

## 1. Goal
Provide a seamless multilingue experience (Spanish/English) that allows streamers to change the app's visual language dynamically without restarting background ASR threads. Optimize UI layouts to prevent horizontal clipping, improve the user experience by merging settings tabs, ensure continuous local visual transcription feedback even when OBS WebSocket is disabled, and transition the project to an open-source MIT licensed codebase.

## 2. Requirements

### REQ-1: Dynamic Language Switch (i18n)
- **Visual Translation:** All labels, buttons, headers, tooltips, and informational texts in the GUI must change immediately when toggling between English and Spanish.
- **Hot-swappable dropdown choices:** Lists such as audio devices, backlog policies, and settings profiles must display their translated equivalents dynamically without crashing config mapping states.
- **Process Independent:** Toggling visual language must NOT restart the ASR core processes, WASAPI audio loopback, or Silero VAD queues.

### REQ-2: Settings Tab Reorganization
- **Tab Reduction:** Merge the separate "Perfiles" (Profiles) tab into the "Avanzado" (Advanced) tab.
- **Layout:** Position the profile selection dropdown at the top of the Advanced tab, separated from advanced variables by a clean, subtle visual divider line.
- **No Disruptions:** Remove the redundant language selector from the Advanced tab, placing the global locale toggle exclusively in the primary sidebar.

### REQ-3: Local Transcription Preview Fallback
- **No Black Box:** Streamers must receive direct feedback. When "Send subtitles to OBS" is toggled off:
  - `core/engine.py` must continue capturing WASAPI audio, running VAD, and invoking Whisper ASR.
  - Core transcript logs and events must still be broadcast to the IPC queue.
  - The local live preview panel in `main.py` must render transcription outputs in real-time.
- **Overlay State:** The external HTML overlay (`subtitulos_obs.html`) will not receive subtitle payloads, but the local visualizer remains fully alive.

### REQ-4: Dynamic Technical Status Mappings
- **IPC Translation:** Background developer logs sent via multiprocessing IPC queues should remain in their technical/developer format (Spanish) to avoid parsing overhead.
- **Technical Status Pills:** The status bar metrics (`Audio: listening`, `VAD: phrase sent`, `OBS: connected`, etc.) must be dynamically translated based on the active GUI language.

### REQ-5: Open Source MIT License Migration
- **Standardization:** Change license terms in `LICENSE` and all source files to the MIT License under the Plynte Open-Source Organization.
- **Copyright Header:** Standardize headers across Python source files.

### REQ-6: Portable Windows Bundle Setup
- **Standalone Build:** Configure PyInstaller options to bundle Python Embedded, dependencies (PyTorch, Silero VAD, faster-whisper), assets, and layout configurations.
- **Automation:** Provide `build_portable.py` and batch compilation scripts to streamline builds.

## 3. Acceptance Criteria

- **AC-1:** Toggling between Spanish and English updates all GUI text, headers, sidebar, logs preview, and status pills in real time.
- **AC-2:** Selecting a profile from the dropdown at the top of the Advanced tab updates settings instantly.
- **AC-3:** With OBS disabled, speaking into the audio device triggers transcription outputs immediately in the local preview panel.
- **AC-4:** Running unit tests (`pytest tests/test_main.py`) passes successfully without regressions.
