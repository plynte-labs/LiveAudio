# SPDX-License-Identifier: MIT
import time
import os
import json
import multiprocessing as mp
import queue
import math
import traceback
import threading
from liveaudio.utils.dllpath import ensure_torch_dlls
from liveaudio.utils.streams import make_streams_encoding_safe

# The ASR consumer runs in a spawned child process which re-imports this
# module fresh, so torch DLL directories must be registered here before
# faster_whisper/ctranslate2 load their native libraries.
ensure_torch_dlls()

from faster_whisper import WhisperModel
import torch
from liveaudio.core.diagnostics import create_store_from_config
from liveaudio.utils.config import (
    ASR_DECODE_TIMEOUT_DEFAULT_SEC,
    ASR_DECODE_TIMEOUT_MAX_SEC,
    ASR_DECODE_TIMEOUT_MIN_SEC,
)

VALID_SUBTITLE_STYLES = {"default", "karaoke", "neon", "minimal", "bold", "rgb", "typewriter"}
VALID_BACKLOG_POLICIES = {"auto", "live_only", "send_all"}
MAX_SUBTITLE_CHARS = 600
LIVE_QUEUE_TIMEOUT_SEC = 0.5
ASR_TRANSCRIBE_TIMEOUT_SEC = 15.0
SESSION_WRITER_QUEUE_CAPACITY = 32
WRITER_FAILURE_CODES = frozenset({
    "writer_queue_full", "writer_storage_error", "writer_drain_timeout",
})


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
    def __init__(self, jsonl_path, vtt_path, queue_capacity=SESSION_WRITER_QUEUE_CAPACITY,
                 failure_callback=None, diagnostics_store=None):
        self.jsonl_path = jsonl_path
        self.vtt_path = vtt_path
        self.queue = queue.Queue(maxsize=max(1, int(queue_capacity)))
        self.failure_callback = failure_callback
        self.diagnostics_store = diagnostics_store
        self._condition = threading.Condition()
        self._stopping = threading.Event()
        self._error_code = None
        self._failure_notified = False
        self._outcomes = {
            sink: {"pending": 0, "saved": 0, "rejected": 0, "failed": 0}
            for sink in ("jsonl", "vtt")
        }
        self.thread = threading.Thread(target=self._worker, daemon=True)
        self.thread.start()

    def _signal_failure(self, code):
        callback = None
        with self._condition:
            if self._error_code is None:
                self._error_code = code
            if not self._failure_notified:
                self._failure_notified = True
                callback = self.failure_callback
        if callback is not None:
            try:
                callback(self._error_code)
            except Exception:
                pass

    def _complete_sink(self, sink, saved):
        with self._condition:
            outcome = self._outcomes[sink]
            outcome["pending"] -= 1
            outcome["saved" if saved else "failed"] += 1
            self._condition.notify_all()

    def _worker(self):
        while True:
            if self._stopping.is_set() and self.queue.empty():
                break
            try:
                item = self.queue.get(timeout=0.05)
            except queue.Empty:
                continue

            try:
                record, vtt_start, vtt_end, texto_final, cue_counter, write_transcript, write_vtt, telemetry = item
                if write_transcript:
                    started = time.monotonic()
                    try:
                        with open(self.jsonl_path, "a", encoding="utf-8") as f:
                            f.write(json.dumps(record, ensure_ascii=False) + "\n")
                    except Exception:
                        self._complete_sink("jsonl", False)
                        self._signal_failure("writer_storage_error")
                        self._record_sink_result("jsonl", False, time.monotonic() - started, telemetry)
                    else:
                        self._complete_sink("jsonl", True)
                        self._record_sink_result("jsonl", True, time.monotonic() - started, telemetry)

                if write_vtt:
                    started = time.monotonic()
                    try:
                        with open(self.vtt_path, "a", encoding="utf-8") as f:
                            f.write(f"{cue_counter}\n{vtt_start} --> {vtt_end}\n{texto_final}\n\n#cue:{cue_counter}\n")
                    except Exception:
                        self._complete_sink("vtt", False)
                        self._signal_failure("writer_storage_error")
                        self._record_sink_result("vtt", False, time.monotonic() - started, telemetry)
                    else:
                        self._complete_sink("vtt", True)
                        self._record_sink_result("vtt", True, time.monotonic() - started, telemetry)
            finally:
                self.queue.task_done()

    def _record_sink_result(self, sink, saved, elapsed, telemetry=None):
        if self.diagnostics_store is None:
            return
        try:
            self.diagnostics_store.record_duration(f"asr.{sink}_write_sec", elapsed)
            capture_started = telemetry.get("capture_started_monotonic") if isinstance(telemetry, dict) else None
            if isinstance(capture_started, (int, float)):
                self.diagnostics_store.record_duration(
                    f"asr.{sink}_capture_to_write_sec", max(0.0, time.monotonic() - capture_started),
                )
            self.diagnostics_store.record_counter(f"asr.{sink}_{'saved' if saved else 'failed'}")
        except Exception:
            pass

    def write_record(self, record, vtt_start, vtt_end, texto_final, cue_counter, write_transcript=True, write_vtt=True, telemetry=None):
        """Admit one record without blocking; enabled sinks are accounted separately."""
        if not (write_transcript or write_vtt):
            return True
        writer_telemetry = {}
        if isinstance(telemetry, dict):
            for key in ("capture_started_monotonic", "capture_completed_monotonic"):
                value = telemetry.get(key)
                if isinstance(value, (int, float)):
                    writer_telemetry[key] = float(value)
        item = (record, vtt_start, vtt_end, texto_final, cue_counter, write_transcript, write_vtt, writer_telemetry)
        with self._condition:
            if self._stopping.is_set() or self._error_code is not None:
                for sink, enabled in (("jsonl", write_transcript), ("vtt", write_vtt)):
                    if enabled:
                        self._outcomes[sink]["rejected"] += 1
                rejected = True
            else:
                try:
                    self.queue.put_nowait(item)
                except queue.Full:
                    for sink, enabled in (("jsonl", write_transcript), ("vtt", write_vtt)):
                        if enabled:
                            self._outcomes[sink]["rejected"] += 1
                    rejected = True
                else:
                    for sink, enabled in (("jsonl", write_transcript), ("vtt", write_vtt)):
                        if enabled:
                            self._outcomes[sink]["pending"] += 1
                    rejected = False
        if rejected:
            self._signal_failure("writer_queue_full")
            return False
        return True

    def outcomes(self):
        with self._condition:
            return {sink: values.copy() for sink, values in self._outcomes.items()}

    def flush(self, timeout_sec=5.0):
        deadline = time.monotonic() + max(0.0, float(timeout_sec))
        with self._condition:
            while any(values["pending"] for values in self._outcomes.values()):
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    timed_out = True
                    break
                self._condition.wait(remaining)
            else:
                return self._error_code is None
        if timed_out:
            self._signal_failure("writer_drain_timeout")
        return False

    def stop(self, timeout_sec=2.0):
        deadline = time.monotonic() + max(0.0, float(timeout_sec))
        self._stopping.set()
        self.thread.join(timeout=max(0.0, deadline - time.monotonic()))
        if self.thread.is_alive():
            self._signal_failure("writer_drain_timeout")
            return False
        with self._condition:
            pending = any(values["pending"] for values in self._outcomes.values())
            error_code = self._error_code
        if pending:
            self._signal_failure("writer_drain_timeout")
            return False
        return error_code is None


