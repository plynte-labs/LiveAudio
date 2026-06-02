# Guía de inicio rápido para nuevos usuarios

Este documento explica paso a paso cómo preparar tu entorno, instalar dependencias y poner en marcha **LiveAudio** por primera vez.

---

## 1. Requisitos previos

Antes de empezar, asegúrate de tener:

- **Windows 10/11**, **Linux** o **macOS**.
- **Python 3.10 o superior** instalado.
  - Para verificar: `python --version`
- (Opcional pero recomendado) **GPU NVIDIA** con drivers actualizados.

---

## 2. Crear el entorno virtual

Es **altamente recomendable** usar un entorno aislado para evitar conflictos entre librerías.

### Opción A: Conda (recomendado para PyTorch)

```bash
# Crear entorno
conda create -n liveaudio python=3.11

# Activar entorno
conda activate liveaudio

# Instalar dependencias
pip install -r requirements.txt
```

### Opción B: venv (entorno nativo de Python)

```bash
# Windows
python -m venv venv
venv\Scripts\activate

# Linux / macOS
python3 -m venv venv
source venv/bin/activate

# Instalar dependencias
pip install -r requirements.txt
```

---

## 3. Verificar instalación

Después de instalar, comprueba que las librerías principales están disponibles:

```bash
python -c "import torch; print('PyTorch:', torch.__version__)"
python -c "import faster_whisper; print('Faster Whisper OK')"
python -c "import sounddevice; print('Sounddevice OK')"
python -c "import websockets; print('Websockets OK')"
python -c "import customtkinter; print('CustomTkinter OK')"
```

Si alguna falla, reinstálala individualmente:

```bash
pip install <nombre-de-la-libreria>
```

---

## 4. Ejecutar LiveAudio

Con el entorno activado:

```bash
python main.py
```


Al iniciar por primera vez, se creará automáticamente un archivo `config.json` con valores predeterminados.

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

Tu entorno virtual no está activado. Asegúrate de hacer `conda activate liveaudio` o `venv\Scripts\activate` antes de ejecutar.

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

## 9. Consejos de rendimiento

- **Cierra programas innecesarios** mientras streameas para liberar CPU/GPU.
- **Usa SSD** para la carpeta de sesiones; escribir VTT/JSONL en disco lento puede causar micro-lag.
- **Mantén los drivers de NVIDIA actualizados** si usas CUDA.
- Si experimentas **stuttering** en juegos pesados, prueba el modelo `small` en CPU con la mitad de los hilos disponibles.
