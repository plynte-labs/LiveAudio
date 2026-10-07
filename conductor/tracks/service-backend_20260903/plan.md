# Plan — Modo servicio backend headless (opencohost)

Track: `service-backend_20260903` · Workflow: `conductor/workflow.md` (Dream Team SDD Protocol)

> Solo documentación en este track-step. La implementación corre con `conductor-implement` cuando el PO lo apruebe.

## Fase 0 — Intake & Rapid Planning (completada)

- [ ] Task: Consolidar decisiones PO + hallazgos de los 4 agentes en spec
  - [ ] Verificar 5 decisiones PO cerradas contra memoria Engram (`liveaudio`)
  - [ ] Consolidar hallazgos Arquitectura / Performance / QA / Research sin omitir riesgos
- [ ] Task: Conductor - User Manual Verification 'Fase 0' (Protocol in workflow.md)
  - [ ] Verificación manual: PO confirma que spec.md refleja las 5 decisiones y los TBD-1..TBD-6 tienen dueño

## Fase 1 — Documentation Delegation & Track Scaffolding (actual)

- [ ] Task: Escaffold del track `service-backend_20260903`
  - [ ] Crear `spec.md`, `plan.md`, `tasks.md`, `metadata.json`, `index.md`
  - [ ] Registrar track en `conductor/tracks.md` sin commit
- [ ] Task: Cerrar TBDs bloqueantes de testeabilidad (TBD-1..TBD-6 con auditor/PO)
  - [ ] TBD-1 control plane fuera de TCP
  - [ ] TBD-2 formato estado de carga lazy
  - [ ] TBD-3 política fallo de carga modelo
  - [ ] TBD-4 defaults config ausente/corrupta
  - [ ] TBD-5 ruta/rotación health-file
  - [ ] TBD-6 colisión 8765 / discovery lado lector
- [ ] Task: Revisión auditor de docs (gaps, términos, matriz M1-M10 mapeada a tasks)
- [ ] Task: Conductor - User Manual Verification 'Fase 1' (Protocol in workflow.md)
  - [ ] Verificación manual: abrir `spec.md`/`plan.md`/`tasks.md`, confirmar trazabilidad REQ→ticket y que no hay secretos/PII

## Fase 2 — Implementation Delegation (futura, no ejecutar ahora)

- [x] Task: Crear rama `feature/service-backend` (auditor, tras aprobación)
  - [x] Actualizar `conductor/tracks.md` a `[~] In Progress`
- [x] Task: Implementar `EngineService` sin CTK (workers + Manager dict read-only)
  - [x] Extraer servicio reutilizando `run_audio`/`run_asr`/`run_ws_server`
  - [x] Prohibir `save_config` / persistencia de normalización en el servicio (`load_config_readonly`)
- [x] Task: Implementar CLI `liveaudio --service --parent-pid PID` + supervisor (`freeze_support`, graceful shutdown, backoff, techo 3/5min, recrear colas)
- [x] Task: Implementar watchdog dueño-por-proceso (Windows `OpenProcess` / POSIX `kill pid 0`)
- [x] Task: Implementar ASR Prewarm por defecto + opción --lazy + transiciones de estado de carga
  - [x] Prewarm por defecto (--prewarm) con opción --lazy; transiciones `asr_state` (starting -> loading -> ready); pre-listo responde indisponible/cargando (stt_loading)
- [x] Task: Implementar fallback `base..base+9` + fail-fast + anuncio `hello`/`health` (reutilizar PR #12 si ya está en master)
- [~] Task: Implementar backpressure 64KB/retry-10, reciclaje ASR a N=3, mitigación replay sin cota
  - [x] Backpressure/retry intactos + replay acotado (256, drop-oldest, `ws.replay_drops`); [~] reciclaje a N=3 queda como deuda documentada
- [x] Task: Implementar health stdout JSON-lines + health-file opcional sin transcripts + métricas
- [~] Task: Escribir tests `test_resilience_*.py` / `test_idempotent_*.py` + huecos (`hello.port` efectivo, scan fallback)
  - [x] 45 tests gate en verde; [~] matriz manual M1-M10 y sign-offs para Fase 3
- [~] Task: Actualizar docs usuario (`README`, `GETTING_STARTED`, `WEBSOCKET_OBS`, `HISTORIAL_CAMBIOS`) + glosario
  - [x] Docs LiveAudio del comportamiento implementado; [~] changelog espejo VoiceAI = segunda unidad
- [ ] Task: Conductor - User Manual Verification 'Fase 2' (Protocol in workflow.md)
  - [ ] Verificación manual: `python -m compileall main.py core utils`; matriz M1-M10 ejecutada; `send_all` documentado como burst; sin regresión CTK 3 pasos

## Fase 3 — Collective Review & Sign-Off (futura)

- [ ] Task: Review 4 agentes (Architecture / Performance / QA / Research) con sign-off estructurado
  - [ ] Reasignar áreas ante timeout (documentar motivo)
- [ ] Task: Verificar gates: tests resiliencia/idempotencia existen, docs actualizadas, changelog, 4 sign-offs
- [ ] Task: Conductor - User Manual Verification 'Fase 3' (Protocol in workflow.md)
  - [ ] Verificación manual: checklist pre-merge del workflow (spec↔impl, tests, docs, sign-offs, changelog)

## Fase 4 — Presentation & Feedback (futura)

- [ ] Task: Presentar trabajo + resultados + docs al PO y esperar feedback explícito
- [ ] Task: Merge a `master` solo con aprobación, actualizar track a `[x]`, borrar rama en 24h/siguiente sesión
- [ ] Task: Conductor - User Manual Verification 'Fase 4' (Protocol in workflow.md)
  - [ ] Verificación manual: PO valida end-to-end opencohost→servicio→overlay tras merge
