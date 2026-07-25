# SPDX-License-Identifier: MIT
import asyncio
import errno
import http
import json
import os
import queue
import socket

from websockets.asyncio.server import serve, broadcast
from liveaudio.core.diagnostics import create_store_from_config
from liveaudio.utils.streams import make_streams_encoding_safe

WS_PORT_FALLBACK_RANGE = 10

# Origenes permitidos en el handshake WebSocket. El navegador NO aplica
# same-origin a los WebSockets: cualquier web abierta mientras se transmite
# podria conectarse a ws://127.0.0.1:8765 y leer la transcripcion en vivo, asi
# que filtrar por Origin es responsabilidad del servidor.
#   - "http://absolute": host interno de CEF para las fuentes de navegador
#     locales de OBS. Valor MEDIDO contra el OBS real del mantenedor
#     (2026-07-24), no supuesto; no es "null", y una web no puede falsificarlo
#     porque el navegador escribe Origin desde el origen real del documento.
# Ausencia de cabecera Origin: se acepta aparte (ver _reject_foreign_origin).
# Todo cliente no navegador (scripts, herramientas nativas, integraciones
# futuras) no envia Origin, y un proceso no navegador puede falsificar
# cualquier valor: eso queda fuera de este filtro por diseño.
WS_ALLOWED_ORIGINS = frozenset({"http://absolute"})


def port_available(port, host="127.0.0.1"):
    """Comprueba si un puerto TCP local está libre intentando un bind real.

    Chequeo consultivo con carrera TOCTOU inherente: otro proceso puede tomar
    el puerto entre esta comprobación y el bind real del servidor. El monitor
    de salud del proceso WS cubre esa carrera. En POSIX replica el
    SO_REUSEADDR que asyncio aplica en el bind real del servidor (evita
    falsos "ocupado" por sockets en TIME_WAIT tras un Stop→Start); en
    Windows no se usa, porque allí permitiría bindear sobre un puerto
    activo y ocultaría conflictos reales.
    """
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
            if os.name == "posix":
                probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            probe.bind((host, port))
        return True
    except OSError:
        return False


def port_range_available(base, host="127.0.0.1"):
    """Return whether any candidate in the bounded fallback range is free."""
    top_port = min(base + WS_PORT_FALLBACK_RANGE - 1, 65535)
    return any(port_available(port, host) for port in range(base, top_port + 1))


def _emit(log_queue, event):
    if log_queue is None:
        return
    try:
        log_queue.put_nowait(event)
    except Exception:
        pass


def _emit_log(log_queue, message):
    _emit(log_queue, {"type": "log", "message": message})


def _record_network_runtime_health(
    diagnostics_store,
    *,
    client_count=None,
    replay_buffer_size=None,
    retry_buffer_size=None,
    backpressure=None,
    queue_drained_count=None,
    rejected_client=None,
):
    if diagnostics_store is None:
        return
    if rejected_client:
        diagnostics_store.record_counter("ws.rejected_clients")
    if queue_drained_count is not None:
        diagnostics_store.record_counter("ws.queue_drained_messages", int(queue_drained_count))
    if backpressure:
        diagnostics_store.record_counter("ws.backpressure_events")
    payload = {}
    if client_count is not None:
        payload["client_count"] = int(client_count)
    if replay_buffer_size is not None:
        payload["replay_buffer_size"] = int(replay_buffer_size)
    if retry_buffer_size is not None:
        payload["retry_buffer_size"] = int(retry_buffer_size)
    if backpressure is not None:
        payload["backpressure"] = bool(backpressure)
    if rejected_client is not None:
        payload["rejected_client"] = bool(rejected_client)
    if payload:
        diagnostics_store.record_state("ws.runtime", payload)


