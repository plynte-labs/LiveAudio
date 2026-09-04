# Spec — Modo servicio backend headless (opencohost)

Track: `service-backend_20260903` · Status: new · Type: feature

## Overview

Permitir que opencohost levante LiveAudio como **backend headless sin CTK** (sin ventana de bienvenida, sin 3 pasos manuales) mediante spawn por proceso. El backend vive hasta el cierre/crash del proceso dueño, obedece la configuración guardada desde CTK (alcance/transcripción/OBS/puerto/backlog), carga Whisper de forma perezosa (lazy cargar-al-usar), y expone el mismo data plane WebSocket loopback con descubrimiento del puerto efectivo vía `hello`/`health`. Windows primero, Linux portable.

Fuera de este track queda cualquier cambio en opencohost/VoiceAI salvo el contrato documentado (hello/health/recv-only/Origin/rango de puertos).

Decisiones PO cerradas (base normativa, no reabrir sin PO):
1. Backend spawn por opencohost, vive hasta cierre/crash del dueño. Windows primero, Linux portable.
2. Seguridad dueño-por-proceso: watchdog parent-PID, sin puerto control TCP nuevo, data plane loopback intacto.
3. Prewarm del motor ASR al arrancar el servicio por defecto (--prewarm) para que el servicio esté listo inmediatamente al presionar PTT, manteniendo --lazy como opción configurable.
4. Alcance configurable desde CTK; el servicio obedece la config guardada (`save_transcript`, `save_vtt`, `obs_enabled`, `ws_port`, backlog `auto`/`live_only`/`send_all`).
5. Mismo puerto con fallback `base..base+9`; opencohost descubre el puerto efectivo vía `hello`/`health`, nunca asume fijo. Flujo CTK de 3 pasos intacto.

## Requisitos funcionales

- **REQ-1 — Spawn y ciclo de vida propiedad del dueño.** opencohost lanza el backend como subproceso CLI propuesto `liveaudio --service --parent-pid PID`. El backend vive hasta cierre/crash del dueño; al detectar muerte del padre termina graceful. Supervisor headless con `freeze_support` (Windows empaquetado).
  - Fuente: PO-1; Arquitectura (supervisor headless, CLI propuesta).
- **REQ-2 — Dueño único (dueño-por-proceso).** Solo el proceso padre que hizo spawn es dueño. Watchdog parent-PID (`OpenProcess` en Windows / `kill pid 0` en POSIX). Sin puerto de control TCP nuevo; el control plane queda fuera de TCP (TBD: mecanismo exacto — ver TBD-1). Data plane WebSocket loopback intacto.
  - Fuente: PO-2; Arquitectura (watchdog, control plane fuera de TCP).
- **REQ-3 — Prewarm del motor ASR por defecto + estado de carga.** El modelo Whisper y el pipeline de audio inician carga inmediatamente al arrancar el servicio (a menos que se especifique --lazy). El servicio emite transiciones de estado asr_state (starting -> loading -> ready). Pre-listo, las sondas STT reportan indisponible/cargando (stt_loading).
  - Fuente: PO-3; Performance (prewarm/lazy + emitir estado); QA AC-3.
- **REQ-4 — Alcance configurable desde CTK, servicio obedece config guardada.** El servicio lee la config guardada en modo read-only (`load_config`; prohibido `save_config` y prohibido persistir normalización desde el servicio). Claves obedecidas: `save_transcript`, `save_vtt`, `obs_enabled`, `ws_port`, backlog (`auto`/`live_only`/`send_all`). Reutilizar `Manager.dict` de `load_config` en workers (`run_audio`/`run_asr`/`run_ws_server`).
  - Fuente: PO-4; Arquitectura (`EngineService` vía workers + Manager dict read-only).
- **REQ-5 — Mismo puerto con fallback y descubrimiento, nunca fijo.** Bind en `ws_port` base con fallback `base..base+9`; fail-fast si el rango se agota. opencohost/descubrimiento obtienen el puerto efectivo vía `hello`/`health` (no asumir fijo). CTK 3 pasos intacto (sin regresión del flujo GUI).
  - Fuente: PO-5; Arquitectura (fail-fast rango agotado); Research (colisión 8765 API VoiceAI vs STT; VoiceAI hoy no descubre — edición manual).
- **REQ-6 — Data plane recv-only + hello.** El WS del servicio es recv-only: ignora payloads inbound salvo `hello` (handshake que anuncia el puerto efectivo). Tres payloads con el mismo texto se comportan igual que hoy (sin duplicar efectos). Origen loopback admitido incl. `http://localhost:1420` (opencohost UI).
  - Fuente: QA AC-5; Research (matriz hello/payloads/recv-only/Origin OK).
- **REQ-7 — Shutdown graceful y respawn limpio.** Al terminar: señal `None` a colas, `join → terminate`, `Manager.shutdown`. En respawn (supervisor): backoff exponencial, morir tras 3 muertes en 5 min, vaciar y recrear ambas colas.
  - Fuente: Arquitectura + Performance (supervisor/respawn).
