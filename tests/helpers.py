# SPDX-License-Identifier: GPL-3.0-or-later
"""Common test helpers and mocks for LiveAudio test suite."""

import multiprocessing as mp
from unittest.mock import MagicMock, patch
import json


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
        "output_dir": "sessions",
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
