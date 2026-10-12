# SPDX-License-Identifier: MIT
"""ASR Worker IPC Protocol Version 1 Definitions and Message Codecs.

Defines the versioned IPC contract between the Rust Core Supervisor
and the Faster-Whisper Python Worker process.
"""

from __future__ import annotations

import base64
import json
import os
import sys
import time
from dataclasses import asdict, dataclass, field
from typing import Any, Dict, List, Optional, Tuple, Union

import numpy as np

PROTOCOL_VERSION: int = 1


# ---------------------------------------------------------------------------
# Envelope & Common Data Structures
# ---------------------------------------------------------------------------

@dataclass
class MessageEnvelope:
    """Versioned envelope wrapping every command and event on the wire."""
    version: int
    seq: int
    session_id: str
    attempt_id: int
    timestamp_ms: int
    correlation_id: Optional[str] = None


@dataclass
class WordTiming:
    """Individual word timestamp and confidence."""
    word: str
    start: float
    end: float
    probability: float


@dataclass
class SegmentResult:
    """Transcribed segment with timing and confidence statistics."""
    id: int
    start: float
    end: float
    text: str
    avg_logprob: float
    no_speech_prob: float
    compression_ratio: float
    words: List[WordTiming] = field(default_factory=list)


# ---------------------------------------------------------------------------
# Rust -> Worker Command Payloads
# ---------------------------------------------------------------------------

@dataclass
class InitPayload:
    model_name: str = "small"
    device: str = "cpu"
    device_index: int = 0
    compute_type: str = "default"
    cpu_threads: int = 4
    cache_dir: Optional[str] = None
    local_files_only: bool = False
    language: str = "es"
    initial_prompt: Optional[str] = None
    beam_size: int = 5
    temperature: float = 0.0
    vad_filter: bool = False
    condition_on_previous_text: bool = False
    audio_framing: str = "base64"
    auto_cpu_fallback: bool = True


@dataclass
class TranscribePayload:
    utterance_id: str
    sequence: int
    audio_data: Optional[str] = None  # Base64-encoded f32le PCM
    sample_rate: int = 16000
    channels: int = 1
    duration_ms: int = 0
    language: Optional[str] = None
    context_prompt: Optional[str] = None
    beam_size: int = 5
    temperature: float = 0.0
    timeout_ms: int = 15000


@dataclass
class UpdateConfigPayload:
    language: Optional[str] = None
    initial_prompt: Optional[str] = None
    beam_size: Optional[int] = None
    cpu_threads: Optional[int] = None


@dataclass
class ModelSwapPayload:
    model_name: str
    device: str
    compute_type: str = "default"
    cpu_threads: Optional[int] = None
    drain_active_chunk: bool = True
    drain_timeout_ms: int = 5000


@dataclass
class PingPayload:
    client_monotonic_ms: int = 0


@dataclass
class ShutdownPayload:
    grace_timeout_ms: int = 3000
    flush_cache: bool = True


# ---------------------------------------------------------------------------
# Worker -> Rust Event Payloads
# ---------------------------------------------------------------------------

@dataclass
class ReadyPayload:
    model_name: str
    model_path: str
    device: str
    device_index: int
    compute_type: str
    ram_used_mb: int
    load_time_sec: float
    worker_pid: int
    python_version: str
    faster_whisper_version: str
    ctranslate2_version: str
    cuda_available: bool
    device_name: Optional[str] = None
    vram_total_mb: Optional[int] = None
    vram_free_mb: Optional[int] = None
    fallback_from: Optional[str] = None


@dataclass
class StatusPayload:
    state: str
    phase: str
    attempt: int
    text: str
    asr_state_legacy: str = "loading"
    is_download: bool = False
    code: Optional[str] = None


@dataclass
class DownloadProgressPayload:
    phase: str
    bytes_downloaded: int
    text: str
    attempt: int
    percent: Optional[float] = None
    bytes_total: Optional[int] = None
    speed_bps: Optional[int] = None
    eta_sec: Optional[float] = None
    file_name: Optional[str] = None
    code: Optional[str] = None


@dataclass
class TranscriptionResultPayload:
    utterance_id: str
    sequence: int
    text: str
    language: str
    language_probability: float
    duration_sec: float
    inference_sec: float
    real_time_factor: float
    segments: List[SegmentResult]
    ram_used_mb: int
    vram_free_mb: Optional[int] = None


@dataclass
class ErrorPayload:
    code: str
    message: str
    exception_type: str
    recoverable: bool
    fallback_occurred: bool
    action_suggested: str
    utterance_id: Optional[str] = None
    details: Optional[Dict[str, Any]] = None


@dataclass
class PongPayload:
    client_monotonic_ms: int
    worker_monotonic_ms: int
    state: str
    queue_depth: int
    ram_used_mb: int
    active_utterance_id: Optional[str] = None
    vram_free_mb: Optional[int] = None


# ---------------------------------------------------------------------------
# Top-level Typed Messages
# ---------------------------------------------------------------------------

@dataclass
class RustCommand:
    """Parsed command received from Rust supervisor."""
    envelope: MessageEnvelope
    cmd: str
    payload: Union[
        InitPayload,
        TranscribePayload,
        UpdateConfigPayload,
        ModelSwapPayload,
        PingPayload,
        ShutdownPayload,
        Dict[str, Any]
    ]


