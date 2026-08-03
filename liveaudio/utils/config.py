# SPDX-License-Identifier: MIT
import os
import json
import multiprocessing as mp
import time


def get_data_home():
    """Return the LiveAudio data home directory (not created here).

    Resolution order:
    1. LIVEAUDIO_HOME environment variable (explicit override / tests)
    2. %APPDATA%\\LiveAudio on Windows
    3. $XDG_CONFIG_HOME/liveaudio or ~/.config/liveaudio elsewhere
    """
    home = os.environ.get("LIVEAUDIO_HOME")
    if not home:
        if os.name == "nt":
            base = os.environ.get("APPDATA") or os.path.expanduser("~")
            home = os.path.join(base, "LiveAudio")
        else:
            base = os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser(os.path.join("~", ".config"))
            home = os.path.join(base, "liveaudio")
    return os.path.normpath(os.path.abspath(home))


def _config_file():
    return os.path.join(get_data_home(), "config.json")


def _lock_file():
    return _config_file() + ".lock"


def _ensure_data_home():
    home = get_data_home()
    os.makedirs(home, exist_ok=True)
    return home


VALID_DEVICES = {"cpu", "cuda"}
VALID_MODELS = {
    "tiny (Más rápido, baja precisión)",
    "base (Rápido)",
    "small (Balance CPU)",
    "turbo (Máxima precisión GPU)",
}
MODEL_BY_KEY = {model.split()[0]: model for model in VALID_MODELS}
VALID_SUBTITLE_STYLES = {"default", "karaoke", "neon", "minimal", "bold", "rgb", "typewriter"}
VALID_SUBTITLE_DISPLAY_MODES = {"single", "ribbon", "adaptive"}
VALID_BACKLOG_POLICIES = {"auto", "live_only", "send_all"}
VALID_DIAGNOSTICS_LEVELS = {"off", "minimal", "deep"}

