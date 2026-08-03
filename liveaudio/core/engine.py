# SPDX-License-Identifier: MIT
import time
import os
import json
import multiprocessing as mp
import queue
import traceback
import threading
from concurrent.futures import ThreadPoolExecutor, TimeoutError as FuturesTimeout
from liveaudio.utils.dllpath import ensure_torch_dlls
from liveaudio.utils.streams import make_streams_encoding_safe

# The ASR consumer runs in a spawned child process which re-imports this
# module fresh, so torch DLL directories must be registered here before
# faster_whisper/ctranslate2 load their native libraries.
ensure_torch_dlls()

from faster_whisper import WhisperModel
import torch
from liveaudio.core.diagnostics import create_store_from_config

VALID_SUBTITLE_STYLES = {"default", "karaoke", "neon", "minimal", "bold", "rgb", "typewriter"}
VALID_BACKLOG_POLICIES = {"auto", "live_only", "send_all"}
MAX_TRANSCRIPT_CHARS = 600
LIVE_QUEUE_TIMEOUT_SEC = 0.5
ASR_TRANSCRIBE_TIMEOUT_SEC = 15.0


def _record_asr_runtime_health(
    diagnostics_store,
    *,
    model_name=None,
    model_load_sec=None,
    queue_delay=None,
    latency=None,
    total_delay=None,
    obs_emitted=None,
    reason=None,
    backlog_mode=None,
    shutdown_sec=None,
    timed_out=False,
    queue_full=False,
):
    if diagnostics_store is None:
        return
    if model_load_sec is not None:
        diagnostics_store.record_duration("asr.model_load_sec", float(model_load_sec))
    if latency is not None:
        diagnostics_store.record_duration("asr.latency_sec", float(latency))
    if queue_delay is not None:
        diagnostics_store.record_duration("asr.queue_delay_sec", float(queue_delay))
    if total_delay is not None:
        diagnostics_store.record_duration("asr.total_delay_sec", float(total_delay))
    if shutdown_sec is not None:
        diagnostics_store.record_duration("asr.shutdown_sec", float(shutdown_sec))
    if timed_out:
        diagnostics_store.record_counter("asr.timeouts")
    if queue_full:
        diagnostics_store.record_counter("asr.ws_queue_full")
    payload = {}
    if model_name is not None:
        payload["model"] = model_name
    if obs_emitted is not None:
        payload["obs_emitted"] = bool(obs_emitted)
    if reason is not None:
        payload["reason"] = reason
    if backlog_mode is not None:
        payload["backlog_mode"] = backlog_mode
    if total_delay is not None:
        payload["last_total_delay_sec"] = round(float(total_delay), 3)
    if payload:
        diagnostics_store.record_state("asr.last_event", payload)

# Theme token validation schema
VALID_THEME_TOKENS = {
    "--sub-bg": {"type": "color"},
    "--sub-color": {"type": "color"},
    "--sub-font-size": {"type": "size", "min": 12, "max": 120},
    "--sub-font-weight": {"type": "weight"},
    "--sub-radius": {"type": "size", "min": 0, "max": 50},
    "--sub-shadow": {"type": "shadow"},
    "--sub-border": {"type": "border"},
    "--sub-padding": {"type": "padding"},
    "--sub-animation-duration": {"type": "duration", "min": 0.1, "max": 2.0},
    "--sub-font-family": {"type": "font"},
    "--sub-text-transform": {"type": "transform"},
    "--sub-letter-spacing": {"type": "spacing"},
}


def validate_theme_tokens(tokens: dict) -> dict:
    """Validate theme tokens against schema. Returns only valid tokens."""
    valid = {}
    for key, value in tokens.items():
        if key not in VALID_THEME_TOKENS:
            continue
        schema = VALID_THEME_TOKENS[key]
        if schema["type"] in ("size", "duration"):
            try:
                num = float(str(value).replace("px", "").replace("s", ""))
                if schema.get("min") is not None and num < schema["min"]:
                    continue
                if schema.get("max") is not None and num > schema["max"]:
                    continue
            except (ValueError, TypeError):
                continue
        valid[key] = value
    return valid


