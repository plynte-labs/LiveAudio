# Project Tracks

This file tracks major LiveAudio tracks. Each track should have its own detailed plan in `conductor/tracks/<track_id>/` when managed through Conductor.

---

- [ ] **Track: Test-suite & launch-readiness cleanup (collateral — DEFERRED)**
  *Link: N/A — noted follow-up; no track folder yet.*
  *Status: 🔲 proposed. Collateral issues surfaced during the VAD/subtitle work, intentionally deferred OUT of their tracks (not bugs introduced by them): (1) `.atl/skill-registry.md` carries absolute Windows paths (`C:\Users\...`) after a `gentle-ai skill-registry refresh`, failing `tests/test_public_launch_readiness.py::test_skill_registry_has_no_absolute_windows_paths` — regenerate with relative paths or revert that working-tree change. (2) Pre-existing `tests/test_audio.py::TestVadThresholdEnforcement` + `tests/test_noise_detection.py` re-implement the VAD check locally with `>=`, contradicting the production strict `>` — route through a shared helper so the suite documents one correct boundary. (3) Adaptive-ribbon LOW edges (from Track B re-judge, not blocking): (a) the SINGLE↔RIBBON flap can recur only when `subtitle_catchup_interval_sec > ~5.65s` (slider max is 10s, but all profiles use 0.8–2.0s and default 1.5s) — consider tying the slider max to ~5s; (b) a non-replay live line interleaved between spaced replay payloads momentarily demotes (`subtitulos_obs.html` adaptive enqueue `replayActive = !!isReplay`) — arguable "caught up" semantics, decide if it should latch instead.*

---

- [~] **Track: Modo servicio backend headless (opencohost)**
  *Link: [./tracks/service-backend_20260903/](./tracks/service-backend_20260903/)*
  *Status: in progress. Unidad 1 implementada sin commit en `feature/service-backend` (T1-T6, T8 hechos; T7/T9/T10 parciales; TBD-1..TBD-5 cerrados por auditor; corrección normativa `base..base+9`). Segunda unidad pendiente: auto-discovery VoiceAI + changelog espejo.*

---

- [ ] **Track: WebSocket IPC Auth Token (Anti-Eavesdropping & Anti-Impersonation)**
  *Link: [./tracks/ws-ipc-auth-token_20260911/](./tracks/ws-ipc-auth-token_20260911/)*
  *Status: 🔲 proposed / spec drafted. Scope confirmed with PO (Option 1: mandatory in headless mode, optional toggle in desktop GUI). Scaffolding created in `conductor/tracks/ws-ipc-auth-token_20260911/` (`spec.md` + `plan.md`).*

---

- [ ] **Track: Multi-Source Capture with Source Tagging**
  *Link: [./tracks/multi-source-capture_20261015/](./tracks/multi-source-capture_20261015/)*
  *Status: new — pending PO decisions. Draft spec + plan from the owner's Spanish draft; PO decisions 1-4 open with recommendations; all tasks unchecked. Verification notes in spec.md: single device per instance CONFIRMED; WASAPI loopback in cpal_source.rs CONFIRMED (device enumeration lives in device.rs); server.rs:635-655 DIFFERENT (replay burst at 648-673, PTT residue NOT FOUND); audio_device CONFIRMED.*

---

- [~] **Track: In-App Updater for Desktop Application (Tauri 2)**
  *Link: [./tracks/in-app-updater/](./tracks/in-app-updater/)*
  *Status: In Progress (`[~]`). Phases 1-5 complete (backend updater, stream safety lock, bilingual UI, CI signing & manifest assembly, documentation). Final VM test matrix verification pending release candidate.*


