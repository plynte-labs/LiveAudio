# Architecture Decisions — LiveAudio

Este documento registra las decisiones de arquitectura del proyecto LiveAudio, el porqué de cada una, y los trade-offs considerados.

---

## ADR-001: Silero VAD como módulo separado de Whisper

**Estado**: ✅ Implementado
**Archivo**: `core/audio.py`

### Decisión

Silero VAD corre **antes** de Whisper, en un proceso separado (`audio_producer`), no se usa el `vad_filter=True` integrado de Whisper.

### Por qué

| Criterio | Silero separado | Whisper vad_filter integrado |
|----------|----------------|------------------------------|
| Control granular del threshold | ✅ Configurable (`VAD_THRESHOLD`) | ❌ Hardcoded internamente |
| Corte de frases por pausas naturales | ✅ `silence_timeout` + `max_chunk_duration` | ❌ Solo filtra silencio, no segmenta |
| Detección de desconexión de hardware | ✅ Watchdog independiente del ASR | ❌ Acoplado al modelo |
| Ahorro de GPU/VRAM | ✅ No manda audio silencioso al modelo | ⚠️ El modelo recibe todo y filtra internamente |

### Trade-offs

- **Complejidad**: Mantener dos modelos (Silero + Whisper) en lugar de uno.
- **Primera palabra**: Silero necesita ~100-200ms para detectar voz con confianza. Los primeros chunks se pierden antes de que `speech_prob > VAD_THRESHOLD`. **Mitigado con pre-buffer** (ver ADR-007).

### Alternativas rechazadas

- **WebRTC VAD**: Más ligero pero menos preciso en español con ruido de fondo.
- **Energía RMS simple**: No distingue voz de ruido ambiental (teclado, música).

---

## ADR-002: Ring Buffer desacoplado entre captura y VAD

**Estado**: ✅ Implementado
**Archivo**: `core/audio.py`

### Decisión

Arquitectura de 3 capas:

```
┌──────────────┐     ┌────────────┐     ┌──────────────┐     ┌───────────┐
│ Hardware      │────▶│ Ring Buffer │────▶│ VAD Worker   │────▶│ audio_queue│
│ (callback C)  │     │ (deque)     │     │ (Thread)     │     │ (IPC)     │
└──────────────┘     └────────────┘     └──────────────┘     └───────────┘
```

El callback de C **solo copia** audio al ring buffer (~0.01ms). El VAD Worker lee y evalúa Silero en un hilo separado.

### Por qué

1. **El callback de C es tiempo real**: cualquier operación de IA (>1ms) causa glitches de audio.
2. **Desacoplamiento**: si el VAD tiene un pico de latencia, el audio se acumula en el ring buffer sin perderse.
3. **Backpressure controlado**: `deque(maxlen=500)` = 16 segundos de buffer. Si el worker se atrasa, los chunks más viejos se descartan automáticamente en lugar de consumir RAM infinita.

### Trade-offs

- **Memoria**: 500 chunks * 512 samples * 4 bytes = ~1MB máximo. Negligible.
- **Complejidad**: 3 capas en lugar de 2. Pero cada capa tiene una responsabilidad clara.

---

## ADR-003: VAD en CPU, ASR en GPU

**Estado**: ✅ Implementado

### Decisión

Silero VAD corre en CPU. Whisper corre en GPU (CUDA) cuando está disponible.

### Por qué

| Modelo | VRAM requerida | Carga de CPU | Razón |
|--------|---------------|--------------|-------|
| Silero VAD | ~0 MB | ~5-10% de 1 core | Extremadamente ligero (< 1MB de modelo) |
| Whisper small | ~1.5 GB | ~30% de 1 core | Modelo pesado, se beneficia de GPU |
| Whisper turbo | ~5 GB | ~50% de 1 core | Modelo muy pesado, requiere GPU |

Si Silero corriera en GPU, competiría con Whisper por VRAM, especialmente en el modelo `turbo`.

### Trade-offs

- **Latencia del VAD**: CPU es más lento que GPU para inferencia, pero Silero es tan ligero que la diferencia es imperceptible (~2ms vs ~0.5ms por chunk).
- **No es un cuello de botella**: El VAD evalúa chunks de 32ms, tiene tiempo de sobra en CPU.

---

## ADR-004: Audio producer como proceso separado (multiprocessing)

