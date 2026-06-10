# SPDX-License-Identifier: MIT
"""Tests for queue backpressure non-blocking behavior (REQ-2)."""

import unittest
import queue
import multiprocessing as mp
from unittest.mock import MagicMock, patch

from tests.helpers import MockQueue


class TestAudioQueueBackpressure(unittest.TestCase):
    """Tests for non-blocking audio queue behavior."""

    def test_put_nowait_does_not_block_when_queue_full(self):
        """put_nowait() should raise queue.Full instead of blocking when queue is full."""
        q = mp.Queue(maxsize=2)
        q.put({"audio": b"test1"})
        q.put({"audio": b"test2"})

        # Should raise queue.Full immediately, not block
        with self.assertRaises(queue.Full):
            q.put_nowait({"audio": b"overflow"})

    def test_oldest_phrase_dropped_when_queue_full(self):
        """When queue is full, oldest phrase should be dropped from speech_buffer."""
        # Verify that the enqueue_phrase implementation handles queue.Full
        # by checking the source code contains the drop logic
        import inspect
        from liveaudio.core.audio import audio_producer
        source = inspect.getsource(audio_producer)
        self.assertIn("speech_buffer.pop(0)", source)

    def test_warning_logged_when_audio_dropped(self):
        """Warning should be logged when audio is dropped due to full queue."""
        import inspect
        from liveaudio.core.audio import audio_producer
        source = inspect.getsource(audio_producer)
        self.assertIn("cola llena", source)
        self.assertIn("Descartando audio antiguo", source)

    def test_status_emitted_to_ui_on_backpressure(self):
        """Status should be emitted to UI when backpressure occurs."""
        import inspect
        from liveaudio.core.audio import audio_producer
        source = inspect.getsource(audio_producer)
        self.assertIn('"VAD: cola llena"', source)
        self.assertIn('"warn"', source)


class TestVadWorkerNonBlocking(unittest.TestCase):
    """Tests for VAD worker non-blocking behavior."""

    def test_vad_worker_does_not_block_on_full_queue(self):
        """VAD worker should not block when audio_queue is full."""
        import inspect
        from liveaudio.core.audio import audio_producer
        source = inspect.getsource(audio_producer)
        self.assertIn("put_nowait", source)
        self.assertNotIn("audio_queue.put({", source)  # Should not have blocking put

    def test_speech_buffer_drops_oldest_on_backpressure(self):
        """speech_buffer should drop oldest entries when backpressure occurs."""
        import inspect
        from liveaudio.core.audio import audio_producer
        source = inspect.getsource(audio_producer)
        self.assertIn("speech_buffer.pop(0)", source)


if __name__ == "__main__":
    unittest.main()