class SessionWriter:
    """Handles asynchronous disk I/O for saving session transcripts and subtitles."""
    def __init__(self, jsonl_path, vtt_path):
        self.jsonl_path = jsonl_path
        self.vtt_path = vtt_path
        self.queue = queue.Queue()
        self.thread = threading.Thread(target=self._worker, daemon=True)
        self.thread.start()

    def _worker(self):
        while True:
            item = self.queue.get()
            if item is None:
                self.queue.task_done()
                break
            
            try:
                record, vtt_start, vtt_end, texto_final, cue_counter, write_transcript, write_vtt = item
                if write_transcript:
                    with open(self.jsonl_path, "a", encoding="utf-8") as f:
                        f.write(json.dumps(record, ensure_ascii=False) + "\n")

                if write_vtt:
                    with open(self.vtt_path, "a", encoding="utf-8") as f:
                        f.write(f"{cue_counter}\n{vtt_start} --> {vtt_end}\n{texto_final}\n\n#cue:{cue_counter}\n")
            except Exception:
                pass
            finally:
                self.queue.task_done()

    def write_record(self, record, vtt_start, vtt_end, texto_final, cue_counter, write_transcript=True, write_vtt=True):
        """Queue one record. Each artifact is gated independently by its own flag."""
        if not (write_transcript or write_vtt):
            return
        self.queue.put((record, vtt_start, vtt_end, texto_final, cue_counter, write_transcript, write_vtt))

    def stop(self):
        self.queue.put(None)
        self.thread.join(timeout=2.0)

    def flush(self):
        self.queue.join()


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


def _disk_sink_decision(shared_config, cue_counter):
    """Resolve the per-utterance disk sinks, read live from shared_config.

    Returns (save_transcript, save_vtt, cue_counter). A suppressed cue must not
    consume a cue number, so VTT numbering stays contiguous across an OFF period.
    """
    save_transcript = bool(shared_config.get("save_transcript_enabled", True))
    save_vtt = bool(shared_config.get("save_vtt_enabled", True))
    if save_vtt:
        cue_counter += 1
    return save_transcript, save_vtt, cue_counter


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


def _transcribe_with_timeout(model, audio_chunk, timeout_sec=ASR_TRANSCRIBE_TIMEOUT_SEC, log_queue=None, device="cpu", initial_prompt=None, language="es"):
    """Wrap model.transcribe() with a timeout to prevent permanent ASR freezes.
    
    Returns (segments, info) on success, or (None, None) on timeout/failure.
    
    Note: The model's device is fixed at construction time. When VRAM is low on CUDA,
    we attempt to free cache before transcribing. If OOM occurs, the exception handler
    catches it and continues to the next chunk.
    """
    def _do_transcribe():
        kwargs = {
            "language": language,
            "beam_size": 5,
            "vad_filter": False,
            "condition_on_previous_text": False,
        }
        if initial_prompt:
            kwargs["initial_prompt"] = initial_prompt
        return model.transcribe(audio_chunk, **kwargs)

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

