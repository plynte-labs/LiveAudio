# Track unified-first-run_20260905 — Specification

> **Status:** Planning baseline; implementation is partial/interrupted. See `handoff.md` for current execution state; task checkboxes are not yet reconciled.
> **Dependency:** `firstuse-startup-progress_20260905` is implemented and reviewed in the current working tree, with manual M1–M8 still pending. This track may consume that behavior but must not mark M1–M8 complete.
> **Scope:** A unified first-run experience across the stdlib/tkinter launcher and the LiveAudio app. The transport remains two sequential processes; the continuity is visual and semantic.

## Outcome

On a clean machine, a user sees one truthful, bilingual checklist from installation selection through readiness. The launcher hands off a validated installation snapshot, closes after its bounded visual handoff, and the app paints early before heavy imports. Subsequent VAD, Whisper download/load, retry, and failure states remain visible without invented percentages, unsafe process replacement, or silent recovery.

## Quick path

1. Create the launcher/app phase vocabulary and acceptance contract below.
2. Implement with TDD, preserving the existing first-use changes and unrelated dirty files.
3. Run focused tests, compileall, four-agent review, and rebuild VM v2.
4. Run automated validation for its own task evidence; mark manual E2E rows only after E2E-1…E2E-10 have real evidence.

## Existing evidence and traceability anchors

| Evidence | Current behavior | Planning implication |
|---|---|---|
| `packaging/launcher.py:909-947` | Detached `Popen`, environment handoff, `LIVEAUDIO_HOME`, `HF_HOME`, `LIVEAUDIO_LAUNCHER`. | Add an atomic `handoff.json`; do not add runtime IPC. |
| `packaging/launcher.py:1257-1313` | Bootstrap runs source/uv work, launches the app, then reports `1.0`. | `1.0` must not mean ASR ready; only real byte/tqdm evidence may produce a percentage. |
| `packaging/launcher.py:1335-1359` | Windows GUI uses a bounded `FindWindowW` visual barrier. | Keep the bounded visual barrier if useful, but never wait for ASR `ready`; launcher is not the ASR supervisor. |
| `packaging/launcher.py:71-76`, `:950-984`; `liveaudio/app.py:261-340` | Window title is `Plynte LiveAudio`; app paints after construction. | Preserve title detection compatibility while moving app paint before heavy imports. |
| `packaging/launcher.py:831-894` | `uv sync` advances heuristically on output keywords and caps at `0.95`. | Use honest indeterminate state for sync; expose detail text, not a fake percentage. |
| `liveaudio/core/audio.py:146-193` | Silero VAD load has status/log messages but no byte/tqdm progress. | Put VAD in `provision-*`, heartbeat periodically, classify errors, and offer phase retry. |
| `docs/PACKAGING_AND_UPDATES.md:30-40` | Documents current splash/window behavior and launcher exit. | Update docs after implementation; distinguish window-visible from ASR-ready. |

## Product decisions (PO-approved, 2026-09-05)

| ID | Decision | Requirement mapping |
|---|---|---|
| D1 | “Unified” means visual and semantic continuity, not transport unification. | REQ-1, REQ-2 |
| D2 | Use Option C: two sequential windows; same checklist, copy, and state language; no new IPC; do not keep the launcher alive until ASR `ready`. | REQ-1, REQ-3 |
| D3 | Launcher writes an atomic handoff snapshot with exactly `hf_home`, `install_root`, `extra`, `app_version`, `attempt`, and `launcher_phases_done`. | REQ-4, REQ-5 |
| D4 | The app paints early, before heavy imports, then continues later phases itself. | REQ-9 |
| D5 | VAD joins the `provision-*` catalog with honest indeterminate state, periodic heartbeat, `network/tls/cache-corrupt` errors, and retry for that phase. | REQ-7, REQ-8 |
| D6 | Never display a simulated percentage. | REQ-6, REQ-7 |
| D7 | `uv sync`, VAD, and post-download loading use indeterminate state plus an honest message. | REQ-6, REQ-7 |
| D8 | Percentages are allowed only when backed by real bytes or tqdm evidence. | REQ-6 |
| D9 | Launcher copy is fully bilingual ES/EN and uses the same vocabulary as the app. Use the existing local config/install-location persistence with an allowlisted optional `language` value (`es` or `en`); it is not a seventh handoff field. | REQ-2, REQ-3, REQ-9 |
| D10 | `--reinstall` preserves `hf-cache` unconditionally in this MVP. No purge UI or purge command is added; model purging requires separate future PO approval. Lock retention with regression tests. | REQ-10 |
| D11 | Option A is rejected: no launcher Whisper pre-download, launcher remains stdlib + tkinter, and no duplicate `huggingface_hub` or provisioning catalog. | REQ-11 |
| D12 | Option B (live launcher + IPC) is a later spike and is outside this MVP. | REQ-12 |
| D13 | `snapshot_download`, mirrors, custom resume behavior, and fake percentages are out of scope. | REQ-13 |

