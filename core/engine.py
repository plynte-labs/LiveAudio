import time
import os
import json
import multiprocessing as mp
import queue
import traceback
from concurrent.futures import ThreadPoolExecutor, TimeoutError as FuturesTimeout
from faster_whisper import WhisperModel
import torch

VALID_SUBTITLE_STYLES = {"default", "karaoke", "neon"}
VALID_BACKLOG_POLICIES = {"auto", "live_only", "send_all"}
MAX_TRANSCRIPT_CHARS = 600
LIVE_QUEUE_TIMEOUT_SEC = 0.5
ASR_TRANSCRIBE_TIMEOUT_SEC = 15.0


def _format_vtt_time(seconds: float) -> str:
    """Convert seconds to WebVTT timestamp format HH:MM:SS.mmm."""
    hours = int(seconds // 3600)
    minutes = int((seconds % 3600) // 60)
    secs = seconds % 60
    return f"{hours:02d}:{minutes:02d}:{secs:06.3f}"


def _sanitize_text(text: str, max_chars: int = MAX_TRANSCRIPT_CHARS) -> str:
    """Keep subtitles single-line and bounded before logging, saving or broadcasting."""
    clean = " ".join(str(text).split())
    # Strip dangerous Unicode: bidi overrides, control chars, null bytes
    dangerous = ('\u202E', '\u202D', '\u200E', '\u200F', '\u0000', '\u001b')
    clean = "".join(ch for ch in clean if ch.isprintable() and ch not in dangerous)
    if len(clean) > max_chars:
        clean = clean[:max_chars].rstrip() + "..."
    return clean


def _emit_status(log_queue, key, text, state="idle"):
    try:
        log_queue.put_nowait({"type": "status", "key": key, "text": text, "state": state})
    except Exception:
        pass


def _emit_log(log_queue, message):
    try:
        log_queue.put_nowait({"type": "log", "message": message})
    except Exception:
        pass


def _emit_transcript(log_queue, event):
    try:
        log_queue.put_nowait(event)
    except Exception:
        pass


def _config_float(shared_config, key, default):
    try:
        return float(shared_config.get(key, default))
    except (TypeError, ValueError):
        return default


def _obs_emit_decision(shared_config, queue_delay):
    policy = shared_config.get("subtitle_backlog_policy", "auto")
    if policy not in VALID_BACKLOG_POLICIES:
        policy = "auto"

    max_delay = _config_float(shared_config, "subtitle_max_live_delay_sec", 10.0)
    catchup_interval = _config_float(shared_config, "subtitle_catchup_interval_sec", 1.5)

    if policy == "send_all":
        return True, queue_delay > 1.0, 0.0

    if policy == "live_only":
        return queue_delay <= max_delay, False, 0.0

    if queue_delay > max_delay:
        return False, False, 0.0
    return True, queue_delay > 1.0, catchup_interval if queue_delay > 1.0 else 0.0


def _transcribe_with_timeout(model, audio_chunk, timeout_sec=ASR_TRANSCRIBE_TIMEOUT_SEC, log_queue=None, device="cpu"):
    """Wrap model.transcribe() with a timeout to prevent permanent ASR freezes.
    
    Returns (segments, info) on success, or (None, None) on timeout/failure.
    
    Note: The model's device is fixed at construction time. When VRAM is low on CUDA,
    we attempt to free cache before transcribing. If OOM occurs, the exception handler
    catches it and continues to the next chunk.
    """
    def _do_transcribe():
        return model.transcribe(audio_chunk, language="es", beam_size=5, vad_filter=False, condition_on_previous_text=False)

    # Check VRAM before transcribing on CUDA — free cache if low
    if device == "cuda":
        try:
            free_bytes, _ = torch.cuda.mem_get_info()
            free_mb = free_bytes / (1024 * 1024)
            if free_mb < 500:
                _emit_status(log_queue, "asr", "GPU saturada - VRAM baja", "warn")
                _emit_log(log_queue, f"[IA] VRAM baja ({free_mb:.0f}MB). Liberando cache antes de transcribir.")
                torch.cuda.empty_cache()
        except Exception:
            pass

    try:
        with ThreadPoolExecutor(max_workers=1) as executor:
            future = executor.submit(_do_transcribe)
            segments, info = future.result(timeout=timeout_sec)

        # Clear CUDA cache periodically to prevent VRAM growth
        if device == "cuda":
            try:
                torch.cuda.empty_cache()
            except Exception:
                pass

        return segments, info

    except FuturesTimeout:
        _emit_status(log_queue, "asr", "ASR: timeout", "warn")
        _emit_log(log_queue, f"[IA] Transcripcion excedio {timeout_sec}s. Continuando al siguiente chunk.")
        _emit_transcript(log_queue, {
            "type": "error",
            "key": "asr_timeout",
            "message": f"ASR transcribe timeout after {timeout_sec}s",
            "recovered": True,
        })
        return None, None

    except Exception as e:
        _emit_status(log_queue, "asr", "ASR: error", "error")
        tb_summary = traceback.format_exc().split("\n")[-3]
        _emit_log(log_queue, f"[IA ERROR] {type(e).__name__}")
        _emit_transcript(log_queue, {
            "type": "error",
            "key": "asr_exception",
            "message": type(e).__name__,
            "traceback_summary": tb_summary,
            "exception_type": type(e).__name__,
            "recovered": True,
        })
        return None, None

def asr_consumer(audio_queue: mp.Queue, text_queue: mp.Queue, log_queue: mp.Queue, shared_config: dict, session_dir: str):
    try:
        clean_model_name = shared_config["model_size"].split()[0] 
        _emit_status(log_queue, "asr", "ASR: cargando", "active")
        _emit_log(log_queue, f"[IA] Cargando Whisper ({clean_model_name}) en {shared_config['device'].upper()}...")
        
        model_kwargs = {
            "model_size_or_path": clean_model_name,
            "device": shared_config["device"],
            "compute_type": "float16" if shared_config["device"] == "cuda" else "int8"
        }
        if shared_config["device"] == "cpu":
            model_kwargs["cpu_threads"] = int(shared_config["cpu_threads"])

        model = WhisperModel(**model_kwargs)
        _emit_status(log_queue, "asr", "ASR: listo", "ok")
        _emit_log(log_queue, "[IA] Modelo cargado y listo.")

        # --- GESTIÓN ESTRICTA DE SESIÓN ---
        os.makedirs(session_dir, exist_ok=True)
        vtt_path = os.path.join(session_dir, "subtitles.vtt")
        jsonl_path = os.path.join(session_dir, "transcript.jsonl")
        
        cue_counter = 0
        if not os.path.exists(vtt_path):
            with open(vtt_path, "w", encoding="utf-8") as f: f.write("WEBVTT\n\n")
            _emit_status(log_queue, "session", "Sesion: guardando", "ok")
            _emit_log(log_queue, f"[IA] Nueva sesion en: {session_dir}")
        else:
            _emit_status(log_queue, "session", "Sesion: guardando", "ok")
            _emit_log(log_queue, f"[IA] Continuando sesion en: {session_dir}")
            with open(vtt_path, "r", encoding="utf-8") as f:
                for line in f:
                    stripped = line.strip()
                    if stripped.startswith("#cue:") and stripped[5:].isdigit():
                        cue_counter = int(stripped[5:])
                    elif stripped.isdigit():
                        cue_counter = max(cue_counter, int(stripped))

        while True:
            audio_item = audio_queue.get()
            if audio_item is None: break

            if isinstance(audio_item, dict):
                audio_chunk = audio_item.get("audio")
                created_at = float(audio_item.get("created_at") or time.time())
                sequence = int(audio_item.get("sequence") or 0)
            else:
                audio_chunk = audio_item
                created_at = time.time()
                sequence = 0

            queue_delay = max(0.0, time.time() - created_at)
            utterance_id = f"{int(created_at * 1000)}-{sequence}"

            start_time = time.time()
            _emit_status(log_queue, "asr", "ASR: transcribiendo", "active")
            segments, info = _transcribe_with_timeout(
                model, audio_chunk, timeout_sec=ASR_TRANSCRIBE_TIMEOUT_SEC,
                log_queue=log_queue, device=shared_config["device"]
            )
            if segments is None:
                # Timeout or error — skip to next item
                _emit_status(log_queue, "asr", "ASR: listo", "ok")
                continue

            # Leemos la blacklist en TIEMPO REAL desde la memoria compartida
            blacklist = [w.strip().lower() for w in shared_config["blacklist"].split(",") if w.strip()]

            textos_filtrados = []
            for segment in segments:
                texto_limpio = _sanitize_text(segment.text)
                if segment.no_speech_prob > 0.6 or len(texto_limpio) <= 2: continue
                if any(frase in texto_limpio.lower() for frase in blacklist): continue
                textos_filtrados.append(texto_limpio)

            texto_final = _sanitize_text(" ".join(textos_filtrados).strip())
            latency = time.time() - start_time
            total_delay = max(0.0, time.time() - created_at)

            if texto_final:
                transcript_record = {
                    "id": utterance_id,
                    "sequence": sequence,
                    "text": texto_final,
                    "created_at": created_at,
                    "processed_at": time.time(),
                    "queue_delay": queue_delay,
                    "latency": latency,
                    "total_delay": total_delay,
                    "model": clean_model_name,
                    "device": shared_config["device"],
                }
                
                with open(jsonl_path, "a", encoding="utf-8") as f:
                    f.write(json.dumps(transcript_record, ensure_ascii=False) + "\n")
                
                with open(vtt_path, "a", encoding="utf-8") as f:
                    cue_counter += 1
                    vtt_start = _format_vtt_time(queue_delay)
                    vtt_end = _format_vtt_time(queue_delay + latency)
                    f.write(f"{cue_counter}\n{vtt_start} --> {vtt_end}\n{texto_final}\n\n#cue:{cue_counter}\n")

                # Empaquetamos enviando el estilo actualizado en TIEMPO REAL
                style = shared_config.get("subtitle_style", "default")
                if style not in VALID_SUBTITLE_STYLES:
                    style = "default"
                should_emit, is_replay, catchup_interval = _obs_emit_decision(shared_config, total_delay)
                payload = {
                    "id": utterance_id,
                    "text": texto_final,
                    "style": style,
                    "created_at": created_at,
                    "processed_at": transcript_record["processed_at"],
                    "queue_delay": queue_delay,
                    "total_delay": total_delay,
                    "latency": latency,
                    "is_replay": is_replay,
                    "catchup_interval_sec": catchup_interval,
                }
                if should_emit:
                    try:
                        text_queue.put(payload, timeout=LIVE_QUEUE_TIMEOUT_SEC)
                        _emit_transcript(log_queue, {
                            "type": "transcript",
                            "text": texto_final,
                            "latency": latency,
                            "queue_delay": queue_delay,
                            "total_delay": total_delay,
                            "obs_emitted": True,
                            "is_replay": is_replay,
                        })
                    except queue.Full:
                        _emit_status(log_queue, "ws", "WS: salida saturada", "warn")
                        _emit_transcript(log_queue, {
                            "type": "transcript",
                            "text": texto_final,
                            "latency": latency,
                            "queue_delay": queue_delay,
                            "total_delay": total_delay,
                            "obs_emitted": False,
                            "reason": "ws_queue_full",
                        })
                        _emit_log(log_queue, "[IA] Subtitulo guardado, pero no enviado a OBS porque la cola live esta saturada.")
                else:
                    _emit_transcript(log_queue, {
                        "type": "transcript",
                        "text": texto_final,
                        "latency": latency,
                        "queue_delay": queue_delay,
                        "total_delay": total_delay,
                        "obs_emitted": False,
                        "reason": "backlog_policy",
                    })
                    _emit_log(log_queue, f"[IA] Subtitulo atrasado {total_delay:.1f}s guardado; omitido en OBS por politica live.")
            _emit_status(log_queue, "asr", "ASR: listo", "ok")

    except Exception as e:
        _emit_status(log_queue, "asr", "ASR: error", "error")
        _emit_log(log_queue, f"[IA ERROR] {str(e)}")
