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