## Approved experience checklist

The same ordered checklist and state semantics must be recognizable in ES and EN:

| Phase | ES | EN | Owner |
|---:|---|---|---|
| 0 | Selección de instalación | Installation selection | Launcher |
| 1 | Preparando uv | Preparing uv | Launcher |
| 2 | Código de LiveAudio | LiveAudio code | Launcher |
| 3 | Instalando dependencias | Installing dependencies | Launcher |
| 4 | Abriendo LiveAudio | Opening LiveAudio | Launcher → App |
| 5 | Preparando VAD | Preparing VAD | App |
| 6 | Descargando/cargando Whisper | Downloading/loading Whisper | App |
| 7 | Listo para iniciar | Ready to start | App |

The launcher may finish and close after its own phases and bounded visual handoff. “Opening LiveAudio” means the app window is being opened, not that Whisper is ready. The app owns phases 5–7 and must expose them after early paint.

## Functional requirements

- [ ] **REQ-1 — Unified phase model:** Define one ordered phase vocabulary for launcher and app while retaining separate process ownership and transport.
- [ ] **REQ-2 — Bilingual copy:** Provide complete ES/EN copy for every phase, indeterminate state, error, retry, and ready message. Persist only an allowlisted optional `language` (`es` or `en`) in `install_location.json`; never add language to the handoff payload.
- [ ] **REQ-3 — Sequential-window boundary:** Keep launcher and app as two sequential windows. The launcher may use the existing bounded title-visible barrier, but must never wait for ASR `ready`, share queues, or introduce IPC.
- [ ] **REQ-4 — Atomic handoff:** Before `Popen`, write `handoff.json` atomically in the same directory using a temporary sibling, flush/fsync, and replace. The payload contains exactly the six approved fields; no logs, audio, transcript, PII, secret, or private path beyond the required installation paths.
- [ ] **REQ-5 — Handoff validation:** The app must reject malformed, stale, or path-overridden snapshots safely, including invalid types, unsupported attempts, mismatched installation root, and fields that escape the expected install/cache roots. Invalid handoff falls back to safe local config without crashing.
- [ ] **REQ-6 — Truthful launcher progress:** `uv sync`, VAD, and post-download loading are indeterminate unless real byte/tqdm evidence exists. Launcher global completion must not be presented as total readiness.
- [ ] **REQ-7 — VAD provisioning:** Classify VAD as `provision-*`; expose indeterminate state, a periodic non-blocking heartbeat, localized remediation, and retry only for the VAD phase. Stop the heartbeat in `finally` on success, failure, cancellation, and shutdown.
- [ ] **REQ-8 — VAD failure/replacement safety:** Surface `provision-network`, `provision-tls`, and `provision-cache-corrupt` distinctly. Remove the existing VAD insecure TLS fallback (`audio.py:153-179`) so TLS failures remain truthful. For a pre-ready failure, report the failed phase, stop it cooperatively, join it, prove that it can no longer write, and then replace the producer while preserving the same queue objects and ASR process. Never replace or reuse a queue while a writer is live. A live stalled producer is not forcibly terminated: report `stalled`, use bounded cooperative exit when possible, or require an explicit full stop/restart when necessary; never silently restart ASR.
- [ ] **REQ-9 — Early app paint:** Paint the app checklist before heavy torch/faster-whisper/VAD imports, then continue phases 5–7 and show heartbeat/error/retry states.
- [ ] **REQ-10 — Reinstall cache safety:** `--reinstall` preserves `hf-cache` and its contents/hashes unconditionally in this MVP. Add regression tests for retention; do not add a purge UI or command.
- [ ] **REQ-11 — Launcher boundary:** Keep the launcher stdlib + tkinter; do not pre-download Whisper and do not duplicate Hugging Face integration or the provisioning catalog.
- [ ] **REQ-12 — No new IPC:** Do not implement the live-launcher/IPC spike in this track. The app observes the atomic snapshot and local state only.
- [ ] **REQ-13 — Explicit non-goals:** Do not add `snapshot_download`, mirrors, custom resume protocol, or simulated progress. Document these exclusions in the implementation notes.