DEFAULT_CONFIG = {
    "output_dir": os.path.join(get_data_home(), "sessions"),  # Default sessions dir under the data home
    "device": "cuda",
    "cpu_threads": max(1, mp.cpu_count() // 2),
    "model_size": "small (Balance CPU)",  # Nombres descriptivos por defecto
    "blacklist": "amara.org, subtítulos por, suscríbete, dale like, gracias por ver, memos, gracias, activar la campanita",
    "continuous_session": True,
    "subtitle_style": "default",
    "subtitle_display_mode": "adaptive",
    "subtitle_ribbon_max_lines": 3,
    "subtitle_backlog_policy": "auto",
    "subtitle_max_live_delay_sec": 10.0,
    "subtitle_catchup_interval_sec": 1.5,
    "silence_timeout": 0.8,
    "max_chunk_duration": 5.0,
    "audio_device": None,  # None = dispositivo por defecto del OS
    "selected_profile_id": "balanced",
    "profile_mode": "preset",
    "ws_port": 8765,
    "obs_enabled": True,
    "save_transcript_enabled": True,  # Write transcript.jsonl to disk
    "save_vtt_enabled": True,  # Write subtitles.vtt to disk
    "whisper_context_prompt_es": "",
    "whisper_context_prompt_en": "",
    "asr_language": "es",  # Idioma de voz (ASR): "es" o "en"
    "settings_navigation_mode": "tabs",  # "tabs" o "dropdown"
    "language": None,  # None = autodetectar idioma de UI
    "diagnostics_enabled": False,
    "diagnostics_level": "minimal",
    "diagnostics_export_dir": None,
    "last_update_check": 0,
    "vad_speech_pad_ms": 200,  # Pre-roll de onset (ms) recuperado al inicio de cada frase
    "vad_threshold": 0.5,  # Sensibilidad del VAD Silero (0.1 a 0.9); mayor = menos sensible
}


def _clamp_number(value, default, min_value, max_value, cast=float):
    try:
        number = cast(value)
    except (TypeError, ValueError):
        return default, True
    if number < min_value:
        return min_value, True
    if number > max_value:
        return max_value, True
    return number, False


def _normalize_config(config):
    """Valida tipos/rangos sin eliminar configuracion del usuario."""
    updated = False

    for key, default_value in DEFAULT_CONFIG.items():
        if key not in config:
            config[key] = default_value
            updated = True

    output_dir = config.get("output_dir", "")
    if isinstance(output_dir, str) and output_dir.strip():
        normalized_output_dir = os.path.normpath(os.path.abspath(output_dir))
        if normalized_output_dir != output_dir:
            updated = True
        config["output_dir"] = normalized_output_dir
    else:
        config["output_dir"] = DEFAULT_CONFIG["output_dir"]
        updated = True

    if config.get("device") not in VALID_DEVICES:
        config["device"] = DEFAULT_CONFIG["device"]
        updated = True

    model_key = str(config.get("model_size", "")).split()[0]
    normalized_model = MODEL_BY_KEY.get(model_key)
    if normalized_model:
        if config.get("model_size") != normalized_model:
            config["model_size"] = normalized_model
            updated = True
    else:
        config["model_size"] = DEFAULT_CONFIG["model_size"]
        updated = True

    cpu_threads, changed = _clamp_number(config.get("cpu_threads"), DEFAULT_CONFIG["cpu_threads"], 1, max(1, mp.cpu_count()), int)
    config["cpu_threads"] = cpu_threads
    updated = updated or changed

    silence_timeout, changed = _clamp_number(config.get("silence_timeout"), DEFAULT_CONFIG["silence_timeout"], 0.3, 2.0, float)
    config["silence_timeout"] = round(silence_timeout, 1)
    updated = updated or changed

    max_chunk_duration, changed = _clamp_number(config.get("max_chunk_duration"), DEFAULT_CONFIG["max_chunk_duration"], 2.0, 15.0, float)
    config["max_chunk_duration"] = round(max_chunk_duration, 1)
    updated = updated or changed

    vad_speech_pad_ms, changed = _clamp_number(config.get("vad_speech_pad_ms"), DEFAULT_CONFIG["vad_speech_pad_ms"], 0, 500, int)
    config["vad_speech_pad_ms"] = vad_speech_pad_ms
    updated = updated or changed

    vad_threshold, changed = _clamp_number(config.get("vad_threshold"), DEFAULT_CONFIG["vad_threshold"], 0.1, 0.9, float)
    config["vad_threshold"] = round(vad_threshold, 2)
    updated = updated or changed

    if not isinstance(config.get("continuous_session"), bool):
        config["continuous_session"] = bool(config.get("continuous_session"))
        updated = True

    if not isinstance(config.get("blacklist"), str) or not config.get("blacklist", "").strip():
        config["blacklist"] = DEFAULT_CONFIG["blacklist"]
        updated = True

    # Migración transparente: mover whisper_context_prompt a whisper_context_prompt_es
    old_prompt = config.get("whisper_context_prompt", "")
    if old_prompt and isinstance(old_prompt, str) and old_prompt.strip():
        if not config.get("whisper_context_prompt_es"):
            config["whisper_context_prompt_es"] = old_prompt.strip()
            updated = True
    # Eliminar clave deprecada para mantener archivo limpio
    if "whisper_context_prompt" in config:
        del config["whisper_context_prompt"]
        updated = True

    if not isinstance(config.get("whisper_context_prompt_es"), str):
        config["whisper_context_prompt_es"] = DEFAULT_CONFIG["whisper_context_prompt_es"]
        updated = True

    if not isinstance(config.get("whisper_context_prompt_en"), str):
        config["whisper_context_prompt_en"] = DEFAULT_CONFIG["whisper_context_prompt_en"]
        updated = True

    if config.get("asr_language") not in {"es", "en"}:
        config["asr_language"] = DEFAULT_CONFIG["asr_language"]
        updated = True

    if config.get("subtitle_style") not in VALID_SUBTITLE_STYLES:
        config["subtitle_style"] = DEFAULT_CONFIG["subtitle_style"]
        updated = True

    if config.get("subtitle_display_mode") not in VALID_SUBTITLE_DISPLAY_MODES:
        config["subtitle_display_mode"] = DEFAULT_CONFIG["subtitle_display_mode"]
        updated = True

    ribbon_max_lines, changed = _clamp_number(config.get("subtitle_ribbon_max_lines"), DEFAULT_CONFIG["subtitle_ribbon_max_lines"], 1, 8, int)
    config["subtitle_ribbon_max_lines"] = ribbon_max_lines
    updated = updated or changed

    if config.get("subtitle_backlog_policy") not in VALID_BACKLOG_POLICIES:
        config["subtitle_backlog_policy"] = DEFAULT_CONFIG["subtitle_backlog_policy"]
        updated = True

    max_live_delay, changed = _clamp_number(config.get("subtitle_max_live_delay_sec"), DEFAULT_CONFIG["subtitle_max_live_delay_sec"], 1.0, 120.0, float)
    config["subtitle_max_live_delay_sec"] = round(max_live_delay, 1)
    updated = updated or changed

    catchup_interval, changed = _clamp_number(config.get("subtitle_catchup_interval_sec"), DEFAULT_CONFIG["subtitle_catchup_interval_sec"], 0.0, 10.0, float)
    config["subtitle_catchup_interval_sec"] = round(catchup_interval, 1)
    updated = updated or changed

    audio_device = config.get("audio_device")
    if audio_device is not None and not isinstance(audio_device, dict):
        config["audio_device"] = None
        updated = True

    if not isinstance(config.get("selected_profile_id"), str):
        config["selected_profile_id"] = DEFAULT_CONFIG["selected_profile_id"]
        updated = True

    if config.get("profile_mode") not in {"preset", "custom"}:
        config["profile_mode"] = DEFAULT_CONFIG["profile_mode"]
        updated = True

    ws_port = config.get("ws_port")
    if ws_port is not None:
        port_val, port_changed = _clamp_number(ws_port, 8765, 1, 65535, int)
        config["ws_port"] = port_val
        updated = updated or port_changed

    if not isinstance(config.get("obs_enabled"), bool):
        config["obs_enabled"] = bool(config.get("obs_enabled"))
        updated = True

    for sink_key in ("save_transcript_enabled", "save_vtt_enabled"):
        if not isinstance(config.get(sink_key), bool):
            config[sink_key] = bool(config.get(sink_key))
            updated = True

    if config.get("settings_navigation_mode") not in {"tabs", "dropdown"}:
        config["settings_navigation_mode"] = DEFAULT_CONFIG["settings_navigation_mode"]
        updated = True

    if config.get("language") not in {None, "es", "en"}:
        config["language"] = None
        updated = True

    if not isinstance(config.get("diagnostics_enabled"), bool):
        config["diagnostics_enabled"] = bool(config.get("diagnostics_enabled"))
        updated = True

    if config.get("diagnostics_level") not in VALID_DIAGNOSTICS_LEVELS:
        config["diagnostics_level"] = DEFAULT_CONFIG["diagnostics_level"]
        updated = True

    diagnostics_export_dir = config.get("diagnostics_export_dir")
    if diagnostics_export_dir is None or str(diagnostics_export_dir).strip() == "":
        if diagnostics_export_dir is not None:
            updated = True
        config["diagnostics_export_dir"] = None
    elif isinstance(diagnostics_export_dir, str):
        normalized_export_dir = os.path.normpath(os.path.abspath(diagnostics_export_dir))
        if diagnostics_export_dir != normalized_export_dir:
            updated = True
        config["diagnostics_export_dir"] = normalized_export_dir
    else:
        config["diagnostics_export_dir"] = None
        updated = True

    if not isinstance(config.get("last_update_check"), int):
        config["last_update_check"] = DEFAULT_CONFIG["last_update_check"]
        updated = True

    return config, updated

# Guardar la config tarda milisegundos: un lock retenido durante mas tiempo
# pertenece a un proceso muerto (crash o kill antes de _release_lock). Sin esto,
# un lock huerfano bloquea el guardado para siempre, incluso tras reiniciar.
# Se elige la deteccion por antiguedad y no por PID: comprobar si un PID sigue
# vivo no es portable en Windows sin ctypes, y el margen de 30 s ya es dos
# ordenes de magnitud mayor que cualquier guardado real.
LOCK_STALE_AFTER_SECONDS = 30.0


def _lock_is_stale(lock_path):
    """True si el lock existe y es lo bastante viejo como para estar abandonado."""
    try:
        return (time.time() - os.path.getmtime(lock_path)) > LOCK_STALE_AFTER_SECONDS
    except OSError:
        # Desaparecio entre el intento de creacion y la comprobacion: no es viejo.
        return False


def _acquire_lock(timeout=2.0):
    _ensure_data_home()
    lock_path = _lock_file()
    start = time.time()
    while True:
        try:
            fd = os.open(lock_path, os.O_CREAT | os.O_EXCL | os.O_RDWR)
            os.close(fd)
            return True
        except OSError:
            # La antiguedad se relee justo antes de borrar, asi que si otro
            # proceso ya rompio el lock y creo el suyo, aqui ya se ve fresco.
            # Si aun asi dos procesos lo rompen a la vez, el resultado no puede
            # corromper nada: _save_config_no_lock publica el archivo con un
            # os.replace atomico (gana el ultimo en escribir, sin archivos rotos).
            if _lock_is_stale(lock_path):
                try:
                    os.remove(lock_path)
                    continue  # Reintento inmediato: el lock huerfano ya no esta.
                except OSError:
                    # No se pudo borrar (permisos): seguir esperando el timeout
                    # en vez de girar en un bucle infinito.
                    pass
            if time.time() - start >= timeout:
                return False
            time.sleep(0.05)

def _release_lock():
    try:
        os.remove(_lock_file())
    except OSError:
        pass

def _save_config_no_lock(config):
    _ensure_data_home()
    config_path = _config_file()
    # Escritura atomica: se escribe en un temporal propio del proceso y se
    # publica con os.replace, que es atomico en Windows y POSIX. Asi un crash a
    # media escritura (o dos escritores simultaneos) nunca deja un config.json
    # truncado; como mucho gana el ultimo que publique.
    tmp_path = "{}.{}.tmp".format(config_path, os.getpid())
    try:
        with open(tmp_path, "w", encoding="utf-8") as f:
            json.dump(config, f, indent=4, ensure_ascii=False)
        os.replace(tmp_path, config_path)
    except BaseException:
        try:
            os.remove(tmp_path)
        except OSError:
            pass
        raise


def _write_config_reporting(config, locked, label):
    """Escribe la config informando del fallo. Devuelve True si llego a disco.

    Nunca lanza: el fallo debe ser visible por si solo (print) aunque el
    llamador ignore el valor de retorno, y varios llamadores corren en hilos
    de fondo donde una excepcion mataria el hilo en silencio.
    """
    if not locked:
        print(
            "[Config] {}: config NOT saved, lock busy at {} "
            "(delete it if no other LiveAudio instance is running).".format(label, _lock_file())
        )
        return False
    try:
        _save_config_no_lock(config)
        return True
    except Exception as exc:
        print("[Config] {}: config NOT saved: {}".format(label, exc))
        return False

def _migrate_legacy_config():
    """Copy a CWD config.json (legacy portable layout) to the data home once."""
    config_path = _config_file()
    legacy_path = os.path.normpath(os.path.abspath("config.json"))
    if legacy_path == config_path:
        return
    if os.path.exists(config_path) or not os.path.exists(legacy_path):
        return
    try:
        import shutil
        _ensure_data_home()
        shutil.copyfile(legacy_path, config_path)
    except OSError:
        pass

def load_config():
    """
    Carga la configuracion desde config.json.
    Si faltan keys nuevas, las rellena con los defaults (migracion automatica).
    Valida que output_dir sea una ruta normalizada.
    Auto-detecta GPU: si CUDA no esta disponible, fuerza device a "cpu".
    """
    _migrate_legacy_config()
    locked = _acquire_lock()
    try:
        if not os.path.exists(_config_file()):
            _write_config_reporting(DEFAULT_CONFIG, locked, "initial config")
            config = DEFAULT_CONFIG.copy()
        else:
            with open(_config_file(), "r", encoding="utf-8") as f:
                config = json.load(f)

        config, updated = _normalize_config(config)

        try:
            from liveaudio.utils.cuda import cuda_is_available
            if config.get("device") == "cuda" and not cuda_is_available():
                config["device"] = "cpu"
                updated = True
        except Exception:
            pass

        if updated:
            # La migracion/normalizacion tambien debe persistirse o avisar:
            # perderla en silencio devuelve al usuario a la config vieja.
            _write_config_reporting(config, locked, "normalized config")

        return config
    finally:
        if locked:
            _release_lock()

def save_config(config):
    """Guarda la configuracion en disco. Devuelve True si se escribio.

    Nunca lanza: un fallo se registra por stdout y se devuelve False, para que
    el llamador pueda reaccionar sin que un hilo de fondo muera por excepcion.
    """
    locked = _acquire_lock()
    try:
        return _write_config_reporting(config, locked, "save_config")
    finally:
        if locked:
            _release_lock()
