# SPDX-License-Identifier: MIT
"""Enumeración de dispositivos de audio sin importar torch.

list_audio_devices() se invoca mientras la GUI construye su interfaz. Mantenerlo
en un módulo que nunca importa torch permite que el proceso GUI liste los
dispositivos sin pagar el coste de importar torch (core/audio.py sigue siendo el
dueño del código de VAD/torch que corre en el proceso de audio).
"""

import sys
import sounddevice as sd


def _normalize_device_name(name):
    """Normaliza nombre de dispositivo para deduplicar variantes del mismo hardware.

    Ejemplo: 'Microphone (Realtek Audio)', 'Microphone (Realtek High Definition Audio)'
    ambos se normalizan a 'microphone realtek audio'.
    """
    import re
    # Quitar parentesis y contenido, luego limpiar
    cleaned = re.sub(r'\([^)]*\)', '', name)
    # Quitar caracteres especiales, lowercase
    cleaned = re.sub(r'[^a-z0-9\s]', '', cleaned.lower().strip())
    # Colapsar espacios multiples
    cleaned = re.sub(r'\s+', ' ', cleaned).strip()
    return cleaned


def list_audio_devices():
    """
    Retorna una lista de dispositivos de audio disponibles para captura.
    Cada entrada es un dict con: index, name, hostapi, max_input_channels, is_loopback.
    Incluye dispositivos WASAPI loopback en Windows para capturar audio del sistema.
    Dispositivos duplicados (mismo nombre base) se filtran para evitar confusion.
    """
    devices = sd.query_devices()
    hostapis = sd.query_hostapis()

    result = []
    seen_names = set()  # Para deduplicar por nombre normalizado

    for i, dev in enumerate(devices):
        # Dispositivos de entrada normales (microfonos)
        if dev["max_input_channels"] > 0:
            hostapi_name = hostapis[dev["hostapi"]]["name"]
            norm_name = _normalize_device_name(dev["name"])

            # Saltar duplicados (mismo nombre base, mismo tipo)
            dedup_key = f"input:{norm_name}"
            if dedup_key in seen_names:
                continue
            seen_names.add(dedup_key)

            result.append({
                "index": i,
                "name": dev["name"],
                "hostapi": hostapi_name,
                "max_input_channels": dev["max_input_channels"],
                "is_loopback": False,
                "display": f"🎤 {dev['name']} ({hostapi_name})"
            })

        # Dispositivos de salida WASAPI → loopback (captura del sistema)
        if sys.platform == "win32" and dev["max_output_channels"] > 0:
            hostapi_name = hostapis[dev["hostapi"]]["name"]
            if "WASAPI" in hostapi_name:
                norm_name = _normalize_device_name(dev["name"])

                dedup_key = f"loopback:{norm_name}"
                if dedup_key in seen_names:
                    continue
                seen_names.add(dedup_key)

                result.append({
                    "index": i,
                    "name": dev["name"],
                    "hostapi": hostapi_name,
                    "max_input_channels": dev["max_output_channels"],
                    "is_loopback": True,
                    "display": f"🔊 {dev['name']} (Loopback)"
                })

    return result
