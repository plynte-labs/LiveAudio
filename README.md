# Plynte LiveAudio

LiveAudio es un motor de reconocimiento de voz automático (ASR) en tiempo real, diseñado para streamers y creadores de contenido. Captura audio del micrófono o del sistema, lo transcribe localmente usando **Whisper** (OpenAI) y envía los subtítulos a **OBS Studio** vía **WebSocket**.

**Todo el procesamiento es 100% local.** No se envía nada a la nube.

---

## Características principales

- **Transcripción en tiempo real** con Whisper (modelos `tiny`, `base`, `small`, `turbo`).
- **Detección de voz (VAD)** con Silero VAD para cortar silencios automáticamente.
- **Captura flexible:** micrófono físico o audio del sistema (WASAPI Loopback en Windows).
- **WebSocket integrado** para enviar subtítulos a OBS o cualquier cliente HTML.
- **Control de backlog para OBS:** evita ráfagas de subtítulos viejos tras freezes, sin perder la transcripción guardada.
- **Filtrado de alucinaciones** mediante blacklist personalizable.
- **Gestión de sesiones:** guarda transcripciones en `.jsonl` y subtítulos en `.vtt`.
- **Hot-swap inteligente:** cambia de dispositivo o modelo sin reiniciar el programa.
- **Arquitectura robusta:** procesos aislados (multiprocessing), ring buffer de audio y reconexión automática ante desconexiones de hardware.

---

## Requisitos del sistema

| Componente | Recomendado |
|---|---|
| **SO** | Windows 10/11 (WASAPI Loopback), Linux o macOS |
| **Python** | 3.10 o superior |
| **GPU** | NVIDIA con CUDA (opcional pero recomendado para modelos grandes) |
| **RAM** | 8 GB mínimo, 16 GB recomendado |
| **Micrófono** | Cualquier dispositivo de entrada de audio |

---

## Instalación rápida

1. **Clona el repositorio:**
   ```bash
   git clone <url-del-repo>
   cd LiveAudio
   ```

2. **Crea y activa un entorno virtual (recomendado):**
   ```bash
   # Con conda (recomendado para PyTorch/CUDA)
   conda create -n liveaudio python=3.11
   conda activate liveaudio

   # O con venv
   python -m venv venv
   venv\Scripts\activate  # Windows
   ```

