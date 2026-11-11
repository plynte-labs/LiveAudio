# Plan — Multi-Source Capture with Source Tagging

Track: `multi-source-capture_20261015` · Status: new · Type: feature

> **Bloqueado por decisiones PO 1-4.** No iniciar ninguna tarea hasta que el PO cierre las decisiones 1 a 4 de `spec.md` (proceso único vs. varias instancias, exclusión de eco, prioridad con la GPU ocupada y retrocompatibilidad). Todas las tareas están sin marcar.

## Tareas de Implementación

### Fase 1: Config multi-fuente (REQ-1)
- [ ] **Task 1.1: Esquema `sources` y mapeo del `audio_device` heredado (REQ-1)**
  - Añadir `sources: [{id, device_id, role}]` junto al campo `audio_device` en `crates/liveaudio-core/src/config/mod.rs` (campo en línea 177).
  - Mapear el `audio_device` heredado a una única fuente `{id: "mic", role: "voice"}` cuando no exista `sources`.
  - Escribir tests primero (TDD): una config heredada produce una sola fuente; un `role` distinto de `voice` o `ambient` se rechaza.
  - Depende de: decisión PO 4.
  - Responsable: _por asignar_.

### Fase 2: Pipelines independientes (REQ-2)
- [ ] **Task 2.1: Pipeline propio por fuente (REQ-2)**
  - `ambient`: RNNoise apagado por defecto y VAD más estricto. El umbral se fija midiendo, no en este plan.
  - `voice`: configuración actual de RNNoise y VAD.
  - Escribir tests primero (TDD): cada fuente mantiene su propio estado de VAD y DSP, sin estado compartido.
  - Depende de: Task 1.1.
  - Responsable: _por asignar_.

### Fase 3: Etiquetado en el protocolo (REQ-3)
- [ ] **Task 3.1: `source` y `source_role` en eventos, schema y hello frame (REQ-3)**
  - Añadir `source` y `source_role` a eventos parciales y finales.
  - Subir la versión del schema y listar las fuentes activas en el hello frame.
  - Puntos de partida: `HelloMessage` en `crates/liveaudio-network/src/protocol.rs:13`, `SubtitleWireMessage` en `protocol.rs:34` y `HelloMessage::new` en `crates/liveaudio-network/src/server.rs:639`.
  - Escribir tests primero (TDD): un evento sin `source` se interpreta como `mic` (decisión PO 4).
  - Depende de: Task 2.1.
  - Responsable: _por asignar_.

### Fase 4: ASR compartido con prioridad (REQ-4)
- [ ] **Task 4.1: Instancia única del modelo con cola de inferencia priorizada (REQ-4)**
  - Una sola instancia del modelo ASR. La inferencia se serializa con prioridad `voice > ambient`.
  - Depende de: decisión PO 3 y Task 3.1.
  - Responsable: _por asignar_.
- [ ] **Task 4.2: Medición de latencia del micrófono y fijación de X (REQ-4)**
  - Medir la latencia del micrófono con y sin `ambient` activo y fijar el valor X de REQ-4 a partir de esa medición.
  - Depende de: Task 4.1.
  - Responsable: _por asignar_.

### Fase 5: Exclusión de eco (REQ-5)
- [ ] **Task 5.1: Verificar el proceso que reproduce el TTS de OpenCohost**
  - Confirmar si el TTS sale por el backend Python o por el webview de Tauri. Es prerrequisito de la decisión PO 2.
  - Responsable: _por asignar_.
- [ ] **Task 5.2: Loopback por proceso con `--loopback-exclude-pid` y respaldo a dispositivo dedicado (REQ-5)**
  - El loopback por proceso requiere Windows 10 2004 o posterior (ver Riesgos en `spec.md`).
  - Punto de partida: la resolución de loopback actual está en `crates/liveaudio-audio/src/cpal_source.rs:109-119` (`resolve_device`).
  - Depende de: decisión PO 2 y Task 5.1.
  - Responsable: _por asignar_.
- [ ] **Task 5.3: Evento de advertencia `ambient_echo_risk` (REQ-5)**
  - Emitir `ambient_echo_risk` cuando no exista exclusión posible.
  - Depende de: Task 5.2.
  - Responsable: _por asignar_.

### Fase 6: Replay correcto (REQ-6)
- [ ] **Task 6.1: El replay burst conserva `source` y marca `is_replay=true` (REQ-6)**
  - Punto de partida: el replay burst está en `crates/liveaudio-network/src/server.rs:648-673`. El flag `is_replay` ya se fija en `server.rs:663`.
  - Antes de cerrar el "residuo PTT" de `server.rs:635-655`, confirmar el bug: no se localizó en código (ver `spec.md`, Notas de verificación, punto c).
  - Escribir tests primero (TDD), extendiendo `crates/liveaudio-network/tests/server_lifecycle.rs`.
  - Depende de: Task 3.1.
  - Responsable: _por asignar_.

### Fase 7: CLI multi-fuente (REQ-7)
- [ ] **Task 7.1: `serve --source mic=<device_id> --source system=default_loopback --parent-pid <PID>` (REQ-7)**
  - `--parent-pid` ya existe: `parse_parent_pid_args` en `crates/liveaudio-cli/src/main.rs:620`.
  - `--source` no existe todavía. Hoy `serve` usa un único `capture_device_id()` (`crates/liveaudio-cli/src/main.rs:64`).
  - Depende de: Tasks 1.1 y 4.1.
  - Responsable: _por asignar_.

### Fase 8: Verificación de criterios de aceptación
- [ ] **Task 8.1: Dos fuentes mock concurrentes**
  - Cada evento llega con su `source` correcto y sin cruces.
  - Depende de: Tasks 3.1 y 7.1.
  - Responsable: _por asignar_.
- [ ] **Task 8.2: Prueba de eco**
  - Reproducir TTS con el PID excluido. No debe aparecer ninguna cue en `system`.
  - Depende de: Task 5.2.
  - Responsable: _por asignar_.
- [ ] **Task 8.3: Latencia p50/p95 del micrófono**
  - Con y sin `ambient` activo, dentro del presupuesto de REQ-4.
  - Depende de: Task 4.2.
  - Responsable: _por asignar_.
- [ ] **Task 8.4: VRAM con 2 fuentes**
  - VRAM con 2 fuentes ≈ VRAM con 1 (modelo compartido).
  - Depende de: Task 4.1.
  - Responsable: _por asignar_.
- [ ] **Task 8.5: Retrocompatibilidad de config de una sola fuente**
  - Salida byte-idéntica a la actual, salvo el campo `source`.
  - Depende de: Tasks 1.1 y 3.1.
  - Responsable: _por asignar_.

---

## Quality Gates

1. Decisiones PO 1 a 4 cerradas y registradas en `spec.md` antes de la Fase 1.
2. Cada criterio de aceptación de `spec.md` verificado con evidencia observada, no supuesta.
3. Tests escritos antes de la implementación (Strict TDD activo en el proyecto).
4. La config heredada de una sola fuente no cambia de comportamiento.
