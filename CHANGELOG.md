# Changelog

Todos los cambios notables de LiveAudio se documentan aquí.

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
