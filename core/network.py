import asyncio
import json
import queue

from websockets.asyncio.server import serve, broadcast


def _emit(log_queue, event):
    if log_queue is None:
        return
    try:
        log_queue.put_nowait(event)
    except Exception:
        pass


def _emit_log(log_queue, message):
    _emit(log_queue, {"type": "log", "message": message})


async def _handle_client(websocket, clients, log_queue):
    """Handler para cada conexión WebSocket entrante."""
    remote = websocket.remote_address
    if remote and remote[0] not in ("127.0.0.1", "::1", "localhost"):
        _emit_log(log_queue, f"[WebSocket] Conexion rechazada de {remote[0]}")
        print(f"[WebSocket] Conexion rechazada de {remote[0]}")
        await websocket.close(1008, "Conexiones externas no permitidas")
        return

    client_id = f"{remote[0]}:{remote[1]}" if remote else "unknown"
    clients.add(websocket)
    _emit_log(log_queue, f"[WebSocket] Cliente conectado: {client_id}")
    _emit(log_queue, {"type": "status", "key": "obs", "text": f"OBS: {len(clients)} clientes", "state": "ok"})
    print(f"[WebSocket] Cliente conectado: {client_id}")

    try:
        # Mantener la conexión abierta — wait_closed() es el patrón correcto
        # en websockets 16.x cuando no esperamos mensajes del cliente.
        await websocket.wait_closed()
    finally:
        clients.discard(websocket)
        _emit_log(log_queue, f"[WebSocket] Cliente desconectado: {client_id}")
        _emit(log_queue, {"type": "status", "key": "obs", "text": f"OBS: {len(clients)} clientes", "state": "ok" if clients else "idle"})
        print(f"[WebSocket] Cliente desconectado: {client_id}")


async def _poll_queue(text_queue, server, log_queue):
    """Polling loop que lee la cola IPC y hace broadcast usando la API nativa de websockets 16."""
    replay_buffer = []
    next_replay_at = 0.0

    def _broadcast_msg(msg):
        payload = json.dumps(msg)
        # broadcast() de websockets 16 — envía a TODOS los clientes
        # conectados al servidor sin backpressure, de forma óptima.
        # server.connections devuelve el set de conexiones activas.
        broadcast(server.connections, payload)

    while True:
        try:
            while True:
                msg = text_queue.get_nowait()
                if msg is None:  # Señal de apagado
                    return

                if isinstance(msg, dict) and msg.get("is_replay") and msg.get("catchup_interval_sec", 0) > 0:
                    replay_buffer.append(msg)
                else:
                    _broadcast_msg(msg)

        except queue.Empty:
            pass
        except Exception as e:
            _emit_log(log_queue, f"[WebSocket] Error en polling loop: {e}")
            _emit(log_queue, {"type": "status", "key": "ws", "text": "WS: error", "state": "error"})
            print(f"[WebSocket] Error en polling loop: {e}")
            await asyncio.sleep(0.1)

        now = asyncio.get_running_loop().time()
        if replay_buffer and now >= next_replay_at:
            msg = replay_buffer.pop(0)
            try:
                catchup_interval = float(msg.get("catchup_interval_sec", 0.0))
            except (TypeError, ValueError):
                catchup_interval = 0.0
            _emit(log_queue, {"type": "status", "key": "ws", "text": f"WS: catch-up {len(replay_buffer)}", "state": "warn"})
            _broadcast_msg(msg)
            next_replay_at = now + max(0.0, catchup_interval)

        await asyncio.sleep(0.05)


def run_ws_server(text_queue, log_queue=None, port=8765):
    """Punto de entrada para el multiprocesamiento."""
    _emit_log(log_queue, f"[WebSocket] Iniciando servidor en ws://127.0.0.1:{port}")
    _emit(log_queue, {"type": "status", "key": "ws", "text": "WS: iniciando", "state": "active"})
    print(f"[WebSocket] Iniciando servidor en ws://127.0.0.1:{port}")

    async def main():
        clients = set()

        async def handle_client(websocket):
            await _handle_client(websocket, clients, log_queue)

        async with serve(handle_client, "127.0.0.1", port) as server:
            _emit(log_queue, {"type": "status", "key": "ws", "text": f"WS: localhost:{port}", "state": "ok"})
            # Ejecutar el polling de la cola en paralelo con el servidor
            await _poll_queue(text_queue, server, log_queue)

    try:
        asyncio.run(main())
    except Exception as e:
        _emit_log(log_queue, f"[WebSocket] Error fatal: {e}")
        _emit(log_queue, {"type": "status", "key": "ws", "text": "WS: error", "state": "error"})
        raise