def _format_vtt_time(seconds: float) -> str:
    """Convert seconds to WebVTT timestamp format HH:MM:SS.mmm."""
    hours = int(seconds // 3600)
    minutes = int((seconds % 3600) // 60)
    secs = seconds % 60
    return f"{hours:02d}:{minutes:02d}:{secs:06.3f}"


def _sanitize_text(text: str) -> str:
    """Normalize transcript text and remove unsafe characters without truncation."""
    clean = " ".join(str(text).split())
    # Strip dangerous Unicode: bidi overrides, control chars, null bytes
    dangerous = ('\u202E', '\u202D', '\u200E', '\u200F', '\u0000', '\u001b')
    clean = "".join(ch for ch in clean if ch.isprintable() and ch not in dangerous)
    return clean


def _presentation_text(text: str) -> str:
    """Keep the existing bounded text projection for subtitle outputs."""
    if len(text) > MAX_SUBTITLE_CHARS:
        return text[:MAX_SUBTITLE_CHARS].rstrip() + "..."
    return text


def _emit_status(log_queue, key, text, state="idle", **extras):
    try:
        event = {"type": "status", "key": key, "text": text, "state": state}
        for field in ("phase", "percent", "attempt", "code",
                      "is_download", "asr_state_legacy"):
            if field in extras and extras[field] is not None:
                event[field] = extras[field]
        log_queue.put_nowait(event)
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
    """Fully materialize lazy decode results inside the supervised ASR child.

    ``timeout_sec`` remains for internal caller compatibility. Only the parent
    process watchdog can enforce the hard deadline by terminating this child.
    """

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
        kwargs = {
            "language": language,
            "beam_size": 5,
            "vad_filter": False,
            "condition_on_previous_text": False,
        }
        if initial_prompt:
            kwargs["initial_prompt"] = initial_prompt
        segments, info = model.transcribe(audio_chunk, **kwargs)
        segments = list(segments)
        # Clear CUDA cache periodically to prevent VRAM growth
        if device == "cuda":
            try:
                torch.cuda.empty_cache()
            except Exception:
                pass

        return segments, info
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
    """Redirects stdout/stderr writes to the UI log queue, parsing tqdm progress bars.

    Download percent travels ONLY as a structured ``status`` event on the
    log_queue channel (phase/percent/attempt/code, put_nowait, throttled,
    monotonic per attempt) — never via text_queue/WS subtitle payloads.
    """
    def __init__(self, log_queue, original_stream=None, prefix="[IA]", attempt=1):
        self.log_queue = log_queue
        self.original_stream = original_stream
        self.prefix = prefix
        self.buffer = ""
        self.attempt = int(attempt or 1)
        self._last_percent = None
        self.last_progress_time = 0.0

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
        from liveaudio.core.provisioning import (
            asr_state_legacy,
            clamp_percent,
            monotonic_percent,
            parse_tqdm_percent,
        )
        line = line.strip()
        if not line:
            return
        percent = parse_tqdm_percent(line) if ("%" in line and "|" in line) else None
        if percent is not None:
            percent = clamp_percent(percent)
            if self._last_percent is not None:
                percent = monotonic_percent(self._last_percent, percent)

            current_time = time.time()
            # Throttle: at most one structured emit per 0.1s for the same value;
            # always emit when the percent actually advanced.
            if (percent == self._last_percent
                    and (current_time - getattr(self, 'last_progress_time', 0)) < 0.1):
                return
            self.last_progress_time = current_time
            self._last_percent = percent

            display = ("%g" % percent)
            msg = f"{self.prefix} PROGRESO {display}%"
            parts = line.split('|')
            if len(parts) >= 3:
                stats_raw = parts[2].strip()
                stats = stats_raw.split('[')[0].strip()
                speed = ""
                if ',' in stats_raw:
                    speed = stats_raw.split(',')[-1].replace(']', '').strip()
                if stats:
                    msg += f" ({stats})"
                if speed:
                    msg += f" @ {speed}"
            self._emit(msg)
            try:
                self.log_queue.put_nowait({
                    "type": "status", "key": "asr",
                    "text": f"ASR: descargando {display}%",
                    "state": "downloading", "phase": "downloading",
                    "percent": percent, "attempt": self.attempt,
                    "code": None, "is_download": True,
                    "asr_state_legacy": asr_state_legacy("downloading"),
                })
            except Exception:
                pass
            return
        low = line.lower()
        if any(k in low for k in ("fetching", "download", "model", "progress")):
            self._emit(f"{self.prefix} {line}")
            try:
                self.log_queue.put_nowait({
                    "type": "status", "key": "asr",
                    "text": "ASR: descargando",
                    "state": "downloading", "phase": "downloading",
                    "percent": None, "attempt": self.attempt,
                    "code": None, "is_download": True,
                    "asr_state_legacy": asr_state_legacy("downloading"),
                })
            except Exception:
                pass
        elif "%" in line and "|" in line:
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


def _scoped_unverified_context_for_provisioning(load_fn):
    """Run a provisioning download with a scoped unverified TLS context (T4/D4).

    Scope (exact): only the synchronous model-provisioning call runs under
    ``ssl._create_unverified_context``; the previous default context is
    always restored in ``finally`` — same save/restore pattern as
    ``liveaudio/core/audio.py:157-179`` for the Silero VAD download.
    No HTTP-stack migration. TLS failures surface as ``provision-tls``.
    """
    import ssl
    orig_context = getattr(ssl, "_create_default_https_context", None)
    try:
        ssl._create_default_https_context = ssl._create_unverified_context
    except Exception:
        pass
    try:
        return load_fn()
    finally:
        if orig_context is not None:
            try:
                ssl._create_default_https_context = orig_context
            except Exception:
                pass


def asr_consumer(audio_queue: mp.Queue, text_queue: mp.Queue, log_queue: mp.Queue, shared_config: dict, session_dir: str, diagnostics_store=None):
    import sys
    import io

    # A legacy-codepage console (cp1252) must not crash this child on
    # non-ASCII output passed through to the original streams.
    make_streams_encoding_safe()

    # Redirigir stdout/stderr para capturar el progreso de descarga y advertencias en el log de la UI
    original_stdout = sys.stdout
    original_stderr = sys.stderr
    try:
        _attempt = int((shared_config or {}).get("asr_attempt", 1))
    except (TypeError, ValueError):
        _attempt = 1
    sys.stdout = InterceptingWriter(log_queue, original_stream=original_stdout, prefix="[IA]", attempt=_attempt)
    sys.stderr = InterceptingWriter(log_queue, original_stream=original_stderr, prefix="[IA]", attempt=_attempt)

    from liveaudio.core.provisioning import (
        asr_state_legacy,
        classify_provisioning_error,
    )

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
        _emit_status(log_queue, "asr", "ASR: cargando", "loading",
                     phase="loading", is_download=False,
                     asr_state_legacy=asr_state_legacy("loading"))
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
            model = _scoped_unverified_context_for_provisioning(
                lambda: WhisperModel(**dict(model_kwargs, local_files_only=True)))
        except Exception as cache_err:
            # "Modelo no encontrado" is RESERVED for real absence (D7): a broad
            # cache exception must map to the provision-* catalog instead.
            code = classify_provisioning_error(cache_err)
            if code == "model-not-found":
                _emit_log(log_queue, f"[IA] Modelo no encontrado en caché local. Consultando Hugging Face...")
            else:
                _emit_log(log_queue, f"[IA ERROR] provisioning {code} ({type(cache_err).__name__})")
            try:
                model = _scoped_unverified_context_for_provisioning(
                    lambda: WhisperModel(**model_kwargs))
            except Exception as load_err:
                code = classify_provisioning_error(load_err)
                if shared_config["device"] == "cuda" and code != "provision-tls":
                    _emit_log(log_queue, f"[IA ADVERTENCIA] Falló la carga en CUDA ({type(load_err).__name__}). Reintentando en CPU...")
                    cpu_kwargs = dict(model_kwargs, device="cpu", compute_type="int8", cpu_threads=int(shared_config.get("cpu_threads", 4)))
                    try:
                        model = _scoped_unverified_context_for_provisioning(
                            lambda: WhisperModel(**dict(cpu_kwargs, local_files_only=True)))
                    except Exception:
                        try:
                            model = _scoped_unverified_context_for_provisioning(
                                lambda: WhisperModel(**cpu_kwargs))
                        except Exception as cpu_err:
                            code = classify_provisioning_error(cpu_err)
                            _emit_status(log_queue, "asr", "ASR: error", "failed",
                                         phase="failed", code=code, is_download=False,
                                         asr_state_legacy=asr_state_legacy("failed"))
                            _emit_log(log_queue, f"[IA ERROR] provisioning {code} ({type(cpu_err).__name__})")
                            raise cpu_err
                else:
                    _emit_status(log_queue, "asr", "ASR: error", "failed",
                                 phase="failed", code=code, is_download=False,
                                 asr_state_legacy=asr_state_legacy("failed"))
                    _emit_log(log_queue, f"[IA ERROR] provisioning {code} ({type(load_err).__name__})")
                    raise load_err

        _record_asr_runtime_health(
            diagnostics_store,
            model_name=clean_model_name,
            model_load_sec=time.time() - model_load_started_at,
        )
        _emit_status(log_queue, "asr", "ASR: listo", "ready",
                     phase="ready", is_download=False,
                     asr_state_legacy=asr_state_legacy("ready"))
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

        def report_writer_failure(code):
            if code not in WRITER_FAILURE_CODES:
                return
            try:
                shared_config["writer_failure_code"] = code
                shared_config["writer_failure_attempt"] = _attempt
            except Exception:
                pass
            try:
                log_queue.put_nowait({"type": "fatal", "code": code, "attempt": _attempt})
            except Exception:
                pass

        session_writer = SessionWriter(
            jsonl_path, vtt_path, failure_callback=report_writer_failure,
            diagnostics_store=diagnostics_store,
        )

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
                capture_started = audio_item.get("capture_started_monotonic")
                capture_completed = audio_item.get("capture_completed_monotonic")
                try:
                    item_attempt = int(audio_item.get("attempt", _attempt))
                except (TypeError, ValueError):
                    item_attempt = _attempt
            else:
                audio_chunk = audio_item
                created_at = time.time()
                sequence = 0
                capture_started = None
                capture_completed = None
                item_attempt = _attempt

            queue_delay = max(0.0, time.time() - created_at)
            utterance_id = f"{int(created_at * 1000)}-{sequence}"

            start_time = time.time()
            _emit_status(log_queue, "asr", "ASR: transcribiendo", "transcribing",
                         phase="transcribing", is_download=False,
                         asr_state_legacy=asr_state_legacy("transcribing"))

            # Leer idioma de voz y prompt de contexto en caliente desde shared_config
            asr_lang = shared_config.get("asr_language") or "es"
            prompt_key = f"whisper_context_prompt_{asr_lang}"
            context_prompt = shared_config.get(prompt_key) or None

            raw_decode_timeout = shared_config.get(
                "asr_decode_timeout_sec", ASR_DECODE_TIMEOUT_DEFAULT_SEC,
            )
            try:
                numeric_decode_timeout = float(raw_decode_timeout)
            except (TypeError, ValueError, OverflowError):
                numeric_decode_timeout = float("nan")
            if (isinstance(raw_decode_timeout, bool)
                    or not isinstance(raw_decode_timeout, (int, float))
                    or not math.isfinite(numeric_decode_timeout)):
                timeout_sec = ASR_DECODE_TIMEOUT_DEFAULT_SEC
            else:
                timeout_sec = int(round(min(
                    ASR_DECODE_TIMEOUT_MAX_SEC,
                    max(ASR_DECODE_TIMEOUT_MIN_SEC, numeric_decode_timeout),
                )))

            decode_started_monotonic = time.monotonic()
            if isinstance(capture_started, (int, float)) and isinstance(capture_completed, (int, float)):
                diagnostics_store.record_duration(
                    "asr.utterance_formation_sec", max(0.0, capture_completed - capture_started),
                )
                diagnostics_store.record_duration(
                    "asr.queue_wait_sec", max(0.0, decode_started_monotonic - capture_completed),
                )
            decode_marker = {
                "status": "decoding",
                "attempt": _attempt,
                "utterance_id": utterance_id,
                "started_monotonic": decode_started_monotonic,
                "timeout_sec": timeout_sec,
                "deadline_monotonic": decode_started_monotonic + timeout_sec,
            }
            try:
                shared_config["asr_decode"] = decode_marker
            except Exception:
                pass
            try:
                segments, info = _transcribe_with_timeout(
                    model, audio_chunk, timeout_sec=timeout_sec,
                    log_queue=log_queue, device=shared_config["device"],
                    initial_prompt=context_prompt, language=asr_lang,
                )
            finally:
                decode_completed_monotonic = time.monotonic()
                diagnostics_store.record_duration(
                    "asr.decode_sec", max(0.0, decode_completed_monotonic - decode_started_monotonic),
                )
                if isinstance(capture_started, (int, float)):
                    diagnostics_store.record_duration(
                        "asr.capture_to_final_sec", max(0.0, decode_completed_monotonic - capture_started),
                    )
                try:
                    current_marker = shared_config.get("asr_decode")
                    if (isinstance(current_marker, dict)
                            and current_marker.get("attempt") == _attempt
                            and current_marker.get("utterance_id") == utterance_id):
                        shared_config["asr_decode"] = None
                except Exception:
                    pass
            if segments is None:
                # Timeout or error — skip to next item
                _record_asr_runtime_health(
                    diagnostics_store,
                    model_name=clean_model_name,
                    queue_delay=queue_delay,
                    timed_out=True,
                )
                _emit_status(log_queue, "asr", "ASR: listo", "ready",
                             phase="ready", is_download=False,
                             asr_state_legacy=asr_state_legacy("ready"))
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
                texto_presentacion = _presentation_text(texto_final)
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

                admitted = session_writer.write_record(
                    transcript_record, vtt_start, vtt_end, texto_presentacion, cue_counter,
                    write_transcript=save_transcript, write_vtt=save_vtt,
                    telemetry={
                        "capture_started_monotonic": capture_started,
                        "capture_completed_monotonic": capture_completed,
                    },
                )
                if not admitted:
                    break

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
                    _emit_log(log_queue, "[IA] OBS desactivado; transcripcion procesada, no enviada a OBS.")
                    _emit_transcript(log_queue, {
                        "type": "transcript",
                        "text": texto_presentacion,
                        "latency": latency,
                        "queue_delay": queue_delay,
                        "total_delay": total_delay,
                        "obs_emitted": False,
                        "reason": "obs_disabled",
                    })
                    _emit_status(log_queue, "asr", "ASR: listo", "ready",
                                 phase="ready", is_download=False,
                                 asr_state_legacy=asr_state_legacy("ready"))
                    continue

                # Empaquetamos enviando el estilo actualizado en TIEMPO REAL
                style = shared_config.get("subtitle_style", "default")
                if style not in VALID_SUBTITLE_STYLES:
                    style = "default"
                should_emit, is_replay, catchup_interval = _obs_emit_decision(shared_config, total_delay)
                payload = {
                    "id": utterance_id,
                    "text": texto_presentacion,
                    "style": style,
                    "created_at": created_at,
                    "processed_at": transcript_record["processed_at"],
                    "queue_delay": queue_delay,
                    "total_delay": total_delay,
                    "latency": latency,
                    "is_replay": is_replay,
                    "catchup_interval_sec": catchup_interval,
                    "_telemetry": {
                        "sequence": sequence,
                        "attempt": item_attempt,
                        "capture_started_monotonic": capture_started,
                        "capture_completed_monotonic": capture_completed,
                        "decode_completed_monotonic": decode_completed_monotonic,
                        "queue_enqueued_monotonic": time.monotonic(),
                    },
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
                            "text": texto_presentacion,
                            "latency": latency,
                            "queue_delay": queue_delay,
                            "total_delay": total_delay,
                            "obs_emitted": True,
                            "is_replay": is_replay,
                        })
                    except queue.Full:
                        diagnostics_store.record_counter("asr.text_queue_drops")
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
                            "text": texto_presentacion,
                            "latency": latency,
                            "queue_delay": queue_delay,
                            "total_delay": total_delay,
                            "obs_emitted": False,
                            "reason": "ws_queue_full",
                        })
                        _emit_log(log_queue, "[IA] Subtitulo procesado, pero no enviado a OBS porque la cola live esta saturada.")
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
                        "text": texto_presentacion,
                        "latency": latency,
                        "queue_delay": queue_delay,
                        "total_delay": total_delay,
                        "obs_emitted": False,
                        "reason": "backlog_policy",
                    })
                    _emit_log(log_queue, f"[IA] Subtitulo atrasado {total_delay:.1f}s procesado; omitido en OBS por politica live.")
            _emit_status(log_queue, "asr", "ASR: listo", "ready",
                         phase="ready", is_download=False,
                         asr_state_legacy=asr_state_legacy("ready"))

    except Exception as e:
        _emit_status(log_queue, "asr", "ASR: error", "failed",
                     phase="failed", code=classify_provisioning_error(e),
                     is_download=False,
                     asr_state_legacy=asr_state_legacy("failed"))
        _emit_log(log_queue, f"[IA ERROR] {type(e).__name__}")
    finally:
        shutdown_started_at = time.time()
        if session_writer is not None:
            session_writer.stop(timeout_sec=2.0)
            outcomes = session_writer.outcomes()
            for sink in ("jsonl", "vtt"):
                pending = outcomes[sink]["pending"]
                if pending:
                    diagnostics_store.record_counter(f"asr.{sink}_pending_on_shutdown", pending)
        _record_asr_runtime_health(
            diagnostics_store,
            shutdown_sec=(time.time() - shutdown_started_at) if shutdown_started_at is not None else None,
        )
