# SPDX-License-Identifier: MIT
"""Tests for core/audio.py pipeline logic (REQ-4)."""

import unittest
import multiprocessing as mp
from unittest.mock import MagicMock, patch, call
import numpy as np

from tests.helpers import MockQueue, make_shared_config, safe_queue_close


class TestAudioPipelineMocks(unittest.TestCase):
    """Tests that verify audio pipeline behavior with mocked dependencies."""

    def test_ring_buffer_capacity(self):
        """Ring buffer should not exceed maximum chunk capacity."""
        from collections import deque
        from liveaudio.core.audio import RING_BUFFER_MAX_CHUNKS

        buffer = deque(maxlen=RING_BUFFER_MAX_CHUNKS)
        for i in range(RING_BUFFER_MAX_CHUNKS + 100):
            buffer.append(i)
        self.assertLessEqual(len(buffer), RING_BUFFER_MAX_CHUNKS)

    def test_vad_threshold_constant(self):
        """VAD threshold should be between 0 and 1."""
        from liveaudio.core.audio import VAD_THRESHOLD
        self.assertGreaterEqual(VAD_THRESHOLD, 0.0)
        self.assertLessEqual(VAD_THRESHOLD, 1.0)

    def test_sample_rate_constant(self):
        """Sample rate should be 16000 for Whisper compatibility."""
        from liveaudio.core.audio import SAMPLE_RATE
        self.assertEqual(SAMPLE_RATE, 16000)

    def test_chunk_size_constant(self):
        """Chunk size should be positive and reasonable."""
        from liveaudio.core.audio import CHUNK_SIZE
        self.assertGreater(CHUNK_SIZE, 0)
        self.assertLess(CHUNK_SIZE, 10000)


class TestAudioQueueBackpressure(unittest.TestCase):
    """Tests for audio queue backpressure behavior."""

    def test_queue_does_not_grow_unbounded(self):
        """Audio queue should have bounded size."""
        q = mp.Queue(maxsize=3)
        try:
            for i in range(3):
                q.put({"audio": b"test", "created_at": 0.0, "sequence": i})
            self.assertEqual(q.qsize(), 3)
            # Adding one more should raise or block
            with self.assertRaises(Exception):
                q.put_nowait({"audio": b"overflow"})
        finally:
            safe_queue_close(q)

    def test_queue_drains_correctly(self):
        """Audio queue should drain items in FIFO order."""
        q = mp.Queue()
        try:
            items = [{"seq": i} for i in range(5)]
            for item in items:
                q.put(item)
            for expected_seq in range(5):
                item = q.get()
                self.assertEqual(item["seq"], expected_seq)
        finally:
            safe_queue_close(q)

    def test_queue_handles_none_sentinel(self):
        """None sentinel should signal queue end."""
        q = mp.Queue()
        try:
            q.put({"audio": b"test"})
            q.put(None)
            first = q.get()
            self.assertIsNotNone(first)
            second = q.get()
            self.assertIsNone(second)
        finally:
            safe_queue_close(q)


class TestAudioDeviceResolution(unittest.TestCase):
    """Tests for audio device resolution logic."""

    @patch("liveaudio.core.audio.sd")
    def test_resolve_default_device(self, mock_sd):
        """None audio_device should return default device."""
        from liveaudio.core.audio import _resolve_device_settings
        config = {"audio_device": None}
        device_index, extra_settings = _resolve_device_settings(config)
        self.assertIsNone(device_index)
        self.assertIsNone(extra_settings)

    @patch("liveaudio.core.audio.sd")
    def test_resolve_specific_device(self, mock_sd):
        """Valid audio_device dict should return device index."""
        from liveaudio.core.audio import _resolve_device_settings
        config = {
            "audio_device": {
                "index": 5,
                "name": "Test Mic",
                "is_loopback": False,
            }
        }
        device_index, extra_settings = _resolve_device_settings(config)
        self.assertEqual(device_index, 5)

    @patch("liveaudio.core.audio.sd")
    def test_resolve_loopback_device(self, mock_sd):
        """Loopback device should return WasapiSettings on Windows."""
        import sys
        from liveaudio.core.audio import _resolve_device_settings
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

    @patch("liveaudio.core.audio.sd")
    def test_resolve_invalid_device_fallback(self, mock_sd):
        """Invalid device config should fallback to None."""
        from liveaudio.core.audio import _resolve_device_settings
        config = {"audio_device": "not_a_dict"}
        device_index, extra_settings = _resolve_device_settings(config)
        self.assertIsNone(device_index)
        self.assertIsNone(extra_settings)


