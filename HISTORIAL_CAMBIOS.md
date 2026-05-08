# Historial de Auditoría y Cambios (LiveAudio)

Este documento detalla todas las modificaciones realizadas durante la auditoría técnica y de seguridad del proyecto LiveAudio, así como la justificación técnica de cada decisión.

## 1. Reestructuración del Proyecto

Se reorganizaron los archivos para que coincidieran con lo descrito en la documentación de `arquitectura.md`.

*   **Cambio:** Se movió `producer.py` a `core/audio.py`.
    *   **Por qué:** Para separar claramente la lógica dura (procesos en segundo plano) de la interfaz de usuario y mantener la coherencia con el diseño arquitectónico original.
*   **Cambio:** Se movió `ws_server.py` a `core/network.py`.
    *   **Por qué:** Mismo motivo que el anterior, aislando la lógica de red en el módulo correspondiente.
*   **Cambio:** Se crearon los archivos `core/__init__.py` y `utils/__init__.py`.
    *   **Por qué:** Para convertir estos directorios en paquetes de Python reales, evitando problemas de importación (especialmente al empaquetar la aplicación en el futuro).
*   **Cambio:** Se creó `.gitignore` y `requirements.txt`.
    *   **Por qué:** Buenas prácticas de desarrollo. `.gitignore` evita subir archivos temporales (`__pycache__`, logs, sesiones antiguas) al repositorio. `requirements.txt` permite que cualquier persona o máquina replique el entorno fácilmente.

## 2. Nueva Funcionalidad: Selección de Dispositivo de Audio

*   **Cambio:** Se agregó un selector de dispositivos en la interfaz de usuario (`main.py`) y la lógica de selección en `core/audio.py`.
    *   **Por qué:** Originalmente, el sistema estaba limitado a usar el micrófono por defecto del sistema operativo. Esto impedía al usuario elegir entre múltiples micrófonos o capturar el audio interno del sistema.
*   **Cambio:** Se implementó soporte para WASAPI Loopback en Windows.
    *   **Por qué:** Esto permite capturar el "audio del sistema" (por ejemplo, el audio de un juego, un video de YouTube o una llamada de Discord), lo cual es crucial para generar subtítulos de fuentes que no sean el micrófono físico del usuario.

## 3. Corrección de Bugs Críticos (Ciclo de Vida de Procesos)

*   **Cambio:** Se eliminó `os._exit(0)` y se reemplazó el uso abrupto de `terminate()` por un apagado limpio (envío de señal -> `join` con timeout -> `terminate` como último recurso) en `main.py`.
    *   **Por qué:** `os._exit(0)` y `terminate()` mataban los procesos de forma violenta. Esto dejaba archivos a medias (JSONL/VTT), corrompía las colas de comunicación (Pipes) y, en Windows, a menudo dejaba procesos "zombie" (procesos huérfanos que consumían RAM/VRAM) y puertos de audio bloqueados.
*   **Cambio:** Se añadió un tamaño máximo (`maxsize=100`) a las colas de comunicación (`mp.Queue`).
    *   **Por qué:** Si el motor de Whisper (consumidor) se retrasaba procesando audio, el productor seguía metiendo audio a la cola indefinidamente. En sesiones largas, esto causaba fugas de memoria (OOM - Out of Memory) y colgaba la PC.

## 4. Mejoras en Red y WebSockets (`core/network.py`)

*   **Cambio:** Se modificó el servidor WebSocket para soportar múltiples clientes simultáneos mediante un sistema de *broadcast*.
    *   **Por qué:** Antes, el servidor solo enviaba los subtítulos a una única conexión. Si OBS se desconectaba brevemente y se reconectaba, o si el usuario abría el HTML en el navegador al mismo tiempo que en OBS, los mensajes se perdían o el programa fallaba.
*   **Cambio:** Se agregó una validación del header `Origin` en el WebSocket.
    *   **Por qué:** Medida de seguridad. Rechaza conexiones que no provengan de `localhost` (127.0.0.1), evitando que scripts maliciosos externos puedan leer el flujo de subtítulos.
*   **Cambio:** Se actualizó la firma del *handler* para ser compatible con versiones modernas de la librería `websockets` (>= 11).

## 5. Frontend, HTML y CSS (`subtitulos_obs.html`)

