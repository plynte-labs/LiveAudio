# Changelog

Todos los cambios notables de LiveAudio se documentan aquí.

---

## [1.2.5] — 2026-08-03

### Corregido

- **Carga instantánea de modelos ASR local (`local_files_only=True`)** — Reparada la demora síncrona de hasta 3 minutos producida por peticiones de red a Hugging Face en cada inicio. Ahora el motor carga maquetas en disco de forma instantánea (~0.6s) y degrada suavemente a CPU si CUDA falla.

---

## [1.2.4] — 2026-08-03

### Agregado

- **Seguridad y compatibilidad de Origin en WebSocket** — El servidor WebSocket valida el Origin de las conexiones entrantes para bloquear accesos no autorizados desde navegadores externos, permitiendo únicamente integraciones locales loopback (localhost, 127.0.0.1 y ::1) y fuentes de navegador de OBS.
- **Descubrimiento automático de puerto fallback WS** — Si el puerto principal de WebSocket está ocupado, se escanean y ofrecen automáticamente puertos alternativos desde la UI.
- **Toggles independientes para sinks de disco y puerto WS** — Control independiente desde la interfaz para persistencia en archivo de texto o emisión WebSocket.

### Corregido

- **Recuperación de bloqueo silencioso de configuración** — Reparada la falla donde un bloqueo huérfano de `config.json` descartaba cambios de ajustes en silencio. Ahora las escrituras son atómicas y los bloqueos obsoletos (>30s) se limpian automáticamente.
- **Visibilidad del Overlay OBS** — El renderizado de subtítulos en `subtitulos_obs.html` ya no depende de `requestAnimationFrame`, asegurando actualización continua aun cuando la pestaña o el navegador de OBS no esté visible/enfocado.

### Rendimiento

- **Inicialización diferida del Manager** — Postergación de la creación del `Manager` de multiprocessing hasta su primer uso real, reduciendo la huella de memoria y el tiempo de arranque inicial de la interfaz.

---

## [1.2.3] — 2026-07-10

### Corregido

- **Fallo silencioso cuando el puerto WebSocket está ocupado** — si otra aplicación ya usaba el puerto de subtítulos (`ws_port`, por defecto 8765), el servidor WebSocket moría al iniciar mientras la app seguía registrando "Subtitulo enviado a OBS" (ese log medía el encolado interno, no la entrega real) y OBS conectaba con la aplicación equivocada sin mostrar error. Ahora: chequeo previo del puerto antes de arrancar el sistema (diálogo claro con el puerto en conflicto y las dos soluciones: cerrar la otra aplicación o cambiar `ws_port` en `config.json` y agregar `?port=` a la URL del navegador en OBS), monitoreo de salud del proceso WebSocket (a los 2 s del arranque y luego cada 5 s, avisa en consola y en el estado sin robar el foco en medio del stream) y mensaje específico de "puerto en uso" en el log del servidor. En POSIX el chequeo replica el `SO_REUSEADDR` del bind real para no dar falsos "ocupado" por sockets en TIME_WAIT tras un Stop→Start normal.

### Tests

- 8 tests nuevos (TDD): probe de puerto libre/ocupado, camino de error de bind ejercido en proceso, wiring de la GUI y paridad de claves i18n es/en. Suite total: 551.

---

## [1.2.2] — 2026-07-01

### Corregido

- **Actualizaciones in-app visibles y accionables** — el aviso de update ya no se pierde cuando la app detecta una versión nueva en la pantalla de bienvenida y luego reconstruye la pantalla principal. La versión detectada queda guardada como estado de la app y el aviso se vuelve a renderizar al entrar al panel principal.
- **Cadencia del updater** — el chequeo ahora corre en el primer inicio del día y luego, como máximo, cada 6 horas. Si no hay internet o falla la consulta, no interrumpe al usuario ni persiste un timestamp falso.
- **UX de update** — el banner mantiene “Actualizar ahora” y agrega “Más tarde” para posponer el aviso durante la sesión.
- **Versión visible** — la pantalla de bienvenida muestra la versión actual de LiveAudio para evitar confusión entre builds instalados y ejecuciones desde código fuente.

### Tests

- Cobertura agregada para cadencia del updater, fallos de red silenciosos, ciclo de vida del banner al reconstruir la UI y etiqueta de versión en bienvenida.

---

## [1.2.1] — 2026-06-27

### Corregido

- **Ventana casi invisible al cambiar de monitor / redimensionar** — CustomTkinter bajaba la opacidad de la ventana a 0.15 ante un cambio de DPI y la restauraba sin `try/finally`; si algo fallaba en el medio quedaba clavada en 15%. Se desactiva el DPI awareness automático de CTk al import (app y diálogo de crash) y se agrega `minsize` de ventana y de la columna de ajustes para que el panel en vivo no colapse al achicar. (ADR-013)
- **Cierre del proceso Manager** — `_shutdown()` ahora cierra explícitamente el `mp.Manager` del config compartido en vez de dejarlo para el `atexit`. (ADR-014)

