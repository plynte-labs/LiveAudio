# Tasks — Modo servicio backend headless (opencohost)

Track: `service-backend_20260903`. Cada ticket referencia su REQ. Formato de aceptación según `conductor/workflow.md`.

---

## T1 — EngineService sin CTK (workers + Manager dict read-only) · REQ-1, REQ-4

- [x] Extraer servicio headless reutilizando `run_audio` / `run_asr` / `run_ws_server` + `Manager.dict` de `load_config` en modo read-only (sin `save_config`, sin persistir normalización).

```
Given: config guardada desde CTK con alcance/puerto/backlog definidos
When: el servicio arranca en modo headless
Then: levanta audio/ASR/WS con esa config sin abrir ventana CTK y sin escribir config.json
Error Path: config ausente/corrupta → defaults seguros TBD-4, sin escribir al disco, error accionable en health/stdout
UI State: ninguna ventana LiveAudio; CTK posterior muestra la config intacta
OBS Behavior: overlay recibe subtítulos según alcance configurado
```

## T2 — CLI servicio + supervisor headless · REQ-1, REQ-7

- [x] Implementar CLI propuesta `liveaudio --service --parent-pid PID` + supervisor con `freeze_support`, shutdown graceful (`None` a colas, `join→terminate`, `Manager.shutdown`), backoff exponencial, techo 3 muertes/5min, vaciar+recrear ambas colas en respawn.

```
Given: opencohost hace spawn con parent-pid válido
When: el backend corre y luego el dueño termina o un hijo muere
Then: sirve hasta el cierre del dueño; ante muerte de hijo hace respawn con backoff y colas limpias; ante muerte del padre termina graceful
Error Path: hijo muere 3 veces en 5min → el supervisor muere con código/error claro en vez de loop infinito
UI State: sin UI; estado vía health stdout
OBS Behavior: overlay pierde fuente al terminar; reconnect según su lógica actual
```

## T3 — Watchdog dueño-por-proceso, sin TCP control nuevo · REQ-2

- [x] Implementar watchdog parent-PID (`OpenProcess` Windows / `kill pid 0` POSIX). Bloqueado: abrir puerto de control TCP nuevo (el control plane queda fuera de TCP — detalle en TBD-1).

```
Given: servicio corriendo con padre P1
When: P1 muere o un tercero intenta controlarlo
Then: el servicio detecta la muerte del padre y termina; no existe control TCP alternativo aceptando órdenes
Error Path: parent-pid inválido/inexistente al arrancar → termina con error claro, nunca queda huérfano sirviendo
UI State: sin UI; observable vía health (sin transcripts)
OBS Behavior: al terminar, overlay sin fuente, sin crash
```

## T4 — Whisper lazy + estado de carga + indisponible pre-listo · REQ-3

- [x] Cargar modelo al primer uso de STT (no al arranque); emitir estado de carga (formato TBD-2); pre-listo responder `stt_unreachable` / 503; Silero CPU en paralelo (no bloquea).

```
Given: servicio recién arrancado sin modelo cargado
When: llega probe/start y luego el primer uso real de STT
Then: pre-listo responde indisponible; al primer uso carga, emite estado, luego transcribe
Error Path: fallo carga (CUDA/modelo) → error claro + métrica, política TBD-3 (reintento / degradado / salida)
UI State: estado de carga visible vía health (TBD-2)
OBS Behavior: sin subtítulos hasta listo; nada corrupto
```

## T5 — Alcance CTK obedecido read-only · REQ-4

- [x] Aplicar `save_transcript`, `save_vtt`, `obs_enabled`, `ws_port`, backlog `auto`/`live_only`/`send_all` desde config guardada, sin escribir.

```
Given: config CTK guardada con combinación de alcance conocida
When: arranca el servicio
Then: aplica exactamente esa config (incl. `obs_enabled off` → sin emisión; `live_only` → sin replay)
Error Path: clave ausente/corrupta → default seguro TBD-4 documentado
UI State: CTK posterior muestra config sin modificar por el servicio
OBS Behavior: según backlog: auto actual / live_only sin replay / send_all últimos 256 mensajes (drop-oldest, borde en vivo; burst documentado)
```

## T6 — Puerto base+fallback, descubrimiento hello/health, fail-fast · REQ-5, REQ-6

- [x] Bind `ws_port` base con fallback `base..base+9`; fail-fast si se agota; anunciar puerto efectivo vía `hello`/`health`; recv-only (inbound ignorado salvo `hello`); Origin loopback incl. `http://localhost:1420`. Reutilizar PR #12 si ya está en master.

