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

- [x] **Track: VAD Onset Grace & Configurable Silero Pre-roll**
  *Link: [./tracks/vad-onset-grace_20260619/](./tracks/vad-onset-grace_20260619/)*
  *Status: completed. Made the Silero VAD onset pre-roll configurable (replaced the hardcoded `pre_buffer=deque(maxlen=3)` ≈ 96ms at audio.py:308) so soft word onsets stop getting clipped; exposed `vad_speech_pad_ms` and `vad_threshold`. Merged via PR #4 (2026-06-20), commits `67bf887` / `4d86eba` on master.*

---

- [x] **Track: Subtitle Legibility & Animation Polish (OBS Overlay)**
  *Link: [./tracks/subtitle-legibility-anim_20260619/](./tracks/subtitle-legibility-anim_20260619/)*
  *Status: completed. Reconciled the `--sub-animation-duration` mismatch (0.2s CSS vs 0.4s JS), raised `.style-minimal` legibility, lifted the small-source font clamp floor, and capped per-word reveal stagger so long phrases appear fast. Edits `subtitulos_obs.html`. Merged via PR #7 (2026-06-20), commit `ab20acc`; PR #5 (earlier version of this same branch, targeted at `feature/vad-onset-grace`) was closed/superseded.*

---

- [x] **Track: Vertical Ribbon Subtitle Buffer for OBS Overlay**
  *Link: [./tracks/subtitle-ribbon-buffer_20260619/](./tracks/subtitle-ribbon-buffer_20260619/)*
  *Status: completed. Implemented + dual-reviewed across 3 judgment-day cycles (final: pass-with-notes, state machine sound + memory safe). Default `adaptive`, strict TDD, node runtime harness. ADAPTIVE vertical "ribbon" buffer: one subtitle at a time under normal pacing, auto-stacks the N most-recent lines ONLY when subtitles accumulate/queue up (keyed off the existing `pendingQueue` + `is_replay` catch-up signals), then collapses back — so steady speech never shows an oversized text block. Modes `single|ribbon|adaptive`, **default `adaptive`** (LOCKED). Direction (OD-1, newest-on-top) + default/threshold (OD-6) + engine-hint defer (OD-5) RESOLVED. Merged via PR #6 (2026-06-20), commit `c414edb`, landing after the legibility track as required.*

---

- [ ] **Track: Test-suite & launch-readiness cleanup (collateral — DEFERRED)**
  *Link: N/A — noted follow-up; no track folder yet.*
  *Status: 🔲 proposed. Collateral issues surfaced during the VAD/subtitle work, intentionally deferred OUT of their tracks (not bugs introduced by them): (1) `.atl/skill-registry.md` carries absolute Windows paths (`C:\Users\...`) after a `gentle-ai skill-registry refresh`, failing `tests/test_public_launch_readiness.py::test_skill_registry_has_no_absolute_windows_paths` — regenerate with relative paths or revert that working-tree change. (2) Pre-existing `tests/test_audio.py::TestVadThresholdEnforcement` + `tests/test_noise_detection.py` re-implement the VAD check locally with `>=`, contradicting the production strict `>` — route through a shared helper so the suite documents one correct boundary. (3) Adaptive-ribbon LOW edges (from Track B re-judge, not blocking): (a) the SINGLE↔RIBBON flap can recur only when `subtitle_catchup_interval_sec > ~5.65s` (slider max is 10s, but all profiles use 0.8–2.0s and default 1.5s) — consider tying the slider max to ~5s; (b) a non-replay live line interleaved between spaced replay payloads momentarily demotes (`subtitulos_obs.html` adaptive enqueue `replayActive = !!isReplay`) — arguable "caught up" semantics, decide if it should latch instead.*

---

- [~] **Track: Automatic WebSocket Port Fallback & Overlay Endpoint Identification**
  *Link: N/A - no Conductor track folder created; tracked via PR #12.*
  *Status: open PR #12 (`feat/ws-port-fallback` → `master`), commit `7d82cc5`. Server-side bind fallback walks `base..base+9` on `EADDRINUSE`/WinSock `10048` and announces the effective port to the GUI; the OBS overlay now identifies its server via a `hello` handshake before rendering anything, closing the "wrong socket" gap behind issue #9's incident. Closes issue #11. Manually validated end-to-end against real OBS.*
  *Pending: review and merge.*

---

- [~] **Track: Independent Output Sink Toggles & UI WebSocket Port**
  *Link: N/A - no Conductor track folder created; tracked via PR #13.*
  *Status: open PR #13 (`feat/output-sink-toggles`), stacked on #12. Commit `491eb52`. Adds independent `save_transcript_enabled` / `save_vtt_enabled` disk toggles (transcript persistence was previously unconditional) and a `ws_port` field in the UI, warning when the base port changes since a pinned overlay only scans `base..base+9`.*
  *Pending: merge of #12, then review and merge.*

---

- [~] **Track: OBS Overlay Visibility Fix (Hidden-Scene Reveal)**
  *Link: N/A - no Conductor track folder created; tracked via PR #14.*
  *Status: open PR #14 (`fix/overlay-visibility`), stacked on #12. Commit `4d6bac4`. Subtitles never revealed while the OBS scene was hidden because the reveal was gated behind `requestAnimationFrame`, which browsers suspend when the document is hidden, while the expiry `setTimeout`s kept firing. Replaced with a synchronous reflow force; added a `visibilitychange` hard reset so a resumed overlay never replays what was missed.*
  *Pending: merge of #12, then review and merge.*

---

- [~] **Track: Defer Manager Startup for Faster Launch**
  *Link: N/A - no Conductor track folder created; tracked via PR #15.*
  *Status: open PR #15 (`perf/defer-manager-startup`), stacked on #13 (itself stacked on #12). Commit `315eeed`. Moved `mp.Manager()` out of `LiveASRApp.__init__` into a lazy accessor, removing a measured 455-478ms from every launch that was previously paid even when the user never pressed Start.*
  *Pending: merge of #12 and #13, then review and merge.*

---

- **Note: stale branch triage**
  *Status: local branch `audit-ui-security-privacy` deleted on 2026-07-24 after triage — one docs-only commit (`dcdf5c8`), never pushed to the remote; all five findings from its audit report verified as already implemented on master.*
