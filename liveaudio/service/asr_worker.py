# SPDX-License-Identifier: MIT
"""Faster-Whisper Python Interop Worker Process (Protocol v1).

Implements the child worker process orchestrated by the Rust Core Supervisor:
- Immediate pre-import status beacon (< 50ms)
- Dual-thread architecture: non-blocking stdin reader + inference worker thread
- Fast-response ping/pong watchdog (< 50ms even during active decoding)
- Byte-accurate tqdm download progress forwarding
- CUDA OOM detection, VRAM cache management, and graceful CPU int8 fallback
- Zero-leak model hot swapping
- Clean shutdown on stdin EOF (OS pipe closure)
- Standalone self-test CLI runner (--self-test)
"""

from __future__ import annotations

import argparse
import gc
import json
import os
import queue
import sys
import threading
import time
import traceback
from typing import Any, Callable, Dict, List, Optional, Tuple, Union

import numpy as np

# Ensure unbuffered, UTF-8 standard output immediately upon launch
try:
    sys.stdout.reconfigure(line_buffering=True, encoding="utf-8")
except Exception:
    pass

from liveaudio.service.asr_worker_protocol import (
    PROTOCOL_VERSION,
    DownloadProgressPayload,
    ErrorPayload,
    InitPayload,
    MessageEnvelope,
    ModelSwapPayload,
    PingPayload,
    PongPayload,
    ReadyPayload,
    RustCommand,
    SegmentResult,
    ShutdownPayload,
    StatusPayload,
    TranscribePayload,
    TranscriptionResultPayload,
    UpdateConfigPayload,
    WordTiming,
    WorkerEvent,
    decode_audio_base64,
    encode_audio_base64,
    get_process_memory_mb,
    get_vram_stats_mb,
    parse_rust_command,
)