## Handoff contract

### Snapshot schema

`handoff.json` is an ephemeral local coordination artifact, not a session record. It contains exactly:

```json
{
  "hf_home": "<approved local cache root>",
  "install_root": "<approved install root>",
  "extra": "<existing supported extra>",
  "app_version": "<launcher-resolved version>",
  "attempt": 1,
  "launcher_phases_done": [0, 1, 2, 3]
}
```

The launcher writes phases 0–3 only: installation selection, uv preparation, code, and dependency installation. It snapshots before `Popen`; it does not claim phase 4 or app readiness. The app may observe the snapshot locally and marks phase 4 only after its own window/checklist paints. No runtime IPC is added.

### Atomic write and validation

- Write a temporary sibling in the same directory, flush and `fsync`, then `os.replace`.
- Never log the payload. Logs may contain only sanitized error classes/codes.
- Validate exact field set, types, supported `extra`, positive `attempt`, version shape, phase range/order, install-root equality, and cache paths confined to approved roots.
- Reject stale attempts or mismatched versions safely; do not accept path overrides from the payload.
- On malformed/stale data, continue with safe local configuration and show an indeterminate handoff warning, not a traceback.

## State vocabulary

| Semantic state | ES | EN | Percentage rule |
|---|---|---|---|
| Indeterminate provisioning | `Preparando…` | `Preparing…` | No percentage |
| Real download | `Descargando N%` | `Downloading N%` | Only bytes/tqdm; monotonic per attempt |
| Loading | `Cargando…` | `Loading…` | No percentage |
| Heartbeat | `Sigue trabajando…` | `Still working…` | No percentage |
| Ready | `Listo para iniciar` | `Ready to start` | Terminal state |
| Retry | `Reintentar fase` | `Retry phase` | New phase attempt only |

Error codes use the existing stable `provision-*` catalog and add VAD coverage without relabeling real absence. UI shows one remediation line and no traceback.

## Technical boundaries

- **Language persistence:** the existing stdlib `install_location.json` writer (`packaging/launcher.py:194-229`) and locked config save path (`liveaudio/utils/config.py:366-405, :530-541`) are the persistence mechanisms. Add only an optional allowlisted `language` value (`es|en`) to `install_location.json`; do not add it to `handoff.json`. The launcher UI default reads a valid `config.json.language` from the resolved home read-only, then valid `install_location.json.language`, then OS detection. A launcher selection is atomically persisted to `install_location.json` immediately before handoff/Popen. On launcher-origin startup, apply `location.language` only when the existing `LIVEAUDIO_LAUNCHER` flag, install-root equality, and accepted handoff all validate; load the full normalized config and change only `language` when different, then save through the existing locked atomic config path. The environment flag alone is not trusted as authentication. Direct app starts prefer valid config language, then location language, then autodetection. No app-to-location mirror is needed because the next launcher reads the app config first. Lightweight launcher copy may use a stdlib shared file or bounded ES/EN dictionaries with parity tests; it must not duplicate the provisioning/Hugging Face catalog. This design is implementation-ready pending ordinary test coverage.
- **VAD:** current load occurs before stream processing and before audio queue puts (`audio.py:146-195`, queue work begins later around `:256-265`). A pre-ready failed producer must be reported, stopped cooperatively, joined, and proven unable to write before replacement; preserve the same queue objects and ASR process. A live stalled producer cannot be safely force-retried; report `stalled`, stop cooperatively when possible, or require an explicit full stop/restart. Heartbeat cleanup must run in `finally`.
- **TLS:** remove the VAD-specific insecure fallback rather than adding another fallback. Preserve truthful TLS errors and classify them.
- **Cache:** preserve `hf-cache` unconditionally on reinstall and test hashes before/after. Do not add a purge action or claim a new fix until the current behavior is verified.
- **OBS:** provisioning must not emit partial subtitles or bursts. Existing backlog policy and legacy ASR mirror remain compatible.
- **Privacy:** do not persist raw audio, transcripts, sessions, secrets, PII, or private diagnostic payloads in handoff, logs, or Engram.

