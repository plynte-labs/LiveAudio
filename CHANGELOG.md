# Changelog

Todos los cambios notables de LiveAudio se documentan aquí.

---

## [Unreleased] — 2026-05-09

### Public Launch Readiness

- **Canonical public repository** — referencias públicas alineadas a `https://github.com/plynte-labs/LiveAudio`.
- **Portable/runtime dependency parity** — `Pillow` agregado a `requirements.txt` y al instalador portable porque `main.py` importa `PIL.Image` e `ImageTk`.
- **Public artifact sanitization** — los artifacts públicos de workflow deben evitar rutas personales, secretos, envs privados y URLs internas.
- **Non-blocking follow-up clarity** — el track de separación de idioma ASR/UI queda documentado como seguimiento no bloqueante para launch.

### Critical Bug Fixes (Dream Team Audit)

9 fixes de alta y crítica severidad identificados por el equipo de 4 agents especializados.

#### Resilience & Stability

- **ASR freeze recovery** — `model.transcribe()` ahora tiene timeout de 15s vía `ThreadPoolExecutor`. Si Whisper se congela, el sistema continúa al siguiente chunk en vez de quedarse bloqueado indefinidamente.
- **Queue backpressure non-blocking** — El worker VAD ya no se bloquea cuando la cola de audio está llena. Usa `put_nowait()` con estrategia de descarte del fragmento más viejo.
- **Graceful shutdown** — El productor de audio ahora responde a señal de apagado, cierra el stream explícitamente y espera al thread VAD con timeout.
- **VAD download error handling** — Si Silero VAD no se puede cargar (sin internet, cache corrupto), se muestra un error claro en la UI en vez de fallar silenciosamente.

#### Security & Sanitization

- **Unicode bidi strip** — Se eliminan caracteres peligrosos U+202E, U+202D, U+200E, U+200F, U+0000, U+001b tanto en Python (`_sanitize_text()`) como en JavaScript (`subtitulos_obs.html`). Previene ataques de inversión de texto en subtítulos.
- **Error log sanitization** — Los mensajes de error ahora usan `type(e).__name__` en vez de `str(e)` para evitar que contenido transcrito se filtre a logs técnicos.

#### Configuration & Compatibility

- **WebSocket port wiring** — El puerto WS ahora es configurable vía `ws_port` en `config.json`. El HTML de OBS lee el puerto del parámetro `?port=XXXX` en la URL. Default: `8765`.
- **Dependency version pinning** — Todas las dependencias en `requirements.txt` tienen límites superiores para evitar roturas por actualizaciones mayores:
  - `websockets>=14.0,<17.0`
  - `torch>=2.0.0,<2.7.0`
  - `faster-whisper>=1.0.0,<2.0.0`
  - `numpy>=1.24.0,<2.1.0`
  - `sounddevice>=0.4.6,<0.5.0`
  - `customtkinter>=5.2.0,<6.0.0`
- **Output_dir validation** — Antes de aplicar cambios, se valida que la carpeta de salida exista, sea un directorio, y sea escribible. Si no, se aborta con mensaje claro.

#### GPU & Performance

- **GPU auto-detection at startup** — Al iniciar, LiveAudio detecta si CUDA está disponible. Si no, cambia automáticamente a `cpu`. Ya no crashea en máquinas sin GPU.
- **VRAM monitoring** — Antes de cada transcripción en CUDA, se verifica la VRAM disponible. Si hay menos de 500MB libres, se libera la caché de CUDA antes de transcribir.
- **CUDA validation on device change** — Al cambiar a `cuda`, se verifica `torch.cuda.is_available()` Y se ejecuta `torch.zeros(1).cuda()` como prueba. Si falla, se rechaza el cambio con error claro.

### Tests

- 63 tests nuevos agregados (172 total).
- Cobertura: ASR timeout, backpressure, shutdown, Unicode, WS port, dependency pinning, VAD errors, output_dir validation, GPU detection, VRAM monitoring.

### Documentation

- `docs/WEBSOCKET_OBS.md` — Actualizado con puerto configurable y parámetro `?port=`.
- `docs/GETTING_STARTED.md` — Agregada sección de auto-detección de GPU y VRAM monitoring.
- Este CHANGELOG creado.

---

## [Previous]

Sin changelog previo. Cambes anteriores documentados solo en commits de git.
