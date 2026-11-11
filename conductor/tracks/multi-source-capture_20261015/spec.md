# Spec — Multi-Source Capture with Source Tagging

Track: `multi-source-capture_20261015` · Status: new · Type: f

> ⚠ Texto incompleto en el borrador original: «Type: f» [sugerencia: feature]

---

## Overview

Hoy LiveAudio captura **un solo dispositivo por instancia** (\`ntos no indican de qué fuente vienen. El loopback WASAPI (`cpal_source.rs`) captura la salida
completa del sistema, **incluida la voz TTS de Kira**, por lo que usarlo junto al micrófono
produce eco (Kira se transcribe a sí misma) y mezcla voces sin forma de distinguirlas.

> ⚠ Texto incompleto en el borrador original: «(\`ntos» [sugerencia: `eventos`]

Objetivo: capturar N fuentes en paralelo con pipelines independientes y etiquetar cada evento
con su fuente. La separación se resuelve en el origen (rutas distintas), nunca desmezclando.

Fuera de alcance: diarización (hablante A/B dentro de una fuente), separación de señales
mezcladas, identificación de personas.

## Decisiones PO abiertas

1. **Un proceso multi-fuente vs. varias instancias.** Recomenddelo
   ASR compartido en VRAM. Dos instancias duplican la VRAM y los puertos.
   > ⚠ Texto incompleto en el borrador original: «Recomenddelo» [sugerencia: Recomiéndelo]

2. **Exclusión del eco de Kira.** Opciones: (a) loopback por proceso de Windows excluyendo el PID
   que reproduce el TTS; (b) dispositivo de salida dedicado (cable virtual); (c) solo aviso.
   Recomendado: (a), con (b) como respaldo. Antes hay que verificar qué proceso reproduce el TTS
   en OpenCohost (el backend Python o el webview de Tauri).

3. **Prioridad con la GPU ocupada.** Recomendado: el micrófonoel sistema.
   > ⚠ Texto incompleto en el borrador original: «el micrófonoel sistema» [sugerencia: el micrófono antes que el sistema]

4. **Retrocompatibilidad.** Recomendado: una config con un solo dispositivo funciona igual que hoy;
   un evento sin `source` equivale a `mic`.

## Requisitos Funcionales

- **REQ-1 — Config multi-fuente.** `sources: [{id, device_id, role: "voice"|"ambient"}]`.
  El campo heredado `audio_device` se mapea a una única fuente `{id:"mic", role:"voice"}`.
- **REQ-2 — Pipelines independientes.** Cada fuente tiene su ptador.
  RNNoise apagado por defecto en `ambient` (se comería la voz dentro de música o del juego).
  VAD más estricto en `ambient` (Whisper alucina texto sobre la música).
  > ⚠ Texto incompleto en el borrador original: «Cada fuente tiene su ptador.» [sugerencia: pipeline propio]
- **REQ-3 — Etiquetado en el protocolo.** Todo evento parcial o final lleva `source` y
  `source_role`. Se sube la versión del schema y el hello frame lista las fuentes activas.
- **REQ-4 — ASR compartido con prioridad.** Una sola instancia del modelo. La inferencia se
  serializa con prioridad `voice > ambient`. La latencia del micrófono no se degrada más de X ms
  con `ambient` activo (fijar X midiendo).
- **REQ-5 — Exclusión de eco.** CLI `--loopback-exclude-pid <PID>` (loopback por proceso), con
  respaldo a dispositivo dedicado. Si no hay exclusión posible, se emite el evento de advertencia
  `ambient_echo_risk`.
- **REQ-6 — Replay correcto.** El replay burst para clientes que se conectan tarde conserva
  `source` y marca `is_replay=true`. Esto cierra también el bug pendiente de `server.rs:635-655`
  (residuo duplicado de PTT).
- **REQ-7 — CLI.** `serve --source mic=<device_id> --source system=default_loopback --parent-pid <PID>`.

## Requisitos No Funcionales

_El borrador del PO no define requisitos no funcionales. No se añaden en este documento._

## Criterios de Aceptación (Acceptance Criteria)

- Dos fuentes mock concurrentes: cada evento llega con su `source` correcto y sin cruces.
- Prueba de eco: se reproduce TTS con el PID excluido y no aparece ninguna cue en `system`.
- Latencia p50/p95 del micrófono, con y sin `ambient` activo, dentro del presupuesto de REQ-4.
- VRAM con 2 fuentes ≈ VRAM con 1 (modelo compartido).
- Config heredada de una sola fuente: salida byte-idéntica a la actual (salvo el campo `source`).

## Riesgos

- Contención de la GPU: el audio del sistema puede retrasar la voz del streamer.
- El loopback por proceso requiere Windows 10 2004 o posteriorl o
  usar la API de WASAPI directamente.
  > ⚠ Texto incompleto en el borrador original: «posteriorl o» [sugerencia: posterior o]
- Alucinaciones de ASR sobre música y efectos de juego.

## Contrato hacia OpenCohost (fuera de este track)

OpenCohost consume `source`: `voice` va por la ruta del owner (PTT/LiveVoice) y `ambient` solo
como contexto (por ejemplo, el audio del clip para visión). Nunca dispara respuestas directas.

Lo que más conviene cerrar antes de implementar es la decisióno no funciona bien, todo el diseño depende del cable virtual, y eso cambia lo que tiene que configurar el usuario.

> ⚠ Texto incompleto en el borrador original: «la decisióno no funciona bien» [sugerencia: la decisión 2 (exclusión de eco)]

---

## Notas de verificación

Verificado el 2026-10-15 por búsqueda y lectura en `crates/`, `docs/` y `conductor/`. Solo lectura: no se modificó código ni las decisiones o requisitos de este spec.

- **a. Captura de un solo dispositivo por instancia — CONFIRMED.**
  - `crates/liveaudio-audio/src/types.rs:86`: `AudioStreamConfig.device_id: Option<String>`, un único id.
  - `crates/liveaudio-audio/src/cpal_source.rs:184`: `start` resuelve un único dispositivo con `config.device_id`.
  - `crates/liveaudio-cli/src/main.rs:64`: `serve` mapea un único `config.capture_device_id()`.
  - No existe soporte de `--source` ni de varias fuentes en `crates/` (NOT FOUND para multi-fuente).

- **b. Loopback WASAPI en `cpal_source.rs` — CONFIRMED (con matiz).**
  - `crates/liveaudio-audio/src/cpal_source.rs:75`: documenta la captura WASAPI Loopback.
  - `crates/liveaudio-audio/src/cpal_source.rs:109-119`: `resolve_device` detecta `default_loopback` y `loopback:<nombre>`.
  - Matiz: la enumeración de dispositivos loopback está en `crates/liveaudio-audio/src/device.rs:76-117`, no en `cpal_source.rs`.

- **c. `server.rs:635-655` replay burst y residuo PTT — DIFFERENT.**
  - El rango 635-655 contiene el hello frame (`crates/liveaudio-network/src/server.rs:638-646`) y el inicio de la selección del replay (`server.rs:648-657`).
  - El replay burst real está en `crates/liveaudio-network/src/server.rs:648-673`. Ya marca `cue_msg.is_replay = true` en `server.rs:663`.
  - Residuo duplicado de PTT: NOT FOUND en código. El único rastro es el comentario de `server.rs:661-662` y el test `crates/liveaudio-network/tests/server_lifecycle.rs:268` (`test_late_joining_replay_is_flagged_as_replay`), que cubre el comportamiento ya implementado. Buscar `residuo`, `635-655` y `PTT residue` en `crates/`, `docs/` y `conductor/` no devuelve ningún bug documentado.

- **d. Campo de config `audio_device` — CONFIRMED.**
  - `crates/liveaudio-core/src/config/mod.rs:177`: `pub audio_device: Option<serde_json::Value>` (objeto `{id, name}`).
  - `crates/liveaudio-core/src/config/mod.rs:378-379`: `capture_device_id()` lee `audio_device.id`.
  - `config.json:14`: `"audio_device": null`.
  - Matiz: el GUI Python legado tiene el mismo campo en `liveaudio/utils/config.py:123`. Según el CLAUDE.md del proyecto, ese GUI no es la app distribuida.
