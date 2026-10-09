# Track firstuse-startup-progress_20260905 — Tasks

> APROBADO por PO (7 decisiones, 2026-09-05, Engram #6439). Implemented and reviewed in the working tree; manual M1–M8 remain pending evidence.
> Each ticket references its REQ-ID(s). Full acceptance template per ticket
> (Given/When/Then/Error Path/UI State/OBS Behavior) lives in `spec.md` under Acceptance Criteria.

- [x] Task: T1 — Structured download progress across the supervisor hop (REQ-1, REQ-2, REQ-5)
  - [x] Re-verify `supervisor.py:238-244` mapping, `engine.py:339-369` parse, and `app.py:1720-1728` consumer before designing.
  - [x] Specify honest `downloading` state + legacy mirror + `{phase,percent,attempt,code}` wire event (monotonic 0–100, reset once per attempt).
  - [x] Harden tqdm parse (float percent, clamp, staleness signal, indeterminate fallback); pill with `%`, no bar.
  - [x] Acceptance: see `spec.md` T1 block (Given/When/Then/Error Path/UI State/OBS Behavior).

- [x] Task: T2 — Conservative startup/stall watchdog (REQ-3, REQ-6)
  - [x] Re-verify `supervisor.py:248-283` liveness-only supervision.
  - [x] Specify stall 120–180s + informative absolute deadline + manual retry as new attempt (never kill with recent progress; slow → warning only).
  - [x] REQ-6 DECIDIDO opción a heartbeat pre-import (PO 2026-09-05, Engram #6444): implementado en F1 de Fixes de cierre; sin accepted-gap pendiente.
  - [x] Acceptance: see `spec.md` T2 block (Given/When/Then/Error Path/UI State/OBS Behavior).

- [x] Task: T3 — Actionable provisioning error codes (REQ-4)
  - [x] Re-verify `engine.py:471-472` cache-exception label paths before designing.
  - [x] Specify `provision-*` catalog (cache-corrupt, network, auth, disk-full, timeout-stalled, tls, unknown; reserve `model-not-found` for real absence) with ES/EN copy keys.
  - [x] Acceptance: see `spec.md` T3 block (Given/When/Then/Error Path/UI State/OBS Behavior).

- [x] Task: T4 — TLS-context scoping + prewarm copy (REQ-7, REQ-8)
  - [x] Scope the `engine.py:434-438` override via `audio.py:157-179` save/restore (or per-request context) with an exact-scope code comment; surface `provision-tls`; no stack migration.
  - [x] Keep prewarm=true default; ship toggle with explicit ES/EN first-use download/network copy.

## Fixes de cierre (review colectiva 2026-09-05, Engram #6444 — PO: heartbeat + widget)

- [x] F1 — Heartbeat pre-import (REQ-6 opción a): `workers.run_asr` emite `{"type":"status","key":"asr","state":"active","phase":"importing"}` vía put_nowait antes del import pesado; supervisor lo resuelve a `loading` + refresca watchdog; comentario accepted-gap actualizado.
- [x] F2 — Reintentar cableado: botón `asr_retry_action` visible solo en stalled/failed → `request_asr_retry()` (attempt nuevo, % a 0 una vez, luego monotónico); la GUI consume `attempt` y descarta eventos de intentos viejos. Sin auto-retry.
- [x] F3 — Widget prewarm en GUI: switch con copy aprobada ES/EN que lee/escribe la misma config que `--prewarm/--lazy` (default true sin cambios).
- [x] F4 — Docs: README + GETTING_STARTED (tabla de estados, códigos provision-* con remediation, tiempos por modelo) + HISTORIAL_CAMBIOS (entrada bilingüe) + matriz manual M1-M8.

## Matriz manual M1-M8 (ejecutable, precondiciones + OBS esperado)

Precondiciones generales: build/instalación con esta rama, internet disponible salvo M5,
OBS con Browser Source `subtitulos_obs.html`, modelo `small` salvo indicación.

- [ ] M1 — Descarga visible con % monotónico. Precond: caché sin el modelo (renombra la
  carpeta de caché o elige un modelo no descargado). Pasos: inicia y observa el pill ASR.
  Esperado: pill `ASR: descargando N%` con N 0–100 sin retroceder; OBS: sin subtítulos
  aún, sin errores.
- [ ] M2 — Progreso lento: solo aviso, nunca kill. Precond: descarga en curso (M1).
  Esperado: aunque avance lento, el proceso ASR sigue vivo y el % avanza; OBS: silencio
  hasta `ready`, sin bursts.
- [ ] M3 — Stall → stalled + Reintentar. Precond: solo prueba — `startup_stall_sec: 5`
  en config (revertir después) y descarga colgada o caché bloqueada. Esperado: pill
  `ASR: detenido. Pulsa Reintentar.` + botón Reintentar visible; OBS: silencio, sin burst
  al recuperar (manda la política de backlog).
- [ ] M4 — Reintentar crea attempt nuevo. Precond: estado M3. Pasos: pulsa Reintentar.
  Esperado: pill vuelve a cargando/descargando desde 0% una vez y luego monotónico;
  eventos viejos no pisan el % nuevo; OBS: silencio hasta `ready`.
- [ ] M5 — Fallo accionable con código. Precond: sin internet. Pasos: inicia con modelo
  no cacheado. Esperado: pill con hint de una línea (`provision-network`: revisa tu red)
  + log con código y clase sanitizada, sin tracebacks en UI; OBS: sin subtítulos
  fantasma/parciales.
- [ ] M6 — Toggle prewarm persiste. Precond: app cerrada. Pasos: pestaña Modelo,
  cambia el switch prewarm, Aplica, cierra y reabre. Esperado: el switch conserva el
  valor (= `prewarm` en config); default true intacto en instalación fresca; OBS: sin cambios.
- [ ] M7 — Compat legacy OpenCohost. Precond: modo servicio con `--health-file`.
  Esperado: el snapshot lleva `asr_state` honesto + `asr_state_legacy` colapsado a
  `loading/ready/failed`; OBS/overlay sin cambios.
- [ ] M8 — Recuperación sin burst. Precond: M3+M4 con backlog `auto`. Esperado: al
  volver a `ready`, los subtítulos continúan en el borde en vivo (máx. 256 en `send_all`),
  sin volcado del historial; OBS: flujo normal.
  - [x] Acceptance: see `spec.md` T4 block (Given/When/Then/Error Path/UI State/OBS Behavior).