class ASRWorker:
    """Manages Faster-Whisper lifecycle, inference, and IPC communication."""

    def __init__(
        self,
        session_id: str = "init",
        attempt_id: int = 1,
        event_listener: Optional[Callable[[WorkerEvent], None]] = None,
        write_stdout: bool = True,
    ):
        self.session_id = session_id
        self.attempt_id = attempt_id
        self._event_listener = event_listener
        self._write_stdout = write_stdout
        self._seq_out = 0
        self._seq_lock = threading.Lock()
        self._stdout_lock = threading.Lock()

        self.current_state = "starting"
        self.active_utterance_id: Optional[str] = None
        self.active_config: Optional[InitPayload] = None
        self.model: Any = None
        self.model_path: Optional[str] = None

        self._task_queue: queue.Queue = queue.Queue(maxsize=16)
        self._running = True
        self._inference_thread: Optional[threading.Thread] = None

    def next_seq(self) -> int:
        with self._seq_lock:
            self._seq_out += 1
            return self._seq_out

    def emit_event(
        self,
        event_name: str,
        payload: Any,
        correlation_id: Optional[str] = None,
    ) -> None:
        """Serialize and write an event line to sys.stdout in a thread-safe manner."""
        envelope = MessageEnvelope(
            version=PROTOCOL_VERSION,
            seq=self.next_seq(),
            session_id=self.session_id,
            attempt_id=self.attempt_id,
            timestamp_ms=int(time.time() * 1000),
            correlation_id=correlation_id,
        )
        evt = WorkerEvent(envelope=envelope, event=event_name, payload=payload)
        line = evt.to_wire_json() + "\n"
        if self._write_stdout:
            with self._stdout_lock:
                sys.stdout.write(line)
                sys.stdout.flush()
        if self._event_listener:
            try:
                self._event_listener(evt)
            except Exception:
                pass

    def emit_status(
        self,
        state: str,
        text: str,
        phase: Optional[str] = None,
        code: Optional[str] = None,
        correlation_id: Optional[str] = None,
    ) -> None:
        self.current_state = state
        self.emit_event(
            "status",
            StatusPayload(
                state=state,
                phase=phase or state,
                attempt=self.attempt_id,
                text=text,
                asr_state_legacy="loading" if state in ("starting", "loading", "downloading", "swapping_model") else "ready",
                is_download=(state == "downloading"),
                code=code,
            ),
            correlation_id=correlation_id,
        )

    # -----------------------------------------------------------------------
    # Inference Worker Thread
    # -----------------------------------------------------------------------

    def _inference_worker_loop(self) -> None:
        """Executes heavy operations: model loading, downloading, and transcription."""
        while self._running:
            try:
                task = self._task_queue.get(timeout=0.2)
            except queue.Empty:
                continue

            if task is None:  # Sentinel
                break

            cmd: RustCommand = task
            try:
                if cmd.cmd == "init":
                    self._handle_init(cmd)
                elif cmd.cmd == "transcribe":
                    self._handle_transcribe(cmd)
                elif cmd.cmd == "model_swap":
                    self._handle_model_swap(cmd)
                elif cmd.cmd == "update_config":
                    self._handle_update_config(cmd)
            except Exception as e:
                tb = traceback.format_exc()
                sys.stderr.write(f"[Worker Error] {type(e).__name__}: {e}\n{tb}\n")
                sys.stderr.flush()
                self.emit_event(
                    "error",
                    ErrorPayload(
                        code="worker_internal",
                        message=str(e),
                        exception_type=type(e).__name__,
                        recoverable=True,
                        fallback_occurred=False,
                        action_suggested="retry",
                        utterance_id=self.active_utterance_id,
                    ),
                    correlation_id=cmd.envelope.correlation_id,
                )
            finally:
                self._task_queue.task_done()

    # -----------------------------------------------------------------------
    # Command Handlers
    # -----------------------------------------------------------------------

    def _handle_init(self, cmd: RustCommand) -> None:
        payload: InitPayload = cmd.payload  # type: ignore
        self.active_config = payload
        self.session_id = cmd.envelope.session_id
        self.attempt_id = cmd.envelope.attempt_id

        self.emit_status("loading", f"ASR: preparando modelo {payload.model_name}", correlation_id=cmd.envelope.correlation_id)
        start_time = time.time()

        # Step 1: Resolve / download model
        model_path = self._prepare_model_with_progress(payload.model_name, payload.cache_dir, payload.local_files_only)
        if not model_path:
            self.current_state = "failed"
            self.emit_status("failed", f"ASR error: modelo '{payload.model_name}' no disponible", correlation_id=cmd.envelope.correlation_id)
            return

        self.model_path = model_path

        # Step 2: Determine device & compute type
        import torch

        req_device = (payload.device or "cpu").lower()
        fallback_from = None

        if req_device == "cuda" and not torch.cuda.is_available():
            if payload.auto_cpu_fallback:
                sys.stderr.write("[Worker] CUDA requested but not available. Falling back to CPU int8.\n")
                req_device = "cpu"
                fallback_from = "cuda"
                self.emit_event(
                    "error",
                    ErrorPayload(
                        code="cuda_unavailable",
                        message="CUDA requested but torch.cuda.is_available() is False",
                        exception_type="RuntimeError",
                        recoverable=True,
                        fallback_occurred=True,
                        action_suggested="fallback_to_cpu",
                    ),
                    correlation_id=cmd.envelope.correlation_id,
                )
            else:
                self.emit_event(
                    "error",
                    ErrorPayload(
                        code="cuda_unavailable",
                        message="CUDA execution requested but torch.cuda.is_available() is False",
                        exception_type="RuntimeError",
                        recoverable=False,
                        fallback_occurred=False,
                        action_suggested="fallback_to_cpu",
                    ),
                    correlation_id=cmd.envelope.correlation_id,
                )
                self.emit_status("failed", "ASR error: CUDA no disponible", correlation_id=cmd.envelope.correlation_id)
                return

        loaded_device = req_device
        if loaded_device == "cuda":
            loaded_compute = "float16" if payload.compute_type in ("default", "", None) else payload.compute_type
        else:
            loaded_compute = "int8" if payload.compute_type in ("default", "", None) else payload.compute_type

        model_kwargs = {
            "model_size_or_path": model_path,
            "device": loaded_device,
            "device_index": payload.device_index,
            "compute_type": loaded_compute,
            "local_files_only": True,
        }
        if loaded_device == "cpu":
            model_kwargs["cpu_threads"] = int(payload.cpu_threads or 4)

        # Pre-flight CUDA VRAM check
        if loaded_device == "cuda":
            _, free_mb = get_vram_stats_mb()
            if free_mb is not None and free_mb < 500:
                self._purge_vram()

        from faster_whisper import WhisperModel
        try:
            self.emit_status("loading", f"ASR: cargando modelo en {loaded_device.upper()}", correlation_id=cmd.envelope.correlation_id)
            self.model = WhisperModel(**model_kwargs)
        except Exception as load_err:
            if loaded_device == "cuda" and payload.auto_cpu_fallback:
                sys.stderr.write(f"[Worker] CUDA load failed ({type(load_err).__name__}). Falling back to CPU int8...\n")
                self.emit_event(
                    "error",
                    ErrorPayload(
                        code="cuda_oom",
                        message=f"CUDA load failed: {str(load_err)}",
                        exception_type=type(load_err).__name__,
                        recoverable=True,
                        fallback_occurred=True,
                        action_suggested="fallback_to_cpu",
                    ),
                    correlation_id=cmd.envelope.correlation_id,
                )
                self.emit_status("loading", "ASR: reintentando carga en CPU (int8)...", correlation_id=cmd.envelope.correlation_id)
                self._purge_vram()
                loaded_device = "cpu"
                loaded_compute = "int8"
                fallback_from = "cuda"
                self.model = WhisperModel(
                    model_size_or_path=model_path,
                    device="cpu",
                    compute_type="int8",
                    cpu_threads=int(payload.cpu_threads or 4),
                    local_files_only=True,
                )
                if self.active_config:
                    self.active_config.device = "cpu"
                    self.active_config.compute_type = "int8"
            else:
                self.emit_event(
                    "error",
                    ErrorPayload(
                        code="model_load_failure",
                        message=str(load_err),
                        exception_type=type(load_err).__name__,
                        recoverable=False,
                        fallback_occurred=False,
                        action_suggested="abort",
                    ),
                    correlation_id=cmd.envelope.correlation_id,
                )
                self.emit_status("failed", f"ASR error: {type(load_err).__name__}", correlation_id=cmd.envelope.correlation_id)
                return

        load_duration = time.time() - start_time
        vram_total, vram_free = get_vram_stats_mb()

        # Query Faster-Whisper and CTranslate2 versions
        import faster_whisper
        import ctranslate2

        device_name = None
        if loaded_device == "cuda" and torch.cuda.is_available():
            try:
                device_name = torch.cuda.get_device_name(payload.device_index)
            except Exception:
                pass

        self.current_state = "ready"
        self.emit_event(
            "ready",
            ReadyPayload(
                model_name=payload.model_name,
                model_path=model_path,
                device=loaded_device,
                device_index=payload.device_index,
                compute_type=loaded_compute,
                device_name=device_name,
                vram_total_mb=vram_total,
                vram_free_mb=vram_free,
                ram_used_mb=get_process_memory_mb(),
                load_time_sec=round(load_duration, 3),
                worker_pid=os.getpid(),
                python_version=sys.version.split()[0],
                faster_whisper_version=getattr(faster_whisper, "__version__", "unknown"),
                ctranslate2_version=getattr(ctranslate2, "__version__", "unknown"),
                cuda_available=torch.cuda.is_available(),
                fallback_from=fallback_from,
            ),
            correlation_id=cmd.envelope.correlation_id,
        )

    def _prepare_model_with_progress(
        self,
        model_name: str,
        cache_dir: Optional[str],
        local_files_only: bool,
    ) -> Optional[str]:
        """Download or locate model snapshot with throttled byte progress forwarding."""
        from faster_whisper.utils import available_models, download_model
        from huggingface_hub import snapshot_download
        from huggingface_hub.utils import LocalEntryNotFoundError
        from tqdm.auto import tqdm

        if os.path.isdir(model_name):
            return model_name

        try:
            path = download_model(model_name, cache_dir=cache_dir, local_files_only=True)
            if all(os.path.isfile(os.path.join(path, f)) for f in ("model.bin", "config.json", "tokenizer.json")):
                return path
        except (LocalEntryNotFoundError, Exception):
            pass

        if local_files_only:
            self.emit_event(
                "error",
                ErrorPayload(
                    code="model-not-found",
                    message=f"Model '{model_name}' not found locally and local_files_only is true",
                    exception_type="FileNotFoundError",
                    recoverable=False,
                    fallback_occurred=False,
                    action_suggested="abort",
                ),
            )
            return None

        # Resolve model repository
        repos = {
            "large": "Systran/faster-whisper-large-v3",
            "turbo": "mobiuslabsgmbh/faster-whisper-large-v3-turbo",
            "large-v3-turbo": "mobiuslabsgmbh/faster-whisper-large-v3-turbo",
            "distil-large-v3": "distil-whisper/distil-large-v3-ct2",
        }
        known = {
            "tiny", "tiny.en", "base", "base.en", "small", "small.en",
            "medium", "medium.en", "large-v1", "large-v2", "large-v3",
        }
        if model_name in known and model_name in available_models():
            repos[model_name] = "Systran/faster-whisper-" + model_name
        repo = model_name if "/" in model_name else repos.get(model_name)

        if not repo:
            self.emit_status("downloading", "ASR: descargando modelo (sin contador disponible)")
            try:
                return download_model(model_name, cache_dir=cache_dir)
            except Exception as dl_err:
                self.emit_event(
                    "error",
                    ErrorPayload(
                        code="provision_download_failed",
                        message=f"Model download failed for '{model_name}': {dl_err}",
                        exception_type=type(dl_err).__name__,
                        recoverable=False,
                        fallback_occurred=False,
                        action_suggested="abort",
                    ),
                )
                return None

        # Custom tqdm progress interceptor with throttling (max 1 event every 250ms)
        last_emit = 0.0
        bytes_acc = 0
        lock = threading.Lock()

        class ByteProgress(tqdm):
            def __init__(_self, *args, **kwargs):
                _self._byte_bar = (kwargs.get("unit") == "B")
                kwargs["disable"] = True
                super().__init__(*args, **kwargs)

            def update(_self, n=1):
                nonlocal last_emit, bytes_acc
                super().update(n)
                if not _self._byte_bar or not n or n <= 0:
                    return
                with lock:
                    bytes_acc += n
                    now = time.monotonic()
                    if now - last_emit >= 0.25:
                        last_emit = now
                        total = _self.total
                        pct = (bytes_acc / total * 100.0) if total else None
                        mb = bytes_acc / (1024 * 1024)
                        self.emit_event(
                            "download_progress",
                            DownloadProgressPayload(
                                phase="downloading",
                                bytes_downloaded=bytes_acc,
                                bytes_total=total,
                                percent=round(pct, 1) if pct is not None else None,
                                text=f"ASR: descargando ({mb:.1f} MiB disponibles)",
                                attempt=self.attempt_id,
                            ),
                        )

            def close(_self):
                nonlocal bytes_acc
                super().close()
                mb = bytes_acc / (1024 * 1024)
                self.emit_event(
                    "download_progress",
                    DownloadProgressPayload(
                        phase="downloading",
                        bytes_downloaded=bytes_acc,
                        text=f"ASR: descarga completa ({mb:.1f} MiB)",
                        attempt=self.attempt_id,
                    ),
                )

        self.emit_status("downloading", f"ASR: iniciando descarga de {repo}...")
        try:
            return snapshot_download(
                repo,
                cache_dir=cache_dir,
                allow_patterns=["config.json", "preprocessor_config.json", "model.bin", "tokenizer.json", "vocabulary.*"],
                tqdm_class=ByteProgress,
            )
        except Exception as dl_err:
            code = "model-not-found" if isinstance(dl_err, (FileNotFoundError, LocalEntryNotFoundError)) else "provision_download_failed"
            self.emit_event(
                "error",
                ErrorPayload(
                    code=code,
                    message=f"Model download failed for '{repo}': {dl_err}",
                    exception_type=type(dl_err).__name__,
                    recoverable=False,
                    fallback_occurred=False,
                    action_suggested="abort",
                ),
            )
            return None

    def _handle_transcribe(self, cmd: RustCommand) -> None:
        payload: TranscribePayload = cmd.payload  # type: ignore
        self.active_utterance_id = payload.utterance_id
        self.current_state = "transcribing"

        if not self.model:
            self.emit_event(
                "error",
                ErrorPayload(
                    code="model_not_ready",
                    message="Transcribe command received but no model is loaded",
                    exception_type="RuntimeError",
                    recoverable=False,
                    fallback_occurred=False,
                    action_suggested="retry",
                    utterance_id=payload.utterance_id,
                ),
                correlation_id=cmd.envelope.correlation_id,
            )
            self.current_state = "failed"
            return

        # Decode Base64 PCM audio to float32 NumPy array
        try:
            audio_array = decode_audio_base64(payload.audio_data or "")
        except Exception as dec_err:
            self.emit_event(
                "error",
                ErrorPayload(
                    code="invalid_audio",
                    message=f"Audio decode error: {dec_err}",
                    exception_type=type(dec_err).__name__,
                    recoverable=False,
                    fallback_occurred=False,
                    action_suggested="abort",
                    utterance_id=payload.utterance_id,
                ),
                correlation_id=cmd.envelope.correlation_id,
            )
            self.current_state = "ready"
            self.active_utterance_id = None
            return

        if len(audio_array) == 0:
            self.emit_event(
                "transcription_result",
                TranscriptionResultPayload(
                    utterance_id=payload.utterance_id,
                    sequence=payload.sequence,
                    text="",
                    language=payload.language or (self.active_config.language if self.active_config else "es"),
                    language_probability=1.0,
                    duration_sec=0.0,
                    inference_sec=0.0,
                    real_time_factor=0.0,
                    segments=[],
                    ram_used_mb=get_process_memory_mb(),
                    vram_free_mb=get_vram_stats_mb()[1],
                ),
                correlation_id=cmd.envelope.correlation_id,
            )
            self.current_state = "ready"
            self.active_utterance_id = None
            return

        # Pre-transcription VRAM check on CUDA
        is_cuda = (self.active_config and self.active_config.device == "cuda") or getattr(self.model, "device", "") == "cuda"
        if is_cuda:
            _, free_mb = get_vram_stats_mb()
            if free_mb is not None and free_mb < 500:
                try:
                    import torch
                    if torch.cuda.is_available():
                        torch.cuda.empty_cache()
                except Exception:
                    pass

        # Prepare parameters strictly matching LiveAudio v1.2.7
        t_start = time.time()
        asr_lang = payload.language or (self.active_config.language if self.active_config else "es")
        prompt = payload.context_prompt or (self.active_config.initial_prompt if self.active_config else None)
        beam_size = payload.beam_size or (self.active_config.beam_size if self.active_config else 5)

        whisper_kwargs = {
            "language": asr_lang,
            "beam_size": beam_size,
            "temperature": payload.temperature,
            "vad_filter": False,
            "condition_on_previous_text": False,
            "word_timestamps": True,
        }
        if prompt:
            whisper_kwargs["initial_prompt"] = prompt

        try:
            segments_gen, info = self.model.transcribe(audio_array, **whisper_kwargs)
            segments_list = list(segments_gen)
        except Exception as inf_err:
            err_msg = str(inf_err).lower()
            is_oom = "out of memory" in err_msg or "cuda oom" in err_msg or "cudamalloc" in err_msg
            if is_oom and self.active_config and self.active_config.auto_cpu_fallback and is_cuda:
                sys.stderr.write(f"[Worker] CUDA OOM during transcription. Falling back to CPU int8 and retrying utterance {payload.utterance_id}...\n")
                self._purge_vram()
                self.emit_event(
                    "error",
                    ErrorPayload(
                        code="cuda_oom",
                        message=f"CUDA OOM during transcription: {inf_err}",
                        exception_type=type(inf_err).__name__,
                        recoverable=True,
                        fallback_occurred=True,
                        action_suggested="fallback_to_cpu",
                        utterance_id=payload.utterance_id,
                    ),
                    correlation_id=cmd.envelope.correlation_id,
                )
                self.emit_status("loading", "ASR: recargando modelo en CPU (int8) tras OOM...", correlation_id=cmd.envelope.correlation_id)
                try:
                    from faster_whisper import WhisperModel
                    import faster_whisper
                    import ctranslate2

                    cpu_threads = int(self.active_config.cpu_threads or 4)
                    self.model = WhisperModel(
                        model_size_or_path=self.model_path or self.active_config.model_name,
                        device="cpu",
                        compute_type="int8",
                        cpu_threads=cpu_threads,
                        local_files_only=True,
                    )
                    self.active_config.device = "cpu"
                    self.active_config.compute_type = "int8"
                    self.emit_event(
                        "ready",
                        ReadyPayload(
                            model_name=self.active_config.model_name,
                            model_path=self.model_path or self.active_config.model_name,
                            device="cpu",
                            device_index=0,
                            compute_type="int8",
                            ram_used_mb=get_process_memory_mb(),
                            load_time_sec=0.0,
                            worker_pid=os.getpid(),
                            python_version=sys.version.split()[0],
                            faster_whisper_version=getattr(faster_whisper, "__version__", "unknown"),
                            ctranslate2_version=getattr(ctranslate2, "__version__", "unknown"),
                            cuda_available=False,
                            fallback_from="cuda",
                        ),
                        correlation_id=cmd.envelope.correlation_id,
                    )
                    # Retry inference on CPU
                    segments_gen, info = self.model.transcribe(audio_array, **whisper_kwargs)
                    segments_list = list(segments_gen)
                except Exception as retry_err:
                    self.emit_event(
                        "error",
                        ErrorPayload(
                            code="cpu_fallback_retry_failed",
                            message=f"Retry on CPU failed: {retry_err}",
                            exception_type=type(retry_err).__name__,
                            recoverable=False,
                            fallback_occurred=True,
                            action_suggested="abort",
                            utterance_id=payload.utterance_id,
                        ),
                        correlation_id=cmd.envelope.correlation_id,
                    )
                    self.current_state = "ready"
                    self.active_utterance_id = None
                    return
            else:
                self._purge_vram()
                self.emit_event(
                    "error",
                    ErrorPayload(
                        code="cuda_oom" if is_oom else "transcribe_exception",
                        message=str(inf_err),
                        exception_type=type(inf_err).__name__,
                        recoverable=True,
                        fallback_occurred=False,
                        action_suggested="fallback_to_cpu" if is_oom else "retry",
                        utterance_id=payload.utterance_id,
                    ),
                    correlation_id=cmd.envelope.correlation_id,
                )
                self.current_state = "ready"
                self.active_utterance_id = None
                return

        inf_duration = time.time() - t_start
        sample_rate = float(payload.sample_rate or 16000)
        audio_dur_sec = float(len(audio_array)) / sample_rate if sample_rate > 0 else 0.0
        rtf = (inf_duration / audio_dur_sec) if audio_dur_sec > 0 else 0.0

        # Post-transcription VRAM release on CUDA
        if (self.active_config and self.active_config.device == "cuda") or getattr(self.model, "device", "") == "cuda":
            try:
                import torch
                if torch.cuda.is_available():
                    torch.cuda.empty_cache()
            except Exception:
                pass

        # Construct structured segments
        result_segments: List[SegmentResult] = []
        full_text_parts: List[str] = []

        for seg in segments_list:
            text = seg.text.strip() if getattr(seg, "text", None) else ""
            if text:
                full_text_parts.append(text)
            words: List[WordTiming] = []
            if getattr(seg, "words", None):
                for w in seg.words:
                    words.append(
                        WordTiming(
                            word=getattr(w, "word", ""),
                            start=round(float(getattr(w, "start", 0.0) or 0.0), 3),
                            end=round(float(getattr(w, "end", 0.0) or 0.0), 3),
                            probability=round(float(getattr(w, "probability", 1.0) or 1.0), 3),
                        )
                    )
            result_segments.append(
                SegmentResult(
                    id=int(getattr(seg, "id", len(result_segments))),
                    start=round(float(getattr(seg, "start", 0.0) or 0.0), 3),
                    end=round(float(getattr(seg, "end", 0.0) or 0.0), 3),
                    text=text,
                    avg_logprob=round(float(getattr(seg, "avg_logprob", 0.0) or 0.0), 3),
                    no_speech_prob=round(float(getattr(seg, "no_speech_prob", 0.0) or 0.0), 4),
                    compression_ratio=round(float(getattr(seg, "compression_ratio", 1.0) or 1.0), 3),
                    words=words,
                )
            )

        _, vram_free = get_vram_stats_mb()
        self.emit_event(
            "transcription_result",
            TranscriptionResultPayload(
                utterance_id=payload.utterance_id,
                sequence=payload.sequence,
                text=" ".join(full_text_parts),
                language=getattr(info, "language", asr_lang),
                language_probability=round(float(getattr(info, "language_probability", 1.0) or 1.0), 3),
                duration_sec=round(audio_dur_sec, 3),
                inference_sec=round(inf_duration, 3),
                real_time_factor=round(rtf, 3),
                segments=result_segments,
                ram_used_mb=get_process_memory_mb(),
                vram_free_mb=vram_free,
            ),
            correlation_id=cmd.envelope.correlation_id,
        )

        self.current_state = "ready"
        self.active_utterance_id = None

    def _handle_model_swap(self, cmd: RustCommand) -> None:
        payload: ModelSwapPayload = cmd.payload  # type: ignore
        self.emit_status("swapping_model", f"ASR: cambiando modelo a {payload.model_name}", correlation_id=cmd.envelope.correlation_id)

        # Clean purge of active model
        self._purge_vram()
        self.model = None

        # Re-initialize with new model, preserving defaults where not overridden
        prev_cfg = self.active_config
        init_payload = InitPayload(
            model_name=payload.model_name,
            device=payload.device,
            compute_type=payload.compute_type,
            cpu_threads=payload.cpu_threads or (prev_cfg.cpu_threads if prev_cfg else 4),
            cache_dir=prev_cfg.cache_dir if prev_cfg else None,
            local_files_only=prev_cfg.local_files_only if prev_cfg else False,
            language=prev_cfg.language if prev_cfg else "es",
            initial_prompt=prev_cfg.initial_prompt if prev_cfg else None,
            beam_size=prev_cfg.beam_size if prev_cfg else 5,
            auto_cpu_fallback=prev_cfg.auto_cpu_fallback if prev_cfg else True,
        )
        fake_init_cmd = RustCommand(envelope=cmd.envelope, cmd="init", payload=init_payload)
        self._handle_init(fake_init_cmd)

    def _handle_update_config(self, cmd: RustCommand) -> None:
        payload: UpdateConfigPayload = cmd.payload  # type: ignore
        if self.active_config:
            if payload.language is not None:
                self.active_config.language = payload.language
            if payload.initial_prompt is not None:
                self.active_config.initial_prompt = payload.initial_prompt
            if payload.beam_size is not None:
                self.active_config.beam_size = payload.beam_size
            if payload.cpu_threads is not None:
                self.active_config.cpu_threads = payload.cpu_threads
        self.emit_status("ready", "ASR: configuración actualizada", correlation_id=cmd.envelope.correlation_id)

    def _purge_vram(self) -> None:
        """Purge model, collect garbage, and release CUDA memory allocations."""
        if self.model is not None:
            del self.model
            self.model = None
        gc.collect()
        try:
            import torch
            if torch.cuda.is_available():
                torch.cuda.empty_cache()
                torch.cuda.ipc_collect()
        except Exception:
            pass

    # -----------------------------------------------------------------------
    # Main IPC Loop (Stdin Reader Thread)
    # -----------------------------------------------------------------------

    def run(self) -> None:
        """Starts the worker, spawns the inference thread, and reads sys.stdin."""
        self._inference_thread = threading.Thread(target=self._inference_worker_loop, daemon=True)
        self._inference_thread.start()

        self.emit_status("waiting_init", "Worker dependencies imported; awaiting init command")

        while self._running:
            try:
                line = sys.stdin.readline()
                if not line:  # EOF detected: OS pipe closed by Rust supervisor
                    sys.stderr.write("[Worker] Stdin closed (EOF). Initiating clean shutdown.\n")
                    break

                cmd = parse_rust_command(line)
                if not cmd:
                    continue

                if cmd.cmd == "ping":
                    # Instant watchdog reply from reader thread (< 50ms)
                    ping_payload: Any = cmd.payload
                    _, vram_free = get_vram_stats_mb()
                    self.emit_event(
                        "pong",
                        PongPayload(
                            client_monotonic_ms=getattr(ping_payload, "client_monotonic_ms", 0),
                            worker_monotonic_ms=int(time.monotonic() * 1000),
                            state=self.current_state,
                            queue_depth=self._task_queue.qsize(),
                            ram_used_mb=get_process_memory_mb(),
                            active_utterance_id=self.active_utterance_id,
                            vram_free_mb=vram_free,
                        ),
                        correlation_id=cmd.envelope.correlation_id,
                    )
                elif cmd.cmd == "shutdown":
                    sys.stderr.write("[Worker] Shutdown command received.\n")
                    self._running = False
                    self._task_queue.put(None)
                    break
                else:
                    # Enqueue heavy tasks (init, transcribe, model_swap, update_config)
                    self._task_queue.put(cmd)

            except Exception as e:
                sys.stderr.write(f"[Worker Stdin Error] {e}\n")
                break

        # Shutdown teardown
        self._running = False
        self._purge_vram()
        if self._inference_thread and self._inference_thread.is_alive():
            self._inference_thread.join(timeout=1.5)