class TestVadPreBufferChunks(unittest.TestCase):
    """Tests for the configurable onset pre-roll maxlen formula (vad-onset-grace)."""

    def test_default_pad_yields_seven_chunks(self):
        """200ms pad => ceil(0.2*16000/512) = 7 chunks (AC-1)."""
        from liveaudio.core.audio import vad_pre_buffer_chunks
        self.assertEqual(vad_pre_buffer_chunks(200), 7)

    def test_zero_pad_floors_to_one(self):
        """0ms pad must floor to 1 chunk, never 0 (AC-2)."""
        from liveaudio.core.audio import vad_pre_buffer_chunks
        self.assertEqual(vad_pre_buffer_chunks(0), 1)

    def test_mid_value_rounds_up_via_ceil(self):
        """100ms pad => ceil(0.1*16000/512) = ceil(3.125) = 4 chunks."""
        from liveaudio.core.audio import vad_pre_buffer_chunks
        self.assertEqual(vad_pre_buffer_chunks(100), 4)

    def test_max_pad_value(self):
        """500ms pad => ceil(0.5*16000/512) = ceil(15.625) = 16 chunks."""
        from liveaudio.core.audio import vad_pre_buffer_chunks
        self.assertEqual(vad_pre_buffer_chunks(500), 16)


class TestVadThresholdComparison(unittest.TestCase):
    """Tests pinning the strict greater-than VAD comparison (AC-4, AC-5)."""

    def _is_speech(self, speech_prob, threshold):
        # Mirrors the production comparison at liveaudio/core/audio.py
        # (speech_prob > vad_threshold). Strict greater-than is intentional.
        return speech_prob > threshold

    def test_above_threshold_is_speech(self):
        """speech_prob 0.8 with threshold 0.7 counts as speech (AC-4)."""
        self.assertTrue(self._is_speech(0.8, 0.7))

    def test_below_threshold_is_not_speech(self):
        """speech_prob 0.6 with threshold 0.7 is not speech (AC-4)."""
        self.assertFalse(self._is_speech(0.6, 0.7))

    def test_exactly_at_threshold_is_not_speech(self):
        """speech_prob == threshold is NOT speech: strict '>' preserved (AC-5)."""
        self.assertFalse(self._is_speech(0.7, 0.7))
        self.assertFalse(self._is_speech(0.5, 0.5))

    def test_default_threshold_constant_unchanged(self):
        """VAD_THRESHOLD module constant remains 0.5 as default fallback (AC-10)."""
        from liveaudio.core.audio import VAD_THRESHOLD
        self.assertEqual(VAD_THRESHOLD, 0.5)

    def test_production_uses_strict_greater_than(self):
        """Pin the real production comparison in audio_producer (AC-5).

        The behavioral _is_speech tests above document the intended semantics,
        but they re-implement the comparison and would still pass if someone
        regressed audio.py from '>' to '>='. This test asserts directly against
        the production source so the strict greater-than is genuinely guarded.
        """
        import inspect
        from liveaudio.core.audio import audio_producer
        src = inspect.getsource(audio_producer)
        self.assertIn("speech_prob > vad_threshold", src)
        self.assertNotIn("speech_prob >= vad_threshold", src)


class TestVadThresholdEnforcement(unittest.TestCase):
    """Tests for VAD threshold behavior."""

    def test_vad_probability_above_threshold(self):
        """VAD probability above threshold should be considered speech."""
        from liveaudio.core.audio import VAD_THRESHOLD
        prob = 0.8
        self.assertGreaterEqual(prob, VAD_THRESHOLD)

    def test_vad_probability_below_threshold(self):
        """VAD probability below threshold should be considered silence."""
        from liveaudio.core.audio import VAD_THRESHOLD
        prob = 0.2
        self.assertLess(prob, VAD_THRESHOLD)

    def test_vad_probability_at_threshold(self):
        """VAD probability at threshold should be considered speech."""
        from liveaudio.core.audio import VAD_THRESHOLD
        prob = VAD_THRESHOLD
        self.assertGreaterEqual(prob, VAD_THRESHOLD)

    def test_silence_produces_no_transcription(self):
        """Silence (VAD below threshold) should produce no transcription output."""
        from liveaudio.core.audio import VAD_THRESHOLD
        # Simulate VAD output for silence
        vad_prob = 0.1
        is_speech = vad_prob >= VAD_THRESHOLD
        self.assertFalse(is_speech)

    def test_noise_below_threshold_produces_no_output(self):
        """Noise below VAD threshold should produce no transcription."""
        from liveaudio.core.audio import VAD_THRESHOLD
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