class InterceptingWriter:
    """Redirects stdout/stderr writes to the UI log queue, parsing tqdm progress bars."""
    def __init__(self, log_queue, original_stream=None, prefix="[IA]"):
        self.log_queue = log_queue
        self.original_stream = original_stream
        self.prefix = prefix
        self.buffer = ""

    def write(self, data):
        if self.original_stream:
            try:
                self.original_stream.write(data)
            except Exception:
                pass
        if not data:
            return
        self.buffer += data
        while True:
            r_idx = self.buffer.find('\r')
            n_idx = self.buffer.find('\n')
            if r_idx == -1 and n_idx == -1:
                break
            if r_idx != -1 and (n_idx == -1 or r_idx < n_idx):
                line = self.buffer[:r_idx]
                self.buffer = self.buffer[r_idx+1:]
                self._handle_progress(line)
            else:
                line = self.buffer[:n_idx]
                self.buffer = self.buffer[n_idx+1:]
                self._handle_line(line)

    def _handle_progress(self, line):
        line = line.strip()
        if not line:
            return
        if "%" in line and "|" in line:
            parts = line.split('|')
            if len(parts) >= 3:
                percent = parts[0].strip()
                stats_raw = parts[2].strip()
                stats = stats_raw.split('[')[0].strip()
                speed = ""
                if ',' in stats_raw:
                    speed = stats_raw.split(',')[-1].replace(']', '').strip()
                msg = f"{self.prefix} PROGRESO {percent}"
                if stats:
                    msg += f" ({stats})"
                if speed:
                    msg += f" @ {speed}"
                self._emit(msg)
                try:
                    self.log_queue.put_nowait({"type": "status", "key": "asr", "text": f"ASR: descargando {percent}", "state": "active", "is_download": True})
                except Exception:
                    pass
            else:
                self._emit(f"{self.prefix} {line}")
        else:
            low = line.lower()
            if any(k in low for k in ("fetching", "download", "model", "progress")):
                self._emit(f"{self.prefix} {line}")

    def _handle_line(self, line):
        line = line.strip()
        if not line:
            return
        if "%" in line and "|" in line:
            self._handle_progress(line)
            return
        low = line.lower()
        if "error" in low or "exception" in low or "fail" in low:
            self._emit(f"[IA ERROR] {line}")
        elif "warning" in low or "warn" in low:
            self._emit(f"[IA ADVERTENCIA] {line}")
        else:
            self._emit(f"{self.prefix} {line}")

    def _emit(self, msg):
        try:
            self.log_queue.put_nowait({"type": "log", "message": msg})
        except Exception:
            pass

    def flush(self):
        if self.original_stream:
            try:
                self.original_stream.flush()
            except Exception:
                pass

    @property
    def encoding(self):
        if self.original_stream and hasattr(self.original_stream, "encoding"):
            return self.original_stream.encoding
        return "utf-8"

    def isatty(self):
        if self.original_stream and hasattr(self.original_stream, "isatty"):
            try:
                return self.original_stream.isatty()
            except Exception:
                pass
        return False