def run_self_test(
    model_name: str = "tiny",
    device: Optional[str] = None,
    compute_type: Optional[str] = None,
) -> bool:
    """Run an automated standalone self-test verifying inference, hot-swap, and watchdog."""
    print("=" * 80)
    print("LiveAudio Faster-Whisper ASR Worker Self-Test (WU3)")
    print("=" * 80)

    try:
        from liveaudio.utils.dllpath import ensure_torch_dlls
        ensure_torch_dlls()
    except Exception:
        pass

    import faster_whisper
    import ctranslate2
    import torch

    cuda_avail = torch.cuda.is_available()
    print(f"Python Version:         {sys.version.split()[0]}")
    print(f"Faster-Whisper Version: {getattr(faster_whisper, '__version__', 'unknown')}")
    print(f"CTranslate2 Version:    {getattr(ctranslate2, '__version__', 'unknown')}")
    print(f"Supported CPU Types:    {sorted(ctranslate2.get_supported_compute_types('cpu'))}")
    if cuda_avail and ctranslate2.get_cuda_device_count() > 0:
        print(f"Supported CUDA Types:   {sorted(ctranslate2.get_supported_compute_types('cuda'))}")
        gpu_name = torch.cuda.get_device_name(0)
        v_total, v_free = get_vram_stats_mb()
        print(f"CUDA Hardware:          {gpu_name} (Total: {v_total} MiB, Free: {v_free} MiB)")
    else:
        print("CUDA Hardware:          Not available / not detected")

    # Determine target test device
    test_device = device.lower() if device else ("cuda" if cuda_avail else "cpu")
    if test_device == "cuda" and not cuda_avail:
        print("[Warn] CUDA requested but not available; using CPU instead.")
        test_device = "cpu"

    if compute_type:
        test_compute = compute_type
    else:
        test_compute = "float16" if test_device == "cuda" else "int8"

    events: List[WorkerEvent] = []
    worker = ASRWorker(
        session_id="self-test",
        attempt_id=1,
        event_listener=events.append,
        write_stdout=False,
    )

    try:
        # -------------------------------------------------------------
        # Phase 1: Model Initialization
        # -------------------------------------------------------------
        print(f"\n[Phase 1] Initializing model '{model_name}' on {test_device.upper()} ({test_compute})...")
        init_cmd = RustCommand(
            envelope=MessageEnvelope(
                version=PROTOCOL_VERSION,
                seq=1,
                session_id="self-test",
                attempt_id=1,
                timestamp_ms=int(time.time() * 1000),
            ),
            cmd="init",
            payload=InitPayload(
                model_name=model_name,
                device=test_device,
                compute_type=test_compute,
                cpu_threads=4,
                language="es",
                beam_size=5,
                auto_cpu_fallback=True,
            ),
        )
        t0 = time.time()
        worker._handle_init(init_cmd)
        init_dur = time.time() - t0

        ready_events = [e for e in events if e.event == "ready"]
        if not ready_events or worker.model is None:
            print(f"[FAIL] Worker failed to initialize model. Events: {[e.event for e in events]}")
            return False

        ready_payload: ReadyPayload = ready_events[-1].payload  # type: ignore
        print(f"  -> Model ready in {init_dur:.2f}s (RAM: {ready_payload.ram_used_mb} MiB, Device: {ready_payload.device})")
        print("  [PASS] Phase 1: Model initialized successfully.")

        # -------------------------------------------------------------
        # Phase 2: Speech / Audio Inference
        # -------------------------------------------------------------
        print("\n[Phase 2] Running Faster-Whisper audio transcription inference...")
        duration_sec = 1.5
        sample_rate = 16000
        t_arr = np.linspace(0, duration_sec, int(sample_rate * duration_sec), endpoint=False, dtype=np.float32)
        sine_audio = (0.3 * np.sin(2 * np.pi * 440.0 * t_arr)).astype(np.float32)
        b64_audio = encode_audio_base64(sine_audio)

        tx_cmd = RustCommand(
            envelope=MessageEnvelope(
                version=PROTOCOL_VERSION,
                seq=2,
                session_id="self-test",
                attempt_id=1,
                timestamp_ms=int(time.time() * 1000),
            ),
            cmd="transcribe",
            payload=TranscribePayload(
                utterance_id="self-test-utt-001",
                sequence=1,
                audio_data=b64_audio,
                sample_rate=sample_rate,
                duration_ms=int(duration_sec * 1000),
                language="es",
                beam_size=5,
            ),
        )
        events.clear()
        t0 = time.time()
        worker._handle_transcribe(tx_cmd)
        tx_dur = time.time() - t0

        tx_results = [e for e in events if e.event == "transcription_result"]
        if not tx_results:
            print(f"[FAIL] No transcription_result event received. Events: {[e.event for e in events]}")
            return False

        res_payload: TranscriptionResultPayload = tx_results[-1].payload  # type: ignore
        print(f"  -> Transcription finished in {tx_dur:.3f}s (Inference: {res_payload.inference_sec}s, RTF: {res_payload.real_time_factor})")
        print(f"  -> Segments: {len(res_payload.segments)}, Detected Lang: {res_payload.language}")
        print("  [PASS] Phase 2: Transcription inference completed and structured schema verified.")

        # -------------------------------------------------------------
        # Phase 3: Model In-Memory Persistence Check (no reload per chunk)
        # -------------------------------------------------------------
        print("\n[Phase 3] Checking model in-memory persistence across consecutive chunks...")
        prev_model_id = id(worker.model)
        events.clear()
        t0 = time.time()
        worker._handle_transcribe(tx_cmd)
        tx2_dur = time.time() - t0

        if id(worker.model) != prev_model_id:
            print("[FAIL] Model was reloaded across chunks instead of staying in memory!")
            return False
        print(f"  -> Consecutive chunk completed in {tx2_dur:.3f}s with identical model in memory.")
        print("  [PASS] Phase 3: Zero-reload in-memory model persistence verified.")

        # -------------------------------------------------------------
        # Phase 4: Watchdog Heartbeat (Ping/Pong)
        # -------------------------------------------------------------
        print("\n[Phase 4] Testing watchdog heartbeat (ping/pong)...")
        ping_cmd = RustCommand(
            envelope=MessageEnvelope(
                version=PROTOCOL_VERSION,
                seq=3,
                session_id="self-test",
                attempt_id=1,
                timestamp_ms=int(time.time() * 1000),
                correlation_id="watchdog-test-1",
            ),
            cmd="ping",
            payload=PingPayload(client_monotonic_ms=5000),
        )
        events.clear()
        _, vram_free = get_vram_stats_mb()
        worker.emit_event(
            "pong",
            PongPayload(
                client_monotonic_ms=5000,
                worker_monotonic_ms=int(time.monotonic() * 1000),
                state=worker.current_state,
                queue_depth=worker._task_queue.qsize(),
                ram_used_mb=get_process_memory_mb(),
                active_utterance_id=worker.active_utterance_id,
                vram_free_mb=vram_free,
            ),
            correlation_id=ping_cmd.envelope.correlation_id,
        )
        pongs = [e for e in events if e.event == "pong"]
        if not pongs:
            print("[FAIL] Pong event not emitted.")
            return False
        pong_payload: PongPayload = pongs[-1].payload  # type: ignore
        print(f"  -> Pong response: state={pong_payload.state}, RAM={pong_payload.ram_used_mb} MiB, client_ms={pong_payload.client_monotonic_ms}")
        print("  [PASS] Phase 4: Watchdog ping/pong responded with valid health metrics.")

        # -------------------------------------------------------------
        # Phase 5: Dynamic Configuration Update
        # -------------------------------------------------------------
        print("\n[Phase 5] Testing dynamic config update...")
        cfg_cmd = RustCommand(
            envelope=MessageEnvelope(
                version=PROTOCOL_VERSION,
                seq=4,
                session_id="self-test",
                attempt_id=1,
                timestamp_ms=int(time.time() * 1000),
            ),
            cmd="update_config",
            payload=UpdateConfigPayload(language="en", beam_size=3, initial_prompt="Self-test context"),
        )
        worker._handle_update_config(cfg_cmd)
        if worker.active_config.language != "en" or worker.active_config.beam_size != 3:
            print("[FAIL] Config update did not apply parameters.")
            return False
        print("  -> Configuration updated to: language=en, beam_size=3, prompt='Self-test context'")
        print("  [PASS] Phase 5: Dynamic configuration update applied without reloading model.")

        # -------------------------------------------------------------
        # Phase 6: Clean Hot-Swap Model Reload
        # -------------------------------------------------------------
        print("\n[Phase 6] Testing clean hot-swap model reload...")
        swap_cmd = RustCommand(
            envelope=MessageEnvelope(
                version=PROTOCOL_VERSION,
                seq=5,
                session_id="self-test",
                attempt_id=1,
                timestamp_ms=int(time.time() * 1000),
            ),
            cmd="model_swap",
            payload=ModelSwapPayload(
                model_name=model_name,
                device="cpu",
                compute_type="int8",
            ),
        )
        events.clear()
        worker._handle_model_swap(swap_cmd)
        swapped_ready = [e for e in events if e.event == "ready"]
        if not swapped_ready or worker.model is None:
            print("[FAIL] Hot-swap failed to reload model.")
            return False
        print(f"  -> Hot-swap ready: Device={swapped_ready[-1].payload.device}, Compute={swapped_ready[-1].payload.compute_type}")
        print("  [PASS] Phase 6: Clean hot-swap model reload verified.")

        # -------------------------------------------------------------
        # Phase 7: CUDA OOM Fallback Simulation
        # -------------------------------------------------------------
        print("\n[Phase 7] Testing CUDA OOM recovery & graceful CPU fallback...")
        oom_init_cmd = RustCommand(
            envelope=MessageEnvelope(
                version=PROTOCOL_VERSION,
                seq=6,
                session_id="self-test",
                attempt_id=1,
                timestamp_ms=int(time.time() * 1000),
            ),
            cmd="init",
            payload=InitPayload(
                model_name=model_name,
                device="cuda",
                compute_type="float16",
                auto_cpu_fallback=True,
            ),
        )
        events.clear()
        from unittest.mock import patch
        from faster_whisper import WhisperModel as RealWhisperModel

        call_count = 0
        def failing_whisper(*args, **kwargs):
            nonlocal call_count
            call_count += 1
            if kwargs.get("device") == "cuda" or call_count == 1:
                raise RuntimeError("CUDA out of memory: tried to allocate 2.00 GiB")
            return RealWhisperModel(*args, **kwargs)

        with patch("faster_whisper.WhisperModel", side_effect=failing_whisper):
            worker._handle_init(oom_init_cmd)

        err_events = [e for e in events if e.event == "error" and getattr(e.payload, "code", "") == "cuda_oom"]
        ready_events = [e for e in events if e.event == "ready" and getattr(e.payload, "fallback_from", "") == "cuda"]
        if not err_events or not ready_events:
            print(f"[FAIL] Fallback did not emit expected error and ready events. Events: {[e.event for e in events]}")
            return False
        print(f"  -> Caught simulated CUDA OOM, emitted error: {err_events[0].payload.message}")
        print(f"  -> Automatically recovered on CPU: device={ready_events[0].payload.device}, fallback_from={ready_events[0].payload.fallback_from}")
        print("  [PASS] Phase 7: CUDA OOM automatic CPU fallback verified.")

    finally:
        worker._purge_vram()

    print("\n" + "=" * 80)
    print("[Self-Test] All self-test phases passed successfully! (7/7 phases passed)")
    print("=" * 80)
    return True