### Rendimiento

- **RAM base: procesos GUI y WebSocket libres de torch** — en Windows `spawn` reimportaba torch en cada proceso. La GUI, el servidor WebSocket y el Manager ya no cargan torch ni faster-whisper: imports diferidos al sitio de spawn vía shims libres de torch (`core/workers.py`), enumeración de dispositivos en un módulo sin torch (`core/devices.py`), sonda CUDA out-of-process (`utils/cuda.py`, porque `import ctranslate2` arrastra torch) y localización de `torch/lib` vía `find_spec` sin importar torch (`utils/dllpath.py`). (ADR-014)
- **Fuga de RAM en sesiones largas** — el Silero VAD corría sin `torch.inference_mode()`, acumulando un grafo de autograd anclado a su estado LSTM persistente que nunca se liberaba. Medido: +583 MB / 20k chunks sin el guard, vs 0 MB plano con él. (ADR-015)

### Documentación

- ADR-013/014/015 en `docs/ARCHITECTURE_DECISIONS.md`; "decision ladder" de minimalismo en `CLAUDE.md` y `AGENTS.md`; proposals de seguimiento en `openspec/changes/` (probe CUDA en build frozen, allocator nativo del ASR, fallback cuda→cpu).

### Tests

- `tests/test_lazy_imports.py` — garantiza que importar la GUI no cargue torch/faster-whisper/ctranslate2. Suite total: 537 en verde.

---

## [1.1.0] — 2026-06-10 (sin publicar)

### Nuevo sistema de empaquetado y distribución

Reemplazo completo del workflow de distribución portable (Python embebido + launcher C# + PyInstaller) por un instalador liviano basado en [uv](https://docs.astral.sh/uv/).

- **Instalador bootstrapper** — `LiveAudio-Setup-X.Y.Z.exe` (Windows) y `LiveAudio-X.Y.Z-linux-x64.tar.gz` (Linux), ~25-45 MB. En el primer arranque detecta GPU NVIDIA automáticamente (driver ≥525, VRAM ≥4 GB) y descarga Python 3.11 + dependencias con el backend correcto de torch (`cpu` o `cu121`). Arranques posteriores son instantáneos.
- **Reestructura a paquete** — `main.py`, `core/` y `utils/` se movieron al paquete `liveaudio/` con `pyproject.toml` (hatchling) y `uv.lock`. Entry point: `liveaudio = "liveaudio.app:main"`. Versión única en `liveaudio/__init__.py`.
- **Extras de torch en conflicto** — `uv sync --extra cpu` o `--extra cu121`; los índices de PyTorch se enrutan por extra vía `tool.uv.sources`. Cada release publica `requirements-cpu.txt` / `requirements-cu121.txt` como alternativa pip.
- **Datos de usuario en ubicación estándar** — config y sesiones viven en `%APPDATA%\LiveAudio` / `~/.config/liveaudio` (override con `LIVEAUDIO_HOME`); migración automática del `config.json` legacy. Modo portable con `portable.marker`.
- **Actualizaciones in-app** — botón "Update now" que delega en el launcher (`--update`): descarga solo el código nuevo, reutiliza torch cacheado, verifica SHA256.
- **CI/CD** — `ci.yml` (ruff + pytest + self-test del launcher en Windows y Ubuntu) y `release.yml` (tag `v*` → draft release con instaladores, wheel, src zip y `SHA256SUMS.txt`).
- **Carga de DLLs robusta** — el hack de `PATH` para `torch\lib` se reemplazó por `os.add_dll_directory` + prepend de PATH en `liveaudio/utils/dllpath.py`, aplicado también en el proceso hijo de ASR.
- **SSL verificado en el updater** — primero contexto verificado; fallback no verificado solo ante `SSLCertVerificationError`, con warning en log.

### Eliminado

- `build_portable.py`, `compile_portable.bat`, `compile_pyinstaller.bat`, `LiveAudio.spec`, `setup_linux.sh` — reemplazados por el bootstrapper y CI.
- Launcher C# compilado con `csc.exe`, parcheo de `python310._pth` y `tkinter-embed` — ya no son necesarios.

### Documentación

- `docs/EMPAQUETADO_Y_ACTUALIZACION.md` → `docs/PACKAGING_AND_UPDATES.md` (doc canónico de packaging, en inglés).
- README y CONTRIBUTING actualizados al flujo nuevo (instalador para usuarios, `uv sync` para desarrollo).

### Tests

- 384 tests (69 nuevos: launcher, updater, pinning contra pyproject.toml).

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