## Acceptance criteria by ticket

### T1 — Shared phase vocabulary (REQ-1, REQ-2, REQ-3)

- **Given:** launcher and app are running sequentially in either supported language.
- **When:** the user advances through phases 0–7.
- **Then:** both surfaces use the same ordered phase names and distinguish window-visible from ASR-ready.
- **Error Path:** invalid or missing language data follows the documented precedence and falls back to OS detection, logging a sanitized code only; unrelated config fields remain unchanged.
- **UI State:** one current phase, one honest message, and a visible next/terminal state in ES or EN.
- **OBS Behavior:** no subtitle output before the app is ready; existing backlog behavior remains unchanged.

### T2 — Launcher handoff and bounded visual barrier (REQ-3, REQ-4, REQ-5)

- **Given:** bootstrap phases 0–3 completed and the installed app executable exists.
- **When:** the launcher writes the snapshot and starts the app.
- **Then:** the snapshot is atomically readable, validated by the app, and the launcher exits after its bounded visual handoff without waiting for ASR `ready`.
- **Error Path:** malformed/stale snapshot, process exit, or window timeout produces a localized actionable error and safe fallback; no crash or false ready.
- **UI State:** launcher shows “Opening LiveAudio”; app shows its early checklist independently.
- **OBS Behavior:** no WebSocket/OBS side effects from the launcher handoff.

### T3 — Handoff integrity and privacy (REQ-4, REQ-5)

- **Given:** an atomic snapshot may be missing, truncated, stale, or path-manipulated.
- **When:** the app reads it.
- **Then:** exact schema, roots, version, attempt, and phase checks decide accept/reject before use.
- **Error Path:** reject safely, use local defaults, and expose only a sanitized `provision-handoff` message.
- **UI State:** “Preparando estado de instalación…” / “Preparing installation state…” remains indeterminate.
- **OBS Behavior:** no audio, transcript, or subtitle payload is written or broadcast.

### T4 — Honest uv/dependency progress (REQ-6, REQ-11, REQ-13)

- **Given:** `uv sync` is slow, silent, or emits non-progress status lines.
- **When:** the launcher receives output.
- **Then:** it shows indeterminate status and useful detail; a percentage appears only from real bytes/tqdm evidence.
- **Error Path:** timeout/network/TLS errors map to stable `provision-*` codes with retry guidance.
- **UI State:** “Installing dependencies…” / “Instalando dependencias…” without a fake percentage or fake global 100%.
- **OBS Behavior:** no OBS connection or subtitle activity is started by dependency installation.

### T5 — Early app paint (REQ-9)

- **Given:** the launcher has started the app and heavy imports may take a long time.
- **When:** app startup begins.
- **Then:** the checklist window paints before heavy torch/faster-whisper/VAD work and then advances locally.
- **Error Path:** an import failure is localized and retryable where safe, without leaving a blank/loading-only window.
- **UI State:** “Abriendo LiveAudio” / “Opening LiveAudio” transitions to “Preparando VAD” / “Preparing VAD” with a heartbeat.
- **OBS Behavior:** no subtitles until the runtime is genuinely ready.

