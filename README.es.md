# Plynte LiveAudio

[![Licencia: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE) [![Rust 2021](https://img.shields.io/badge/Rust-2021%20Edition-orange.svg)](https://www.rust-lang.org/) [![Tauri v2](https://img.shields.io/badge/Tauri-v2-blue.svg)](https://tauri.app/) [![Plataforma](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux-lightgrey.svg)](#requisitos-del-sistema)

**Subtítulos en tiempo real, 100% locales para streamers.** LiveAudio captura tu micrófono o el audio del sistema, transcribe localmente en tu GPU o CPU mediante Whisper y envía los subtítulos en vivo a OBS Studio. Nada sale a la nube.

<img width="1917" height="1020" alt="LiveAudio v1.5.1" src="https://github.com/user-attachments/assets/7b4c167c-9473-4009-836a-c5cb3592b29f" />

> **[⬇ Descargar la última versión](https://github.com/plynte-labs/LiveAudio/releases/latest)** · [Sitio Web](https://liveaudio.opencohost.com) · [Guía de integración con OBS](docs/WEBSOCKET_OBS.md) · [English Version](README.md)

---

## Por qué está diseñado así

La versión v1.2 estaba escrita 100% en Python y consumía cerca de 1 GB de memoria RAM en reposo. En la versión v1.5 migramos todos los componentes críticos en latencia a un **workspace de Rust**, manteniendo únicamente la inferencia de Whisper en Python como un proceso hijo supervisado:

```
Micrófono / Loopback ──► Rust (Captura CPAL) ──► Rust (Silero VAD, ONNX) ──► Worker Python (Faster-Whisper)
                                                                                       │
                              Overlay OBS ◄── Rust (WebSocket Tokio) ◄─────────────────┘
```

- **Núcleo en Rust:** Captura de audio, detección de actividad de voz (VAD), servidor WebSocket, persistencia de configuración y supervisión de procesos (mediante Win32 Job Objects, garantizando cero procesos huérfanos/zombies).
- **Worker de Python:** Exclusivo para Faster-Whisper, aislado en un subproceso y autoreiniciado automáticamente ante cualquier fallo.
- **IPC tipado:** Comunicación de alta velocidad mediante JSON Lines estructurado entre ambos extremos.

## Resultados comparativos (v1.5 en Rust vs v1.2 en Python)

| Métrica | Python v1.2.7 | Rust v1.5.0 | Mejora |
|---|---|---|---|
| Memoria en reposo | ~959 MB | ~73 MB (núcleo + worker inactivo) | ~13.1× menos RAM |
| Inicio en frío (Cold start) | ~2,812 ms | ~17 ms (CLI) | ~166× más rápido |
| VAD por frame de 32 ms | 0.358 ms | 0.114 ms | ~3.1× más rápido |
| Cierre limpio (Clean shutdown) | ~3,401 ms | ~7 ms | ~485× más rápido |
| Rendimiento ASR (`small`, RTX 3060, fp16) | — | ~25.7× tiempo real | Latencia ultra baja |

Medido en: Windows 11, RTX 3060, AMD Ryzen 7 3700G, 32 GB RAM. Metodología detallada en el [reporte de benchmarks](docs/migration/wu9_benchmark_comparison_report.md).

---

## Guía de Inicio Rápido (Streamers y Creadores)

### 1. Descarga y Ejecución
1. Descarga **`LiveAudio-Setup-1.5.1.exe`** (o la versión portable en `.zip`) desde [GitHub Releases](https://github.com/plynte-labs/LiveAudio/releases/latest).
2. Ejecuta la aplicación. En el primer inicio, LiveAudio prepara de forma autónoma su entorno aislado de Python 3.11 y el modelo Silero VAD en ONNX. No es necesario instalar Python ni CUDA manualmente.

### 2. Configuración en 3 Clics
En la pestaña **Inicio (Setup)**:
1. **Fuente de audio:** Elige tu micrófono físico o **Audio del sistema (WASAPI Loopback)** para transcribir el juego o sonido de la PC.
2. **Idioma hablado:** Selecciona `es` (Español) o `en` (Inglés).
3. **Perfil recomendado:** Escoge `Balanceado` (por defecto) o el que mejor se ajuste a tu hardware.
4. Presiona **Iniciar Transcripción**.

### 3. Conexión con OBS Studio
1. En OBS Studio, haz clic en **+** en el panel **Fuentes** y añade una fuente **Navegador (Browser)**.
2. Marca la casilla **Archivo local (Local file)** y selecciona:
   ```
   liveaudio/assets/subtitulos_obs.html
   ```
3. Configura **Ancho: 1920**, **Alto: 200** y **FPS: 60**.
4. Desmarca *"Apagar fuente cuando no sea visible"* y desmarca *"Actualizar el navegador cuando la escena se active"* para mantener la conexión WebSocket estable.
5. ¡Empieza a hablar! Los subtítulos aparecerán en pantalla de inmediato.

---

## Integración y Personalización en OBS Studio

El overlay incluido se comunica con el servidor WebSocket local de LiveAudio (`ws://127.0.0.1:8765`) en loopback seguro.

### Temas Visuales Incluidos
Selecciona tu tema favorito desde la pestaña **Subtítulos** en tiempo real:

| Tema | Descripción Visual |
|---|---|
| `default` | Texto blanco con sombra suave sobre caja negra translúcida y animación suave. |
| `karaoke` | Resaltado dinámico palabra por palabra con animación `popIn` en amarillo dorado y borde negro. |
| `neon` | Estética cyberpunk retro con resplandor cian brillante y tipografía en mayúsculas. |
| `minimal` | Tipografía limpia sin caja de fondo; ideal para no tapar gameplay. |
| `bold` | Alto contraste, trazo grueso y transiciones rápidas para streams de acción. |
| `rgb` | Gradiente vertical multicolor que recorre las palabras habladas. |
| `typewriter` | Efecto clásico de máquina de escribir letra por letra con cursor parpadeante en fuente monoespaciada. |

### Modos de Visualización
- **Adaptive:** Ajusta inteligentemente la duración y el formato según la extensión de cada frase.
- **Single:** Muestra únicamente la última frase expresada; ideal para overlays compactos.
- **Ribbon:** Apila verticalmente un historial de subtítulos (configurable entre 1 y 8 líneas).

### Detección Dinámica de Puertos
LiveAudio utiliza por defecto el puerto `8765`. Si dicho puerto está ocupado por otra aplicación, el servidor escanea automáticamente el rango `8765..8774` para encontrar un puerto libre. En OBS basta con indicar el puerto efectivo en la URL:
```
file:///ruta/a/liveaudio/assets/subtitulos_obs.html?port=8766
```

---

## Perfiles de Hardware Preconfigurados

Los perfiles predefinidos equilibran latencia y consumo de recursos sin configuraciones complejas:

| Perfil | Dispositivo | Tamaño de Modelo | Tiempo Silencio | Política Backlog | Uso Recomendado |
|---|---|---|---|---|---|
| **Rápido** | CPU | `base` | `0.4s` | `live_only` (máx 5s) | Frases muy cortas, mínima demora y bajo uso de CPU. |
| **Balanceado** | CUDA / GPU | `small` | `0.8s` | `auto` (máx 10s) | **Recomendado para la mayoría de streams.** Gran precisión con respuesta sub-segundo. |
| **Calidad** | CUDA / GPU | `turbo` | `1.0s` | `auto` (máx 15s) | Máxima fidelidad para podcasts, entrevistas y charlas extensas. |
| **Streaming Estable** | CPU | `small` | `0.6s` | `live_only` (máx 6s) | Libera por completo la tarjeta gráfica para juegos AAA intensivos. |

---

## Novedades y Capacidades Destacadas de la v1.5.1

### 1. Interfaz de Escritorio en Tauri 2 Bilingüe
- Cambio instantáneo entre **Español (🇪🇸)** e **Inglés (🇺🇸)**.
- Ventana sin marcos moderna con controles integrados de minimizar, maximizar, restaurar y cerrar.
- Panel lateral de **Preview de Subtítulos en Vivo** y botón **Probar** para verificar el estilo en OBS con un solo clic.

### 2. Guard de Cierre Protegido (Close Guard)
- Si intentas cerrar la ventana mientras una transcripción está en curso, LiveAudio muestra un modal de confirmación accesible.
- Evita interrumpir transmisiones en vivo por error. Aceptar realiza un apagado ordenado (`ExitRequested`); cancelar mantiene la sesión activa.

### 3. Corrección de Replay para PTT (Push-to-Talk)
- Diseñado para integraciones como OpenCohost que abren una conexión WebSocket por cada pulsación de PTT.
- Los subtítulos históricos emitidos en la conexión tardía se etiquetan con `is_replay: true`, evitando que frases previas se repitan como si fueran voz nueva.

### 4. Política Anti-Ráfaga y Gestión de Backlog
- **`auto`**: Entrega subtítulos en vivo al instante. Retrasos breves se dosifican a un ritmo natural (`catchup_interval_sec = 1.5s`). Los subtítulos que superen el tiempo límite (`subtitle_max_live_delay_sec`) se guardan en disco pero se omiten en OBS para evitar spam visual.
- **`live_only`**: Muestra estrictamente subtítulos frescos, descartando el backlog si el sistema se congela temporalmente.
- **`send_all`**: Garantiza la emisión de todas las transcripciones en un búfer circular acotado (máximo 256 elementos con descarte de los más antiguos) preservando el tiempo real.

### 5. Win32 Job Objects (Cero Procesos Zombies)
- Implementado en Rust nativo (`crates/liveaudio-core/src/job_object/mod.rs`) mediante la directiva `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
- Si la aplicación de escritorio se cierra abruptamente, crashea o es finalizada desde el Administrador de Tareas, el kernel de Windows liquida al instante todos los procesos hijos de Python ASR. Cero memoria o VRAM bloqueada en segundo plano.

### 6. Aprovisionamiento Honesto y Errores Claros
- Progreso real y monótono de descarga (0–100%) con indicadores de KiB/MiB sin barras falsas ni porcentajes simulados.
- Códigos de diagnóstico accionables (`provision-network`, `provision-disk-full`, `provision-tls`, `provision-cache-corrupt`) con botón de reintento directo.

### 7. Grabación Completa de Sesiones
- Cada sesión guarda automáticamente la transcripción limpia en formato `.jsonl` y los subtítulos sincronizados en formato estándar WebVTT (`.vtt`) dentro de la carpeta `sessions/`.

---

## Requisitos del Sistema

| Componente | Mínimo | Recomendado |
|---|---|---|
| **Sistema Operativo** | Windows 10/11 x86_64, Linux x86_64 | Windows 11 (soporte nativo WASAPI Loopback) |
| **Tarjeta Gráfica** | No requerida (modo CPU funcional) | NVIDIA GTX 1660 / RTX 2060 o superior con CUDA 12.1+ |
| **VRAM** | 2 GB (modelos `tiny` / `base`) | 6 GB o más (modelos `small` / `turbo`) |
| **Memoria RAM** | 8 GB | 16 GB |
| **Almacenamiento** | 500 MB (app) + 1 GB por modelo | Unidad de estado sólido (SSD) |
| **Dispositivo de Entrada** | Micrófono estándar USB / 3.5mm o loopback | Micrófono cardioide dedicado o audio de sistema |

### Compatibilidad con Máquinas Virtuales (VMs)
- **Sin GPU passthrough:** Funciona de forma totalmente estable en modo CPU multi-hilo (1 a 16 hilos).
- **Con vGPU / PCIe passthrough:** Aceleración completa por hardware mediante CUDA.
- **Dispositivo de audio:** Asegúrate de exponer un adaptador de audio virtual al sistema invitado.

---

## Guía de Desarrollo y Compilación desde el Código

### Requisitos Previos
- **Rust:** `1.80+` (edición 2021) con `cargo`
- **Node.js:** `18+` (LTS) con `npm`
- **Python:** `3.11` administrado mediante [uv](https://docs.astral.sh/uv/)

### Configuración del Entorno

```bash
# 1. Clonar el repositorio
git clone https://github.com/plynte-labs/LiveAudio.git
cd LiveAudio

# 2. Sincronizar dependencias del worker de Python con uv
# Para aceleración con NVIDIA GPU (CUDA 12.1):
uv sync --extra cu121

# O para desarrollo exclusivo en CPU:
uv sync --extra cpu

# 3. Iniciar la aplicación de escritorio en modo desarrollo (Tauri 2)
cargo run -p liveaudio-desktop

# 4. Iniciar el servicio CLI sin interfaz (headless)
cargo run -p liveaudio-cli -- --help
```

### Ejecución de Pruebas Automatizadas

```bash
# Tests del workspace completo de Rust (core, network, audio, vad, ipc)
cargo test --workspace

# Tests del frontend de Tauri (DOM, i18n, guard de cierre)
cd desktop && node test_frontend.js

# Tests de integración IPC del worker Python
.venv\Scripts\pytest tests/test_asr_worker_ipc.py
```

### Compilación para Producción (Release)

```bash
# 1. Compilar el sidecar CLI en release
cargo build --release -p liveaudio-cli

# 2. Empaquetar la aplicación de escritorio con Tauri 2
cd desktop
npx @tauri-apps/cli@2.12.1 build --config src-tauri/tauri.release.conf.json
```

---

## Estructura del Proyecto

```
LiveAudio/
├── crates/
│   ├── liveaudio-core/       # Ciclo de vida, máquina de estados, job objects y aprovisionamiento
│   ├── liveaudio-audio/      # Captura de audio CPAL (Micrófono y WASAPI loopback)
│   ├── liveaudio-vad/        # Detección Silero VAD v6 (motor ONNX Runtime)
│   ├── liveaudio-network/    # Servidor WebSocket Tokio-Tungstenite y protocolo OBS
│   ├── liveaudio-ipc/        # Protocolo IPC tipado JSON Lines entre Rust y Python
│   └── liveaudio-cli/        # Servicio CLI headless y binario sidecar para Tauri
├── desktop/
│   ├── src-tauri/            # Backend de Tauri 2 (comandos en Rust y control de ventana)
│   └── src/                  # Frontend ligero (HTML, CSS, JS, i18n y vista previa en vivo)
├── liveaudio/
│   ├── service/              # Runtime del worker Faster-Whisper en Python
│   └── assets/               # subtitulos_obs.html (overlay Browser Source)
└── docs/                     # Especificaciones del protocolo WebSocket, reportes y benchmarks
```

---

## Solución de Problemas Frecuentes

| Síntoma | Causa Probable | Solución |
|---|---|---|
| **No transcribe audio / subtítulos vacíos** | Dispositivo de audio incorrecto | Ve a la pestaña **Inicio** y confirma que tu micrófono o WASAPI Loopback esté activo. Verifica los permisos de audio. |
| **Latencia elevada (>3s) en CPU** | Modelo demasiado pesado para el procesador | Cambia a `tiny` o `base`, o elige el perfil `Rápido`. |
| **El overlay en OBS aparece desconectado** | LiveAudio no está iniciado o puerto incorrecto | Verifica que hayas pulsado **Iniciar Transcripción**. Comprueba el puerto efectivo en la píldora de estado (ej: `8765`). Si cambió, añade `?port=XXXX` a la URL en OBS. |
| **Error de memoria CUDA (Out of Memory)** | VRAM ocupada por juegos u otros programas | Cambia a modelo `small` o modo CPU. LiveAudio purga la caché de CUDA automáticamente si la VRAM baja de 500 MB. |
| **Descarga del modelo atascada** | Corte de red o fallo en el apretón TLS | Haz clic en **Reparar entorno / Reintentar**. LiveAudio reanudará la descarga sin reiniciar desde cero. |

---

## Licencia y Créditos

Distribuido bajo la **Licencia MIT**. Consulta [LICENSE](LICENSE) para más detalles.

### Tecnologías de Código Abierto Principales
- [Tauri 2](https://tauri.app/) — Framework de escritorio nativo, ligero y seguro.
- [Faster-Whisper](https://github.com/SYSTRAN/faster-whisper) y [CTranslate2](https://github.com/OpenNMT/CTranslate2) — Motor de inferencia Whisper de alto rendimiento.
- [Silero VAD](https://github.com/snakers4/silero-vad) y [ONNX Runtime](https://onnxruntime.ai/) — Detección de voz rápida y nativa.
- [CPAL](https://github.com/RustAudio/cpal) — Captura de audio de baja latencia multiplataforma.
- [Tokio](https://tokio.rs/) — Runtime asíncrono y servidor WebSocket de alto rendimiento en Rust.
