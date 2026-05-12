# Integración WebSocket y OBS Studio

LiveAudio expone un **servidor WebSocket local** que envía los subtítulos transcritos en tiempo real. Cualquier cliente que se conecte a ese puerto recibirá los mensajes automáticamente.

---

## 1. Arquitectura de comunicación

```
+-------------+        audio_queue         +----------------+
|  Productor  |---------------------------->|     Motor      |
|   (audio)   |                             |    Whisper     |
+-------------+                             +--------+-------+
                                                     |
                                                     | text_queue
                                                     v
+-------------+        ws://127.0.0.1:8765   +----------------+
|  Cliente    |<-----------------------------|  Servidor WS   |
|   (OBS)     |        JSON subtitle payload  |  (network.py)  |
+-------------+                              +----------------+
```

1. El **motor ASR** (`engine.py`) transcribe el audio y empuja el resultado a `text_queue`.
2. El **servidor WebSocket** (`network.py`) lee `text_queue` y hace **broadcast** a todos los clientes conectados.
3. El **HTML de OBS** (`subtitulos_obs.html`) se conecta vía WebSocket y actualiza el DOM con animaciones CSS.

---

## 2. Detalles del servidor WebSocket

| Parámetro | Valor |
|---|---|
| **Protocolo** | WebSocket (`ws://`) |
| **Host** | `127.0.0.1` (localhost únicamente) |
| **Puerto** | `8765` (configurable vía `ws_port` en `config.json`) |
| **Endpoint** | `/` (raíz) |
| **Formato de mensaje** | JSON |

### Payload enviado

Cuando Whisper produce una transcripción válida y la política de backlog permite mostrarla en OBS, se envía un objeto JSON:

```json
{
  "id": "1760000000000-12",
  "text": "Hola a todos, bienvenidos al stream",
  "style": "default",
  "created_at": 1760000000.0,
  "processed_at": 1760000001.3,
  "queue_delay": 0.2,
  "total_delay": 1.3,
  "latency": 1.1,
  "is_replay": false,
  "catchup_interval_sec": 0.0
}
```

| Campo | Tipo | Descripción |
|---|---|---|
| `id` | `string` | Identificador estable de la frase dentro de la sesión. |
| `text` | `string` | Texto transcrito limpio (después del filtrado por blacklist). |
| `style` | `string` | Estilo visual seleccionado: `default`, `karaoke`, `neon`, `minimal`, `bold`, `rgb` o `typewriter`. |
| `created_at` | `number` | Timestamp de creación del segmento de audio. |
| `processed_at` | `number` | Timestamp al terminar la transcripción. |
| `queue_delay` | `number` | Tiempo que el audio esperó antes de entrar al ASR. |
| `total_delay` | `number` | Atraso total desde audio hasta subtítulo listo. |
| `latency` | `number` | Tiempo usado por Whisper para transcribir. |
| `is_replay` | `boolean` | Indica si el subtítulo es backlog/catch-up. |
| `catchup_interval_sec` | `number` | Pacing recomendado para backlog en modo automático. |

El HTML de OBS solo necesita `text` y `style`; los demás campos son metadata para diagnóstico y control de backlog.

---

## 3. Política de backlog para OBS

LiveAudio separa la persistencia completa de la salida visual en OBS. Toda transcripción válida se guarda en disco, pero no todo backlog tiene que mostrarse en vivo.

| Modo UI | Valor config | Comportamiento |
|---|---|---|
| Auto | `auto` | Envía subtítulos frescos, emite backlog corto con pacing y omite de OBS lo que supere el atraso máximo. |
| Solo en vivo | `live_only` | Guarda todo, pero solo muestra subtítulos dentro de `subtitle_max_live_delay_sec`. |
| Enviar todo | `send_all` | Manda todo a OBS aunque llegue tarde. |

Opciones de `config.json`:

```json
{
  "subtitle_backlog_policy": "auto",
  "subtitle_max_live_delay_sec": 10.0,
  "subtitle_catchup_interval_sec": 1.5
}
```

---

## 4. Seguridad

- **Solo conexiones locales:** el servidor rechaza cualquier conexión que no provenga de `127.0.0.1`, `::1` o `localhost`.
- **Sin autenticación:** al estar limitado a localhost, no se requiere token ni login.
- **Broadcast:** si múltiples clientes se conectan (OBS + navegador de pruebas), todos reciben los mismos mensajes simultáneamente.

---

## 5. Configurar OBS Studio

### Paso 1: Añadir fuente Navegador

1. En OBS, haz clic en el botón **+** en el panel de **Fuentes**.
2. Selecciona **Navegador** (Browser Source).
3. Asigna un nombre, por ejemplo: `Subtitulos LiveAudio`.

### Paso 2: Configurar propiedades

| Propiedad | Valor recomendado |
|---|---|
| **URL / Archivo local** | Activa **"Archivo local"** y selecciona `subtitulos_obs.html` de la carpeta del proyecto. |
| **Ancho** | `1920` |
| **Alto** | `200` |
| **FPS** | `30` o `60` |
| **Apagar fuente cuando no esté visible** | Desactivado (recomendado para mantener la conexión WS). |
| **Actualizar navegador cuando la escena se active** | Desactivado (evita reconexiones innecesarias). |

