# SPDX-License-Identifier: MIT
"""Tests for VAD download error handling (REQ-7)."""

import unittest
from unittest.mock import patch, MagicMock
import queue

from liveaudio.core.audio import audio_producer


class TestVADErrorHandling(unittest.TestCase):
    """Tests for VAD model loading error handling."""

    @patch("liveaudio.core.audio.torch.hub.load")
    @patch("liveaudio.core.audio.sd.InputStream")
    def test_vad_load_failure_emits_error(self, mock_stream, mock_load):
        """VAD load failure should emit error status and clear log message."""
        mock_load.side_effect = RuntimeError("Failed to download VAD model")

        audio_queue = queue.Queue(maxsize=100)
        log_queue = queue.Queue()
        # vad_threshold is now a real, consumed config key; 0.5 preserves the
        # prior default behavior. The VAD load fails before the worker loop, so
        # the threshold has no behavioral effect on this error-path test.
        config = {
            "device_index": None,
            "vad_threshold": 0.5,
            "silence_timeout": 0.8,
            "max_chunk_duration": 5.0,
        }

        with self.assertRaises(RuntimeError):
            audio_producer(audio_queue, config, log_queue)

        # Collect log messages
        messages = []
        while not log_queue.empty():
            try:
                messages.append(log_queue.get_nowait())
            except queue.Empty:
                break

        # Check for error status
        error_found = any(
            m.get("type") == "status" and "error" in m.get("text", "").lower()
            for m in messages if isinstance(m, dict)
        )
        self.assertTrue(error_found, f"Expected error status, got: {messages}")

    @patch("liveaudio.core.audio.torch.hub.load")
    def test_vad_load_failure_logs_clear_message(self, mock_load):
        """VAD load failure should log a clear message about the issue."""
        mock_load.side_effect = RuntimeError("Network error")

        audio_queue = queue.Queue(maxsize=100)
        log_queue = queue.Queue()
        config = {
            "device_index": None,
            "vad_threshold": 0.5,
            "silence_timeout": 0.8,
            "max_chunk_duration": 5.0,
        }

        with self.assertRaises(RuntimeError):
            audio_producer(audio_queue, config, log_queue)

        # Collect log messages
        messages = []
        while not log_queue.empty():
            try:
                messages.append(log_queue.get_nowait())
            except queue.Empty:
                break

        # Check for clear error message mentioning VAD
        vad_error_found = any(
            (isinstance(m, dict) and "VAD" in m.get("message", "")) or
            (isinstance(m, str) and "VAD" in m)
            for m in messages
        )
        self.assertTrue(vad_error_found, f"Expected VAD error log, got: {messages}")


if __name__ == "__main__":
    unittest.main()