**Estado**: ✅ Implementado
**Archivo**: `core/audio.py` → `audio_producer()`

### Decisión

El `audio_producer` corre en un `mp.Process` dedicado, separado del proceso principal de la UI y del ASR consumer.

### Por qué

1. **Aislamiento de fallos**: si el audio producer crashea (dispositivo desconectado, driver corrupto), no tumba la UI ni el motor ASR.
2. **Reconexión automática**: el producer tiene un watchdog que detecta desconexión de hardware y reintenta cada 3 segundos.
3. **GIL bypass**: `multiprocessing` evita el Global Interpreter Lock de Python. La captura de audio y el VAD corren en paralelo real con el ASR.

### Trade-offs

- **IPC overhead**: comunicación via `mp.Queue` (serialización pickle). Negligible para chunks de audio.
- **Complejidad de shutdown**: requiere señalización explícita (`shutdown_event`, `worker_running`).

---

## ADR-005: SessionWriter para I/O asíncrono de disco

**Estado**: ✅ Implementado
**Archivo**: `core/engine.py` → `SessionWriter`

### Decisión

La escritura de transcript (JSONL + VTT) se abstrae en un `SessionWriter` con hilo secundario y cola IPC. El hot path del ASR no bloquea en I/O de disco.

### Por qué

- **El guardado síncrono bloqueaba el hilo de inferencia**: escribir a disco mientras Whisper está transcribiendo causa retrasos acumulativos.
- **Persistencia garantizada**: el transcript se guarda independientemente del estado de OBS/WebSocket.

### Trade-offs

- **Complejidad**: cola IPC + hilo dedicado para escritura.
- **Riesgo de pérdida**: si el proceso muere abruptamente, los registros en la cola de escritura se pierden. Mitigado con flush periódico.

---

## ADR-006: Subtítulos como HTML estático con WebSocket

**Estado**: ✅ Implementado
**Archivo**: `subtitulos_obs.html`

### Decisión

Los subtítulos se renderizan en un archivo HTML estático que OBS consume como Browser Source, conectado vía WebSocket al backend Python.

### Por qué

| Alternativa | Pros | Contras |
|-------------|------|---------|
| **HTML + WebSocket (elegido)** | Sin dependencias externas, funciona en cualquier OBS, personalizable con CSS | Limitado a lo que el browser source permite |
| **OBS WebSocket API** | Control total de OBS | Requiere plugin obs-websocket, más complejo |
| **OBS plugin nativo** | Integración perfecta | C/C++, mantenimiento pesado, solo Windows/Mac |
| **NDI/SRT** | Baja latencia | Overkill para subtítulos, requiere hardware extra |

### Trade-offs

- **Sin detección de versión de OBS**: `window.obsstudio.pluginVersion` devuelve la versión del plugin browser source (CEF), NO la versión de OBS Studio. No se puede detectar la versión real de OBS desde el browser source.
- **Caché de OBS**: OBS cachea el HTML. Los cambios requieren "Refresh cache of current page" o re-agregar la fuente.

---

## ADR-007: Pre-buffer para recuperar primera palabra

**Estado**: ✅ Implementado (2026-05-12)
**Archivo**: `core/audio.py` → `vad_worker()`

### Decisión

Guardar los últimos 3 chunks descartados en un `deque(maxlen=3)`. Cuando el VAD detecta voz (transición silencio → habla), prependé esos chunks al `speech_buffer`.

### Por qué

Silero necesita ~100-200ms de audio para detectar voz con confianza. Sin pre-buffer, los primeros ~96ms (3 chunks × 32ms) se pierden antes de que `speech_prob > VAD_THRESHOLD`.

### Implementación

```python
pre_buffer = collections.deque(maxlen=3)  # ~96ms de audio

# En el loop del VAD:
if speech_prob > VAD_THRESHOLD:
    if not is_speaking:
        speech_buffer.extend(pre_buffer)  # Recuperar primeros ms
        pre_buffer.clear()
    # ... resto del manejo de voz
else:
    pre_buffer.append(audio_chunk)  # Guardar chunk descartado
```

### Trade-offs

- **Falsos positivos**: si hay un ruido breve que no es voz, los 3 chunks de pre-buffer se prependen igual. El impacto es mínimo (~96ms de silencio/ruido al inicio de la frase).
- **Memoria**: 3 chunks × 512 samples × 4 bytes = ~6KB. Negligible.