### T6 — VAD indeterminate provisioning (REQ-7, REQ-8)

- **Given:** Silero VAD is absent, cached, downloading, loading, or slow.
- **When:** VAD preparation runs.
- **Then:** status is indeterminate, heartbeat is periodic/non-blocking, and `provision-network`, `provision-tls`, or `provision-cache-corrupt` is classified accurately.
- **Error Path:** retry only the VAD phase; preserve ASR attempt/process, stop/join a pre-ready producer before replacement, prove no remaining writes, preserve the same queues, and avoid forced termination of a live stalled producer.
- **UI State:** localized phase message, heartbeat, one remediation line, and `Retry phase` when retryable.
- **OBS Behavior:** no partial subtitles or burst during VAD retry/recovery.

### T7 — Real Whisper progress and retry (REQ-6, REQ-7)

- **Given:** Whisper download emits real tqdm/byte evidence or only load/import activity.
- **When:** app processes the event stream or a retry is requested.
- **Then:** real percentage is monotonic per attempt; load/import stays indeterminate; retry resets only its phase attempt once.
- **Error Path:** stale events are rejected; unparseable progress becomes indeterminate, never zero/fake progress.
- **UI State:** `Downloading N%`, `Loading…`, `Still working…`, or localized retry/error copy.
- **OBS Behavior:** ASR/OBS remains silent until ready and does not replay a burst after recovery.

### T8 — Reinstall cache preservation (REQ-10)

- **Given:** an install has an existing `hf-cache` with known hashes.
- **When:** the user invokes `--reinstall`.
- **Then:** cache files remain unchanged; this MVP has no purge action.
- **Error Path:** failed preservation check stops safely and explains the required action; no silent deletion.
- **UI State:** reinstall progress is honest and confirms cache retention.
- **OBS Behavior:** reinstall is offline from OBS/audio/session state.

### T9 — Launcher scope and exclusions (REQ-11, REQ-12, REQ-13)

- **Given:** a clean launcher build.
- **When:** it installs or reopens LiveAudio.
- **Then:** it remains stdlib + tkinter, does not pre-download Whisper, add IPC, duplicate Hugging Face/catalog logic, select mirrors, or invent resume behavior.
- **Error Path:** unsupported option is rejected with a clear out-of-scope message.
- **UI State:** only approved checklist phases are shown.
- **OBS Behavior:** no change to browser-source transport or backlog policy.

### T10 — Bilingual end-to-end readiness (REQ-1, REQ-2, REQ-9)

- **Given:** ES or EN is selected and the machine is clean, partial-cache, or warm-cache.
- **When:** all phases complete or one fails and retries.
- **Then:** every launcher/app label, error, retry, and ready state is localized consistently.
- **Error Path:** missing translation uses a safe fallback and is reported before closure.
- **UI State:** final state is `Listo para iniciar` / `Ready to start` only after the app completes its own phases.
- **OBS Behavior:** subtitle behavior is unchanged and safe through startup/retry.

## Manual E2E matrix

No row may be marked complete without attached real evidence. `[ ]` means planned/not run.

