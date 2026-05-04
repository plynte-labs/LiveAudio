import asyncio
import json
import queue

from websockets.asyncio.server import serve, broadcast


async def _handle_client(websocket):
    """Handler para cada conexión WebSocket entrante."""
    remote = websocket.remote_address
    if remote and remote[0] not in ("127.0.0.1", "::1", "localhost"):
        print(f"[WebSocket] ⛔ Conexión rechazada de {remote[0]}")
        await websocket.close(1008, "Conexiones externas no permitidas")
        return

    client_id = f"{remote[0]}:{remote[1]}" if remote else "unknown"
    print(f"[WebSocket] 🟢 Cliente conectado: {client_id}")

    try:
        # Mantener la conexión abierta — wait_closed() es el patrón correcto
        # en websockets 16.x cuando no esperamos mensajes del cliente.
        await websocket.wait_closed()
    finally:
        print(f"[WebSocket] 🔴 Cliente desconectado: {client_id}")


async def _poll_queue(text_queue, server):
    """Polling loop que lee la cola IPC y hace broadcast usando la API nativa de websockets 16."""
    while True:
        try:
            msg = text_queue.get_nowait()
            if msg is None:  # Señal de apagado
                break

            payload = json.dumps(msg)
            # broadcast() de websockets 16 — envía a TODOS los clientes
            # conectados al servidor sin backpressure, de forma óptima.
            # server.connections devuelve el set de conexiones activas.
            broadcast(server.connections, payload)

        except queue.Empty:
            await asyncio.sleep(0.05)
        except Exception as e:
            print(f"[WebSocket] Error en polling loop: {e}")
            await asyncio.sleep(0.1)


def run_ws_server(text_queue):
    """Punto de entrada para el multiprocesamiento."""
    print("[WebSocket] Iniciando servidor en ws://127.0.0.1:8765")

    async def main():
        async with serve(_handle_client, "127.0.0.1", 8765) as server:
            # Ejecutar el polling de la cola en paralelo con el servidor
            await _poll_queue(text_queue, server)

    asyncio.run(main())
