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
  *Status: pending manual verification. Implementation complete on `master`, 47 stress tests. Historical pre-migration PR reference removed; do not treat old personal-fork PR links as current release status. Non-blocking follow-up for public launch because the app remains functional today.*
  *Pending: manual test of migration, UI dropdown, prompt swap, and live inference language switch.*

---

- [ ] **Track: Experimental Moonshine ASR Integration**
  *Link: [./tracks/moonshine-asr-integration_20260607/](./tracks/moonshine-asr-integration_20260607/)*
  *Status: 🔲 proposed/pending. Research and baseline integration of the low-latency Moonshine model as an alternative to Whisper.*

---

- [~] **Track: VAD Onset Grace & Configurable Silero Pre-roll**
  *Link: [./tracks/vad-onset-grace_20260619/](./tracks/vad-onset-grace_20260619/)*
  *Status: [~] In Progress on branch `feature/vad-onset-grace` (strict TDD). Was: proposed. Make the Silero VAD onset pre-roll configurable (today hardcoded `pre_buffer=deque(maxlen=3)` ≈ 96ms at audio.py:308) so soft word onsets stop getting clipped; expose `vad_speech_pad_ms` and `vad_threshold`. INDEPENDENT — touches only audio.py/config.py/app.py; can run in parallel with the subtitle tracks.*

---

- [~] **Track: Subtitle Legibility & Animation Polish (OBS Overlay)**
  *Link: [./tracks/subtitle-legibility-anim_20260619/](./tracks/subtitle-legibility-anim_20260619/)*
  *Status: [~] In Progress (strict TDD; batched on the `feature/vad-onset-grace` working tree, to be split into `feature/subtitle-legibility-anim` at commit time). Was: proposed. Reconcile the `--sub-animation-duration` mismatch (0.2s CSS vs 0.4s JS), raise `.style-minimal` legibility, lift the small-source font clamp floor, and cap per-word reveal stagger so long phrases appear fast. Edits `subtitulos_obs.html`. SHARES that file with the ribbon track → must land FIRST.*

---

- [~] **Track: Vertical Ribbon Subtitle Buffer for OBS Overlay**
  *Link: [./tracks/subtitle-ribbon-buffer_20260619/](./tracks/subtitle-ribbon-buffer_20260619/)*
  *Status: [~] Implemented + dual-reviewed across 3 judgment-day cycles (final: pass-with-notes, state machine sound + memory safe); pending commit/merge. Default `adaptive`, strict TDD, node runtime harness, batched working tree. ADAPTIVE vertical "ribbon" buffer: one subtitle at a time under normal pacing, auto-stacks the N most-recent lines ONLY when subtitles accumulate/queue up (keyed off the existing `pendingQueue` + `is_replay` catch-up signals), then collapses back — so steady speech never shows an oversized text block (today `showSubtitle()` shows one at a time, subtitulos_obs.html:418). Modes `single|ribbon|adaptive`, **default `adaptive`** (LOCKED). Direction (OD-1, newest-on-top) + default/threshold (OD-6) + engine-hint defer (OD-5) RESOLVED. Edits `subtitulos_obs.html` → DEPENDS on the legibility track landing first (workflow.md:49 overlap rule).*

---

- [ ] **Track: Test-suite & launch-readiness cleanup (collateral — DEFERRED)**
  *Link: N/A — noted follow-up; no track folder yet.*
  *Status: 🔲 proposed. Collateral issues surfaced during the VAD/subtitle work, intentionally deferred OUT of their tracks (not bugs introduced by them): (1) `.atl/skill-registry.md` carries absolute Windows paths (`C:\Users\...`) after a `gentle-ai skill-registry refresh`, failing `tests/test_public_launch_readiness.py::test_skill_registry_has_no_absolute_windows_paths` — regenerate with relative paths or revert that working-tree change. (2) Pre-existing `tests/test_audio.py::TestVadThresholdEnforcement` + `tests/test_noise_detection.py` re-implement the VAD check locally with `>=`, contradicting the production strict `>` — route through a shared helper so the suite documents one correct boundary. (3) Adaptive-ribbon LOW edges (from Track B re-judge, not blocking): (a) the SINGLE↔RIBBON flap can recur only when `subtitle_catchup_interval_sec > ~5.65s` (slider max is 10s, but all profiles use 0.8–2.0s and default 1.5s) — consider tying the slider max to ~5s; (b) a non-replay live line interleaved between spaced replay payloads momentarily demotes (`subtitulos_obs.html` adaptive enqueue `replayActive = !!isReplay`) — arguable "caught up" semantics, decide if it should latch instead.*