### Alternativas consideradas

- **Bajar `VAD_THRESHOLD` a 0.3**: detecta antes pero muchos más falsos positivos (ruido de teclado, música).
- **`speech_pad_ms` de Silero**: agrega padding de silencio, no recupera audio real.

---

## ADR-008: Whisper `initial_prompt` para reducir alucinaciones

**Estado**: ✅ Implementado (2026-05-12)
**Archivos**: `utils/config.py`, `main.py`, `core/engine.py`

### Decisión

Exponer `whisper_context_prompt` como campo de texto en la UI (pestaña Rendimiento). Se pasa como `initial_prompt` a `model.transcribe()`.

### Por qué

Faster-whisper soporta `initial_prompt` como contexto de vocabulario. El modelo tiende a seguir ese estilo y vocabulario, reduciendo alucinaciones de palabras fuera de contexto.

### Trade-offs

- **No elimina alucinaciones de silencio**: para eso se necesita VAD + blacklist (ya implementados).
- **Depende del streamer**: cada tipo de contenido necesita un prompt distinto. Por eso está en la UI, no hardcoded.

---

## ADR-009: Backpressure y backlog policy para OBS

**Estado**: ✅ Implementado
**Archivos**: `core/engine.py`, `core/network.py`, `subtitulos_obs.html`

### Decisión

Separar persistencia de output visual. Toda transcripción se guarda en disco, pero no todo backlog se muestra en OBS.

### Políticas

| Modo | Comportamiento |
|------|---------------|
| `auto` | Envía subtítulos frescos, emite backlog corto con pacing, omite lo muy atrasado |
| `live_only` | Solo muestra subtítulos dentro de `subtitle_max_live_delay_sec` |
| `send_all` | Manda todo a OBS aunque llegue tarde |

### Por qué

Después de un freeze de GPU/CPU, el backlog acumulado puede causar un "burst" de subtítulos en OBS que solapa y confunde al espectador.

---

## ADR-010: CSS Custom Properties para temas de subtítulos

**Estado**: ✅ Implementado
**Archivo**: `subtitulos_obs.html`

### Decisión

Usar CSS custom properties (`--sub-bg`, `--sub-color`, etc.) como motor de temas. 7 presets: `default`, `karaoke`, `neon`, `minimal`, `bold`, `rgb`, `typewriter`.

### Por qué

- **Sin editor visual**: presets-only, sin la complejidad de un theme builder completo.
- **Cambio en tiempo real**: el backend envía `{type: "theme", tokens: {...}}` por WebSocket y el HTML aplica los cambios sin recargar.
- **Validación**: el HTML rechaza tokens inválidos (tamaño fuera de rango, color mal formado).

---

## ADR-011: OBS enable/disable toggle (modo solo transcript)

**Estado**: ✅ Implementado (2026-05-12)
**Archivos**: `utils/config.py`, `core/engine.py`, `main.py`

### Decisión

Config key `obs_enabled` (boolean, default `True`). Si es `false`, el transcript se guarda en disco pero NO se emite por WebSocket a OBS.

### Por qué

Algunos usuarios solo necesitan la transcripción guardada (JSONL + VTT) sin subtítulos en vivo.

---

## ADR-012: Animaciones de entrada/salida espejo

**Estado**: ✅ Implementado (2026-05-12)
**Archivo**: `subtitulos_obs.html`

### Decisión

Todas las animaciones de entrada y salida usan la misma distancia (20px), duración (`var(--sub-animation-duration)`) y easing, pero en dirección opuesta.

### Por qué

Animaciones asimétricas (entrada diferente a salida) se sienten "rotas" visualmente. El espejo da coherencia.

---

## Glosario

| Término | Significado |
|---------|-------------|
| VAD | Voice Activity Detection — detecta si hay voz o silencio |
| ASR | Automatic Speech Recognition — transcripción de voz a texto |
| Hot path | Código crítico de rendimiento que se ejecuta frecuentemente |
| Ring buffer | Buffer circular que descarta lo más viejo cuando está lleno |
| Backpressure | Mecanismo para evitar que un componente lento sature al sistema |
| Browser source | Fuente de navegador en OBS que renderiza HTML |
| Pre-buffer | Buffer de chunks descartados para recuperar audio perdido |
