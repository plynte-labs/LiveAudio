#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Standalone preview server for the OBS subtitle overlay.

Lets you SEE the overlay (styles, sizes, weights, the adaptive ribbon) in any
browser WITHOUT running the full app, a microphone, the ASR model, OBS, or a
release build. It is a tiny WebSocket *server* that pushes sample subtitle
payloads to the overlay, which is a WebSocket *client*.

Usage:
    .venv/Scripts/python.exe tools/preview_overlay.py
then open in any browser:
    file:///E:/LiveAudio/liveaudio/assets/subtitulos_obs.html?port=8765

It cycles through all 7 styles (so you can confirm size/weight are now uniform),
then fires a catch-up burst (is_replay) so the adaptive ribbon promotes to the
stacked view and demotes back after silence. Loops until you close it (Ctrl+C).

This is a dev/preview helper — it is NOT part of any feature track and does not
change the app or the overlay.
"""
import asyncio
import json
import sys

import websockets

HOST = "127.0.0.1"
# The real app holds 8765. Pass a free port (e.g. 8799) to run side by side:
#   python tools/preview_overlay.py 8799   -> open the overlay with ?port=8799
PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8765

STYLES = ["default", "karaoke", "neon", "minimal", "bold", "rgb", "typewriter"]

# One line per style — same length so the size/weight comparison is fair.
STYLE_LINES = {
    "default": "Estilo default: tamaño y peso ahora unificados",
    "karaoke": "Estilo karaoke: mismo tamaño, color propio",
    "neon": "Estilo neon: mismo tamaño, glow propio",
    "minimal": "Estilo minimal: mismo tamaño y peso que el resto",
    "bold": "Estilo bold: la referencia de tamaño y peso",
    "rgb": "Estilo rgb: mismo tamaño, animacion por palabra",
    "typewriter": "Estilo typewriter: mismo tamaño, fuente mono",
}

# Lines fired rapidly with is_replay=True to trigger the adaptive ribbon.
RIBBON_BURST = [
    "Linea 1 del backlog (la mas antigua, abajo)",
    "Linea 2 encimandose",
    "Linea 3 mas reciente",
    "Linea 4 arriba de todo (la mas nueva)",
]


async def _send(ws, text, style, is_replay=False):
    await ws.send(json.dumps({
        "text": text,
        "style": style,
        "is_replay": is_replay,
        "catchup_interval_sec": 1.5,
    }))


async def run_demo(ws):
    # 1) Cycle every style so size + weight uniformity is visible.
    for style in STYLES:
        await _send(ws, STYLE_LINES[style], style)
        await asyncio.sleep(2.5)

    # 2) Catch-up burst -> adaptive promotes to the RIBBON (newest on top).
    await _send(ws, "Hablando normal: una sola linea", "bold")
    await asyncio.sleep(1.5)
    for line in RIBBON_BURST:
        await _send(ws, line, "bold", is_replay=True)
        await asyncio.sleep(0.4)

    # 3) Silence -> ribbon settles and demotes back to a single line.
    await asyncio.sleep(5.0)


async def handler(websocket, *args):
    print("Overlay conectado -> corriendo demo (Ctrl+C para salir)")
    try:
        while True:
            await run_demo(websocket)
    except websockets.ConnectionClosed:
        print("Overlay desconectado.")


async def main():
    url = "file:///E:/LiveAudio/liveaudio/assets/subtitulos_obs.html?port=%d" % PORT
    print("Preview server escuchando en ws://%s:%d" % (HOST, PORT))
    print("Abri en el navegador (o como Browser Source en OBS):")
    print("  " + url)
    print("Probar un estilo fijo: agrega &style=bold ; forzar cinta: &mode=ribbon")
    async with websockets.serve(handler, HOST, PORT):
        await asyncio.Future()  # run forever


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("\nPreview detenido.")
