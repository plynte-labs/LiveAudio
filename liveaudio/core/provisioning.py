# SPDX-License-Identifier: MIT
"""First-use startup progress (track firstuse-startup-progress_20260905).

Stdlib-only shared helpers for the honest ASR provisioning path:

- honest ``asr_state`` values + legacy mirror for OpenCohost compat (D2),
- structured ``{phase,percent,attempt,code}`` progress wire format (D1),
- tqdm percent parsing with monotonic clamp per attempt (D1/D3),
- ``provision-*`` error catalog with ``model-not-found`` reserved (D7),
- conservative stall watchdog defaults (D3).

Privacy: nothing here touches transcripts, audio, or paths — percents,
phases, attempts, and stable codes only.
"""

import re

HONEST_ASR_STATES = (
    "downloading",
    "loading",
    "transcribing",
    "ready",
    "stalled",
    "failed",
)

_LEGACY_MAP = {
    "downloading": "loading",
    "loading": "loading",
    "transcribing": "loading",
    "stalled": "loading",
    "ready": "ready",
    "failed": "failed",
}

# D3: stall = 120-180s with zero events/progress (default inside the range).
STALL_MIN_SEC = 120.0
STALL_MAX_SEC = 180.0
DEFAULT_STALL_SEC = 150.0
# Informative absolute startup deadline: advisory copy + retry offer, never a kill.
ABSOLUTE_STARTUP_SEC = 300.0

# D7: "model-not-found" is RESERVED for real absence; everything else is provision-*.
MODEL_NOT_FOUND_CODE = "model-not-found"
PROVISION_CODES = (
    MODEL_NOT_FOUND_CODE,
    "provision-cache-corrupt",
    "provision-network",
    "provision-auth",
    "provision-disk-full",
    "provision-timeout-stalled",
    "provision-tls",
    "provision-unknown",
)

PROVISION_I18N_KEYS = {
    "model-not-found": "model_not_found_hint",
    "provision-cache-corrupt": "provision_cache_corrupt_hint",
    "provision-network": "provision_network_hint",
    "provision-auth": "provision_auth_hint",
    "provision-disk-full": "provision_disk_full_hint",
    "provision-timeout-stalled": "provision_timeout_stalled_hint",
    "provision-tls": "provision_tls_hint",
    "provision-unknown": "provision_unknown_hint",
}

_PERCENT_RE = re.compile(r"(\d+(?:\.\d+)?)\s*%")


def asr_state_legacy(state):
    """Collapse an honest state to the legacy mirror (loading/ready/failed)."""
    return _LEGACY_MAP.get(state, "loading")


def parse_tqdm_percent(line):
    """Extract a float percent 0-100 from a tqdm-style line, else None."""
    if not line:
        return None
    match = _PERCENT_RE.search(str(line))
    if not match:
        return None
    try:
        return clamp_percent(float(match.group(1)))
    except (TypeError, ValueError):
        return None


def clamp_percent(value):
    """Clamp a percent to the 0-100 float range."""
    try:
        number = float(value)
    except (TypeError, ValueError):
        return 0.0
    if number != number:  # NaN guard
        return 0.0
    return max(0.0, min(100.0, number))


def monotonic_percent(previous, candidate, attempt_changed=False):
    """Monotonic percent per attempt: reset to 0 exactly once on retry.

    Same attempt: never regress (clamp to previous). New attempt: accept the
    candidate (the caller resets to 0 first, then this stays monotonic).
    """
    candidate = clamp_percent(candidate)
    if attempt_changed:
        return candidate
    try:
        previous = clamp_percent(previous)
    except (TypeError, ValueError):
        return candidate
    return max(previous, candidate)


def _message_blob(exc):
    try:
        return ("%s %s" % (type(exc).__name__, exc)).lower()
    except Exception:
        return ""


def classify_provisioning_error(exc):
    """Map a provisioning failure to a stable ``provision-*`` code.

    Never includes raw tracebacks or user text — callers log only the
    sanitized exception class name alongside the code.
    """
    blob = _message_blob(exc)
    name = type(exc).__name__ if exc is not None else ""

    if isinstance(exc, FileNotFoundError) or (
        ("not found" in blob or "no such file" in blob)
        and not any(k in blob for k in ("permission", "network", "connect", "tls", "ssl"))
    ):
        if "hugging" not in blob and "http" not in blob and "url" not in blob:
            return MODEL_NOT_FOUND_CODE

    if "no space" in blob or "disk full" in blob or "enospc" in blob:
        return "provision-disk-full"
    if "tls" in blob or "ssl" in blob or "certificate" in blob:
        return "provision-tls"
    if isinstance(exc, PermissionError) or any(
        k in blob for k in ("auth", "denied", "unauthorized", "forbidden", "401", "403")
    ):
        return "provision-auth"
    if isinstance(exc, (ConnectionError, TimeoutError)) or any(
        k in blob for k in ("connect", "network", "unreachable", "dns", "urlerror",
                            "max retries", "could not resolve", "timed out", "timeout")
    ):
        if "timed out" in blob or "timeout" in blob or isinstance(exc, TimeoutError):
            return "provision-timeout-stalled"
        return "provision-network"
    if any(k in blob for k in ("corrupt", "checksum", "bad file", "invalid cache")):
        return "provision-cache-corrupt"
    _ = name
    return "provision-unknown"


