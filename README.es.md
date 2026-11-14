# LiveAudio

por Plynte Labs / FranGuh

[![Licencia: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE) [![Rust 2021](https://img.shields.io/badge/Rust-2021%20Edition-orange.svg)](https://www.rust-lang.org/) [![Tauri v2](https://img.shields.io/badge/Tauri-v2-blue.svg)](https://tauri.app/) [![DirectML](https://img.shields.io/badge/DirectML-DX12%20GPU-purple.svg)](https://learn.microsoft.com/en-us/windows/ai/directml/dml) [![Plataforma](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux-lightgrey.svg)](#requisitos-del-sistema)

**Subtítulos en tiempo real y 100% locales para streamers.** LiveAudio captura tu micrófono o el audio del sistema, transcribe localmente en tu GPU o CPU mediante Whisper nativo sobre ONNX Runtime DirectML o CPU (y un motor NVIDIA CUDA opcional), y envía los subtítulos en vivo a OBS Studio sin depender de la nube. El audio y las transcripciones nunca salen de tu ordenador; la aplicación solo consulta GitHub Releases en busca de nuevas versiones cada 24 horas (se puede desactivar en Ajustes > Avanzado).

<img width="1917" height="1020" alt="LiveAudio v1.6.0" src="https://github.com/user-attachments/assets/7b4c167c-9473-4009-836a-c5cb3592b29f" />

> **[⬇ Descargar la última versión](https://github.com/plynte-labs/LiveAudio/releases/latest)** · [Sitio Web](https://liveaudio.opencohost.com) · [Guía de integración con OBS](docs/WEBSOCKET_OBS.md) · [English Version](README.md)

---

## Por qué está diseñado así

Las primeras versiones (v1.2) estaban escritas en Python y consumían cerca de 1 GB de memoria RAM en reposo. En la versión v1.5 migramos los componentes de baja latencia a Rust, y ahora LiveAudio es **100% Rust Nativo + Tauri 2** con **CERO dependencias de Python**:

```
Micrófono / Loopback ──► Rust (Captura CPAL) ──► Rust (Silero VAD, ONNX) ──► Rust ASR (Whisper, ONNX Runtime DirectML/CPU)
                                                                                       │
                              Overlay OBS ◄── Rust (WebSocket / HTTP Tokio) ◄──────────┘
```

- **Núcleo y Pipeline 100% Rust Nativo:** Captura de audio (CPAL), detección de actividad de voz (Silero VAD en ONNX Runtime), motor de transcripción Whisper ASR en Rust nativo con aceleración DirectX 12 mediante ONNX Runtime DirectML (NVIDIA, AMD e Intel) y CPU multi-hilo, motor NVIDIA CUDA opcional sobre CTranslate2, servidor WebSocket y persistencia de configuración.
- **Cero Requisito de Python:** No requiere instalar Python, crear entornos virtuales (`.venv`) ni configurar CUDA. La versión de Windows incluye `DirectML.dll` y `onnxruntime.dll` (`1.24.4-directml`) listos para usar con cualquier tarjeta gráfica compatible con DirectX 12.
- **Supervisión y Seguridad de Procesos:** Win32 Job Objects en Windows garantizan un apagado limpio con 0 procesos huérfanos o zombies.
- **Canales de Alta Velocidad:** El flujo de audio, VAD, fragmentos ASR y emisión a OBS se comunican mediante canales asíncronos sin bloqueos.

## Resultados comparativos (Rust ORT vs Python Legacy)

| Métrica | Python v1.2.7 Legacy | Rust v1.6.0 Nativo | Mejora |
|---|---|---|---|
| Memoria en reposo | ~959 MB | ~65 MB (stack nativo completo) | ~14.7× menos RAM |
| Inicio en frío (Cold start) | ~2,812 ms | ~17 ms (CLI) | ~166× más rápido |
| VAD por frame de 32 ms | 0.358 ms | 0.114 ms | ~3.1× más rápido |
| Cierre limpio (Clean shutdown) | ~3,401 ms | ~7 ms (Job Objects) | ~485× más rápido |
| Aceleración por hardware | Exclusivo CUDA (NVIDIA) | DirectML (NVIDIA, AMD, Intel), CPU y CUDA opcional (NVIDIA) | Compatibilidad universal DX12 |

Medido en: Windows 11, RTX 3060, AMD Ryzen 7 3700G, 32 GB RAM.

---

## Guía de Inicio Rápido (Streamers y Creadores)

### 1. Descarga y Ejecución
1. Descarga **`LiveAudio-Setup-1.6.0.exe`** (o la versión portable en `.zip`) desde [GitHub Releases](https://github.com/plynte-labs/LiveAudio/releases/latest).
2. Ejecuta la aplicación. LiveAudio funciona de inmediato con su motor embebido ONNX Runtime DirectML y Silero VAD. No es necesario instalar Python ni controladores CUDA adicionales.

> **Actualización desde v1.5.1 o anterior:** Los usuarios en la versión 1.5.1 o anterior deben instalar la versión 1.6.0 manualmente una vez. Las versiones previas no cuentan con soporte de actualización integrada ni con la clave pública de firma. A partir de 1.6.0, las actualizaciones se gestionan directamente desde la aplicación.

### 2. Configuración en 3 Clics
En la pestaña **Inicio (Setup)**:
1. **Fuente de audio:** Elige tu micrófono físico o **Audio del sistema (WASAPI Loopback)** para transcribir el juego o sonido de la PC.
2. **Idioma hablado:** Selecciona `es` (Español) o `en` (Inglés).
3. **Perfil recomendado:** Escoge `Balanceado` (por defecto) o el que mejor se ajuste a tu hardware.
4. Presiona **Iniciar Transcripción**.

### 3. Conexión con OBS Studio
1. En OBS Studio, haz clic en **+** en el panel **Fuentes** y añade una fuente **Navegador (Browser)**.
2. Configura la **URL** como:
   ```
   http://127.0.0.1:8765/overlay
   ```
   *(O marca **Archivo local** y selecciona `assets/subtitulos_obs.html` en la carpeta de la aplicación).*
3. Configura **Ancho: 1920**, **Alto: 1080** y **FPS: 60**.
4. Desmarca *"Apagar fuente cuando no sea visible"* y desmarca *"Actualizar el navegador cuando la escena se active"* para mantener la conexión WebSocket estable.
5. ¡Empieza a hablar! Los subtítulos aparecerán en pantalla de inmediato.

---

## Integración y Personalización en OBS Studio

El overlay para fuentes de navegador se conecta al servidor local de WebSocket de LiveAudio (`ws://127.0.0.1:8765`) o se carga directamente desde el servidor HTTP integrado en `http://127.0.0.1:8765/overlay`.

### Temas Disponibles
Elige el estilo visual desde la pestaña **Subtítulos** sin reiniciar la aplicación:

| Tema | Aspecto Visual |
|---|---|
| `default` | Texto blanco limpio con sombra suave y fondo semitransparente oscuro. |
| `karaoke` | Resaltado dinámico palabra por palabra con animación `popIn` en amarillo dorado y borde negro. |
| `neon` | Estilo retro-futurista con bordes cian brillante, brillo neón y espaciado tipográfico amplio. |
| `minimal` | Subtítulos limpios sin recuadro ni fondo, con desvanecimiento sutil. |
| `bold` | Tipografía pesada y contraste alto para juegos de ritmo rápido. |
| `rgb` | Paleta arcoíris cíclica palabra por palabra. |
| `typewriter` | Letra monoespaciada con efecto mecanográfico caracter a caracter. |

---

## Novedades Principales de v1.6.0

### 0. Actualizador Integrado con Protección de Transmisiones (Stream-Safe Updater)
- **Bloqueo de Transmisión Activa:** Comprobación periódica cada 24 h en GitHub Releases. Las descargas e instalaciones se bloquean automáticamente si la transcripción está activa o si hay clientes de OBS conectados.
- **Firmas Criptográficas Minisign:** Verificación obligatoria de firmas Ed25519 antes de ejecutar cualquier instalador.
- **Privacidad Total:** Cero telemetría ni recopilación de identificadores. Opción de "Instalar al cerrar" diferida para no interferir con directos.

### 1. Arquitectura 100% Rust Nativo + ONNX Runtime DirectML
- Eliminación total de dependencias de Python.
- Aceleración gráfica universal DirectX 12 mediante DirectML (NVIDIA, AMD, Intel) y fallback multi-hilo en CPU.
- Motor NVIDIA CUDA opcional (CTranslate2) para GPUs NVIDIA. Es opt-in: la app descarga cuBLAS y cuDNN de NVIDIA bajo petición, tras aceptar sus términos de licencia.

### 2. Interfaz de Escritorio Bilingüe en Tauri 2
- Selector rápido entre **Español (🇪🇸)** e **Inglés (🇺🇸)**.
- Navegación clara en cuatro secciones: **Inicio**, **Subtítulos**, **Avanzado** y **Archivos**.

### 3. Captura Nativa de Audio y WASAPI Loopback
- Captura de micrófonos y audio interno de Windows vía CPAL con remuestreo automático a 16 kHz mono.

### 4. Detección de Actividad de Voz con Silero VAD v6
- Inferencia nativa ultrarrápida evaluando frames de 32 ms en ~0.11 ms sin fugas de memoria.

### 5. Grabación Dual de Sesiones
- Guardado automático de transcripciones en formato estructurado `.jsonl` y subtítulos temporizados `.vtt` en la carpeta `sessions/`.

---

## Requisitos del Sistema

| Componente | Mínimo | Recomendado |
|---|---|---|
| **Sistema Operativo** | Windows 10/11 x86_64, Linux x86_64 | Windows 11 (soporte WASAPI Loopback nativo) |
| **Python** | **No requerido** (Stack 100% Rust) | Ninguno |
| **GPU** | Ninguna (funciona en modo CPU multi-hilo) | GPU compatible con DirectX 12 (NVIDIA, AMD, Intel) |
| **VRAM** | 2 GB (para modelos `tiny` / `base`) | 4 GB+ (para modelos `small` / `turbo`) |
| **RAM** | 4 GB | 8 GB+ |
| **Almacenamiento** | 250 MB (app) + 500 MB por modelo | Disco de estado sólido (SSD) |
| **Dispositivo de Entrada** | Micrófono estándar USB / 3.5mm o loopback | Micrófono cardioide dedicado o audio de sistema |

---

## Guía de Desarrollo y Compilación desde el Código

### Requisitos Previos
- **Rust:** `1.80+` (edición 2021) con `cargo`
- **Node.js:** `22+` (LTS) con `npm`
- **Python:** **Cero Python requerido**

### Configuración del Entorno

```bash
# 1. Clonar el repositorio
git clone https://github.com/plynte-labs/LiveAudio.git
cd LiveAudio

# 2. Iniciar la aplicación de escritorio en modo desarrollo (Tauri 2)
cargo run -p liveaudio-desktop

# 3. Iniciar el servicio CLI sin interfaz (headless)
cargo run -p liveaudio-cli -- --help
```

### Ejecución de Pruebas Automatizadas

```bash
# Tests del workspace completo de Rust (core, network, audio, vad, asr)
cargo test --workspace

# Tests del frontend de Tauri (DOM, i18n, guard de cierre)
cd desktop && node test_frontend.js

# Tests del actualizador y manifiesto
node --test tests-js/test_updater_manifest.mjs
```

### Compilación para Producción (Release)

```bash
# 1. Compilar el sidecar CLI en release
cargo build --release -p liveaudio-cli

# 2. Empaquetar la aplicación de escritorio con Tauri 2 (instalador NSIS y bundle de actualización)
cd desktop
npx @tauri-apps/cli@2.12.1 build --config src-tauri/tauri.release.conf.json
```

---

## Estructura del Proyecto

```
LiveAudio/
├── crates/
│   ├── liveaudio-core/       # Ciclo de vida, configuración, estados y job objects
│   ├── liveaudio-audio/      # Captura de audio CPAL (Micrófono y WASAPI loopback)
│   ├── liveaudio-vad/        # Detección Silero VAD v6 (motor ONNX Runtime)
│   ├── liveaudio-asr-worker/ # Worker nativo de Whisper ASR (ONNX Runtime DirectML/CPU)
│   ├── liveaudio-asr-worker-ct2/ # Worker CUDA opcional de Whisper (CTranslate2, compilación independiente)
│   ├── liveaudio-network/    # Servidor WebSocket Tokio y overlay HTTP integrado
│   ├── liveaudio-ipc/        # Protocolo IPC tipado
│   └── liveaudio-cli/        # Servicio CLI headless y binario sidecar para Tauri
├── desktop/
│   ├── src-tauri/            # Backend de Tauri 2 (comandos en Rust, actualizador y ventanas)
│   └── src/                  # Frontend ligero (HTML, CSS, JS, i18n y vista previa en vivo)
├── assets/                   # subtitulos_obs.html (overlay Browser Source)
├── packaging/
│   ├── onnxruntime/          # Binarios DirectML (Windows) y CPU (Linux) de ONNX Runtime
│   └── assemble_latest_manifest.mjs # Ensamblado de latest.json firmado (Node 22)
├── tests-js/                 # Suites de pruebas en Node 22 para frontend y actualizador
└── docs/                     # Especificaciones del protocolo, documentación MCP y guías VM
```

---

## Solución de Problemas Frecuentes

| Síntoma | Causa Probable | Solución |
|---|---|---|
| **No transcribe audio / subtítulos vacíos** | Dispositivo de audio incorrecto | Ve a la pestaña **Inicio** y confirma que tu micrófono o WASAPI Loopback esté activo. Verifica los permisos de audio. |
| **Latencia elevada (>3s) en CPU** | Modelo demasiado pesado para el procesador | Cambia a `tiny` o `base`, o elige el perfil `Rápido`. |
| **El overlay en OBS aparece desconectado** | LiveAudio no está iniciado o puerto incorrecto | Verifica que hayas pulsado **Iniciar Transcripción**. Comprueba el puerto efectivo en la píldora de estado (ej: `8765`). Si cambió, añade `?port=XXXX` a la URL en OBS o usa `http://127.0.0.1:<port>/overlay`. |
| **Fallo de inicialización DirectML / GPU** | Controladores gráficos desactualizados | Actualiza los controladores de tu GPU (NVIDIA, AMD o Intel) o cambia a modo CPU en los ajustes. |
| **Descarga del modelo atascada** | Corte de red o fallo TLS | Haz clic en **Reintentar**. LiveAudio reanudará la descarga sin reiniciar desde cero. |

---

## Licencia y Créditos

Distribuido bajo la **Licencia MIT**. Consulta [LICENSE](LICENSE) para más detalles.

### Tecnologías de Código Abierto Principales
- [Tauri 2](https://tauri.app/) — Framework de escritorio nativo, ligero y seguro.
- [ONNX Runtime](https://onnxruntime.ai/) y [DirectML](https://learn.microsoft.com/en-us/windows/ai/directml/dml) — Aceleración gráfica multiplataforma de alto rendimiento.
- [Silero VAD](https://github.com/snakers4/silero-vad) — Detección de voz rápida y nativa.
- [CPAL](https://github.com/RustAudio/cpal) — Captura de audio de baja latencia multiplataforma.
- [Tokio](https://tokio.rs/) — Runtime asíncrono y servidor WebSocket de alto rendimiento en Rust.