*   **Cambio:** Se añadieron animaciones de salida unificadas y suaves (fade-out y deslizamiento) para todos los estilos visuales.
    *   **Por qué:** La experiencia de usuario era pobre porque los subtítulos desaparecían de golpe, causando parpadeos visuales en el stream.
*   **Cambio:** Se implementó una limpieza del DOM (`container.innerHTML = ''`) después de que la animación de salida termina.
    *   **Por qué:** El código anterior dejaba los elementos `<div>` invisibles acumulándose en el HTML. Tras horas de stream, esto degradaba el rendimiento del *Browser Source* de OBS por tener miles de nodos "fantasma".
*   **Cambio:** Se añadió el meta tag `<meta name="viewport">`.
    *   **Por qué:** Mejora la compatibilidad y renderizado si se abre desde navegadores móviles o ventanas redimensionadas.

## 6. Optimizaciones y Limpieza de Código

*   **Cambio:** Se movió la importación de `json` en `core/engine.py` fuera del bucle principal (`while True`).
    *   **Por qué:** Importar módulos repetidamente dentro de un ciclo caliente (hot-path) degrada ligeramente el rendimiento.
*   **Cambio:** Se automatizó la migración en `utils/config.py`.
    *   **Por qué:** Si se añaden nuevas opciones al código (como el `audio_device`), el sistema actualizará automáticamente el archivo `config.json` del usuario sin borrar su configuración anterior ni crashear.
*   **Cambio:** Se eliminó código muerto y referencias rotas (ej. `setup_initial_config` en config.py que llamaba a bibliotecas no importadas).
    *   **Por qué:** Reduce la deuda técnica y evita crasheos si esas funciones se llegaran a invocar por error.
*   **Cambio:** Se modificó el filtro de advertencias (`warnings.filterwarnings`) para silenciar solo `UserWarning`.
    *   **Por qué:** Silenciar absolutamente todas las advertencias escondía errores críticos o deprecaciones de seguridad. Ahora la consola sigue limpia pero alertará de problemas graves.

## 7. Auditoría UI/UX, Seguridad y Privacidad (2026-05-05)

*   **Cambio:** Se creó `docs/AUDITORIA_UI_SEGURIDAD.md` con hallazgos por severidad, cambios aplicados, riesgos residuales y plan de pruebas manuales.
    *   **Por qué:** Las transcripciones, logs y sesiones son datos sensibles; la auditoría deja explícitos los límites de privacidad y las validaciones necesarias.
*   **Cambio:** Se agregó en `main.py` una vista principal con estados visibles para Audio, VAD, ASR, WebSocket, OBS/clientes y sesión, además de un preview separado para el último subtítulo.
    *   **Por qué:** El flujo en vivo debe indicar qué parte del pipeline está activa o fallando sin depender de logs técnicos.
*   **Cambio:** Se movieron los logs técnicos a un modo avanzado y se limitó su crecimiento visible.
    *   **Por qué:** Reduce ruido visual, consumo durante sesiones largas y exposición accidental de contenido sensible.
*   **Cambio:** Se agregaron eventos de estado desde `core/audio.py`, `core/engine.py` y `core/network.py` sin cambiar las colas principales de audio/transcripción.
    *   **Por qué:** Mejora observabilidad sin romper ASR local, captura, VAD ni WebSocket/OBS.
*   **Cambio:** Se añadió validación defensiva de `config.json` en `utils/config.py` y whitelist de estilos en `core/engine.py` y `subtitulos_obs.html`.
    *   **Por qué:** Evita valores corruptos o extremos y reduce riesgos de render/configuración insegura.

## 8. Nueva Funcionalidad: Política Configurable de Backlog OBS (2026-05-07)

*   **Cambio:** Se agregaron los modos `auto`, `live_only` y `send_all` para decidir qué hacer con subtítulos atrasados tras freezes de GPU/CPU o saturación del ASR.
    *   **Por qué:** Evita que OBS reciba de golpe muchos subtítulos viejos después de varios minutos de atraso, sin perder la transcripción completa guardada en disco.
*   **Cambio:** Se añadieron `subtitle_backlog_policy`, `subtitle_max_live_delay_sec` y `subtitle_catchup_interval_sec` a la configuración validada.
    *   **Por qué:** Permite ajustar el comportamiento según el tipo de stream: priorizar live limpio, enviar todo, o usar una política automática.