def build_progress_event(phase, percent, attempt, code, text):
    """Build the approved structured progress event (D1/D2/D3 wire format)."""
    percent_value = None if percent is None else clamp_percent(percent)
    return {
        "type": "status",
        "key": "asr",
        "state": phase,
        "phase": phase,
        "percent": percent_value,
        "attempt": attempt,
        "code": code,
        "is_download": bool(phase == "downloading"),
        "text": text,
        "asr_state_legacy": asr_state_legacy(phase),
    }

class PreparationProgress:
    """Bounded, path-free status callbacks; heartbeat is stopped before ready."""

    def __init__(self, emit, attempt=1, interval=5.0):
        import threading
        self.emit = emit
        self.attempt = attempt
        self.interval = interval
        self._stop = threading.Event()
        self._lock = threading.Lock()
        self._event = None
        self._last_emit = 0.0
        self._thread = threading.Thread(target=self._heartbeat, daemon=True)

    def __enter__(self):
        self._thread.start()
        return self

    def __exit__(self, *args):
        self._stop.set()
        self._thread.join()

    def report(self, phase, text, bytes_available=None, force=False):
        import time
        with self._lock:
            if self._stop.is_set():
                return
            event = build_progress_event(phase, None, self.attempt, None, text)
            if bytes_available is not None:
                event['bytes_available'] = int(bytes_available)
            changed = self._event is None or self._event['text'] != text
            self._event = event
            now = time.monotonic()
            if force or (changed and bytes_available is None) or now - self._last_emit >= 0.5:
                self.emit(event)
                self._last_emit = now

    def _heartbeat(self):
        while not self._stop.wait(self.interval):
            with self._lock:
                if self._event is not None:
                    self.emit(dict(self._event))


def prepare_model(model_name, emit, attempt=1):
    """Resolve the snapshot before loading a device, without assuming cache errors."""
    import os
    from faster_whisper.utils import available_models, download_model
    from huggingface_hub import snapshot_download
    from huggingface_hub.utils import LocalEntryNotFoundError
    from tqdm.auto import tqdm

    def report(text, phase='loading', bytes_available=None, force=False):
        event = build_progress_event(phase, None, attempt, None, text)
        if bytes_available is not None:
            event['bytes_available'] = bytes_available
        if force:
            event['force'] = True
        emit(event)

    report('ASR: consultando caché local')
    if os.path.isdir(model_name):
        return model_name
    try:
        path = download_model(model_name, local_files_only=True)
        # A snapshot directory alone can be an incomplete previous download.
        if all(os.path.isfile(os.path.join(path, name)) for name in ('model.bin', 'config.json', 'tokenizer.json')):
            return path
    except LocalEntryNotFoundError:
        pass

    # Public available_models validates supported aliases. Unknown future aliases
    # use faster-whisper's own resolver rather than guessing a repository.
    repos = {
        'large': 'Systran/faster-whisper-large-v3',
        'turbo': 'mobiuslabsgmbh/faster-whisper-large-v3-turbo',
        'large-v3-turbo': 'mobiuslabsgmbh/faster-whisper-large-v3-turbo',
        'distil-large-v3.5': 'distil-whisper/distil-large-v3.5-ct2',
    }
    known = {'tiny', 'tiny.en', 'base', 'base.en', 'small', 'small.en',
             'medium', 'medium.en', 'large-v1', 'large-v2', 'large-v3',
             'distil-large-v2', 'distil-medium.en', 'distil-small.en', 'distil-large-v3'}
    if model_name in known and model_name in available_models():
        prefix = 'faster-' if model_name.startswith('distil-') else 'faster-whisper-'
        repos[model_name] = 'Systran/' + prefix + model_name.replace('distil-', 'distil-whisper-', 1)
    repo = model_name if '/' in model_name else repos.get(model_name)
    report('ASR: consultando Hugging Face; preparando descarga')
    if repo is None:
        report('ASR: descarga en curso (sin contador disponible)', 'downloading')
        return download_model(model_name)

    class ByteProgress(tqdm):
        def __init__(self, *args, **kwargs):
            import threading
            self._byte_lock = threading.Lock()
            self._closed = False
            self._bytes = 0
            self._byte_bar = kwargs.get('unit') == 'B'
            kwargs['disable'] = True
            super().__init__(*args, **kwargs)

        def update(self, n=1):
            super().update(n)
            with self._byte_lock:
                if not self._closed and self._byte_bar and n and n > 0:
                    self._bytes += n
                    report('ASR: descargando (%.1f MiB disponibles)' % (self._bytes / 1048576),
                           'downloading', int(self._bytes))

        def close(self):
            with self._byte_lock:
                if not self._closed:
                    self._closed = True
                    if self._byte_bar and self._bytes:
                        report('ASR: descargando (%.1f MiB disponibles)' % (self._bytes / 1048576),
                               'downloading', int(self._bytes), force=True)
            super().close()

    report('ASR: descarga en curso (tamaño total pendiente)', 'downloading')
    return snapshot_download(repo, allow_patterns=[
        'config.json', 'preprocessor_config.json', 'model.bin',
        'tokenizer.json', 'vocabulary.*',
    ], tqdm_class=ByteProgress)
