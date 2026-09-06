# Track firstuse-startup-progress_20260905 — Specification

> Status: **IMPLEMENTADO en rama `feature/firstuse-startup-progress` (sin commit). REQ-6 decidido opción a heartbeat pre-import (PO 2026-09-05, Engram #6444).**
> Source: read-only exploratory audit (Front 1) + 4 consultas de diseño + aprobación PO (Engram #6439).
> Implementado T1–T4 + fixes de cierre F1–F4 (ver tasks.md); F1 supervisor consume `attempt` y rechaza eventos stale (QA re-review Engram #6445).

## Outcome

First-use startup shows honest, actionable Whisper provisioning state instead of a
generic loading/error collapse, so a streamer on a clean machine knows whether the
model is downloading (with real progress), stalled, cached-but-broken, or failed.

## Quick path

1. Read Decisions first (what the PO approved).
2. Implement per REQ-ID using the wire format + state machine below.
3. Prove each ticket with its Given/When/Then block; record line drift in Findings.

## Decisions (PO-approved 2026-09-05, Engram #6439)

| # | Decision |
|---|---|
| 1 | INTERMEDIA: endurecer parse tqdm + `%` estructurado `{phase,percent,attempt,code}`. `snapshot_download` queda como spike posterior (Out of Scope). |
| 2 | Compat: `asr_state` honesto (`downloading/loading/transcribing/ready/stalled/failed`) + campo espejo legacy que colapsa a `loading/ready/failed` para OpenCohost. |
| 3 | Watchdog CONSERVADOR en supervisor: stall = 120–180s sin ningún evento/progreso → `stalled` + Reintentar manual; progreso lento → solo aviso; NUNCA kill con progreso reciente. Retry = attempt nuevo, `%` resetea a 0 una sola vez, luego monotónico (clamp 0–100, reset solo si attempt cambia). |
| 4 | TLS ACOTADO: engine copia patrón save/restore de `audio.py:157-179` (o contexto por-request) + código `provision-tls`; sin migrar stacks HTTP. |
| 5 | Prewarm=true se mantiene + toggle con copy ES/EN explícita de descarga/red en primer uso. |
| 6 | Pill con `%` monotónico + fallback indeterminado (sin barra). |
| 7 | "Modelo no encontrado" RESERVADO a ausencia real; resto → catálogo `provision-*` con hint de una línea, sin tracebacks en UI. |
| Abierto → DECIDIDO (a) | REQ-6 = heartbeat pre-import (opción a, PO 2026-09-05, Engram #6444): el hijo ASR emite `{"type":"status","key":"asr","state":"active","phase":"importing"}` vía put_nowait ANTES del import pesado torch/faster-whisper en `workers.run_asr`; el watchdog trata silencio-desde-provisioning_started_at como stalled-import. Prewarm = widget switch en la GUI con la copy aprobada (no CLI-only). |

## Overview

The service supervisor maps every active ASR engine state — including an in-flight
transcription and a download carrying a percentage — to a single `asr_state=loading`
and drops the download progress on the floor (verified:
`liveaudio/service/supervisor.py:238-244`). Model construction has no application-level
startup/stall deadline; the supervisor only watches process liveness
(`liveaudio/service/supervisor.py:248-283`). A broad cache exception is mislabeled as
"model missing" (`liveaudio/core/engine.py:471-472`), and all failures collapse to a
generic ASR error / `failed` state instead of actionable provisioning codes. The engine
already parses a best-effort percentage from tqdm output (`liveaudio/core/engine.py:339-369`)
and the GUI consumes the `is_download` flag (`liveaudio/app.py:1720-1728`) — future work
must harden that existing progress path, not claim progress is absent. The GUI child WS has
periodic health checks, but the ASR worker import precedes engine status emission with no
analogous observed health check on the GUI child. The engine forces the default HTTPS
context to unverified permanently with no restore (`liveaudio/core/engine.py:434-438`;
contrast save/restore pattern in `liveaudio/core/audio.py:157-179` — Engram #6438).
Prewarm defaults to true (`ProcessSupervisor.__init__ prewarm=True`). Whether capture
starting early is acceptable is a product boundary clarified by Decision 5, not a
demonstrated privacy bug. Probes reproduced transcription→loading collapse and progress
loss with stdlib-only AST-extracted tests — no service/GPU/mic/network/writes.

## Functional Requirements (APROBADAS)

- **REQ-1 (APROBADA, D2):** Supervisor MUST emit honest `asr_state`
  (`downloading/loading/transcribing/ready/stalled/failed`) instead of collapsing every
  active state to `loading` (`supervisor.py:238-244`). MUST also emit legacy mirror field
  collapsing to `loading/ready/failed` for OpenCohost compat.
- **REQ-2 (APROBADA, D1/D2):** Download progress MUST travel as a structured event
  `{phase,percent,attempt,code}` (see Wire format) from engine → supervisor → GUI,
  preserving the existing tqdm source (`engine.py:339-369`).
- **REQ-3 (APROBADA, D3):** Model construction MUST have a CONSERVATIVE supervisor
  watchdog: stall = 120–180s without any event/progress → `stalled` + manual Retry;
  slow-but-progressing → warning only, NEVER kill. Plus an absolute informative startup
  deadline (advisory copy + retry offer, not a kill). Independent of the process-liveness
  watchdog (`supervisor.py:248-283`).
- **REQ-4 (APROBADA, D7):** Cache failures MUST NOT be labeled "model missing" except on
  real absence (`engine.py:471-472`). Each provisioning failure class MUST map to the
  `provision-*` catalog below with a one-line ES/EN hint; unknown → `provision-unknown`
  with sanitized exception class, never a raw traceback.
- **REQ-5 (APROBADA, D1/D3/D6):** The existing tqdm parse (`engine.py:339-369`) MUST be
  hardened: float percent 0–100, monotonic clamp per attempt (reset to 0 exactly once on
  attempt change), stale-progress detection feeding the REQ-3 watchdog, indeterminate
  fallback text when unparseable. GUI pill shows `Downloading N%`; no progress bar.
- **REQ-6 (DECIDIDA opción a, PO 2026-09-05, Engram #6444):** el hijo ASR emite
  un heartbeat pre-import `{"type":"status","key":"asr","state":"active",
  "phase":"importing"}` vía put_nowait en `workers.run_asr` ANTES del import
  pesado torch/faster-whisper. El supervisor lo resuelve a `loading` honesto y
  refresca el reloj del watchdog; el silencio desde provisioning_started_at se
  trata como stalled-import. La GUI lo renderiza como cargando, nunca en blanco.
- **REQ-7 (APROBADA, D4):** Engine TLS override (`engine.py:434-438`) MUST copy the
  save/restore pattern from `audio.py:157-179` (or use a per-request context) and surface
  TLS failures as `provision-tls`. MUST NOT migrate HTTP stacks.
- **REQ-8 (APROBADA, D5):** Prewarm default true is KEPT. A toggle with explicit ES/EN
  copy about first-use download/network behavior MUST ship alongside.

## Wire format (progress event, APROBADO D1/D2/D3)

```json
{"type": "status", "key": "asr", "state": "downloading",
 "phase": "downloading", "percent": 42.5, "attempt": 2,
 "code": null, "is_download": true, "text": "ASR: descargando 42%",
 "asr_state_legacy": "loading"}
```

Rules: `percent` float 0–100; `attempt` int, new attempt on every manual retry;
`percent` resets to 0 exactly once when `attempt` changes, then MUST be monotonic
(clamp, never regress); `code` null on success path, else a `provision-*` code;
`is_download` kept for current GUI consumer (`app.py:1720-1728`);
`asr_state_legacy` collapses honest state → `loading/ready/failed` for OpenCohost;
`text` stays human-readable, never parsed back (no reverse-matched literals for logic).

## State machine (APROBADA D2/D3)

States: `downloading → loading → transcribing → ready`, plus `stalled` and `failed`
from any provisioning state. `stalled` entered ONLY after 120–180s with zero
events/progress; exits via manual retry (new `attempt`) or resumed progress.
`failed` carries a `provision-*` code. Legacy mirror: `downloading/loading/transcribing/stalled`
→ `loading`; `ready` → `ready`; `failed` → `failed`.

## Error catalog with ES/EN copy (propuesta, REQ-4/REQ-7)

| Code | ES | EN |
|---|---|---|
| `model-not-found` (RESERVED, real absence only) | "Modelo no encontrado en caché ni en remoto." | "Model not found in cache or remote." |
| `provision-cache-corrupt` | "Caché del modelo dañada. Reintenta para redescargar." | "Model cache is corrupt. Retry to re-download." |
| `provision-network` | "Sin conexión. Revisa tu red y reintenta." | "No connection. Check your network and retry." |
| `provision-auth` | "Acceso denegado al descargar. Revisa credenciales." | "Download access denied. Check credentials." |
| `provision-disk-full` | "Disco lleno. Libera espacio y reintenta." | "Disk full. Free space and retry." |
| `provision-timeout-stalled` | "La descarga tardó demasiado. Reintenta." | "Download took too long. Retry." |
| `provision-tls` | "Fallo de conexión segura. Reintenta." | "Secure connection failed. Retry." |
| `provision-unknown` | "Error inesperado al preparar el modelo." | "Unexpected error while preparing the model." |

UI rule: error pill + one remediation line, no tracebacks (sanitized class only in logs).

## Defaults (APROBADOS D3)

- Stall window: 120–180s (default inside range, configurable) with zero events/progress.
- Slow progress → warning copy only, never kill/restart.
- Absolute startup deadline: informative ("taking longer than expected — still working / retry",
  localized) + retry offer; never kills a progressing download alone.
- Retry: manual button, new `attempt`, single `%` reset to 0, then monotonic.

## Non-Functional Requirements

- No mic/GPU/network/service use in verification probes; stdlib-only AST-extracted
  reproductions stay the pattern for this track's evidence phase.
- Progress rendering must be monotonic and never move backward except on retry reset.
- Error codes must be stable strings suitable for docs + support triage.
- Privacy: sanitized technical summaries only; no audio/transcripts/secrets/PII.

## Acceptance Criteria

### T1 — Preserve download progress across the supervisor hop (REQ-1, REQ-2, REQ-5)

- Given: a Whisper download emitting tqdm-style percentage lines
- When: the supervisor pumps the log queue and the GUI renders status
- Then: the GUI pill shows Downloading N% (0–100, monotonic per attempt) via the structured event
- Error Path: unparseable lines fall back to indeterminate "downloading…" text, never a crash or a frozen 0%
- UI State: status pill reads Downloading N% (localized key, not a reverse-matched literal)
- OBS Behavior: unchanged; subtitles keep flowing or stay silent per current backlog policy

### T2 — Conservative startup/stall watchdog (REQ-3, REQ-6)

- Given: a hung or extremely slow model download/load with a live process
- When: 120–180s pass with zero events/progress (or the absolute informative deadline expires)
- Then: the supervisor surfaces `stalled` (or advisory copy) with code `provision-timeout-stalled` and offers manual retry; liveness watchdog alone is no longer the only signal
- Error Path: expiry never kills a merely-slow-but-progressing download without the stall condition also firing
- UI State: user sees "taking longer than expected — still working / retry" (localized), not a permanent spinner
- OBS Behavior: no subtitle burst on recovery; backlog policy governs catch-up

### T3 — Actionable provisioning error codes (REQ-4)

- Given: each failure class (cache-corrupt, network, auth, disk-full, timeout, TLS)
- When: provisioning fails
- Then: the UI shows the matching `provision-*` code with its one-line ES/EN hint; "model missing" appears ONLY on real absence
- Error Path: unknown exceptions map to `provision-unknown` with the sanitized exception class, never a raw traceback
- UI State: error pill + remediation line, localized in ES/EN
- OBS Behavior: no partial/phantom subtitles emitted for failed provisioning

### T4 — TLS-context scoping + prewarm copy decision (REQ-7, REQ-8)

- Given: current forced-unverified default HTTPS context + prewarm default true
- When: reviewed under the track's security pass
- Then: the unverified default is scoped (save/restore per `audio.py:157-179` or per-request context) with a code comment citing the exact scope; TLS failures use `provision-tls`; prewarm ships the PO-signed toggle copy
- Error Path: TLS failures surface as TLS-specific errors, not generic ASR errors
- UI State: prewarm toggle/label copy states capture + download behavior explicitly
- OBS Behavior: unchanged

## Out of Scope (APROBADO — no implementar)

- `snapshot_download` migration: queda como spike posterior documentado, NO parte de este track (D1).
- Mirror selection, resume-protocol changes, download-speed optimization.
- Migración de stacks HTTP/TLS más allá del save/restore acotado (D4).
- Any specific model-download transport behavior beyond what the probes reproduced.
- That the Hugging Face HTTP stack inherits the engine's unverified context (unproven — cross-ref only).
- That prewarm autostart is a privacy violation (product boundary, resolved by D5 copy).
- Any leak/allocator claim about model memory.

## Findings

| File:line | Evidence | Severity | Blocking? |
|---|---|---|---|
| `liveaudio/service/supervisor.py:238-244` | All active ASR states (incl. transcribing + % download) mapped to `asr_state=loading`; progress discarded. Quote: `mapped = {"active": "loading", "ok": "ready", "error": "failed"}.get(state)`. Verified verbatim on `develop @ e004ae3`. | High | Yes (for T1) |
| `liveaudio/service/supervisor.py:248-283` | `_check_children`/`poll_once` supervise process liveness only; no startup/stall deadline for model construction observed. Verified verbatim. | High | Yes (for T2) |
| `liveaudio/core/engine.py:367,467-491,539,708` | Model construction / status emission paths cited by audit; broad cache exception mislabeled as model-missing; failures collapse to generic ASR error + `failed`. Auditor-verified; re-verify exact lines before implementation (drift note below). | Medium | No (T3 covers) |
| `liveaudio/app.py:1693-1728` | GUI best-effort tqdm percentage parse EXISTS — harden, don't re-introduce. Auditor-verified; re-verify exact lines before implementation. | Medium | No (T1/T2 cover) |
| GUI child WS health vs ASR import gap | GUI child WS has periodic health checks; ASR worker import precedes engine status emission with no analogous observed GUI-child health check. Auditor-verified observation. | Low | No |
| Engine forced-unverified HTTPS default | Engine forces default HTTPS context to unverified; cross-ref VAD acquisition (`audio.py:150-193`). Do NOT assert HF stack inherits it. | Medium | No (T4 covers) |
| Prewarm audio+ASR autostart | Prewarm starts capture + ASR; product-boundary copy question, not a proven privacy bug. | Low | No (T4 covers) |

> Drift note: `supervisor.py:238-244` and `:248-283` verified byte-for-byte on
> `develop @ e004ae3` during track writing. `engine.py` / `app.py` line numbers above
> are carried from the verified audit context; the implementing agent MUST re-confirm
> each exact line before editing and record any drift here.
>
> Drift note 2026-09-05 (verified pre-approval update, no code touched): `supervisor.py:238-244`
> and `:248-283` re-verified verbatim on checkout; `engine.py:434-438` SSL override without
> restore re-verified; `audio.py:157-179` save/restore pattern re-verified; prewarm default true
> re-verified (`supervisor.py:78,93`); cache mislabel `engine.py:471-472` re-verified.
> DRIFT: tqdm parse lives in `liveaudio/core/engine.py:339-369` (`InterceptingWriter._handle_progress`,
> emits `state:"active"` + `is_download:True` with percent embedded in text, no structured
> percent/attempt/code fields, throttle-only 0.1s, no monotonic clamp) — NOT a GUI parse.
> `liveaudio/app.py:1720-1728` is consumer-only (`is_download` flag + reverse-matched
> `"cargando"/"descargando"` literals, no `%`/`tqdm` parse; grep for tqdm/progreso/percent
> in `app.py` finds nothing). Original spec row citing `app.py:1693-1728` as the parse location
> is therefore corrected: harden `engine.py:339-369`, keep `app.py:1720-1728` as the consumer
> to update. `_emit_status` (`engine.py:174-178`) confirms the wire gap: only
> `{type,key,text,state}`, no percent/attempt/code. Transcription collapse confirmed via
> `engine.py:539` (`"ASR: transcribiendo"`, `state:"active"`).

## Traceability

- Engram observation **#6428** + session **#6432** (Front 1: first-use / startup /
  Whisper progress audit).
- Engram observation **#6438** (engine TLS override never restored vs VAD save/restore).
- Engram observation **#6439** (PO aprobó las 7 decisiones de diseño, 2026-09-05).
- Cross-ref: `vad-silence_20260905` (TLS acquisition), `i18n-completeness_20260905`
  (localized progress/error strings must not use reverse-matched literals).
