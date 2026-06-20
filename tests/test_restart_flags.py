# SPDX-License-Identifier: MIT
"""Tests for _pending_restart_flags audio-restart triggers (vad-onset-grace, AC-8)."""

import types
import unittest

from liveaudio.app import LiveASRApp


def _make_stub(config_data):
    """Minimal stand-in exposing only what _pending_restart_flags reads."""
    return types.SimpleNamespace(config_data=config_data)


class TestPendingRestartFlags(unittest.TestCase):
    """_pending_restart_flags returns (needs_asr_restart, needs_audio_restart)."""

    def setUp(self):
        self.base = {
            "device": "cpu",
            "model_size": "small (Balance CPU)",
            "cpu_threads": 2,
            "audio_device": None,
            "silence_timeout": 0.8,
            "max_chunk_duration": 5.0,
            "vad_speech_pad_ms": 200,
            "vad_threshold": 0.5,
        }

    def _flags(self, draft):
        stub = _make_stub(dict(self.base))
        return LiveASRApp._pending_restart_flags(stub, draft)

    def test_no_change_no_restart(self):
        """Identical draft requires no restart."""
        needs_asr_restart, needs_audio_restart = self._flags(dict(self.base))
        self.assertFalse(needs_asr_restart)
        self.assertFalse(needs_audio_restart)

    def test_pad_change_triggers_audio_restart_only(self):
        """Changing only vad_speech_pad_ms triggers audio restart, not ASR (AC-8)."""
        draft = dict(self.base)
        draft["vad_speech_pad_ms"] = 100
        needs_asr_restart, needs_audio_restart = self._flags(draft)
        self.assertFalse(needs_asr_restart)
        self.assertTrue(needs_audio_restart)

    def test_threshold_change_triggers_audio_restart_only(self):
        """Changing only vad_threshold triggers audio restart, not ASR (AC-8)."""
        draft = dict(self.base)
        draft["vad_threshold"] = 0.7
        needs_asr_restart, needs_audio_restart = self._flags(draft)
        self.assertFalse(needs_asr_restart)
        self.assertTrue(needs_audio_restart)

    def test_model_change_triggers_asr_restart(self):
        """Sanity: model change still triggers ASR restart (tuple order asr-first)."""
        draft = dict(self.base)
        draft["model_size"] = "turbo (Máxima precisión GPU)"
        needs_asr_restart, needs_audio_restart = self._flags(draft)
        self.assertTrue(needs_asr_restart)
        self.assertFalse(needs_audio_restart)


if __name__ == "__main__":
    unittest.main()
