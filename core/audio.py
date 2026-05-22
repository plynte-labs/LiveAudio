# SPDX-License-Identifier: GPL-3.0-or-later
import os
import sys
import time
import queue
import threading
import collections
import multiprocessing as mp
import numpy as np
import sounddevice as sd
import torch
import warnings

# Suprimir solo advertencias de PyTorch/UserWarning, no todas
warnings.filterwarnings("ignore", category=UserWarning)

# --- CONFIGURACIÓN ESTÁTICA ---
SAMPLE_RATE = 16000  # Whisper y Silero requieren 16kHz
CHUNK_SIZE = 512     # Tamaño de ventana para el VAD (32 milisegundos)
VAD_THRESHOLD = 0.5  # Probabilidad mínima para considerar que hay voz (0.0 a 1.0)

# Ring Buffer: capacidad máxima en chunks antes de descartar los más viejos.
# 500 chunks * 32ms = 16 segundos de buffer. Más que suficiente para absorber
# cualquier pico de CPU sin perder audio.
RING_BUFFER_MAX_CHUNKS = 500


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


def _resolve_device_settings(config):
    """
    Resuelve el dispositivo de audio y sus extra_settings a partir de la config.
    Retorna (device_index_or_None, extra_settings_or_None).
    """
    audio_device = config.get("audio_device")
    
    if audio_device is None:
        return None, None  # Dispositivo por defecto del OS
    
    try:
        device_info = audio_device
        device_index = device_info.get("index")
        is_loopback = device_info.get("is_loopback", False)
        
        extra_settings = None
        if is_loopback and sys.platform == "win32":
            extra_settings = sd.WasapiSettings(exclusive=False)
        
        return device_index, extra_settings
    except Exception:
        return None, None  # Fallback al default


