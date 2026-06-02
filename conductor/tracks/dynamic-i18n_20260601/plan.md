# Implementation Plan: Dynamic Internationalization (i18n) & Settings Reorganization

This document outlines the design decisions and step-by-step implementation completed to deliver the dynamic i18n, settings layout compression, local preview fallback, and MIT open-source transition.

## 1. Design Decisions

### i18n Decoupling
To avoid restarting the multi-process engine when toggling the GUI language, we implemented a decoupled translation layer. The core processes continue using static configuration profiles, while the UI dynamically fetches labels from `utils/i18n.py` using `t(key, locale)` and updates Tkinter string variables (`tk.StringVar`).

### Settings Layout Reorganization
By merging the "Perfiles" (Profiles) tab into the "Avanzado" (Advanced) tab, we reduced the sidebar tab count from 5 to 4. This resolves UI clipping issues on narrow screens. The profile picker is placed at the top of the Advanced panel with a horizontal divider separating it from the other settings.

### Continuous local transcription preview
Previously, the engine skipped transcribing when the OBS WebSocket was disabled, causing a "black box" where the user couldn't see if the system was functioning. We separated local UI logging from the WebSocket output. The engine now continuously outputs transcription text to the main queue for GUI rendering, even when "Send subtitles to OBS" is off.

---

## 2. Implementation Steps

### Step 1: Standardize Locale Engine (`utils/i18n.py`)
- Created comprehensive dictionary mapping Spanish and English labels.
- Added translation utility `t(key, locale)` and a mechanism to get translated values for settings choice lists dynamically.

### Step 2: Clean and Reorganize GUI (`main.py`)
- Reduced settings sidebar tabs from 5 to 4.
- Moved Profiles dropdown widgets to the top of the Advanced tab, separated by a visual divider line.
- Removed the language dropdown from the Advanced tab, consolidating the locale switch exclusively in the sidebar.
- Bound dynamic language updates to GUI elements, updates headers, log toggles, preview texts, and dropdown widgets.

### Step 3: Enable continuous local preview (`core/engine.py`)
- Modified engine queue loops to always broadcast transcripts to the GUI queue even if OBS integration is disabled.
- Modified the main UI update loops in `main.py` to render ASR transcriptions to the local preview widget.

### Step 4: IPC technical status updates (`main.py`)
- Localized dynamic background process statuses (`Audio: listening`, `VAD: phrase sent`, etc.) on the main window status bar on the fly.

### Step 5: Transition all codebases to the MIT License
- Created `update_licenses.py` to rewrite license headers to MIT across Python files.
- Replaced the repository `LICENSE` file.

### Step 6: Package configuration (`build_portable.py` & batch scripts)
- Set up PyInstaller specifications and batch scripts to build a completely self-contained, standalone Windows portable package.
