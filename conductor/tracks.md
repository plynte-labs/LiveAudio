# Project Tracks

This file tracks major LiveAudio tracks. Each track should have its own detailed plan in `conductor/tracks/<track_id>/` when managed through Conductor.

---

- [x] **Track: Configurable OBS subtitle backlog policy**
  *Link: N/A - completed before Conductor track artifacts existed.*
  *Status: completed before Conductor initialization. Commit: `e6575b2`.*

- [x] **Track: Specialized agent team workflow**
  *Link: N/A - completed before Conductor track artifacts existed.*
  *Status: completed before Conductor initialization. Commit: `1644b11`.*

- [x] **Track: Settings profiles and apply flow**
  *Link: N/A - completed before Conductor track artifacts existed.*
  *Status: completed before Conductor initialization. Commit: `26598e8`.*

---

- [x] **Track: Critical Bug Fixes — Resilience & Stability**
  *Link: [./tracks/critical-bugfixes_20260509/](./tracks/critical-bugfixes_20260509/)*
  *Status: completed. Commit: `05bf60b`.*

---

- [x] **Track: Subtitle Style System v2**
  *Link: [./tracks/subtitle-style-system_20260509/](./tracks/subtitle-style-system_20260509/)*
  *Status: completed. SDD formal workflow with 3 chained PRs. 18 tasks, 242 tests, 7 presets.*

---

- [x] **Track: Dynamic Internationalization (i18n) & Settings Reorganization**
  *Link: N/A - completed directly to optimize workspace open-source transition.*
  *Status: completed. Real-time dynamic i18n for English/Spanish, translated preview panels, dynamic status pills, and merged Profiles settings tab. Commits: `5a6fad8`, `c7dc1c3`, `9b5ea8f`.*

---

- [x] **Track: Portable Packaging, CUDA 12 Fixes & Application Resilience**
  *Link: [./tracks/portable-packaging-resilience_20260606/](./tracks/portable-packaging-resilience_20260606/)*
  *Status: completed. Upgraded PyTorch to CUDA 12.1 (cu121), injected DLL paths into bat/exe environments, implemented async update checks (every 24hs), and global crash diagnostics window redirecting to GitHub.*

---

- [ ] **Track: ASR Language Separation with Dual Context Prompts**
  *Link: [./tracks/asr-language-separation_20260601/](./tracks/asr-language-separation_20260601/)*
  *Status: 🔲 pending manual verification. Implementation complete, 47 stress tests, PR #1 approved. Non-blocking follow-up for public launch because the app remains functional today.*
  *Pending: manual test of migration, UI dropdown, prompt swap, and live inference language switch.*

---

- [ ] **Track: Experimental Moonshine ASR Integration**
  *Link: [./tracks/moonshine-asr-integration_20260607/](./tracks/moonshine-asr-integration_20260607/)*
  *Status: 🔲 proposed/pending. Research and baseline integration of the low-latency Moonshine model as an alternative to Whisper.*