@dataclass
class WorkerEvent:
    """Event emitted by Worker to Rust supervisor."""
    envelope: MessageEnvelope
    event: str
    payload: Union[
        ReadyPayload,
        StatusPayload,
        DownloadProgressPayload,
        TranscriptionResultPayload,
        ErrorPayload,
        PongPayload,
        Dict[str, Any]
    ]

    def to_wire_json(self) -> str:
        """Serialize event to a single-line JSON string."""
        d = {
            "version": self.envelope.version,
            "seq": self.envelope.seq,
            "session_id": self.envelope.session_id,
            "attempt_id": self.envelope.attempt_id,
            "correlation_id": self.envelope.correlation_id,
            "timestamp_ms": self.envelope.timestamp_ms,
            "event": self.event,
            "payload": asdict(self.payload) if hasattr(self.payload, "__dataclass_fields__") else self.payload
        }
        return json.dumps(d, ensure_ascii=False)


# ---------------------------------------------------------------------------
# Codec and Parsing Helpers
# ---------------------------------------------------------------------------

def parse_rust_command(line: str) -> Optional[RustCommand]:
    """Parse a single JSON line into a typed RustCommand."""
    if not line or not line.strip():
        return None
    try:
        raw = json.loads(line)
    except json.JSONDecodeError:
        return None

    if not isinstance(raw, dict):
        return None

    envelope = MessageEnvelope(
        version=int(raw.get("version", PROTOCOL_VERSION)),
        seq=int(raw.get("seq", 0)),
        session_id=str(raw.get("session_id", "default")),
        attempt_id=int(raw.get("attempt_id", 1)),
        correlation_id=raw.get("correlation_id"),
        timestamp_ms=int(raw.get("timestamp_ms", int(time.time() * 1000))),
    )

    cmd = str(raw.get("cmd", ""))
    raw_payload = raw.get("payload", {})
    if not isinstance(raw_payload, dict):
        raw_payload = {}

    payload: Any
    if cmd == "init":
        payload = InitPayload(**raw_payload)
    elif cmd == "transcribe":
        payload = TranscribePayload(**raw_payload)
    elif cmd == "update_config":
        payload = UpdateConfigPayload(**raw_payload)
    elif cmd == "model_swap":
        payload = ModelSwapPayload(**raw_payload)
    elif cmd == "ping":
        payload = PingPayload(**raw_payload)
    elif cmd == "shutdown":
        payload = ShutdownPayload(**raw_payload)
    else:
        payload = raw_payload

    return RustCommand(envelope=envelope, cmd=cmd, payload=payload)


def decode_audio_base64(audio_b64: str) -> np.ndarray:
    """Decode Base64 string to a 1D float32 numpy array sampled at 16kHz.
    
    Validates that decoded byte length is a multiple of 4 bytes (IEEE-754 float32).
    """
    if not audio_b64:
        return np.zeros(0, dtype=np.float32)
    raw_bytes = base64.b64decode(audio_b64)
    if len(raw_bytes) % 4 != 0:
        raise ValueError(f"Invalid PCM f32 byte length: {len(raw_bytes)} (not divisible by 4)")
    # Interpret as float32 array (little-endian)
    return np.frombuffer(raw_bytes, dtype=np.float32)


def encode_audio_base64(audio: np.ndarray) -> str:
    """Encode float32 numpy array to Base64 ASCII string."""
    f32_arr = np.ascontiguousarray(audio, dtype=np.float32)
    return base64.b64encode(f32_arr.tobytes()).decode("ascii")


def get_process_memory_mb() -> int:
    """Get current process RSS memory in megabytes (cross-platform)."""
    try:
        import psutil
        process = psutil.Process(os.getpid())
        return int(process.memory_info().rss / (1024 * 1024))
    except Exception:
        pass

    if sys.platform == "win32":
        try:
            import ctypes
            from ctypes import wintypes

            class PROCESS_MEMORY_COUNTERS(ctypes.Structure):
                _fields_ = [
                    ("cb", wintypes.DWORD),
                    ("PageFaultCount", wintypes.DWORD),
                    ("PeakWorkingSetSize", ctypes.c_size_t),
                    ("WorkingSetSize", ctypes.c_size_t),
                    ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
                    ("QuotaPagedPoolUsage", ctypes.c_size_t),
                    ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                    ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
                    ("PagefileUsage", ctypes.c_size_t),
                    ("PeakPagefileUsage", ctypes.c_size_t),
                ]

            pmc = PROCESS_MEMORY_COUNTERS()
            pmc.cb = ctypes.sizeof(PROCESS_MEMORY_COUNTERS)
            kernel32 = ctypes.WinDLL("kernel32")
            kernel32.GetCurrentProcess.restype = wintypes.HANDLE
            psapi = ctypes.WinDLL("psapi")
            psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.POINTER(PROCESS_MEMORY_COUNTERS), wintypes.DWORD]
            psapi.GetProcessMemoryInfo.restype = wintypes.BOOL
            h_proc = kernel32.GetCurrentProcess()
            if psapi.GetProcessMemoryInfo(h_proc, ctypes.byref(pmc), pmc.cb):
                return int(pmc.WorkingSetSize / (1024 * 1024))
        except Exception:
            pass

    return 0


def get_vram_stats_mb() -> Tuple[Optional[int], Optional[int]]:
    """Get (vram_total_mb, vram_free_mb) from PyTorch if CUDA is active."""
    try:
        import torch
        if torch.cuda.is_available():
            free_bytes, total_bytes = torch.cuda.mem_get_info()
            return int(total_bytes / (1024 * 1024)), int(free_bytes / (1024 * 1024))
    except Exception:
        pass
    return None, None