def _reject_foreign_origin(connection, request, log_queue=None):
    """Hook process_request: corta el handshake si el Origin no está permitido.

    Se filtra aquí y no con el parámetro origins= de serve() a propósito: ese
    parámetro responde 403 antes de que corra el handler, así que el rechazo
    sería mudo. Si una versión futura de OBS cambiara su origen interno, el
    overlay moriría en silencio — exactamente el fallo que este proyecto ya
    peleó dos veces (#9, #11). Aquí el rechazo se ve en consola, en el log
    (con el Origin recibido, para diagnosticarlo de un vistazo) y en el chip.

    Devuelve None para continuar el handshake, o una Response para abortarlo.
    """
    origins = request.headers.get_all("Origin")
    if not origins:
        return None  # Cliente no navegador: no manda Origin.
    if len(origins) == 1 and origins[0] in WS_ALLOWED_ORIGINS:
        return None
    # Varias cabeceras Origin nunca son legítimas: falla cerrado y se registran
    # todas, en vez de reventar dentro del hook y degradar a un 500 mudo.
    received = ", ".join(origins)
    _emit_log(log_queue, f"[WebSocket] Conexion rechazada por Origin no permitido: {received}")
    _emit(log_queue, {"type": "status", "key": "obs", "text": "OBS: origen rechazado", "state": "warn"})
    print(f"[WebSocket] Conexion rechazada por Origin no permitido: {received}")
    return connection.respond(http.HTTPStatus.FORBIDDEN, "Origen no permitido\n")


async def _handle_client(websocket, clients, log_queue, diagnostics_store=None, effective_port=8765):
    """Handler para cada conexión WebSocket entrante."""
    remote = websocket.remote_address
    if remote and remote[0] not in ("127.0.0.1", "::1", "localhost"):
        _record_network_runtime_health(diagnostics_store, client_count=len(clients), rejected_client=True)
        _emit_log(log_queue, f"[WebSocket] Conexion rechazada de {remote[0]}")
        print(f"[WebSocket] Conexion rechazada de {remote[0]}")
        await websocket.close(1008, "Conexiones externas no permitidas")
        return

    client_id = f"{remote[0]}:{remote[1]}" if remote else "unknown"
    clients.add(websocket)
    _record_network_runtime_health(diagnostics_store, client_count=len(clients), rejected_client=False)
    _emit_log(log_queue, f"[WebSocket] Cliente conectado: {client_id}")
    _emit(log_queue, {"type": "status", "key": "obs", "text": f"OBS: {len(clients)} clientes", "state": "ok"})
    print(f"[WebSocket] Cliente conectado: {client_id}")

    try:
        await websocket.send(json.dumps({
            "type": "hello", "app": "liveaudio", "proto": 1, "port": effective_port,
        }))
        async for message in websocket:
            pass  # Client messages ignored (OBS browser source is receive-only)
    except Exception:
        pass  # Connection closed or error
    finally:
        clients.discard(websocket)
        _record_network_runtime_health(diagnostics_store, client_count=len(clients), rejected_client=False)
        _emit_log(log_queue, f"[WebSocket] Cliente desconectado: {client_id}")
        _emit(log_queue, {"type": "status", "key": "obs", "text": f"OBS: {len(clients)} clientes", "state": "ok" if clients else "idle"})
        print(f"[WebSocket] Cliente desconectado: {client_id}")