### Paso 3: Posicionar en la escena

- Coloca la fuente en la parte **inferior central** de la pantalla.
- Ajusta el tamaño para que ocupe el ancho completo o el que prefieras.

### Paso 4: Iniciar LiveAudio

1. Abre `main.py` y pulsa **INICIAR SISTEMA**.
2. Vuelve a OBS: los subtítulos aparecerán automáticamente cuando hables.

---

## 6. Estilos visuales disponibles

Puedes cambiar el estilo desde la pestaña **"Subtítulos"** en LiveAudio.

### `default`
- Texto blanco con fondo negro semitransparente.
- Sombra de texto para legibilidad sobre cualquier fondo.
- Animación de entrada/salida suave.

### `karaoke`
- Cada palabra aparece escalonada con animación `popIn`.
- Ideal para resaltar frases épicas o musicales.
- Color amarillo brillante con contorno negro.

### `neon`
- Estilo retro-futurista con borde cian brillante.
- Texto en mayúsculas y espaciado amplio.
- Fondo oscuro con glow (`box-shadow`).

### `minimal`
- Limpio, sin fondo, fade sutil.
- Ideal para streams con diseño propio.

### `bold`
- Alto contraste, texto grueso, animación rápida.
- Presencia fuerte en pantalla.

### `rgb`
- Cada palabra recibe un color diferente del arcoíris.
- Efecto visual dinámico y llamativo.

### `typewriter`
- Las palabras aparecen una por una con efecto de máquina de escribir.
- Cursor parpadeante, fuente monoespaciada.

---

## 7. Modo solo transcript (sin OBS)

Si solo necesitás guardar transcripciones sin enviar subtítulos a OBS:

1. Abrí la pestaña **Subtítulos**.
2. Desactivá el switch **"Enviar subtítulos a OBS"**.
3. Las transcripciones se guardan en disco normalmente (JSONL + VTT).

---

## 8. Personalizar el HTML (avanzado)

Si quieres modificar colores, fuentes o animaciones, edita directamente `subtitulos_obs.html`.

### Cambiar fuente

Busca la regla `body` en el `<style>`:

```css
body {
    font-family: 'Arial', sans-serif; /* Cambia aquí */
}
```

### Cambiar duración en pantalla

Busca la función `showSubtitle` y modifica el valor de `5000` (milisegundos):

```javascript
hideTimeout = setTimeout(() => {
    // ... animación de salida
}, 5000); // <-- 5 segundos
```

### Cambiar puerto del WebSocket

El puerto se configura desde `config.json` — no hace falta editar código:

1. Abre `config.json` y cambia `ws_port`:
   ```json
   {
     "ws_port": 9876
   }
   ```
2. Reinicia LiveAudio. El servidor se iniciará en el nuevo puerto.
3. Actualiza la URL de tu Browser Source en OBS agregando el parámetro `?port=`:
   ```
   file:///ruta/a/subtitulos_obs.html?port=9876
   ```
   Si no especificas `?port=`, el HTML usa el default `8765`.

> **Tip:** La app muestra el puerto actual en el log al iniciar: `WS: localhost:9876`.

---

## 9. Depuración

### Verificar que el servidor está activo

Abre el navegador y presiona **F12** → pestaña **Consola**. Si ves:

```
Conectado al motor ASR
```

significa que la conexión WebSocket fue exitosa.

### Mensajes de error comunes

| Mensaje en consola | Causa | Solución |
|---|---|---|
| `Desconectado. Reintentando...` | LiveAudio no está iniciado, el puerto está ocupado, o el `?port=` en la URL de OBS no coincide. | Inicia LiveAudio, verifica el puerto en el log (`WS: localhost:XXXX`), y asegúrate de que la URL de OBS incluya `?port=XXXX` si usás un puerto custom. |
| `Error parseando WebSocket` | Se recibió un mensaje que no es JSON válido. | Revisa `core/network.py`; probablemente se envió algo que no es un dict. |
| No aparece nada en consola | El HTML no está conectado al WS correcto. | Verifica la URL del WebSocket en el script. |

### Probar conectividad manualmente

Si quieres probar el WebSocket sin OBS, puedes usar un cliente simple en Python:

```python
import asyncio
import websockets

async def test():
    uri = "ws://127.0.0.1:8765"
    async with websockets.connect(uri) as ws:
        async for msg in ws:
            print(msg)

asyncio.run(test())
```

Ejecútalo mientras LiveAudio está activo y deberías ver los JSON con las transcripciones.

---

## 10. Notas técnicas

- **Broadcast sin backpressure:** `websockets>=16` usa `broadcast()` que envía a todos los clientes de forma optimizada, sin bloquear si un cliente es lento.
- **Reconexión automática:** el HTML intenta reconectar cada 3 segundos si se pierde la conexión.
- **Limpieza de DOM:** después de cada animación de salida, el nodo se elimina para evitar fugas de memoria en sesiones largas.
