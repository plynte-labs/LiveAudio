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
|   (OBS)     |        JSON {text, style}    |  (network.py)  |
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
| **Puerto** | `8765` |
| **Endpoint** | `/` (raíz) |
| **Formato de mensaje** | JSON |

### Payload enviado

Cada vez que Whisper produce una transcripción válida, se envía un objeto JSON:

```json
{
  "text": "Hola a todos, bienvenidos al stream",
  "style": "default"
}
```

| Campo | Tipo | Descripción |
|---|---|---|
| `text` | `string` | Texto transcrito limpio (después del filtrado por blacklist). |
| `style` | `string` | Estilo visual seleccionado: `default`, `karaoke` o `neon`. |

---

## 3. Seguridad

- **Solo conexiones locales:** el servidor rechaza cualquier conexión que no provenga de `127.0.0.1`, `::1` o `localhost`.
- **Sin autenticación:** al estar limitado a localhost, no se requiere token ni login.
- **Broadcast:** si múltiples clientes se conectan (OBS + navegador de pruebas), todos reciben los mismos mensajes simultáneamente.

---

## 4. Configurar OBS Studio

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

## 5. Estilos visuales disponibles

Puedes cambiar el estilo desde la interfaz de LiveAudio (selector **"Estilo Visual en OBS"**).

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

---

## 6. Personalizar el HTML (avanzado)

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

Si necesitas usar otro puerto (por ejemplo, si `8765` está ocupado):

1. En `core/network.py`, cambia el puerto:
   ```python
   async with serve(_handle_client, "127.0.0.1", 9876) as server:
   ```
2. En `subtitulos_obs.html`, actualiza la URL:
   ```javascript
   ws = new WebSocket('ws://127.0.0.1:9876');
   ```

---

## 7. Depuración

### Verificar que el servidor está activo

Abre el navegador y presiona **F12** → pestaña **Consola**. Si ves:

```
Conectado al motor ASR
```

significa que la conexión WebSocket fue exitosa.

### Mensajes de error comunes

| Mensaje en consola | Causa | Solución |
|---|---|---|
| `Desconectado. Reintentando...` | LiveAudio no está iniciado o el puerto está ocupado. | Inicia LiveAudio y verifica que no haya otra instancia corriendo. |
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

## 8. Notas técnicas

- **Broadcast sin backpressure:** `websockets>=16` usa `broadcast()` que envía a todos los clientes de forma optimizada, sin bloquear si un cliente es lento.
- **Reconexión automática:** el HTML intenta reconectar cada 3 segundos si se pierde la conexión.
- **Limpieza de DOM:** después de cada animación de salida, el nodo se elimina para evitar fugas de memoria en sesiones largas.
