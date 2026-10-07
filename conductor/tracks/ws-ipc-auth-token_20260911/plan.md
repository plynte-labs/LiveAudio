# Plan — WebSocket IPC Auth Token (Anti-Eavesdropping & Anti-Impersonation)

Track: `ws-ipc-auth-token_20260911` · Status: in-planning · Type: feature

## Tareas de Implementación

### Fase 1: Motor y Validación en WebSocket Server (Core)
- [ ] **Task 1.1: Validador de Token en `liveaudio/core/network.py` (REQ-3, NFR-1, NFR-3)**
  - Agregar parámetro `auth_token: Optional[str] = None` al servidor WebSocket.
  - Implementar hook de validación de handshake (`process_request`):
    - Extraer token de query string (`?token=...`) o cabecera `Authorization: Bearer <token>`.
    - Comparar usando `hmac.compare_digest` si `auth_token` está activo.
    - Rechazar con HTTP 401 si no coincide o falta; permitir si coincide o si `auth_token is None`.
  - Escribir tests TDD en `tests/test_idempotent_ws_auth.py` cubriendo conexiones válidas, inválidas, ausentes y vacías.
  - Responsable: `[Agent Arquitecto Deepseek V4 PRO]`.

- [ ] **Task 1.2: Soporte en Overlay de OBS en `subtitulos_obs.html` (REQ-4)**
  - Extraer `token` de `window.location.search`.
  - Propagar `?token=` en la URL del socket al instanciar `new WebSocket(...)`.
  - Responsable: `[Agent UI/UX Security Architect]`.

### Fase 2: Modo Servicio Headless
- [ ] **Task 2.1: Argumento CLI y Generación de Token en `liveaudio/service/` (REQ-1, NFR-2)**
  - Añadir `--auth-token` a `build_arg_parser()`.
  - Si falta en modo headless, generar con `secrets.token_hex(32)`.
  - Pasar el token al `ProcessSupervisor` y al worker `run_ws_server`.
  - Emitir evento inicial `auth_token` en stdout con el esquema `liveaudio.service.event`.
  - Escribir test en `tests/test_idempotent_service_backend.py`.
  - Responsable: `[Agent Performance Minimax M2.7]`.

### Fase 3: Integración con GUI y Configuración
- [ ] **Task 3.1: Esquema de Configuración y UI en `liveaudio/utils/config.py` y `app.py` (REQ-2, NFR-4)**
  - Agregar `auth_token_enabled: bool = False` al esquema de configuración con validación estricta.
  - En la GUI, generar un token efímero de sesión cuando el toggle esté activo.
  - Actualizar botón "Copiar URL para OBS" para incluir `&token=...` solo cuando esté habilitado.
  - Escribir tests en `tests/test_config.py` y `tests/test_apply_settings_save.py`.
  - Responsable: `[Agent QA Qwen 3.6Plus]`.

### Fase 4: Documentación y Revisión Colectiva
- [ ] **Task 4.1: Actualización de Documentación Técnica y Glosario (REQ-5)**
  - Actualizar `docs/WEBSOCKET_OBS.md`, `README.md`, y `HISTORIAL_CAMBIOS.md`.
  - Documentar el contrato para clientes externos (OpenCohost / VoiceAI).
  - Responsable: `[Agent Research Gemini 2.5 Pro]`.
- [ ] **Task 4.2: Dream Team Collective Sign-Off**
  - Architecture: Verificación de `hmac.compare_digest`, loopback y cero persistencia.
  - Performance: Medición de overhead en handshake (<0.1ms) y reconexión.
  - QA: Verificación de AC-1 a AC-4 y no-regresión en OBS.
  - Research: Trazabilidad de requisitos y changelog.

---

## Quality Gates

1. Tests automatizados `tests/test_idempotent_ws_auth.py` cubren AC-1, AC-2, AC-3 y AC-4.
2. `python -m compileall liveaudio core utils` compila con 0 errores.
3. Cero secretos persistidos en disco o logs.
4. Sign-off unánime de los 4 agentes especializados antes del cierre.