```
Given: puerto base ocupado (incl. colisión 8765 API VoiceAI vs STT)
When: el servicio arranca y un cliente conecta
Then: ocupa el primer libre dentro del rango, lo anuncia en hello/health, y el lector descubre el efectivo (nunca asume fijo)
Error Path: rango agotado → fail-fast accionable; payload malformado → ignorado sin tumbar conexión
UI State: sin UI; puerto efectivo visible en hello/health
OBS Behavior: overlay hace hello antes de renderizar (cierra gap wrong-socket); 3 payloads mismo texto inbound no generan duplicados
```

## T7 — Backpressure, ASR timeout, anti-OOM replay · REQ-8

- [~] Mantener `HIGH_WATER` 64KB + retry 10; timeout ASR 15s con reciclaje del hijo a N=3; mitigar `replay_buffer` sin cota (preferir `auto`/`live_only`, documentar `send_all` como burst bloqueante tras freeze).
  - Unidad 1: backpressure/retry intactos + `replay_buffer` acotado a 256 con drop-oldest, contador `ws.replay_drops` y log acotado; techo 3-fallos/5min con fail-fast. Reciclaje del hijo ASR a N=3 timeouts queda como DEUDA documentada (requiere refactor del timeout `ThreadPool`, que no mata el hilo; ver notas de unidad 1 al final).

```
Given: freeze/recuperación con backlog acumulado
When: se reanuda la emisión
Then: sin OOM en auto/live_only; send_all emite como máximo los últimos 256 con burst documentado (no bloqueador)
Error Path: 3 timeouts ASR seguidos → reciclar hijo ASR con colas recreadas
UI State: n/a (métricas en health)
OBS Behavior: burst tras freeze acotado en auto/live_only; send_all puede saturar (documentado)
```

## T8 — Health + métricas sin fugas · REQ-9

- [x] Health stdout JSON-lines + health-file opcional (ruta/rotación TBD-5) SIN texto de transcript; métricas: `model_load_sec`, `latency`/`queue_delay`/`total_delay`, `timeouts`, `ws_queue_full`, `backpressure_events`, `replay`/`retry` sizes, `callback_age`/`reconnects`, aviso VRAM<500MB.
  - Unidad 1: eventos stdout `service_state`/`ws_port`/`asr_state`/`fatal` + snapshot health-file atómico opcional (`--health-file`), cero transcripts/PII (scrub + tests). Métricas de motor (`model_load_sec`, latencias, VRAM) ya las emite el engine por `log_queue`/`diagnostics`; el supervisor expone estados/contadores, no duplica el pipeline de métricas.

```
Given: servicio corriendo
When: se lee stdout/health-file
Then: estado/puertos/contadores visibles, cero texto de transcript, cero secretos/PII
Error Path: health-file no escribible → degradar a stdout con aviso, sin tumbar el servicio
UI State: n/a
OBS Behavior: n/a
```

## T9 — Tests resiliencia/idempotencia + huecos discovery · REQ-1..REQ-9

- [~] Tests `tests/test_resilience_*.py` y `tests/test_idempotent_*.py` (naming del gate); cubrir huecos: `hello.port` efectivo y scan fallback `base..base+9`; trazar a existentes (`test_network`, `ws_port`, `ws_origin`, `overlay_hello`, `obs_port`, `ptt_session`).
  - Unidad 1: `tests/test_resilience_service_backend.py` + `tests/test_idempotent_service_backend.py` (45 tests, en verde). Huecos `hello.port` y rango `base..base+9` cubiertos. Matriz manual M1-M10 y 4 sign-offs quedan para revisión colectiva (Fase 3).

```
Given: suite del track
When: se ejecuta validación (`python -m compileall main.py core utils` + tests resiliencia/idempotencia)
Then: todo en verde; hello.port y fallback cubiertos
Error Path: regresión → test failing con mensaje que apunta al REQ/AC afectado
UI State: n/a
OBS Behavior: matriz M1-M10 (QA) mapeada: M1 bienvenida intacta, M2 WS connect/reconnect, M3 VoiceAI probe/start, M4 backlog 3 modos, M5 burst tras freeze, M6 hot-swap/device, M7 apply/discard, M8 validación, M9 puerto efectivo, M10 resume tras crash
```

## T10 — Docs usuario + glosario + changelogs espejo · REQ-10

- [~] Actualizar `README.md`, `docs/GETTING_STARTED.md`, `docs/WEBSOCKET_OBS.md` (hello proto/port, rango base..base+9 (10 candidatos), recv-only, Origin 1420), `HISTORIAL_CAMBIOS.md`; glosario (puerto base vs efectivo, alcance, hello, recv-only, `stt_unreachable`, dueño-por-proceso, lazy); changelog espejo en opencohost/VoiceAI si cambia discovery.
  - Unidad 1: docs LiveAudio actualizadas solo con comportamiento implementado. El changelog espejo y el auto-discovery VoiceAI son la segunda unidad separada (sin cambios en VoiceAI en este track).