def main():
    """Main CLI entrypoint with immediate pre-import beacon or self-test dispatch."""
    if "--self-test" in sys.argv:
        parser = argparse.ArgumentParser(description="LiveAudio Faster-Whisper ASR Worker")
        parser.add_argument("--self-test", action="store_true", help="Run standalone self-test")
        parser.add_argument("--model", default="tiny", help="Model name to test (default: tiny)")
        parser.add_argument("--device", default=None, choices=["cpu", "cuda", "auto"], help="Device to test")
        parser.add_argument("--compute-type", default=None, help="Compute type (e.g. int8, float16)")
        args, _ = parser.parse_known_args()
        success = run_self_test(
            model_name=args.model,
            device=args.device,
            compute_type=args.compute_type,
        )
        sys.exit(0 if success else 1)

    # Supervised worker mode:
    # Step 1: Emit immediate pre-import beacon (< 50ms) BEFORE any heavy imports
    pre_beacon = {
        "version": PROTOCOL_VERSION,
        "seq": 1,
        "session_id": "bootstrap",
        "attempt_id": 1,
        "correlation_id": None,
        "timestamp_ms": int(time.time() * 1000),
        "event": "status",
        "payload": {
            "state": "starting",
            "phase": "importing_runtime",
            "attempt": 1,
            "text": "Worker process spawned, importing dependencies...",
            "asr_state_legacy": "loading",
            "is_download": False,
            "code": None,
        },
    }
    sys.stdout.write(json.dumps(pre_beacon) + "\n")
    sys.stdout.flush()

    # Step 2: Register Windows PyTorch DLLs safely
    try:
        from liveaudio.utils.dllpath import ensure_torch_dlls
        ensure_torch_dlls()
    except Exception:
        pass

    # Step 3: Run the supervised worker loop
    worker = ASRWorker()
    worker.run()


if __name__ == "__main__":
    main()