| ID | Preconditions | Actions | Expected | Evidence |
|---|---|---|---|---|
| [ ] E2E-1 clean ES CPU | Clean VM; CPU extra; ES; no model/VAD cache | Install and start | Phases 0–7 are visible, honest, and end ready; no fake percentages | [ ] Screenshot/log/package ID |
| [ ] E2E-2 clean EN CUDA | Clean VM; EN; CUDA-capable machine; if CUDA unavailable mark not run | Install and start | Same semantics in EN; CUDA path or explicit not-run record | [ ] Screenshot/log or not-run reason |
| [ ] E2E-3 handoff integrity | Valid, malformed, stale snapshots; window timeout/process exit simulation | Start each case | Valid snapshot paints early; invalid/stale rejects safely; timeout/exit is actionable | [ ] Snapshot fixtures + logs |
| [ ] E2E-4 slow/silent uv | Slow and silent `uv sync` output | Bootstrap | Indeterminate truthful status; no fake percent/global-ready claim | [ ] Timestamped launcher capture |
| [ ] E2E-5 VAD failure/retry | Network/TLS failure and corrupt VAD cache | Start VAD; retry phase | Correct `provision-*`, heartbeat, retry, safe producer boundary | [ ] Logs/screenshots + process evidence |
| [ ] E2E-6 Whisper progress/retry | Real download and stale-event fixture | Download, stall/fail, retry | Real monotonic percent only; stale attempt rejected; loading indeterminate | [ ] Structured events + UI capture |
| [ ] E2E-7 warm/partial/offline cache | Fully warm cache, partial cache, offline | Start each state | Warm start skips downloads; partial/offline states remain honest and recoverable | [ ] Cache hashes + logs |
| [ ] E2E-8 reinstall preservation | Existing `hf-cache` with recorded hashes | Run `--reinstall` without any purge option | Cache remains present and hashes are unchanged; no purge UI or command exists | [ ] Before/after hash manifest |
| [ ] E2E-9 post-download/retry/localization | Download complete; slow load; prewarm switch; localized failures | Load, toggle, retry | Post-download loading is indeterminate; prewarm failure/retry is localized and phase-scoped | [ ] Screenshots + config diff |
| [ ] E2E-10 OBS regression | Browser Source; backlog modes; start/stop/hot-swap | Run startup/retry/recovery and normal session | No bursts, legacy state remains compatible, OBS connect/reconnect and hot-swap regressions absent | [ ] OBS recording/log + checklist |

## Validation and closure gates

- [ ] Add/update TDD tests with RED → GREEN → REFACTOR ordering.
- [ ] Include resilience/idempotency tests named `test_resilience_*.py` / `test_idempotent_*.py`.
- [ ] Run focused tests, then `python -m compileall liveaudio packaging/launcher.py tests`, plus the exact relevant test commands determined by the implementer.
- [ ] Update `README.md`, `docs/GETTING_STARTED.md`, `HISTORIAL_CAMBIOS.md`, and `docs/PACKAGING_AND_UPDATES.md` during implementation.
- [ ] Complete collective review by Architecture, Performance, QA, and Research roles.
- [ ] Rebuild VM v2; keep `build_artifacts/vm-test/` ignored and unversioned.
- [ ] Execute E2E-1…E2E-10 and attach evidence; do not mark manual rows or M1–M8 complete without real evidence.
- [ ] No commit, stage, push, merge, or revert without explicit approval.

Historical evidence from the dependency track: 30 focused tests and 66 resilience/idempotency tests passed, plus compileall, but these were not rerun for this planning-only artifact and do not close this track.

## Out of scope

- [ ] `snapshot_download`, mirrors, custom resume protocol, or download transport migration.
- [ ] Launcher-side Whisper pre-download.
- [ ] A live launcher kept open until ASR `ready`.
- [ ] New runtime IPC or a live-launcher/IPC implementation (Option B spike only).
- [ ] Duplicate Hugging Face integration or provisioning catalog in the launcher.
- [ ] Simulated percentages, including treating launcher global `100%` as total readiness.
- [ ] Any model/VAD purge UI or command in this MVP; future purging requires separate PO approval.
- [ ] Changes to OBS transport or backlog semantics beyond startup safety.

## Traceability

| PO decision | Requirements | Planned tests/manual evidence |
|---|---|---|
| D1–D2 | REQ-1, REQ-3 | T1, T2, T9; E2E-1–3 |
| D3 | REQ-4, REQ-5 | T2, T3; E2E-3 |
| D4 | REQ-9 | T5; E2E-1, E2E-3 |
| D5 | REQ-7, REQ-8 | T6; E2E-5, E2E-9 |
| D6–D8 | REQ-6, REQ-7 | T4, T7; E2E-4, E2E-6, E2E-7 |
| D9 | REQ-2, REQ-9 | T1, T10; E2E-1, E2E-2, E2E-9 |
| D10 | REQ-10 | T8; E2E-8 |
| D11–D13 | REQ-11–REQ-13 | T9; E2E-4, E2E-8 |
