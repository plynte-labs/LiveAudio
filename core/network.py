# SPDX-License-Identifier: MIT
import asyncio
import json
import queue

from websockets.asyncio.server import serve, broadcast
from core.diagnostics import create_store_from_config


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


async def _handle_client(websocket, clients, log_queue, diagnostics_store=None):
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
    _emit_log(log_queue, f"[WebSocket] Iniciando servidor en ws://127.0.0.1:{port}")
    _emit(log_queue, {"type": "status", "key": "ws", "text": "WS: iniciando", "state": "active"})
    print(f"[WebSocket] Iniciando servidor en ws://127.0.0.1:{port}")
    diagnostics_store = diagnostics_store or create_store_from_config(diagnostics_config or {})

    async def main():
        clients = set()

        async def handle_client(websocket):
            await _handle_client(websocket, clients, log_queue, diagnostics_store=diagnostics_store)

        async with serve(handle_client, "127.0.0.1", port, ping_interval=10, ping_timeout=5) as server:
            _emit(log_queue, {"type": "status", "key": "ws", "text": f"WS: localhost:{port}", "state": "ok"})
            # Ejecutar el polling de la cola en paralelo con el servidor
            await _poll_queue(text_queue, server, log_queue, diagnostics_store=diagnostics_store)

    try:
        asyncio.run(main())
    except Exception as e:
        _emit_log(log_queue, f"[WebSocket] Error fatal: {e}")
        _emit(log_queue, {"type": "status", "key": "ws", "text": "WS: error", "state": "error"})
        raise
