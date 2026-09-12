# Spec — WebSocket IPC Auth Token (Anti-Eavesdropping & Anti-Impersonation)

Track: `ws-ipc-auth-token_20260911` · Status: new · Type: feature (security / privacy)

## Overview

Eliminar la superficie de intercepción y espionaje en el canal local WebSocket (`ws://127.0.0.1:8765`), garantizando que solo clientes autorizados (**OpenCohost**, fuentes legítimas de **OBS**) puedan suscribirse y recibir transcripciones de voz en tiempo real.

El filtro de seguridad actual en `liveaudio/core/network.py` valida la cabecera `Origin` para neutralizar ataques de *Cross-Site WebSocket Hijacking* (CSWSH) provenientes de pestañas del navegador (`evil.com`). Sin embargo, un ejecutable, script o herramienta nativa no-navegador ejecutada en el espacio de usuario local puede omitir o falsear la cabecera `Origin` y suscribirse silenciosamente al socket local para leer las transcripciones en texto plano.

Este track implementa autenticación criptográfica efímera mediante token compartido (*shared secret / bearer token*).

Decisiones PO cerradas (base normativa, no reabrir sin PO):
1. **Modo Headless (`liveaudio-service` con OpenCohost):** Autenticación estricta y obligatoria siempre por sesión. Token efímero inyectado vía CLI `--auth-token <hex>` o autogenerado por el servicio y emitido en el evento de inicio por stdout (`{"type": "auth_token", "token": "..."}`).
2. **Modo GUI Tradicional (Desktop):** Configurable mediante toggle en Ajustes (`auth_token_enabled`, por defecto `false`) para no invalidar de golpe las URLs ya configuradas en OBS de usuarios existentes, a menos que el streamer decida activar la protección.
3. **Mecanismo de Handshake:** Token aceptado vía query param (`?token=<hex>`) o cabecera estándar (`Authorization: Bearer <hex>`). Clientes sin token o con token inválido son rechazados de inmediato con HTTP `401 Unauthorized` y código de cierre WebSocket `4401`.
4. **Propagación en OBS:** `subtitulos_obs.html` lee automáticamente `?token=` de su propia URL y lo anexa al handshake de conexión WebSocket.

---

## Requisitos Funcionales

- **REQ-1 — Generación y Aceptación de Token Efímero en Modo Servicio.**
  `liveaudio-service` acepta el argumento opcional `--auth-token <HEX_STRING>` (mínimo 32 caracteres / 128 bits de entropía). Si no se provee por CLI en modo headless, el servicio genera un token criptográfico seguro (`secrets.token_hex(32)`) y lo emite en stdout bajo el esquema `liveaudio.service.event`:
  ```json
  {"schema": "liveaudio.service.event", "version": 1, "service_pid": 1234, "parent_pid": 5678, "type": "auth_token", "token": "..."}
  ```
  El token vive estrictamente en memoria del proceso y nunca se persiste en archivos estáticos (`config.json`).

- **REQ-2 — Toggle Configurable en GUI y Perfiles.**
  La interfaz gráfica y el esquema de configuración en `liveaudio/utils/config.py` agregan el campo booleano `auth_token_enabled` (default `false`).
  - Cuando está deshabilitado: el servidor WebSocket opera en modo abierto/retrocompatible como hoy.
  - Cuando está habilitado: la GUI genera un token efímero por sesión de streaming y el botón "Copiar URL para OBS" genera la URL incluyendo el parámetro: `subtitulos_obs.html?port=8765&token=<hex>`.

- **REQ-3 — Validación Estricta en el Handshake del Servidor WebSocket.**
  En `liveaudio/core/network.py`, durante el handshake de conexión:
  - Si el modo de autenticación está activo (activo incondicional en headless; condicionado a `auth_token_enabled` en GUI):
    - Se extrae el token de la URI (`urllib.parse.parse_qs(path).get("token")`) o del encabezado `Authorization: Bearer <token>`.
    - Se compara en tiempo constante (`hmac.compare_digest`) para prevenir ataques de canal lateral (timing attacks).
    - Si el token es inválido o no existe: el servidor responde inmediatamente con status HTTP `401 Unauthorized` y cierra la conexión (`close_code=4401, reason="Unauthorized"`), impidiendo que el cliente se registre en el broadcast de subtítulos.

- **REQ-4 — Soporte en Overlay de OBS (`subtitulos_obs.html`).**
  El script cliente de `subtitulos_obs.html` parsea el parámetro `token` desde `window.location.search`. Si está presente, lo anexa a la URL de conexión WebSocket: `ws://127.0.0.1:<port>/?token=<token>`.

- **REQ-5 — Manejo Idempotente de Re-conexión y Replicación de Estado.**
  Los clientes autorizados que sufran desconexión temporal pueden reconectarse utilizando el mismo token durante la vida del servicio sin necesidad de renegociación.

---

## Requisitos No Funcionales

- **NFR-1 — Seguridad Criptográfica:** Uso de generadores pseudo-aleatorios criptográficamente seguros (`secrets.token_hex`) y comparación de tokens en tiempo constante (`hmac.compare_digest`).
- **NFR-2 — Privacidad de Transcripciones:** El token nunca se almacena en `config.json`, logs de crash, ni registros persistentes de telemetría.
- **NFR-3 — Cero Impacto en Rendimiento:** La verificación de token se ejecuta en <0.1 ms durante el handshake inicial TCP/HTTP, sin introducir sobrecarga en el bucle caliente de streaming de audio/ASR.
- **NFR-4 — Compatibilidad hacia atrás:** La app de escritorio no rompe configuraciones previas de OBS salvo que el usuario active voluntariamente la protección en Ajustes.

---

## Criterios de Aceptación (Acceptance Criteria)

### AC-1 — Conexión autorizada en modo headless
- **Given:** `liveaudio-service` iniciado con `--auth-token mysecrettoken1234567890abcdef`.
- **When:** Un cliente (OpenCohost / OBS) conecta a `ws://127.0.0.1:8765/?token=mysecrettoken1234567890abcdef`.
- **Then:** La conexión se establece con éxito (101 Switching Protocols) y el cliente recibe eventos de subtítulos.

### AC-2 — Rechazo inmediato de cliente espía o no autorizado
- **Given:** `liveaudio-service` iniciado con autenticación activa.
- **When:** Un proceso local intenta conectar a `ws://127.0.0.1:8765/` sin token, o con un token incorrecto `?token=hacker`.
- **Then:** El servidor rechaza la conexión con HTTP 401 y cierra el socket con código 4401 sin enviar ningún texto de transcripción.

### AC-3 — Retrocompatibilidad en GUI tradicional
- **Given:** LiveAudio GUI iniciado con `auth_token_enabled=false` (valor por defecto).
- **When:** Una fuente de OBS tradicional conecta sin token.
- **Then:** La conexión es admitida como de costumbre para no romper overlays existentes.

### AC-4 — Protección en GUI al activar el toggle
- **Given:** LiveAudio GUI con `auth_token_enabled=true`.
- **When:** Se inicia el motor y se copia la URL de OBS.
- **Then:** La URL contiene `?token=...`, y conexiones sin el parámetro son rechazadas.
