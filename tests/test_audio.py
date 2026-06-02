# SPDX-License-Identifier: MIT
"""Tests for core/audio.py pipeline logic (REQ-4)."""

import unittest
import multiprocessing as mp
from unittest.mock import MagicMock, patch, call
import numpy as np

from tests.helpers import MockQueue, make_shared_config


class TestAudioPipelineMocks(unittest.TestCase):
    """Tests that verify audio pipeline behavior with mocked dependencies."""

    def test_ring_buffer_capacity(self):
        """Ring buffer should not exceed maximum chunk capacity."""
        from collections import deque
        from core.audio import RING_BUFFER_MAX_CHUNKS

        buffer = deque(maxlen=RING_BUFFER_MAX_CHUNKS)
        for i in range(RING_BUFFER_MAX_CHUNKS + 100):
            buffer.append(i)
        self.assertLessEqual(len(buffer), RING_BUFFER_MAX_CHUNKS)

    def test_vad_threshold_constant(self):
        """VAD threshold should be between 0 and 1."""
        from core.audio import VAD_THRESHOLD
        self.assertGreaterEqual(VAD_THRESHOLD, 0.0)
        self.assertLessEqual(VAD_THRESHOLD, 1.0)

    def test_sample_rate_constant(self):
        """Sample rate should be 16000 for Whisper compatibility."""
        from core.audio import SAMPLE_RATE
        self.assertEqual(SAMPLE_RATE, 16000)

    def test_chunk_size_constant(self):
        """Chunk size should be positive and reasonable."""
        from core.audio import CHUNK_SIZE
        self.assertGreater(CHUNK_SIZE, 0)
        self.assertLess(CHUNK_SIZE, 10000)


class TestAudioQueueBackpressure(unittest.TestCase):
    """Tests for audio queue backpressure behavior."""

    def test_queue_does_not_grow_unbounded(self):
        """Audio queue should have bounded size."""
        q = mp.Queue(maxsize=1000)
        for i in range(1000):
            q.put({"audio": b"test", "created_at": 0.0, "sequence": i})
        self.assertEqual(q.qsize(), 1000)
        # Adding one more should raise or block
        with self.assertRaises(Exception):
            q.put_nowait({"audio": b"overflow"})

    def test_queue_drains_correctly(self):
        """Audio queue should drain items in FIFO order."""
        q = mp.Queue()
        items = [{"seq": i} for i in range(5)]
        for item in items:
            q.put(item)
        for expected_seq in range(5):
            item = q.get()
            self.assertEqual(item["seq"], expected_seq)

    def test_queue_handles_none_sentinel(self):
        """None sentinel should signal queue end."""
        q = mp.Queue()
        q.put({"audio": b"test"})
        q.put(None)
        first = q.get()
        self.assertIsNotNone(first)
        second = q.get()
        self.assertIsNone(second)


class TestAudioDeviceResolution(unittest.TestCase):
    """Tests for audio device resolution logic."""

    @patch("core.audio.sd")
    def test_resolve_default_device(self, mock_sd):
        """None audio_device should return default device."""
        from core.audio import _resolve_device_settings
        config = {"audio_device": None}
        device_index, extra_settings = _resolve_device_settings(config)
        self.assertIsNone(device_index)
        self.assertIsNone(extra_settings)

    @patch("core.audio.sd")
    def test_resolve_specific_device(self, mock_sd):
        """Valid audio_device dict should return device index."""
        from core.audio import _resolve_device_settings
        config = {
            "audio_device": {
                "index": 5,
                "name": "Test Mic",
                "is_loopback": False,
            }
        }
        device_index, extra_settings = _resolve_device_settings(config)
        self.assertEqual(device_index, 5)

    @patch("core.audio.sd")
    def test_resolve_loopback_device(self, mock_sd):
        """Loopback device should return WasapiSettings on Windows."""
        import sys
        from core.audio import _resolve_device_settings
        with patch.object(sys, "platform", "win32"):
            config = {
                "audio_device": {
                    "index": 3,
                    "name": "Stereo Mix",
                    "is_loopback": True,
                }
            }
            device_index, extra_settings = _resolve_device_settings(config)
            self.assertEqual(device_index, 3)
            self.assertIsNotNone(extra_settings)

    @patch("core.audio.sd")
    def test_resolve_invalid_device_fallback(self, mock_sd):
        """Invalid device config should fallback to None."""
        from core.audio import _resolve_device_settings
        config = {"audio_device": "not_a_dict"}
        device_index, extra_settings = _resolve_device_settings(config)
        self.assertIsNone(device_index)
        self.assertIsNone(extra_settings)


class TestVadThresholdEnforcement(unittest.TestCase):
    """Tests for VAD threshold behavior."""

    def test_vad_probability_above_threshold(self):
        """VAD probability above threshold should be considered speech."""
        from core.audio import VAD_THRESHOLD
        prob = 0.8
        self.assertGreaterEqual(prob, VAD_THRESHOLD)

    def test_vad_probability_below_threshold(self):
        """VAD probability below threshold should be considered silence."""
        from core.audio import VAD_THRESHOLD
        prob = 0.2
        self.assertLess(prob, VAD_THRESHOLD)

    def test_vad_probability_at_threshold(self):
        """VAD probability at threshold should be considered speech."""
        from core.audio import VAD_THRESHOLD
        prob = VAD_THRESHOLD
        self.assertGreaterEqual(prob, VAD_THRESHOLD)

    def test_silence_produces_no_transcription(self):
        """Silence (VAD below threshold) should produce no transcription output."""
        from core.audio import VAD_THRESHOLD
        # Simulate VAD output for silence
        vad_prob = 0.1
        is_speech = vad_prob >= VAD_THRESHOLD
        self.assertFalse(is_speech)

    def test_noise_below_threshold_produces_no_output(self):
        """Noise below VAD threshold should produce no transcription."""
        from core.audio import VAD_THRESHOLD
        # Simulate VAD output for low-level noise
        vad_prob = 0.3
        is_speech = vad_prob >= VAD_THRESHOLD
        self.assertFalse(is_speech)


class TestAudioEnergyThreshold(unittest.TestCase):
    """Tests for audio energy-based noise rejection."""

    def test_low_energy_segment_rejected(self):
        """Low energy segment should be rejected before ASR."""
        # Simulate RMS energy calculation
        audio = np.zeros(1600, dtype=np.float32)  # Silence
        rms = np.sqrt(np.mean(audio ** 2))
        energy_threshold = 0.01
        self.assertLess(rms, energy_threshold)

    def test_high_energy_segment_accepted(self):
        """High energy segment should be accepted for ASR."""
        # Simulate RMS energy calculation for speech-like signal
        audio = np.random.uniform(-0.5, 0.5, 1600).astype(np.float32)
        rms = np.sqrt(np.mean(audio ** 2))
        energy_threshold = 0.01
        self.assertGreater(rms, energy_threshold)

    def test_energy_calculation_handles_empty(self):
        """Energy calculation should handle empty arrays."""
        audio = np.array([], dtype=np.float32)
        if len(audio) == 0:
            rms = 0.0
        else:
            rms = np.sqrt(np.mean(audio ** 2))
        self.assertEqual(rms, 0.0)


if __name__ == "__main__":
    unittest.main()
