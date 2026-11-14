# Guía de inicio rápido para nuevos usuarios

Este documento explica paso a paso cómo instalar y poner en marcha **LiveAudio** por primera vez.

---

## 1. Requisitos previos

Antes de empezar, asegúrate de tener:

- **Windows 10/11** o **Linux x86_64**.
- (Opcional pero recomendado) **GPU NVIDIA** con drivers actualizados.
- Conexión a internet para la primera ejecución (descarga del modelo).

No necesitas Python ni CUDA: LiveAudio es 100% Rust nativo.

---

## 2. Instalar LiveAudio

### Opción A: Instalador (recomendado para usuarios)

1. Descarga la última versión desde [GitHub Releases](https://github.com/plynte-labs/LiveAudio/releases):
   - **Windows:** `LiveAudio-Setup-X.Y.Z.exe`
   - **Linux:** la app de escritorio como AppImage (`*.AppImage`) o paquete Debian (`*.deb`). El archivo `liveaudio-cli-X.Y.Z-linux-x64.tar.gz` contiene solo el CLI y el worker ASR, no la app. En Linux las actualizaciones son manuales; el actualizador integrado es solo para Windows.
2. Ejecútalo. Al iniciar ASR por primera vez, LiveAudio descarga el modelo seleccionado y muestra el progreso en la interfaz. Puedes cancelarlo y reintentarlo después. Si el runtime necesita repararse, usa **Reparar runtime ASR** en Ajustes.

El runtime nativo de ONNX y el modelo Silero VAD están incluidos en los recursos de la aplicación para Windows y Linux; no se descargan durante el inicio. Si esos archivos faltan o no se pueden cargar, repara o reinstala LiveAudio.

### Opción B: Desde el código fuente (desarrolladores)

Requiere la toolchain estable de Rust y Node.js:

```bash
git clone https://github.com/plynte-labs/LiveAudio.git
cd LiveAudio
cargo build --workspace
```

---

## Preparación del modelo ASR

El registro muestra la consulta de caché, la descarga y la carga en GPU o CPU. Cuando la descarga informa bytes, se muestran B, KiB o MiB disponibles (pueden incluir bytes recuperados de caché), no un porcentaje global ni una estimación de tiempo. Si el contador no está disponible, la descarga se indica sin porcentaje. Los avisos de espera aparecen cada 15 segundos con el tiempo transcurrido de la etapa; no garantizan que la red avance. Un modelo completo en caché evita consultar la red; un fallo de CUDA permite reintentar la carga en CPU sin volver a descargar.

**Espera a que ASR indique que está listo antes de hablar:** durante la preparación puede descartarse audio en cola. La captura y la política de subtítulos OBS no cambian.

## 3. Verificar instalación

Ejecuta `liveaudio-cli doctor` para ver el worker ASR nativo, ONNX Runtime y las rutas de datos detectadas.

---

## 4. Ejecutar LiveAudio

- **Instalador:** abre LiveAudio desde el acceso directo.
- **Desarrolladores:** `cd desktop && npm run dev`.

Al iniciar por primera vez, LiveAudio crea un `config.json` con valores predeterminados en la carpeta de datos: `%APPDATA%\LiveAudio` en Windows, o `~/.config/liveaudio` (o `$XDG_CONFIG_HOME/liveaudio`) en Linux. Puedes cambiar la ubicación con la variable de entorno `LIVEAUDIO_HOME`. Los modelos Whisper se guardan en `%LOCALAPPDATA%\LiveAudio\models\onnx` en Windows.

### Dispositivo de cómputo

Las configuraciones nuevas usan CPU de forma segura. LiveAudio conserva el dispositivo guardado en configuraciones existentes, pero no selecciona CUDA automáticamente ni confirma su disponibilidad mediante el nombre del adaptador. Si tu sistema tiene una instalación CUDA compatible, puedes elegirla manualmente en **Avanzado**; si no, selecciona CPU.

---

## 5. Configuración inicial recomendada

### Cliente de escritorio Tauri

En la interfaz de escritorio, empieza en **Inicio** y elige la fuente de audio, el idioma hablado y un perfil. **Subtítulos** reúne el estilo y la presentación; **Avanzado** contiene modelo, hardware, VAD, entrega atrasada y filtros de vocabulario; **Archivos** muestra la carpeta de sesión y las opciones de continuidad y exportación JSONL/WebVTT. La ruta muestra la carpeta de la sesión actual o la carpeta de salida configurada si todavía no hay una sesión activa; puedes abrirla desde esa sección.

Al cambiar durante una sesión un ajuste que requiere reinicio, LiveAudio pide confirmación antes de guardar y reiniciar el servicio. Los controles JSONL y WebVTT son configurables por separado y conservan sus valores predeterminados actuales y los ajustes guardados.

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
  - Valor recomendado: **0.4s - 0.8s** (por defecto 0.5s)
- **Guillotina (max audio):** duración máxima de un segmento antes de forzar el corte.
  - Valor recomendado: **3s - 15s** (por defecto 3s)
- **Subtítulos parciales en vivo** (Avanzado, requiere reiniciar el servicio): mientras hablas, muestra el texto provisional de la frase cada ~0,5 s en la vista previa y en OBS, y la versión final lo reemplaza. Reduce la espera percibida, pero cada parcial es una transcripción extra: se recomienda con una GPU rápida (CUDA). En DirectML o CPU aumenta la carga y puede retrasar los finales. Los parciales no se guardan en archivos. Está desactivado por defecto y solo funciona en la app de escritorio, no en `liveaudio-cli`.

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

### Objetivo de salida y ventana de frase

En **Audio/VAD**, selecciona el objetivo independientemente del perfil de hardware:

- **Subtítulos:** ventana predeterminada de 5 segundos, ajustable de 1 a 15 segundos para obtener frases finales con mayor rapidez.
- **Transcripción:** ventana predeterminada de 30 segundos, ajustable de 1 a 60 segundos para favorecer la continuidad de la transcripción.
- **Combinado:** usa la ventana larga y prioriza la continuidad de la transcripción. Durante el habla continua, los subtítulos finales pueden retrasarse decenas de segundos.

El silencio todavía puede cerrar una frase antes de alcanzar su límite. Cambiar un perfil de hardware no modifica el objetivo ni la ventana elegidos.

El **límite de decodificación ASR** es independiente de la ventana de frase: inicia cuando Whisper empieza a decodificar una frase ya formada, no durante la captura, la espera en cola ni la carga o preparación del modelo. Su valor predeterminado es 15 segundos y puede ajustarse de 5 a 120 segundos. Si una decodificación supera ese límite, LiveAudio detiene la captura, avisa que el audio o las transcripciones pendientes pueden perderse y no vuelve a procesar automáticamente esa frase.

---

## 5.5. Primera descarga del modelo: estados, códigos y tiempos

En un equipo limpio, el pill ASR muestra el estado real de la descarga de Whisper
(en vez de un "cargando" genérico):

| Estado | Significado | Qué hacer |
|---|---|---|
| `ASR: descargando N%` | Descarga con progreso real (0–100, monotónico por intento) | Esperar; el % nunca retrocede salvo al Reintentar |
| `ASR: descargando…` | Descargando pero sin % parseable (indeterminado) | Esperar; nunca es un 0% congelado |
| `ASR: cargando` / `ASR: transcribiendo` | Cargando el modelo / calentando | Esperar |
| `ASR: tiempo de decodificación excedido` | El decodificador superó 15 s durante una frase | La captura se detiene; el audio/transcriptos pendientes pueden perderse y no se reprocesan automáticamente |
| `ASR: detenido. Pulsa Reintentar.` | 120–180 s sin ningún evento/progreso (stalled) | Pulsar **Reintentar** (intento nuevo, % a 0 una vez, luego monotónico) |
| `ASR: listo` | Modelo cargado | Stremear |
| Error + código | Fallo de aprovisionamiento (ver tabla) | Seguir el hint de una línea y Reintentar |

Códigos `provision-*` (remediation de una línea, sin tracebacks en la UI):

| Código | Qué hacer |
|---|---|
| `model-not-found` (solo ante ausencia real) | Revisa el nombre del modelo y reintenta |
| `provision-cache-corrupt` | Caché dañada — reintenta para redescargar |
| `provision-network` | Sin conexión — revisa tu red y reintenta |
| `provision-auth` | Acceso denegado — revisa credenciales y reintenta |
| `provision-disk-full` | Disco lleno — libera espacio y reintenta |
| `provision-timeout-stalled` | La descarga tardó demasiado — reintenta |
| `provision-tls` | Fallo de conexión segura — reintenta |
| `provision-unknown` | Error inesperado al preparar el modelo |

Tiempos esperados por modelo (orientativo; el tiempo es proporcional a tu conexión —
guía a ~50 Mbps): `tiny` ~150 MB ≈ 30 s · `base` ~300 MB ≈ 1 min ·
`small` ~480 MB ≈ 1.5 min · `turbo` ~1.5 GB ≈ 4–5 min · Silero VAD ~2 MB ≈
instantáneo. Después, la app funciona totalmente offline.

> El switch **Precalentar modelo al iniciar** (pestaña Modelo) controla la descarga en
> primer uso (usa red). Viene activo por defecto; es la misma opción que
> `--prewarm` / `--lazy` del modo servicio.

---

## 6. Integrar con OBS Studio

Consulta la guía completa en [WEBSOCKET_OBS.md](WEBSOCKET_OBS.md).

Resumen rápido:

1. En OBS, añade un fuente **Navegador** (Browser Source).
2. Activa la opción **"Archivo local"** y selecciona `subtitulos_obs.html`.
3. Ajusta el ancho y alto (recomendado: 1920x200).
4. Inicia LiveAudio y los subtítulos aparecerán automáticamente.

### 6.1. Modo servicio headless (integradores, opcional)

Para embeber LiveAudio como backend sin ventana (p. ej. desde opencohost):

```bash
liveaudio-service --parent-pid <PID> [--health-file <ruta>]
```

> Vía soportada: en Windows instalado, `liveaudio-service` (script de consola) es la ÚNICA vía headless soportada — el ejecutable GUI instalado no tiene stdout. `liveaudio --service` solo funciona desde un checkout en terminal (desarrollo).

El servicio vive hasta que el proceso dueño termina (watchdog por PID local a la máquina, sin puerto de control nuevo), obedece la configuración guardada desde la interfaz en modo solo-lectura (cambiar ajustes exige reiniciar el servicio), carga Whisper de forma perezosa al primer cliente WebSocket, y anuncia el puerto efectivo (`base..base+9`) vía `hello.port` y eventos JSON en stdout. `send_all` reemite como máximo los últimos 256 subtítulos tras un freeze (borde en vivo, no historial completo). Detalles del protocolo en [WEBSOCKET_OBS.md](WEBSOCKET_OBS.md). El auto-discovery del lado lector vive en la unidad VoiceAI (`feature/liveaudio-service-client`), DESPUÉS de esta unidad LiveAudio: este modo expone el contrato, no lo consume.

---

## 7. Estructura de archivos generados

Cada vez que inicias una sesión, LiveAudio crea una carpeta con timestamp:

```
sessions/
└── session_2026-05-04_143022/
    ├── subtitles.vtt      # Subtítulos en formato WebVTT estándar
    ├── subtitles.srt      # Los mismos subtítulos en SubRip (p. ej. para DaVinci Resolve)
    ├── transcript.jsonl   # Transcripción cruda con metadatos
    └── session.json       # Metadatos de la sesión
```

`transcript.jsonl`, `subtitles.vtt` y `subtitles.srt` conservan completo el texto transcrito después de la sanitización. Las marcas de tiempo VTT usan el momento de captura relativo al inicio de la sesión, por lo que reflejan también las pausas entre frases; las capturas históricas sin marcas de tiempo no reciben una hora inventada. La presentación de subtítulos en OBS/WebSocket mantiene el límite actual de 600 caracteres. El archivo `.srt` se escribe siempre que el guardado de VTT esté activo. Los controles de guardado de JSONL y VTT siguen siendo independientes.

Cada entrada nueva de JSONL incluye la duración derivada del audio PCM y una instantánea limitada a ajustes de captura y tiempo máximo de decodificación; no incluye prompts ni identificadores de dispositivos.

Si la cola o el almacenamiento de una salida de sesión falla, LiveAudio detiene la captura y muestra una alerta; el contenido pendiente puede no haberse guardado.

---

## 8. Solución de problemas comunes

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