```
Given: implementación completa
When: se lee la documentación
Then: un integrador puede hacer spawn, descubrir el puerto efectivo, entender recv-only/alcance/lazy y la colisión 8765 sin leer código
Error Path: docs incompletas → el track no es cerrable (QA Decision Rule)
UI State: n/a
OBS Behavior: documentado (hello previo a render, backlogs, burst send_all)
```

---

## Unidad 1 — notas de implementación (2026-09-03, rama `feature/service-backend`, sin commit)

Archivos: `liveaudio/service/` (paquete: `__init__.py` fachada/entry `main`, `errors.py`, `watchdog.py`, `health.py`, `lock.py`, `supervisor.py`; contrato externo intacto: `liveaudio.service:main` y re-exports), `liveaudio/cli.py` (nuevo dispatcher), `liveaudio/utils/config.py` (`load_config_readonly` + helper `_apply_cuda_fallback_in_memory` reutilizado por `load_config`), `liveaudio/core/network.py` (hooks `on_first_client`/`first_client_event` opcionales + `REPLAY_BUFFER_MAX=256` con drop-oldest y contador `ws.replay_drops`), `pyproject.toml` (`liveaudio` → `liveaudio.cli:main`, nuevo script `liveaudio-service`), `tests/test_resilience_service_backend.py`, `tests/test_idempotent_service_backend.py` (45 tests en verde).

Decisión flaky-test (2026-09-03): `test_repeated_readonly_loads_agree` observó `device` distinto entre dos cargas porque la detección GPU puede variar con el entorno; cada llamada read-only es un snapshot independiente y NO promete hardware inmutable. Fix: detector `cuda_is_available` mockeado en el test (determinista), no retry. No es bug de la API.

TBDs cerrados según decisión del auditor: TBD-1 sin TCP (watchdog parent-PID + señales); TBD-2 stdout JSON Lines `service_state`/`ws_port`/`asr_state`/`fatal` + snapshot health atómico opcional (`--health-file`, sin rotación); TBD-3 fail-fast, techo 3 fallos/5min con backoff, fallback CUDA→CPU existente intacto; TBD-4 `load_config_readonly()` con defaults normalizados en memoria, jamás escribe; TBD-5 `--health-file` explícito, escritura atómica, warning stdout y continúa si falla. Corrección normativa aplicada: `WS_PORT_FALLBACK_RANGE=10` = `base..base+9` (10 candidatos).

Lazy: supervisor + WS arrancan de inmediato; audio/ASR solo al primer cliente WS (un `Event` IPC; una conexión probe puede disparar la carga, aceptable). `hello` sin cambios (`proto:1`, mismo payload + puerto efectivo). Config snapshot al spawn; cambios CTK requieren reiniciar el servicio. Segundo servicio rechazado por lock de instancia bajo `LIVEAUDIO_HOME` con stale recovery (no reutiliza el lock de config). `send_all` respeta config; replay acotado con drop-oldest + métrica. Timeout ASR: sin API invasiva ni thread-killing; supervisión por señales existentes (`asr` active/ok/error → loading/ready/failed) + fail-fast por muerte de hijo.

DEUDA documentada (no bloquea unidad 1): reciclaje del hijo ASR a N=3 timeouts requiere refactor mayor del timeout `ThreadPool` (no mata el hilo). Segunda unidad separada y REAL: auto-discovery VoiceAI en rama `feature/liveaudio-service-client` (scan `base..base+9` o lectura `hello.port`, probe `stt_unreachable`/503 lado lector y changelog espejo). Orden sugerido: LiveAudio primero (este track), VoiceAI después. Sin cambios en VoiceAI en este track.

Corrección post-review (2026-09-03): `InstanceLock.acquire` crea `LIVEAUDIO_HOME` en instalación fresca y mapea `OSError` a `ServiceError` sanitizado (`service-lock-unwritable`); shutdown con fallback `terminate→kill`, cierre de colas mp (`close`/`cancel_join_thread`) en shutdown/respawn y `Manager.shutdown` garantizado si `start()` falla tras crearlo; `liveaudio-service` documentado como ÚNICA vía headless soportada en Windows instalado (el gui-script no tiene stdout; el .venv instalado genera `liveaudio-service.exe` vía `uv sync` sin cambio de packaging — verificado: `run_uv_sync` no pasa `--no-install-project` y el dist-info existente prueba el mecanismo de entry points); limitación PID-reuse/TOCTOU documentada en `watchdog.py`. `.atl/skill-registry.md` y `opencode.json` no tocados (cambios preexistentes del usuario).
