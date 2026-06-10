# Guía de inicio rápido para nuevos usuarios

Este documento explica paso a paso cómo instalar y poner en marcha **LiveAudio** por primera vez.

---

## 1. Requisitos previos

Antes de empezar, asegúrate de tener:

- **Windows 10/11** o **Linux x86_64**.
- (Opcional pero recomendado) **GPU NVIDIA** con drivers actualizados.
- Conexión a internet para la primera ejecución (descarga de dependencias y modelos).

No necesitas Python instalado: el instalador aprovisiona su propio Python 3.11.

---

## 2. Instalar LiveAudio

### Opción A: Instalador (recomendado para usuarios)

1. Descarga la última versión desde [GitHub Releases](https://github.com/plynte-labs/LiveAudio/releases):
   - **Windows:** `LiveAudio-Setup-X.Y.Z.exe`
   - **Linux:** `LiveAudio-X.Y.Z-linux-x64.tar.gz` (extrae y ejecuta `./liveaudio-launcher`)
2. Ejecútalo. La **primera ejecución** descarga Python y todas las dependencias (~400 MB en CPU, ~2.5 GB con CUDA) y detecta tu GPU automáticamente. Las siguientes ejecuciones arrancan al instante.

### Opción B: Desde el código fuente (desarrolladores)

Requiere [uv](https://docs.astral.sh/uv/):

```bash
git clone https://github.com/plynte-labs/LiveAudio.git
cd LiveAudio

# Solo CPU (la opción más liviana, funciona en cualquier equipo)
uv sync --extra cpu

# O con NVIDIA CUDA
uv sync --extra cu121
```

---

## 3. Verificar instalación

- **Instalador:** ejecuta el launcher con `--self-test` para ver las rutas resueltas y el dispositivo detectado.
- **Desarrolladores:** comprueba que las librerías principales están disponibles:

```bash
uv run python -c "import torch; print('PyTorch:', torch.__version__)"
uv run python -c "import faster_whisper; print('Faster Whisper OK')"
```

---

## 4. Ejecutar LiveAudio

- **Instalador:** abre LiveAudio desde el acceso directo o ejecutando de nuevo el launcher.
- **Desarrolladores:**

```bash
uv run liveaudio
```

Al iniciar por primera vez, se creará automáticamente un archivo `config.json` con valores predeterminados en la carpeta de datos: `%LOCALAPPDATA%\LiveAudio\data` (Windows) o `~/.local/share/liveaudio/data` (Linux) en instalaciones con launcher; `%APPDATA%\LiveAudio` o `~/.config/liveaudio` en ejecuciones de desarrollo. Puedes cambiar la ubicación con la variable de entorno `LIVEAUDIO_HOME`.

### Detección automática de GPU

LiveAudio detecta automáticamente si tenés GPU NVIDIA al iniciar:

- **GPU disponible** → usa `cuda` por defecto.
- **Sin GPU** → cambia automáticamente a `cpu`.

No necesitás configurar nada manualmente. Si querés forzar un dispositivo distinto, podés cambiarlo en la pestaña **Modelo y Hardware**.

---

## 5. Configuración inicial recomendada

### 5.1. Seleccionar carpeta de sesiones

En la pantalla de bienvenida, elige dónde guardar las transcripciones. Por defecto usa una carpeta `sessions/` local.

### 5.2. Elegir dispositivo de audio

En el panel izquierdo:

- **Micrófono:** selecciona un dispositivo con icono 🎤.
- **Audio del sistema (Loopback):** selecciona un dispositivo con icono 🔊 (solo Windows con WASAPI).

### 5.3. Elegir perfil

En la pestaña **Perfiles**, puedes empezar con un preset sin entender cada slider:

| Perfil | Uso recomendado |
|---|---|
| **Rápido** | Menor demora y frases cortas. |
| **Balanceado** | Recomendado para la mayoría. |
| **Calidad** | Más precisión, más consumo de VRAM/latencia. |
| **Streaming estable** | Para jugar o transmitir con la PC ocupada. |

Los cambios no se activan hasta pulsar **Aplicar cambios**. Si modificas un preset, LiveAudio lo aplicará como configuración **Personalizada**.

### 5.4. Elegir hardware y modelo

| Escenario | Hardware | Modelo recomendado |
|---|---|---|
| Gaming pesado (AAA) | CPU | `small` |
| Just Chatting / Gaming ligero | GPU | `turbo` |
| PC de bajos recursos | CPU | `tiny` o `base` |

> **Tip:** Si usas GPU pero no tienes mucha VRAM, prueba con `small` en lugar de `turbo`.

### 5.5. Ajustar latencia

- **Detección de silencio:** tiempo de espera después de que dejas de hablar para cortar el segmento.
  - Valor recomendado: **0.4s - 0.8s**
- **Guillotina (max audio):** duración máxima de un segmento antes de forzar el corte.
  - Valor recomendado: **5s - 15s**

### 5.6. Configurar subtítulos y OBS

En la pestaña **Subtítulos** encontrás:

- **Preview en vivo:** vista previa del estilo seleccionado sin abrir OBS.
- **Estilo visual:** 7 presets disponibles (`default`, `karaoke`, `neon`, `minimal`, `bold`, `rgb`, `typewriter`).
- **Enviar subtítulos a OBS:** activá o desactivá el envío a OBS. Si lo desactivás, las transcripciones se guardan en disco pero no aparecen en OBS.
- **Atraso en OBS:** decide qué se muestra en vivo si Whisper se atrasa.

| Modo | Uso recomendado |
|---|---|
| **Auto** | Recomendado para streaming: evita spam visual de backlog muy viejo. |
| **Solo en vivo** | Prioriza que OBS solo muestre subtítulos frescos. |
| **Enviar todo** | Muestra todo en OBS aunque llegue tarde. |

### 5.7. Blacklist (filtro anti-alucinaciones)

En el cuadro de texto puedes editar las palabras que Whisper suele inventar cuando no hay voz clara.

**Blacklist sugerida por defecto:**

```
amara.org, subtitulos por, suscribete, dale like, gracias por ver,
aplausos, victoria, gracias, memos, flupco, cuanos, kibon,
skip, quita, plechitin, pae
```

Separa cada palabra o frase con comas. Puedes personalizarla a tu gusto.

---

## 6. Integrar con OBS Studio

Consulta la guía completa en [WEBSOCKET_OBS.md](WEBSOCKET_OBS.md).

Resumen rápido:

1. En OBS, añade un fuente **Navegador** (Browser Source).
2. Activa la opción **"Archivo local"** y selecciona `subtitulos_obs.html`.
3. Ajusta el ancho y alto (recomendado: 1920x200).
4. Inicia LiveAudio y los subtítulos aparecerán automáticamente.

---

## 7. Estructura de archivos generados

Cada vez que inicias una sesión, LiveAudio crea una carpeta con timestamp:

```
sessions/
└── session_2026-05-04_143022/
    ├── subtitles.vtt      # Subtítulos en formato WebVTT
    ├── transcript.jsonl   # Transcripción cruda con metadatos
    └── session.json       # Metadatos de la sesión
```

---

## 8. Solución de problemas comunes

### Error: `No module named 'torch'`

- **Instalador:** la primera instalación quedó incompleta. Ejecuta el launcher con `--reinstall`.
- **Desarrolladores:** falta el extra de torch. Ejecuta `uv sync --extra cpu` (o `--extra cu121`) y lanza la app con `uv run liveaudio`.

### Error: `CUDA out of memory`

LiveAudio monitorea la VRAM antes de cada transcripción. Si detecta menos de 500MB libres, libera la caché de CUDA automáticamente y muestra el estado **"GPU saturada - VRAM baja"**. Si aún así falla:

1. Cambiá a un modelo más chico (`tiny` o `base`).
2. Cambiá el dispositivo a `cpu` en la pestaña **Modelo y Hardware**.
3. Cerrá otros programas que usen la GPU (OBS con encoder NVENC, juegos, etc.).

### No aparecen subtítulos en OBS

1. Verifica que LiveAudio esté iniciado (botón verde "DETENER SISTEMA").
2. Comprueba que el puerto WebSocket no esté bloqueado. El default es `8765`; si lo cambiaste en `config.json`, asegurate de que la URL del Browser Source en OBS incluya `?port=XXXX` (ej: `subtitulos_obs.html?port=9876`).
3. Abrí `subtitulos_obs.html` en tu navegador (Chrome/Edge) y revisá la consola (F12) por errores de conexión.

### El audio se corta o hay silencios largos

Ajusta el slider **"Detección de silencio"** a un valor mayor (ej. 1.0s - 1.5s) para que no corte frases muy pausadas.

---

## 9. Diagnóstico local para mantenimiento

LiveAudio incorpora un flujo de diagnóstico **local-first**.

No manda telemetría a servicios externos. La idea es capturar evidencia local cuando algo se atrasa, se congela o no termina limpio.

### Config básica

```json
{
  "diagnostics_enabled": true,
  "diagnostics_level": "minimal",
  "diagnostics_export_dir": null
}
```

### Cuándo usarlo

- si OBS deja de recibir subtítulos
- si el ASR empieza a atrasarse
- si el audio reconecta muchas veces
- si una corrida de tests termina assertions pero no suelta el proceso

### Cómo usarlo

1. Reproduce el problema.
2. En la app, pulsa **Export diagnostics**.
3. Abre el JSON exportado y revisa:
   - estado de procesos
   - tamaños de colas visibles
   - estados `audio` / `asr` / `ws`
4. Si investigás tests, usá los helpers locales del suite para resumir procesos, threads y queues vivos.

### Privacidad

El reporte:

- no exporta audio crudo
- no exporta transcriptos completos como carga de diagnóstico
- sanitiza secrets, passwords, tokens y rutas sensibles

Si necesitás más detalle puntual, subí `diagnostics_level` a `deep` temporalmente.

---

## 10. Consejos de rendimiento

- **Cierra programas innecesarios** mientras streameas para liberar CPU/GPU.
- **Usa SSD** para la carpeta de sesiones; escribir VTT/JSONL en disco lento puede causar micro-lag.
- **Mantén los drivers de NVIDIA actualizados** si usas CUDA.
- Si experimentas **stuttering** en juegos pesados, prueba el modelo `small` en CPU con la mitad de los hilos disponibles.
