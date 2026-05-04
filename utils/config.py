import os
import json
import multiprocessing as mp

CONFIG_FILE = "config.json"

DEFAULT_CONFIG = {
    "output_dir": os.path.abspath("sessions"),  # Usa una carpeta local por defecto
    "device": "cuda",
    "cpu_threads": max(1, mp.cpu_count() // 2),
    "model_size": "small (Balance CPU)",  # Nombres descriptivos por defecto
    "blacklist": "amara.org, subtítulos por, suscríbete, dale like, gracias por ver, memos, gracias, activar la campanita",
    "continuous_session": True,
    "subtitle_style": "default",
    "silence_timeout": 0.8,
    "max_chunk_duration": 5.0,
    "audio_device": None,  # None = dispositivo por defecto del OS
}

def load_config():
    """
    Carga la configuración desde config.json.
    Si faltan keys nuevas, las rellena con los defaults (migración automática).
    Valida que output_dir sea una ruta normalizada.
    """
    if not os.path.exists(CONFIG_FILE):
        save_config(DEFAULT_CONFIG)
        return DEFAULT_CONFIG.copy()
    
    with open(CONFIG_FILE, "r", encoding="utf-8") as f:
        config = json.load(f)
    
    # Migración: rellenar keys faltantes con defaults
    updated = False
    for key, default_value in DEFAULT_CONFIG.items():
        if key not in config:
            config[key] = default_value
            updated = True
    
    # Validación: normalizar output_dir
    output_dir = config.get("output_dir", "")
    if output_dir:
        config["output_dir"] = os.path.normpath(os.path.abspath(output_dir))
    else:
        config["output_dir"] = DEFAULT_CONFIG["output_dir"]
        updated = True
    
    if updated:
        save_config(config)
    
    return config

def save_config(config):
    with open(CONFIG_FILE, "w", encoding="utf-8") as f:
        json.dump(config, f, indent=4, ensure_ascii=False)