*   **Cambio:** `core/audio.py` ahora adjunta metadata de frase (`created_at`, `sequence`) y `core/engine.py` genera `id`, `queue_delay`, `total_delay`, `latency`, modelo y dispositivo en `transcript.jsonl`.
    *   **Por qué:** Mejora observabilidad e idempotencia futura para distinguir frases, atrasos, replay y subtítulos live.
*   **Cambio:** `core/network.py` aplica pacing de catch-up sin bloquear subtítulos live nuevos detrás del backlog.
    *   **Por qué:** El pacing no debe crear más cola ni empeorar la latencia en vivo.
*   **Cambio:** La UI distingue entre transcripción guardada y subtítulo realmente enviado a OBS.
    *   **Por qué:** Evita diagnósticos falsos cuando una política omite visualmente un subtítulo atrasado pero lo conserva en sesión.

## 9. Nueva Funcionalidad: Perfiles y Aplicar Cambios (2026-05-08)

*   **Cambio:** Se agregaron perfiles integrados `Rápido`, `Balanceado`, `Calidad` y `Streaming estable`.
    *   **Por qué:** Reducen la complejidad para usuarios que no quieren ajustar manualmente latencia, modelo, device y política OBS.
*   **Cambio:** Los sliders y selectores sensibles ahora generan cambios pendientes hasta pulsar `Aplicar cambios`.
    *   **Por qué:** Evita reinicios/hot-swap mientras el usuario arrastra controles y reduce sensación de lag.
*   **Cambio:** El panel de ajustes se organizó en pestañas: Perfiles, Audio/VAD, Rendimiento, OBS y Avanzado.
    *   **Por qué:** Hace la configuración más navegable para usuarios promedio y mantiene controles avanzados disponibles.
*   **Cambio:** Si se aplican cambios duros con el motor activo, la UI advierte que habrá hot-swap y posible corte breve.
    *   **Por qué:** Convierte el lag esperado en un evento explícito y controlado.

## 10. Setup SDD Conductor + Engram Workflow (2026-05-08)

*   **Cambio:** Se inicializó el contexto Conductor en `conductor/` con `product.md`, `product-guidelines.md`, `tech-stack.md`, `workflow.md`, `index.md`, `tracks.md` y guía de estilo Python.
    *   **Por qué:** Provee una base estructurada para planificar, implementar y revisar tracks futuros con trazabilidad.
*   **Cambio:** Se instalaron skills Conductor (`conductor-setup`, `conductor-newTrack`, `conductor-implement`, `conductor-status`, `conductor-review`, `conductor-revert`) y se documentó su uso en `docs/SDD_SKILLS_USAGE.md`.
    *   **Por qué:** Habilita flujo SDD (Spec-Driven Development) local sin depender de herramientas externas.
*   **Cambio:** Se crearon `docs/SDD_ENGRAM_WORKFLOW.md`, `docs/NUEVO_PROYECTO_SDD_ENGRAM.md` y `docs/ENGRAM_LOCAL_COMANDOS.md` con flujo operativo, status brownfield y comandos Engram sanitizados.
    *   **Por qué:** Documenta cómo usar memoria persistente Engram solo para este proyecto, evitando contaminación cruzada.
*   **Cambio:** Se agregaron overrides en todas las skills Conductor: política de commit con aprobación explícita, mapeo de herramientas OpenCode (`question`, `apply_patch`, `bash`), resolución de archivos Universal File Resolution Protocol, y regla de privacidad Engram.
    *   **Por qué:** Las skills genéricas auto-commitean y usan herramientas no disponibles; los overrides alinean el flujo con las reglas de LiveAudio.
*   **Cambio:** Se creó `AGENTS.md` con reglas memory-first, preferencias del usuario, flujo SDD y equipo especializado.
    *   **Por qué:** Centraliza las instrucciones operativas para cualquier agente que trabaje en este repositorio.
*   **Cambio:** Se registraron tracks completados en `conductor/tracks.md`: backlog OBS (`e6575b2`), equipo especializado (`1644b11`), perfiles/apply flow (`26598e8`).
    *   **Por qué:** Mantiene historial trazable entre commits y decisiones de producto.
