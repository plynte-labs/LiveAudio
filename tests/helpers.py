# SPDX-License-Identifier: MIT
"""Common test helpers and mocks for LiveAudio test suite."""

import threading
import multiprocessing as mp
from unittest.mock import MagicMock, patch
import json
from liveaudio.core.diagnostics import create_store_from_config


class MockQueue:
    """Mock for multiprocessing.Queue that works in single-process tests."""

    def __init__(self):
        self.items = []
        self.closed = False

    def put(self, item, timeout=None):
        if self.closed:
            raise Exception("Queue is closed")
        self.items.append(item)

    def put_nowait(self, item):
        self.put(item)

    def get(self, timeout=None):
        if not self.items:
            raise Exception("Queue empty")
        return self.items.pop(0)

    def get_nowait(self):
        return self.get()

    def empty(self):
        return len(self.items) == 0

    def qsize(self):
        return len(self.items)


def make_shared_config(overrides=None):
    """Create a shared config dict with sensible defaults for testing."""
    config = {
        "model_size": "small (Balance CPU)",
        "device": "cpu",
        "cpu_threads": 2,
        "blacklist": "amara.org, subtítulos por, suscríbete, dale like",
        "continuous_session": True,
        "subtitle_style": "default",
        "subtitle_backlog_policy": "auto",
        "subtitle_max_live_delay_sec": 10.0,
        "subtitle_catchup_interval_sec": 1.5,
        "silence_timeout": 0.8,
        "max_chunk_duration": 5.0,
        "vad_speech_pad_ms": 200,
        "vad_threshold": 0.5,
        "output_dir": "sessions",
        "diagnostics_enabled": True,
        "diagnostics_level": "deep",
    }
    if overrides:
        config.update(overrides)
    return config


def make_audio_item(audio_data, created_at=None, sequence=0):
    """Create a mock audio item dict for testing."""
    import time

    return {
        "audio": audio_data,
        "created_at": created_at or time.time(),
        "sequence": sequence,
    }


def make_mock_segment(text, no_speech_prob=0.0):
    """Create a mock ASR segment for testing."""
    seg = MagicMock()
    seg.text = text
    seg.no_speech_prob = no_speech_prob
    return seg


def make_mock_transcribe_result(segments=None, language="es"):
    """Create a mock transcribe result."""
    if segments is None:
        segments = []
    info = MagicMock()
    info.language = language
    return iter(segments), info


def safe_queue_close(queue_obj):
    """Best-effort multiprocessing queue cleanup for tests."""
    if queue_obj is None:
        return
    try:
        queue_obj.close()
    except Exception:
        pass
    try:
        queue_obj.join_thread()
    except Exception:
        pass


def summarize_test_resources(processes=None, threads=None, queues=None):
    """Return a bounded resource summary for local test-health diagnostics."""
    processes = [proc for proc in (processes or []) if proc is not None]
    threads = [thr for thr in (threads or []) if thr is not None]
    queues = [q for q in (queues or []) if q is not None]

    alive_processes = sum(1 for proc in processes if getattr(proc, "is_alive", lambda: False)())
    alive_threads = sum(1 for thr in threads if getattr(thr, "is_alive", lambda: False)())

    return {
        "alive_processes": alive_processes,
        "alive_threads": alive_threads,
        "queue_count": len(queues),
        "thread_names": [getattr(thr, "name", "unknown") for thr in threads],
        "process_names": [getattr(proc, "name", "unknown") for proc in processes],
    }


def build_test_health_snapshot(
    config=None,
    *,
    processes=None,
    threads=None,
    queues=None,
    file_timings=None,
    warnings=None,
):
    """Create a local-only test-health snapshot using diagnostics primitives."""
    config = make_shared_config(config or {})
    store = create_store_from_config(config)
    resource_summary = summarize_test_resources(processes=processes, threads=threads, queues=queues)
    return store.snapshot_test_health(
        resource_summary=resource_summary,
        file_timings=file_timings or {},
        warnings=warnings or [],
    )