def audio_producer(audio_queue: mp.Queue, config: dict, log_queue: mp.Queue = None):
    """
    Proceso dedicado a la captura de audio y VAD.
    Vive en su propio núcleo y no bloquea el hilo principal.
    
    Arquitectura Ring Buffer:
    ┌──────────────┐     ┌────────────┐     ┌──────────────┐     ┌───────────┐
    │ Hardware      │────▶│ Ring Buffer │────▶│ VAD Worker   │────▶│ audio_queue│
    │ (callback C)  │     │ (deque)     │     │ (Thread)     │     │ (IPC)     │
    └──────────────┘     └────────────┘     └──────────────┘     └───────────┘
    
    El callback de C SOLO copia audio al ring buffer (operación ~0.01ms).
    El hilo VAD Worker lee del buffer, evalúa Silero, y decide cuándo enviar.
    Si el VAD tiene un pico de latencia, el audio se acumula en el buffer sin perderse.
    """
    def _log(msg):
        if log_queue:
            try:
                log_queue.put_nowait(msg)
            except Exception:
                pass
        print(msg)

    def _status(key, text, state="idle"):
        if log_queue:
            try:
                log_queue.put_nowait({"type": "status", "key": key, "text": text, "state": state})
            except Exception:
                pass

    silence_sec = config.get("silence_timeout", 0.8)
    max_sec = config.get("max_chunk_duration", 5.0)

    SILENCE_CHUNKS_TO_END = int((SAMPLE_RATE / CHUNK_SIZE) * silence_sec)
    MAX_CHUNKS_LIMIT = int((SAMPLE_RATE / CHUNK_SIZE) * max_sec)

    _status("vad", "VAD: cargando", "active")
    _log("[Productor] Cargando modelo Silero VAD en CPU...")
    # El VAD es extremadamente ligero, lo corremos en CPU para reservar la VRAM de la GPU
    try:
        hub_dir = torch.hub.get_dir()
        ruta_local_vad = os.path.join(hub_dir, 'snakers4_silero-vad_master')
        model, utils = torch.hub.load(
            repo_or_dir=ruta_local_vad,
            model='silero_vad',
            source='local',
            force_reload=False,
            onnx=False,
            trust_repo=True
        )
        _status("vad", "VAD: listo", "ok")
    except Exception as e:
        _log(f"[Productor] ERROR cargando modelo VAD: {e}. Verifica conexion a internet o descarga manualmente Silero VAD.")
        _status("vad", "VAD: error de carga", "error")
        raise
    
    # Resolver dispositivo de audio
    device_index, extra_settings = _resolve_device_settings(config)
    
    # --- RING BUFFER ---
    # deque con maxlen: si el worker se atrasa, los chunks más viejos se descartan
    # automáticamente en lugar de consumir RAM infinita.
    ring_buffer = collections.deque(maxlen=RING_BUFFER_MAX_CHUNKS)
    ring_event = threading.Event()  # Señal para despertar al worker cuando hay datos
    
    # Control de vida del worker
    worker_running = threading.Event()
    worker_running.set()
    
    # Señal de shutdown graceful
    shutdown_event = threading.Event()
    
    # Timestamp del último callback (para el watchdog)
    last_callback_time = time.time()
    # Lock ligero para el timestamp (atómico en CPython pero explícito por claridad)
    callback_time_lock = threading.Lock()

    def audio_callback(indata, frames, time_info, status):
        """
        Callback llamado por sounddevice en un hilo de C.
        SOLO copia el audio al ring buffer. Nada de IA aquí.
        Tiempo de ejecución: ~0.01ms (copia de memoria).
        """
        nonlocal last_callback_time
        
        with callback_time_lock:
            last_callback_time = time.time()

        if status:
            _log(f"[Productor] Advertencia de audio: {status}")

        # Extraer el canal mono y copiar (el buffer de C se reutiliza)
        audio_chunk = indata[:, 0].copy()
        
        # Meter al ring buffer (thread-safe en CPython por el GIL)
        ring_buffer.append(audio_chunk)
        
        # Despertar al worker
        ring_event.set()

    def vad_worker():
        """
        Hilo dedicado a procesar el ring buffer con Silero VAD.
        Separado del callback de C para no bloquear la captura de hardware.
        """
        speech_buffer = []
        silence_counter = 0
        is_speaking = False
        last_reported_state = "idle"
        utterance_sequence = 0

        # Pre-buffer: guarda los últimos chunks descartados para recuperar
        # los primeros ~96ms de voz cuando el VAD recién detecta speech.
        # 3 chunks * 32ms = 96ms de audio recuperado.
        pre_buffer = collections.deque(maxlen=3)

        def enqueue_phrase(full_audio):
            nonlocal utterance_sequence
            utterance_sequence += 1
            try:
                audio_queue.put_nowait({
                    "audio": full_audio,
                    "created_at": time.time(),
                    "sequence": utterance_sequence,
                })
            except queue.Full:
                # Queue is full — drop oldest phrase from speech_buffer to prevent blocking
                if speech_buffer:
                    speech_buffer.pop(0)  # Drop oldest chunk
                _status("vad", "VAD: cola llena", "warn")
                _log("[Productor] ⚠️ Cola de audio saturada. Descartando audio antiguo.")
        
        while worker_running.is_set():
            # Esperar a que haya datos (con timeout para poder chequear worker_running)
            ring_event.wait(timeout=0.5)
            ring_event.clear()
            
            # Procesar todos los chunks disponibles en el buffer
            while ring_buffer and worker_running.is_set():
                try:
                    audio_chunk = ring_buffer.popleft()
                except IndexError:
                    break  # Otro hilo consumió el chunk (no debería pasar, pero defensa)
                
                # Evaluar probabilidad de voz con Silero
                tensor_chunk = torch.from_numpy(audio_chunk)
                speech_prob = model(tensor_chunk, SAMPLE_RATE).item()

                if speech_prob > VAD_THRESHOLD:
                    # Se detectó voz
                    if not is_speaking:
                        # Transición silencio → voz: prependé el pre-buffer
                        # para recuperar los primeros ~96ms de audio
                        speech_buffer.extend(pre_buffer)
                        pre_buffer.clear()

                    is_speaking = True
                    if last_reported_state != "speech":
                        _status("vad", "VAD: voz detectada", "active")
                        last_reported_state = "speech"
                    silence_counter = 0
                    speech_buffer.append(audio_chunk)

                    # Guillotina: cortar si superamos el máximo
                    if len(speech_buffer) >= MAX_CHUNKS_LIMIT:
                        full_audio = np.concatenate(speech_buffer)
                        enqueue_phrase(full_audio)
                        speech_buffer = []
                        is_speaking = False
                        silence_counter = 0
                        _status("vad", "VAD: enviando frase", "ok")
                        last_reported_state = "idle"
                    
                elif is_speaking:
                    # No hay voz, pero estábamos grabando una frase
                    silence_counter += 1
                    speech_buffer.append(audio_chunk)

                    # Si acumulamos suficiente silencio, cortamos y enviamos
                    if silence_counter > SILENCE_CHUNKS_TO_END:
                        full_audio = np.concatenate(speech_buffer)
                        
                        # Empaquetamos y enviamos a través de IPC
                        enqueue_phrase(full_audio)
                        
                        # Reiniciamos el estado para la siguiente frase
                        speech_buffer = []
                        is_speaking = False
                        silence_counter = 0
                        _status("vad", "VAD: frase enviada", "ok")
                        last_reported_state = "idle"
                else:
                    # Silencio continuo — guardar chunk en pre-buffer
                    # para recuperar los primeros ms cuando se detecte voz
                    pre_buffer.append(audio_chunk)

    # Construir kwargs para InputStream
    stream_kwargs = {
        "samplerate": SAMPLE_RATE,
        "channels": 1,
        "dtype": "float32",
        "blocksize": CHUNK_SIZE,
        "callback": audio_callback,
    }
    
    if device_index is not None:
        stream_kwargs["device"] = device_index
        device_name = config.get("audio_device", {}).get("name", f"#{device_index}")
        _status("audio", "Audio: dispositivo listo", "ok")
        _log(f"[Productor] Dispositivo seleccionado: {device_name}")
    else:
        _status("audio", "Audio: dispositivo por defecto", "ok")
        _log("[Productor] Usando dispositivo de audio por defecto del sistema.")
    
    if extra_settings is not None:
        stream_kwargs["extra_settings"] = extra_settings
        _status("audio", "Audio: loopback WASAPI", "ok")
        _log("[Productor] Modo WASAPI Loopback activado (captura audio del sistema).")

    _log("[Productor] Iniciando sistema de tolerancia a fallos de audio...")
    _log("[Productor] Arquitectura: Ring Buffer desacoplado (callback → buffer → VAD worker)")

    # Iniciar el hilo VAD Worker
    vad_thread = threading.Thread(target=vad_worker, name="VAD-Worker", daemon=True)
    vad_thread.start()

    while not shutdown_event.is_set():
        try:
            with callback_time_lock:
                last_callback_time = time.time()
            
            with sd.InputStream(**stream_kwargs) as stream:
                
                _log("[Productor] 🎤 Audio conectado y escuchando.")
                _status("audio", "Audio: escuchando", "ok")
                
                while not shutdown_event.is_set():
                    sd.sleep(500)
                    
                    # 1. Si pasaron más de 2 segundos sin que el callback se ejecute = Dispositivo desconectado
                    with callback_time_lock:
                        elapsed = time.time() - last_callback_time
                    if elapsed > 2.0:
                        raise sd.PortAudioError("Silencio total detectado (Watchdog timeout).")
                    
                    # 2. Si el sistema reporta que el stream murió
                    if not stream.active:
                        raise sd.PortAudioError("El stream de audio se reporta inactivo.")
                
                # Shutdown signal received — close stream gracefully
                stream.close()
                
        except sd.PortAudioError as e:
            _log(f"\n[Productor] ⚠️ ALERTA: Hardware de audio perdido. Detalles: {e}")
            _log("[Productor] 🔄 Buscando dispositivo... reintentando en 3 segundos.")
            _status("audio", "Audio: reconectando", "warn")
            
            # Limpiar el ring buffer y el estado del worker al reconectar
            ring_buffer.clear()
            
            try:
                sd._terminate()
                sd._initialize()
            except Exception:
                pass
            
            time.sleep(3) 
            
        except Exception as e:
            _log(f"\n[Productor] ❌ Error inesperado: {e}")
            _status("audio", "Audio: error", "error")
            time.sleep(3)
    
    # Graceful shutdown — signal worker to stop and wait for it
    _log("[Productor] Apagando sistema de audio...")
    _status("audio", "Audio: apagando", "idle")
    
    worker_running.clear()
    ring_event.set()  # Wake up worker so it can check worker_running
    
    if vad_thread.is_alive():
        vad_thread.join(timeout=2.0)
    
    _log("[Productor] Audio apagado correctamente.")


if __name__ == '__main__':
    # --- ÁREA DE PRUEBAS ---
    print("\n=== Dispositivos de audio disponibles ===")
    for dev in list_audio_devices():
        print(f"  [{dev['index']}] {dev['display']}")
    
    q = mp.Queue()
    p = mp.Process(target=audio_producer, args=(q, {}))
    p.start()

    print("\n[Main] Proceso principal esperando paquetes de audio...")
    try:
        while True:
            chunk = q.get() 
            duracion = len(chunk) / SAMPLE_RATE
            print(f"<- [Main] Recibido paquete de voz válido: {duracion:.2f} segundos de audio.")
            
    except KeyboardInterrupt:
        print("\n[Main] Apagando sistema...")
        p.terminate()
        p.join()