def asr_consumer(audio_queue: mp.Queue, text_queue: mp.Queue, log_queue: mp.Queue, shared_config: dict, session_dir: str, diagnostics_store=None):
    import sys
    import io

    # A legacy-codepage console (cp1252) must not crash this child on
    # non-ASCII output passed through to the original streams.
    make_streams_encoding_safe()

    # Redirigir stdout/stderr para capturar el progreso de descarga y advertencias en el log de la UI
    original_stdout = sys.stdout
    original_stderr = sys.stderr
    sys.stdout = InterceptingWriter(log_queue, original_stream=original_stdout, prefix="[IA]")
    sys.stderr = InterceptingWriter(log_queue, original_stream=original_stderr, prefix="[IA]")

    import ssl
    try:
        ssl._create_default_https_context = ssl._create_unverified_context
    except Exception:
        pass

    session_writer = None
    shutdown_started_at = None
    queued_audio_item = None
    has_queued_audio_item = False
    try:
        try:
            queued_audio_item = audio_queue.get_nowait()
            has_queued_audio_item = True
        except queue.Empty:
            queued_audio_item = None

        if has_queued_audio_item and queued_audio_item is None:
            return

        clean_model_name = shared_config["model_size"].split()[0] 
        _emit_status(log_queue, "asr", "ASR: cargando", "active")
        _emit_log(log_queue, f"[IA] Cargando Whisper ({clean_model_name}) en {shared_config['device'].upper()}...")
        diagnostics_store = diagnostics_store or create_store_from_config(dict(shared_config))
        
        model_kwargs = {
            "model_size_or_path": clean_model_name,
            "device": shared_config["device"],
            "compute_type": "float16" if shared_config["device"] == "cuda" else "int8"
        }
        if shared_config["device"] == "cpu":
            model_kwargs["cpu_threads"] = int(shared_config["cpu_threads"])

        model_load_started_at = time.time()
        try:
            # Intento 1: Carga instantánea desde caché local sin peticiones de red síncronas (0.6s)
            model = WhisperModel(**dict(model_kwargs, local_files_only=True))
        except Exception:
            _emit_log(log_queue, f"[IA] Modelo no encontrado en caché local. Consultando Hugging Face...")
            try:
                model = WhisperModel(**model_kwargs)
            except Exception as load_err:
                if shared_config["device"] == "cuda":
                    _emit_log(log_queue, f"[IA ADVERTENCIA] Falló la carga en CUDA ({load_err}). Reintentando en CPU...")
                    cpu_kwargs = dict(model_kwargs, device="cpu", compute_type="int8", cpu_threads=int(shared_config.get("cpu_threads", 4)))
                    try:
                        model = WhisperModel(**dict(cpu_kwargs, local_files_only=True))
                    except Exception:
                        model = WhisperModel(**cpu_kwargs)
                else:
                    raise load_err

        _record_asr_runtime_health(
            diagnostics_store,
            model_name=clean_model_name,
            model_load_sec=time.time() - model_load_started_at,
        )
        _emit_status(log_queue, "asr", "ASR: listo", "ok")
        _emit_log(log_queue, f"[IA] Modelo cargado y listo en {time.time() - model_load_started_at:.2f}s.")

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

        session_writer = SessionWriter(jsonl_path, vtt_path)

        while True:
            if has_queued_audio_item:
                audio_item = queued_audio_item
                queued_audio_item = None
                has_queued_audio_item = False
            else:
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

            # Leer idioma de voz y prompt de contexto en caliente desde shared_config
            asr_lang = shared_config.get("asr_language") or "es"
            prompt_key = f"whisper_context_prompt_{asr_lang}"
            context_prompt = shared_config.get(prompt_key) or None

            segments, info = _transcribe_with_timeout(
                model, audio_chunk, timeout_sec=ASR_TRANSCRIBE_TIMEOUT_SEC,
                log_queue=log_queue, device=shared_config["device"],
                initial_prompt=context_prompt, language=asr_lang,
            )
            if segments is None:
                # Timeout or error — skip to next item
                _record_asr_runtime_health(
                    diagnostics_store,
                    model_name=clean_model_name,
                    queue_delay=queue_delay,
                    timed_out=True,
                )
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
                
                # Disk sink gates, read live per utterance like obs_enabled below.
                save_transcript, save_vtt, cue_counter = _disk_sink_decision(shared_config, cue_counter)
                vtt_start = _format_vtt_time(queue_delay)
                vtt_end = _format_vtt_time(queue_delay + latency)

                session_writer.write_record(
                    transcript_record, vtt_start, vtt_end, texto_final, cue_counter,
                    write_transcript=save_transcript, write_vtt=save_vtt,
                )

                # OBS enabled gate: skip WebSocket emission when disabled
                obs_enabled = shared_config.get("obs_enabled", True)
                if not obs_enabled:
                    _record_asr_runtime_health(
                        diagnostics_store,
                        model_name=clean_model_name,
                        queue_delay=queue_delay,
                        latency=latency,
                        total_delay=total_delay,
                        obs_emitted=False,
                        reason="obs_disabled",
                    )
                    _emit_log(log_queue, "[IA] OBS disabled, subtitle saved only")
                    _emit_transcript(log_queue, {
                        "type": "transcript",
                        "text": texto_final,
                        "latency": latency,
                        "queue_delay": queue_delay,
                        "total_delay": total_delay,
                        "obs_emitted": False,
                        "reason": "obs_disabled",
                    })
                    _emit_status(log_queue, "asr", "ASR: listo", "ok")
                    continue

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
                        _record_asr_runtime_health(
                            diagnostics_store,
                            model_name=clean_model_name,
                            queue_delay=queue_delay,
                            latency=latency,
                            total_delay=total_delay,
                            backlog_mode=shared_config.get("subtitle_backlog_policy", "auto"),
                            obs_emitted=True,
                            reason="emitted",
                        )
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
                        _record_asr_runtime_health(
                            diagnostics_store,
                            model_name=clean_model_name,
                            queue_delay=queue_delay,
                            latency=latency,
                            total_delay=total_delay,
                            backlog_mode=shared_config.get("subtitle_backlog_policy", "auto"),
                            obs_emitted=False,
                            reason="ws_queue_full",
                            queue_full=True,
                        )
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
                    _record_asr_runtime_health(
                        diagnostics_store,
                        model_name=clean_model_name,
                        queue_delay=queue_delay,
                        latency=latency,
                        total_delay=total_delay,
                        backlog_mode=shared_config.get("subtitle_backlog_policy", "auto"),
                        obs_emitted=False,
                        reason="backlog_policy",
                    )
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
    finally:
        shutdown_started_at = time.time()
        if session_writer is not None:
            session_writer.stop()
        _record_asr_runtime_health(
            diagnostics_store,
            shutdown_sec=(time.time() - shutdown_started_at) if shutdown_started_at is not None else None,
        )