- **REQ-8 — Backpressure y límites de memoria.** Mantener `HIGH_WATER 64KB` + retry 10 en WS. `replay_buffer` SIN COTA es riesgo OOM si `send_all` + freeze: preferir `auto`/`live_only` por defecto; `send_all` documentado como propenso a burst bloqueante tras freeze. Reciclar hijo ASR a N=3 timeouts (el timeout de 15 s con `ThreadPool` no mata el hilo).
  - Fuente: Performance.
- **REQ-9 — Health/observabilidad sin fugas.** Health por stdout JSON-lines + health-file opcional. El health-file NUNCA contiene texto de transcript (solo estado/puertos/contadores). Reusar métricas: `model_load_sec`, `latency`/`queue_delay`/`total_delay`, `timeouts`, `ws_queue_full`, `backpressure_events`, `replay`/`retry` sizes, `audio callback_age`/`reconnects`, aviso VRAM < 500 MB. Silero CPU en paralelo a Whisper (no bloquea carga).
  - Fuente: Arquitectura (health) + Performance (métricas).
- **REQ-10 — Docs y glosario actualizados.** Actualizar `README.md`, `docs/GETTING_STARTED.md`, `docs/WEBSOCKET_OBS.md` (proto hello/port, rango base..base+9 (10 candidatos), recv-only, Origin loopback 1420), `HISTORIAL_CAMBIOS.md`. Glosario: puerto base vs efectivo, alcance, hello, recv-only, `stt_unreachable`, dueño-por-proceso, lazy. Changelog en ambos repos si cambia discovery (LiveAudio + opencohost/VoiceAI).
  - Fuente: QA + Research.

## Requisitos no funcionales

- **NFR-1 — Plataforma:** Windows primero (incl. empaquetado/`freeze_support`, `OpenProcess`); Linux portable (`kill pid 0`).
- **NFR-2 — Seguridad:** sin superficie TCP nueva; loopback-only; Origin allowlist incl. opencohost (`http://localhost:1420`); dueño-por-proceso verificable (AC-2).
- **NFR-3 — Resiliencia:** timeout ASR 15 s; reciclaje hijo ASR a N=3; supervisor backoff + techo 3 muertes/5 min; colas recreadas en respawn; sin OOM por replay sin cota en `send_all` (mitigar vía preferencia `auto`/`live_only` + documentar).
- **NFR-4 — Performance:** Whisper lazy; Silero CPU paralelo; backpressure 64 KB + retry 10; `send_all` tras freeze puede causar burst bloqueante (comportamiento conocido y documentado, no bloqueador del track).
- **NFR-5 — Privacidad:** sin secretos/PII/audio/transcripts en logs, health-file, docs o Engram. Health-file sin texto de transcript.
- **NFR-6 — Compatibilidad:** divergencia latente a vigilar: `liveTranscript` solo `.text` vs backend `text`/`segments`/`transcript` (no romper clientes actuales; documentar). Dependencias: `websockets>=14<17` vs `>=12` (fijar matriz), `faster-whisper`/`torch` solo LiveAudio, grafo `mp.Manager` a replicar en servicio.

## Criterios de aceptación

### AC-1 — Spawn/cierre
- Given: opencohost instalado y LiveAudio cerrado.
- When: opencohost hace spawn `liveaudio --service --parent-pid <PID>` y luego el dueño cierra/muere.
- Then: el backend levanta sin ventana CTK, sirve WS en el puerto efectivo anunciado, y termina graceful al morir el padre (colas drenadas, `Manager.shutdown`).
- Error Path: si el rango `base..base+9` está agotado → fail-fast con error accionable (sin colgar, sin puerto fuera de rango).
- UI State: ninguna ventana LiveAudio visible durante el servicio; CTK 3 pasos sigue disponible al abrir la app normal.
- OBS Behavior: overlay conecta al puerto efectivo y recibe subtítulos según alcance configurado.

### AC-2 — Dueño único
- Given: servicio corriendo con padre P1.
- When: otro proceso intenta controlarlo o P1 muere.
- Then: no existe control TCP alternativo que acepte órdenes de terceros; al morir P1 el servicio termina (watchdog). Un segundo owner no puede "adoptarlo".
- Error Path: padre inválido/inexistente al arrancar → el servicio no queda huérfano sirviendo; termina con error claro.
- UI State: sin UI; el estado se observa vía health stdout/health-file (sin transcripts).
- OBS Behavior: al terminar el servicio, el overlay queda sin fuente (reconnect según su lógica actual, sin crash).