3. **Instala las dependencias:**
   ```bash
   pip install -r requirements.txt
   ```

   > **Nota:** Si usas GPU NVIDIA, asegúrate de tener los [drivers CUDA](https://developer.nvidia.com/cuda-downloads) instalados. PyTorch se instalará con soporte CUDA automáticamente si tu sistema lo detecta.

4. **Ejecuta la aplicación:**
   ```bash
   python main.py
   ```

   Si usas un entorno conda específico (como `flux_env`):
   ```bash
   E:\Miniconda\envs\flux_env\python.exe main.py
   ```

---

## Estructura del proyecto

```
LiveAudio/
├── core/
│   ├── audio.py          # Captura de audio, VAD y reconexión automática
│   ├── engine.py         # Motor ASR (Whisper) y guardado de sesiones
│   └── network.py        # Servidor WebSocket (broadcast)
├── utils/
│   └── config.py         # Carga/guardado de configuración persistente
├── docs/
│   ├── GETTING_STARTED.md    # Guía detallada para nuevos usuarios
│   └── WEBSOCKET_OBS.md      # Integración con OBS Studio
├── main.py               # Interfaz gráfica (CustomTkinter) y orquestador
├── config.json           # Configuración local del usuario (ignorado por git)
├── requirements.txt      # Dependencias de Python
├── subtitulos_obs.html   # Browser Source para OBS
└── sessions/             # Transcripciones generadas (ignorado por git)
```

---

## Dependencias principales

| Librería | Versión | Propósito |
|---|---|---|
| `faster-whisper` | >=1.0.0 | Transcripción con Whisper optimizada |
| `torch` | >=2.0.0 | Backend de inferencia (CPU/CUDA) |
| `sounddevice` | >=0.4.6 | Captura de audio en tiempo real |
| `numpy` | >=1.24.0 | Manipulación de buffers de audio |
| `customtkinter` | >=5.2.0 | Interfaz gráfica moderna |
| `websockets` | >=12.0 | Servidor WebSocket para OBS |

---

## Configuración por defecto

Al iniciar por primera vez, se crea un archivo `config.json` con los siguientes valores:

```json
{
    "output_dir": "<ruta_absoluta>/sessions",
    "device": "cuda",
    "cpu_threads": 8,
    "model_size": "small (Balance CPU)",
    "blacklist": "amara.org, subtitulos por, suscribete, dale like, gracias por ver, aplausos, victoria, gracias, memos, flupco, cuanos, kibon, skip, quita, plechitin, pae",
    "continuous_session": true,
    "subtitle_style": "default",
    "subtitle_backlog_policy": "auto",
    "subtitle_max_live_delay_sec": 10.0,
    "subtitle_catchup_interval_sec": 1.5,
    "silence_timeout": 0.8,
    "max_chunk_duration": 5.0,
    "audio_device": null,
    "selected_profile_id": "balanced",
    "profile_mode": "preset"
}
```

Puedes modificar estos valores desde la interfaz gráfica o editando directamente `config.json`.

---

## Uso básico

1. Ejecuta `main.py`.
2. En la pantalla de bienvenida, elige la carpeta donde se guardarán las sesiones.
3. En el panel de ajustes:
   - Elige un **perfil** (`Rápido`, `Balanceado`, `Calidad` o `Streaming estable`) si quieres empezar sin tocar cada slider.
   - Selecciona tu **dispositivo de audio** (micrófono o loopback del sistema).
   - Elige **CPU** o **CUDA** según tu hardware.
   - Selecciona el **tamaño del modelo** (`tiny`, `base`, `small`, `turbo`).
   - Ajusta los **sliders de latencia** y **Atraso en OBS** si necesitas tuning avanzado.
   - Pulsa **Aplicar cambios** para activar y guardar los ajustes; mover sliders ya no reinicia el motor por sí solo.
4. Pulsa **INICIAR SISTEMA**.
5. Abre `subtitulos_obs.html` como **Browser Source** en OBS (ver [docs/WEBSOCKET_OBS.md](docs/WEBSOCKET_OBS.md)).

---

## Perfiles de configuración

Los perfiles son presets integrados para evitar configurar manualmente cada control sensible.

| Perfil | Uso recomendado |
|---|---|
| `Rápido` | Menor demora y frases cortas; baja un poco la precisión. |
| `Balanceado` | Recomendado para la mayoría de sesiones. |
| `Calidad` | Más precisión; puede usar más VRAM y tardar más. |
| `Streaming estable` | Reduce carga de GPU para jugar o transmitir en una PC ocupada. |

Si modificas un perfil integrado, LiveAudio lo tratará como `Personalizado`. Los cambios quedan pendientes hasta pulsar **Aplicar cambios**. Si el motor está activo y el ajuste requiere hot-swap, puede haber un corte breve y la frase actual podría cortarse.

---

## Blacklist predeterminada

La blacklist evita que aparezcan en pantalla palabras o frases comunes que Whisper suele "alucinar" cuando no hay voz clara:

```
amara.org, subtitulos por, suscribete, dale like, gracias por ver,
aplausos, victoria, gracias, memos, flupco, cuanos, kibon,
skip, quita, plechitin, pae
```

Puedes editarla desde la interfaz. Separa las palabras o frases con comas.

---

## Política de atraso en OBS

LiveAudio siempre guarda las transcripciones válidas en la sesión (`transcript.jsonl` y `subtitles.vtt`). La opción **Atraso en OBS** solo controla qué se muestra en vivo en OBS cuando el ASR se atrasó por GPU/CPU ocupada, VRAM llena o un freeze temporal.

| Modo | Comportamiento |
|---|---|
| `Auto` | Envía subtítulos frescos. Si hay backlog corto, lo emite con pacing. Si supera `subtitle_max_live_delay_sec`, lo guarda pero no lo muestra en OBS. |
| `Solo en vivo` | Guarda todo, pero solo muestra en OBS subtítulos dentro del atraso máximo configurado. |
| `Enviar todo` | Envía todo a OBS aunque llegue tarde. Útil si prefieres fidelidad visual completa sobre evitar ráfagas. |

Opciones relacionadas:

| Config | Descripción |
|---|---|
| `subtitle_backlog_policy` | `auto`, `live_only` o `send_all`. |
| `subtitle_max_live_delay_sec` | Atraso máximo para considerar un subtítulo apto para OBS live. |
| `subtitle_catchup_interval_sec` | Separación entre subtítulos de catch-up en modo `auto`. |

---

## Solución de problemas

| Síntoma | Posible causa | Solución |
|---|---|---|
| No transcribe nada | Dispositivo de audio incorrecto | Verifica en la UI que tienes seleccionado el micrófono o loopback correcto. |
| Latencia muy alta | Modelo muy grande en CPU | Cambia a `tiny` o `base`, o usa GPU. |
| OBS no muestra subtítulos | WebSocket no conectado | Verifica que LiveAudio esté iniciado y que el HTML apunte a `ws://127.0.0.1:8765`. |
| Error de CUDA | Drivers desactualizados | Actualiza los drivers de NVIDIA o cambia a CPU en los ajustes. |
| Procesos zombies al cerrar | Cierre abrupto | Usa siempre el botón "DETENER SISTEMA" antes de cerrar la ventana. |

---

## Licencia

Proyecto privado. Todos los derechos reservados.

---

## Créditos

- [OpenAI Whisper](https://github.com/openai/whisper)
- [Faster Whisper](https://github.com/SYSTRAN/faster-whisper)
- [Silero VAD](https://github.com/snakers4/silero-vad)
- [CustomTkinter](https://github.com/TomSchimansky/CustomTkinter)
