# SPDX-License-Identifier: MIT
import time
import json
import urllib.request
import threading
from utils.config import load_config, save_config

APP_VERSION = "1.0.0"  # Versión canónica del software
GITHUB_RELEASES_URL = "https://api.github.com/repos/plynte-labs/LiveAudio/releases/latest"

def parse_version(version_str):
    """Parsea 'v1.2.3' -> (1, 2, 3) para comparación semántica limpia."""
    cleaned = version_str.strip().lower()
    if cleaned.startswith('v'):
        cleaned = cleaned[1:]
    try:
        return tuple(map(int, cleaned.split('.')))
    except ValueError:
        return (0, 0, 0)

def is_new_version_available(current_version, latest_version):
    """Compara si la versión de GitHub es mayor que la local."""
    return parse_version(latest_version) > parse_version(current_version)

def check_for_updates_async(callback):
    """
    Ejecuta el chequeo de actualizaciones en un hilo secundario de forma no bloqueante.
    Solo realiza la consulta HTTP si han pasado > 24 horas desde el último chequeo
    (a menos que la configuración no tenga registro de este).
    
    El `callback` debe aceptar un booleano (si hay update) y un string (el tag de versión).
    """
    def run():
        config = load_config()
        last_check = config.get("last_update_check", 0)
        current_time = int(time.time())
        
        # Verificar cada 24 horas (86400 segundos)
        if current_time - last_check < 86400:
            return
            
        try:
            import ssl
            context = ssl._create_unverified_context()
        except Exception:
            context = None

        try:
            req = urllib.request.Request(
                GITHUB_RELEASES_URL,
                headers={"User-Agent": "Plynte-LiveAudio-Client"}
            )
            with urllib.request.urlopen(req, timeout=5, context=context) as response:
                if response.status == 200:
                    data = json.loads(response.read().decode('utf-8'))
                    latest_tag = data.get("tag_name", "v0.0.0")
                    
                    # Guardar timestamp del chequeo exitoso
                    config["last_update_check"] = current_time
                    save_config(config)
                    
                    if is_new_version_available(APP_VERSION, latest_tag):
                        callback(True, latest_tag)
                    else:
                        callback(False, latest_tag)
        except Exception as e:
            # Fallar en silencio para no arruinar la experiencia del usuario si no hay internet
            print(f"[Updater] No se pudo verificar actualizaciones: {e}")
            
    thread = threading.Thread(target=run, daemon=True)
    thread.start()