async def _poll_queue(text_queue, server, log_queue, diagnostics_store=None):
    """Polling loop que lee la cola IPC y hace broadcast usando la API nativa de websockets 16."""
    replay_buffer = []
    next_replay_at = 0.0
    HIGH_WATER_MARK = 65536  # 64KB — pause production if buffer exceeds this
    MAX_RETRY_BUFFER = 10  # Max messages to buffer during backpressure
    retry_buffer = []  # Buffer for messages that couldn't be sent due to backpressure
    backpressure_start = None  # Track when backpressure started

    def _can_broadcast():
        """Check if any client's buffer exceeds high water mark."""
        for conn in server.connections:
            try:
                if hasattr(conn, 'transport') and conn.transport:
                    buffered = conn.transport.get_write_buffer_size()
                    if buffered > HIGH_WATER_MARK:
                        return False
            except Exception:
                pass  # If we can't check, assume OK
        return True

    def _broadcast_msg(msg):
        payload = json.dumps(msg)
        # broadcast() de websockets 16 — envía a TODOS los clientes
        # conectados al servidor sin backpressure, de forma óptima.
        # server.connections devuelve el set de conexiones activas.
        broadcast(server.connections, payload)

    def _flush_retry_buffer():
        """Try to send all buffered messages. Returns True if all sent."""
        nonlocal backpressure_start
        while retry_buffer:
            if _can_broadcast():
                msg = retry_buffer.pop(0)
                _broadcast_msg(msg)
                if backpressure_start:
                    duration = asyncio.get_running_loop().time() - backpressure_start
                    if duration >= 5.0:
                        _emit_log(log_queue, f"[WebSocket] Backpressure resuelto despues de {duration:.1f}s")
                    backpressure_start = None
            else:
                if not backpressure_start:
                    backpressure_start = asyncio.get_running_loop().time()
                    _emit_log(log_queue, "[WebSocket] Backpressure activo — bufferizando mensajes")
                return False
        return True

    while True:
        drained_messages = 0
        try:
            # First try to flush the retry buffer
            if retry_buffer:
                if not _flush_retry_buffer():
                    _record_network_runtime_health(
                        diagnostics_store,
                        client_count=len(server.connections),
                        replay_buffer_size=len(replay_buffer),
                        retry_buffer_size=len(retry_buffer),
                        backpressure=True,
                    )
                    await asyncio.sleep(0.1)  # Wait before retry
                    continue

            while True:
                msg = text_queue.get_nowait()
                drained_messages += 1
                if msg is None:  # Señal de apagado
                    _record_network_runtime_health(
                        diagnostics_store,
                        client_count=len(server.connections),
                        replay_buffer_size=len(replay_buffer),
                        retry_buffer_size=len(retry_buffer),
                        backpressure=False,
                        queue_drained_count=max(0, drained_messages - 1),
                    )
                    return

                if isinstance(msg, dict) and msg.get("is_replay") and msg.get("catchup_interval_sec", 0) > 0:
                    replay_buffer.append(msg)
                else:
                    if _can_broadcast():
                        _broadcast_msg(msg)
                    else:
                        # Buffer the message for retry instead of losing it
                        if len(retry_buffer) >= MAX_RETRY_BUFFER:
                            dropped = retry_buffer.pop(0)  # Drop oldest
                            _emit_log(log_queue, "[WebSocket] Buffer de retry lleno — descartando mensaje viejo")
                        retry_buffer.append(msg)
                        if not backpressure_start:
                            backpressure_start = asyncio.get_running_loop().time()
                        _emit(log_queue, {"type": "status", "key": "ws", "text": "WS: backpressure", "state": "warn"})
                        _record_network_runtime_health(
                            diagnostics_store,
                            client_count=len(server.connections),
                            replay_buffer_size=len(replay_buffer),
                            retry_buffer_size=len(retry_buffer),
                            backpressure=True,
                            queue_drained_count=max(0, drained_messages),
                        )
                        break  # Exit inner while to wait for buffer to clear

        except queue.Empty:
            pass
        except Exception as e:
            _emit_log(log_queue, f"[WebSocket] Error en polling loop: {e}")
            _emit(log_queue, {"type": "status", "key": "ws", "text": "WS: error", "state": "error"})
            print(f"[WebSocket] Error en polling loop: {e}")
            await asyncio.sleep(0.1)
        finally:
            if drained_messages:
                _record_network_runtime_health(
                    diagnostics_store,
                    client_count=len(server.connections),
                    replay_buffer_size=len(replay_buffer),
                    retry_buffer_size=len(retry_buffer),
                    backpressure=bool(retry_buffer),
                    queue_drained_count=drained_messages,
                )

        now = asyncio.get_running_loop().time()
        if replay_buffer and now >= next_replay_at:
            # Check backpressure before replay burst
            if _can_broadcast():
                msg = replay_buffer.pop(0)
                try:
                    catchup_interval = float(msg.get("catchup_interval_sec", 0.0))
                except (TypeError, ValueError):
                    catchup_interval = 0.0
                _emit(log_queue, {"type": "status", "key": "ws", "text": f"WS: catch-up {len(replay_buffer)}", "state": "warn"})
                _broadcast_msg(msg)
                next_replay_at = now + max(0.0, catchup_interval)
            else:
                # Postpone replay if backpressure active
                next_replay_at = now + 0.5

        await asyncio.sleep(0.05)