### AC-3 — Prewarm del motor ASR por defecto + transición a ready (o lazy con --lazy)
- Given: servicio recién arrancado con prewarm por defecto (o --lazy si configurado).
- When: el servicio arranca e inicia carga inmediata de Whisper y pipeline de audio (prewarm al arrancar) o se usa STT por primera vez bajo --lazy.
- Then: el servicio emite transiciones de estado asr_state (starting -> loading -> ready); pre-listo responde indisponible/cargando (stt_loading / 503), y transiciona a ready para responder inmediatamente al presionar PTT.
- Error Path: fallo de carga CUDA/modelo → error claro + métrica `model_load_sec`/fallo, servicio sigue vivo para reintento o termina según política documentada (ver TBD-3).
- UI State: estado de carga observable vía health (formato en TBD-2).
- OBS Behavior: sin subtítulos hasta que el modelo esté listo; sin mensajes corruptos.

### AC-4 — Alcance obedece CTK
- Given: config guardada desde CTK (`save_transcript`, `save_vtt`, `obs_enabled`, `ws_port`, backlog `auto`/`live_only`/`send_all`).
- When: arranca el servicio (sin tocar CTK).
- Then: el servicio aplica exactamente esa config en modo read-only (no escribe `config.json`, no persiste normalización).
- Error Path: config ausente/corrupta → defaults seguros documentados (ver TBD-4); nunca escribe una config "reparada" al disco desde el servicio.
- UI State: CTK muestra la misma config al abrirse después (no modificada por el servicio).
- OBS Behavior: `obs_enabled off` → sin emisión; backlog `live_only` → sin replay tras freeze; `auto` → comportamiento actual; `send_all` → últimos 256 mensajes (drop-oldest, borde en vivo) con riesgo documentado de burst.

### AC-5 — Recv-only hello ignorado + 3 payloads mismo texto
- Given: cliente WS conectado (incl. Origin `http://localhost:1420`).
- When: envía `hello` y luego 3 payloads con el mismo texto (inbound).
- Then: `hello` recibe handshake con puerto efectivo y no altera estado; los 3 payloads inbound se ignoran sin efectos laterales (sin transcripción fantasma, sin duplicados emitidos).
- Error Path: payload malformado → ignorado sin cerrar la conexión salvo violación de protocolo documentada.
- UI State: sin UI implicada.
- OBS Behavior: overlay renderiza igual que hoy; `hello` precede al render (cierra el gap "wrong socket").

## Out of Scope

- Cambios de código en opencohost/VoiceAI (solo contrato documentado + changelog espejo si cambia discovery).
- Nuevo puerto de control TCP, autenticación por token WS, panel de autorización por conexión.
- Cambios al flujo CTK de 3 pasos (debe quedar intacto).
- Soporte macOS, instalador del servicio, auto-update del servicio, firma de binarios.
- Resolver el burst bloqueante de `send_all` tras freeze más allá de documentarlo y preferir `auto`/`live_only`.
- Fijar la divergencia `liveTranscript .text` vs `text`/`segments`/`transcript` si requiere cambio de protocolo (solo documentar + no romper).

## Supuestos y deudas

- SUP-1: el fallback `base..base+9` y el handshake `hello` ya existen en `master` (PR #12 referenciado en tracks.md); el track los reutiliza, no los reinventa.
- SUP-2: VoiceAI hoy NO descubre el puerto (edición manual); el track NO es cerrable sin scan `base..base+9` o lectura de `hello.port` en el lado lector (Research). El lado opencohost/VoiceAI se coordina fuera de este track.
- DEUDA-1: `replay_buffer` sin cota (riesgo OOM) — mitigación mínima en este track (defaults + docs), fix estructural futuro.
- DEUDA-2: timeout ASR con `ThreadPool` no mata el hilo — mitigación: reciclaje a N=3 en este track.
- DEUDA-3: `websockets>=14<17` vs `>=12` — fijar matriz en implementación.
- Trazabilidad a tests existentes: `test_network`, `ws_port`, `ws_origin`, `overlay_hello`, `obs_port`, `ptt_session`. Huecos conocidos: ningún test cubre `hello.port` efectivo ni scan fallback (crear en implementación).

## TBDs (no inventar; dueño entre paréntesis)

- **TBD-1 (auditor/PO):** mecanismo exacto del "control plane fuera de TCP" — ¿solo CLI args + señales de proceso + health read-only? Definir antes de implementar REQ-2.
- **TBD-2 (auditor/PO):** formato exacto del "estado de carga" lazy (¿línea JSON en stdout? ¿campo en health-file? ¿evento WS?). Definir antes de AC-3 testeable.
- **TBD-3 (auditor/PO):** política tras fallo de carga del modelo (¿reintentar, seguir vivo degradado, o salir con código?). Definir antes de AC-3 Error Path.
- **TBD-4 (auditor/PO):** defaults seguros cuando la config está ausente/corrupta (valores concretos por clave). Definir antes de AC-4 Error Path.
- **TBD-5 (auditor/PO):** ruta y nombre del health-file opcional + retención/rotación. Definir antes de REQ-9 testeable.
- **TBD-6 (auditor/PO):** confirmación de que la colisión `8765` API VoiceAI vs STT-default se resuelve por discovery (scan o `hello.port`) y en qué repo vive ese cambio.