def run_ws_server(text_queue, log_queue=None, port=8765, diagnostics_store=None, diagnostics_config=None):
    """Punto de entrada para el multiprocesamiento."""
    # A legacy-codepage console (cp1252) must not crash this child on
    # non-ASCII print output; the log_queue path keeps the original text.
    make_streams_encoding_safe()
    _emit_log(log_queue, f"[WebSocket] Iniciando servidor en ws://127.0.0.1:{port}")
    _emit(log_queue, {"type": "status", "key": "ws", "text": "WS: iniciando", "state": "active"})
    print(f"[WebSocket] Iniciando servidor en ws://127.0.0.1:{port}")
    diagnostics_store = diagnostics_store or create_store_from_config(diagnostics_config or {})

    async def main():
        clients = set()

        def check_origin(connection, request):
            return _reject_foreign_origin(connection, request, log_queue)

        last_bind_error = None
        top_port = min(port + WS_PORT_FALLBACK_RANGE - 1, 65535)
        for candidate in range(port, top_port + 1):
            async def handle_client(websocket, effective_port=candidate):
                await _handle_client(
                    websocket, clients, log_queue,
                    diagnostics_store=diagnostics_store,
                    effective_port=effective_port,
                )
            try:
                server_ctx = serve(
                    handle_client, "127.0.0.1", candidate,
                    ping_interval=10, ping_timeout=5, process_request=check_origin,
                )
                server = await server_ctx.__aenter__()
            except OSError as bind_error:
                if bind_error.errno in (errno.EADDRINUSE, 10048):
                    last_bind_error = bind_error
                    continue
                raise
            try:
                if candidate != port:
                    _emit_log(log_queue, f"[WebSocket] Puerto {port} ocupado; usando puerto de respaldo {candidate}")
                    print(f"[WebSocket] Puerto {port} ocupado; usando puerto de respaldo {candidate}")
                _emit(log_queue, {"type": "status", "key": "ws", "text": f"WS: localhost:{candidate}", "state": "ok"})
                # Puerto efectivo hacia la GUI (WPF-3), antes de servir la cola.
                _emit(log_queue, {"type": "ws_port", "port": candidate, "base": port})
                # Ejecutar el polling de la cola en paralelo con el servidor
                await _poll_queue(text_queue, server, log_queue, diagnostics_store=diagnostics_store)
            finally:
                await server_ctx.__aexit__(None, None, None)
            return

        # Rango agotado (WPF-1): degrada al fail-fast existente (dialogo + monitor).
        raise last_bind_error

    try:
        asyncio.run(main())
    except OSError as e:
        if e.errno in (errno.EADDRINUSE, 10048):
            _emit_log(log_queue, f'[WebSocket] Puerto {port} en uso por otra aplicacion. Cierra esa aplicacion o cambia "ws_port" en config.json.')
            _emit(log_queue, {"type": "status", "key": "ws", "text": f"WS: puerto {port} ocupado", "state": "error"})
        else:
            _emit_log(log_queue, f"[WebSocket] Error fatal: {e}")
            _emit(log_queue, {"type": "status", "key": "ws", "text": "WS: error", "state": "error"})
        raise
    except Exception as e:
        _emit_log(log_queue, f"[WebSocket] Error fatal: {e}")
        _emit(log_queue, {"type": "status", "key": "ws", "text": "WS: error", "state": "error"})
        raise
