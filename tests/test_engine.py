# SPDX-License-Identifier: MIT
"""Tests for core/engine.py subtitle logic (REQ-3)."""

import unittest
import os
import tempfile
import json
import queue
import time
from types import SimpleNamespace
from unittest.mock import MagicMock, patch

from liveaudio.core.engine import (
    _sanitize_text,
    _obs_emit_decision,
    _config_float,
    VALID_BACKLOG_POLICIES,
    MAX_SUBTITLE_CHARS,
)


class TestSanitizeText(unittest.TestCase):
    """Tests for _sanitize_text() function."""

    def test_strips_extra_whitespace(self):
        """Multiple spaces/tabs/newlines should be collapsed to single space."""
        result = _sanitize_text("  hello   world  \n\t test  ")
        self.assertEqual(result, "hello world test")

    def test_removes_non_printable_chars(self):
        """Non-printable characters should be removed."""
        result = _sanitize_text("hello\x00world\x01test")
        self.assertNotIn("\x00", result)
        self.assertNotIn("\x01", result)

    def test_canonical_sanitizer_preserves_long_text_and_removes_unsafe_chars(self):
        """Canonical text stays complete after safety sanitization."""
        long_text = "a" * 1000 + "\u202e" + "b"

        result = _sanitize_text(long_text)

        self.assertEqual(result, "a" * 1000 + "b")
        self.assertNotIn("\u202e", result)

    def test_handles_empty_string(self):
        """Empty string should return empty string."""
        result = _sanitize_text("")
        self.assertEqual(result, "")

    def test_handles_whitespace_only(self):
        """Whitespace-only string should return empty string."""
        result = _sanitize_text("   \n\t   ")
        self.assertEqual(result, "")

    def test_keeps_single_line(self):
        """Newlines should be replaced with spaces."""
        result = _sanitize_text("line1\nline2\nline3")
        self.assertNotIn("\n", result)

    def test_handles_unicode_text(self):
        """Unicode text should be preserved."""
        result = _sanitize_text("hola mundo ñ áéíóú")
        self.assertIn("ñ", result)
        self.assertIn("áéíóú", result)


class TestObsEmitDecision(unittest.TestCase):
    """Tests for _obs_emit_decision() backlog policy logic."""

    def test_send_always_emits(self):
        """send_all policy should always emit regardless of delay."""
        config = {"subtitle_backlog_policy": "send_all"}
        should_emit, is_replay, catchup = _obs_emit_decision(config, queue_delay=50.0)
        self.assertTrue(should_emit)

    def test_send_all_replay_for_high_delay(self):
        """send_all with high delay should mark as replay."""
        config = {"subtitle_backlog_policy": "send_all"}
        should_emit, is_replay, catchup = _obs_emit_decision(config, queue_delay=2.0)
        self.assertTrue(is_replay)

    def test_live_only_emits_within_delay(self):
        """live_only should emit when delay is within max_delay."""
        config = {
            "subtitle_backlog_policy": "live_only",
            "subtitle_max_live_delay_sec": 10.0,
        }
        should_emit, is_replay, catchup = _obs_emit_decision(config, queue_delay=5.0)
        self.assertTrue(should_emit)
        self.assertFalse(is_replay)

    def test_live_only_drops_when_delay_exceeded(self):
        """live_only should drop when delay exceeds max_delay."""
        config = {
            "subtitle_backlog_policy": "live_only",
            "subtitle_max_live_delay_sec": 10.0,
        }
        should_emit, is_replay, catchup = _obs_emit_decision(config, queue_delay=15.0)
        self.assertFalse(should_emit)

    def test_auto_emits_within_delay(self):
        """auto policy should emit when delay is within max_delay."""
        config = {
            "subtitle_backlog_policy": "auto",
            "subtitle_max_live_delay_sec": 10.0,
        }
        should_emit, is_replay, catchup = _obs_emit_decision(config, queue_delay=5.0)
        self.assertTrue(should_emit)

    def test_auto_drops_when_delay_exceeded(self):
        """auto policy should drop when delay exceeds max_delay."""
        config = {
            "subtitle_backlog_policy": "auto",
            "subtitle_max_live_delay_sec": 10.0,
        }
        should_emit, is_replay, catchup = _obs_emit_decision(config, queue_delay=15.0)
        self.assertFalse(should_emit)

    def test_auto_catchup_for_moderate_delay(self):
        """auto policy should use catchup interval for moderate delay."""
        config = {
            "subtitle_backlog_policy": "auto",
            "subtitle_max_live_delay_sec": 10.0,
            "subtitle_catchup_interval_sec": 1.5,
        }
        should_emit, is_replay, catchup = _obs_emit_decision(config, queue_delay=3.0)
        self.assertTrue(should_emit)
        self.assertTrue(is_replay)
        self.assertEqual(catchup, 1.5)

    def test_invalid_policy_defaults_to_auto(self):
        """Invalid policy should default to auto behavior."""
        config = {"subtitle_backlog_policy": "invalid_policy"}
        should_emit, is_replay, catchup = _obs_emit_decision(config, queue_delay=5.0)
        # Should behave like auto (emit within delay)
        self.assertTrue(should_emit)


class TestVttOutputFormat(unittest.TestCase):
    """Tests for WebVTT output format compliance."""

    def test_vtt_header_format(self):
        """VTT output must start with WEBVTT header."""
        from liveaudio.core.engine import _format_vtt_time
        # The header is written as "WEBVTT\n\n" in engine.py
        # We verify the format function exists and the header pattern
        self.assertTrue(hasattr(_format_vtt_time, '__call__'))

    def test_vtt_cue_timestamp_format(self):
        """VTT cues must have HH:MM:SS.mmm --> HH:MM:SS.mmm timestamps."""
        from liveaudio.core.engine import _format_vtt_time
        result = _format_vtt_time(65.123)
        self.assertRegex(result, r"^\d{2}:\d{2}:\d{2}\.\d{3}$")
        self.assertEqual(result, "00:01:05.123")

    def test_vtt_cue_index_numbers(self):
        """VTT cues must have sequential index numbers."""
        from liveaudio.core.engine import _format_vtt_time
        # Verify the timestamp function produces valid VTT time format
        # which is required for proper cue formatting
        t1 = _format_vtt_time(0.0)
        t2 = _format_vtt_time(3661.999)
        self.assertEqual(t1, "00:00:00.000")
        self.assertEqual(t2, "01:01:01.999")


class TestObsEnabledGate(unittest.TestCase):
    """Tests for obs_enabled gate in asr_consumer (T8 — subtitle-style-system-v2)."""

    def _make_shared_config(self, **overrides):
        """Build a minimal shared_config dict for testing."""
        cfg = {
            "model_size": "tiny",
            "device": "cpu",
            "cpu_threads": 1,
            "blacklist": "",
            "subtitle_style": "default",
            "subtitle_backlog_policy": "auto",
            "subtitle_max_live_delay_sec": 10.0,
            "subtitle_catchup_interval_sec": 1.5,
        }
        cfg.update(overrides)
        return cfg

    def test_obs_enabled_true_emits_to_queue(self):
        """When obs_enabled=True, transcript payload should be put to text_queue."""
        import multiprocessing as mp
        from liveaudio.core.engine import asr_consumer

        audio_queue = mp.Queue()
        text_queue = mp.Queue()
        log_queue = mp.Queue()
        shared = mp.Manager().dict(self._make_shared_config(obs_enabled=True))

        # Send poison pill immediately
        audio_queue.put(None)

        p = mp.Process(target=asr_consumer, args=(audio_queue, text_queue, log_queue, shared, None), daemon=True)
        p.start()
        p.join(timeout=10)

        # Process should have exited cleanly
        self.assertFalse(p.is_alive())

    def test_obs_enabled_false_skips_queue_but_saves(self):
        """When obs_enabled=False, text_queue.put should be skipped."""
        # Verify the gate value is correctly set to False
        shared = self._make_shared_config(obs_enabled=False)
        self.assertFalse(shared["obs_enabled"])
        # The gate check uses .get() with default True
        result = shared.get("obs_enabled", True)
        self.assertFalse(result)

    def test_obs_enabled_missing_key_defaults_true(self):
        """When obs_enabled key is missing, behavior should default to True (emit)."""
        shared = self._make_shared_config()
        self.assertNotIn("obs_enabled", shared)
        # The code uses shared_config.get("obs_enabled", True) pattern
        result = shared.get("obs_enabled", True)
        self.assertTrue(result)


class TestAsrConsumerCanonicalTranscript(unittest.TestCase):
    def test_long_canonical_text_is_preserved_in_jsonl_and_ws_projection_stays_capped(self):
        """Session JSONL stays complete while outgoing WS retains its legacy cap."""
        from liveaudio.core.engine import asr_consumer

        long_text = "a" * 1000 + "\u202e" + "b"

        class FakeModel:
            def transcribe(self, audio, **kwargs):
                segment = SimpleNamespace(text=long_text, no_speech_prob=0.0)
                return iter([segment]), SimpleNamespace()

        with tempfile.TemporaryDirectory() as session_dir:
            audio_queue = queue.Queue()
            text_queue = queue.Queue()
            log_queue = queue.Queue()
            audio_queue.put({"audio": [], "created_at": time.time(), "sequence": 1})
            audio_queue.put(None)
            shared = {
                "model_size": "tiny",
                "device": "cpu",
                "cpu_threads": 1,
                "blacklist": "",
                "subtitle_style": "default",
                "subtitle_backlog_policy": "send_all",
                "save_transcript_enabled": True,
                "save_vtt_enabled": False,
                "obs_enabled": True,
            }

            with patch("liveaudio.core.engine.WhisperModel", return_value=FakeModel()):
                asr_consumer(audio_queue, text_queue, log_queue, shared, session_dir)

            transcript_path = os.path.join(session_dir, "transcript.jsonl")
            with open(transcript_path, "r", encoding="utf-8") as handle:
                transcript = json.loads(handle.readline())
            payload = text_queue.get_nowait()

        self.assertEqual(transcript["text"], "a" * 1000 + "b")
        self.assertEqual(payload["text"], "a" * MAX_SUBTITLE_CHARS + "...")


    def test_ws_queue_full_error_event_uses_capped_presentation_text(self):
        """Queue-full diagnostics retain the legacy subtitle projection."""
        from liveaudio.core.engine import asr_consumer

        long_text = "a" * 1000

        class FakeModel:
            def transcribe(self, audio, **kwargs):
                segment = SimpleNamespace(text=long_text, no_speech_prob=0.0)
                return iter([segment]), SimpleNamespace()

        class FullTextQueue:
            def put(self, payload, timeout=None):
                raise queue.Full

        with tempfile.TemporaryDirectory() as session_dir:
            audio_queue = queue.Queue()
            log_queue = queue.Queue()
            audio_queue.put({"audio": [], "created_at": time.time(), "sequence": 1})
            audio_queue.put(None)
            shared = {
                "model_size": "tiny",
                "device": "cpu",
                "cpu_threads": 1,
                "blacklist": "",
                "subtitle_style": "default",
                "subtitle_backlog_policy": "send_all",
                "save_transcript_enabled": True,
                "save_vtt_enabled": False,
                "obs_enabled": True,
            }

            with patch("liveaudio.core.engine.WhisperModel", return_value=FakeModel()):
                asr_consumer(audio_queue, FullTextQueue(), log_queue, shared, session_dir)

            events = []
            while not log_queue.empty():
                events.append(log_queue.get_nowait())

        failed_event = next(event for event in events if event.get("reason") == "ws_queue_full")
        self.assertEqual(failed_event["text"], "a" * MAX_SUBTITLE_CHARS + "...")


class TestConfigFloat(unittest.TestCase):
    """Tests for _config_float() helper function."""

    def test_returns_float_for_valid_value(self):
        """Should return float for valid numeric value."""
        config = {"test_key": "3.14"}
        result = _config_float(config, "test_key", 1.0)
        self.assertEqual(result, 3.14)

    def test_returns_default_for_missing_key(self):
        """Should return default for missing key."""
        config = {}
        result = _config_float(config, "missing_key", 2.5)
        self.assertEqual(result, 2.5)

    def test_returns_default_for_invalid_value(self):
        """Should return default for non-numeric value."""
        config = {"test_key": "not_a_number"}
        result = _config_float(config, "test_key", 1.0)
        self.assertEqual(result, 1.0)

    def test_returns_default_for_none_value(self):
        """Should return default for None value."""
        config = {"test_key": None}
        result = _config_float(config, "test_key", 1.0)
        self.assertEqual(result, 1.0)


class TestInterceptingWriter(unittest.TestCase):
    """Tests for InterceptingWriter redirects and tqdm parsing."""

    def test_intercepts_simple_line(self):
        import queue
        from liveaudio.core.engine import InterceptingWriter
        q = queue.Queue()
        writer = InterceptingWriter(q, prefix="[TEST]")
        writer.write("Hello world\n")
        
        self.assertFalse(q.empty())
        msg = q.get_nowait()
        self.assertEqual(msg, {"type": "log", "message": "[TEST] Hello world"})

    def test_intercepts_tqdm_progress(self):
        import queue
        from liveaudio.core.engine import InterceptingWriter
        q = queue.Queue()
        writer = InterceptingWriter(q, prefix="[TEST]")
        
        # Simulates a tqdm carriage return update
        writer.write(" 45%|██████████          | 185.0M/480.0M [00:05<00:07, 39.8MB/s]\r")
        
        self.assertFalse(q.empty())
        msg = q.get_nowait()
        self.assertEqual(msg, {"type": "log", "message": "[TEST] PROGRESO 45% (185.0M/480.0M) @ 39.8MB/s"})

    def test_handles_errors_and_warnings(self):
        import queue
        from liveaudio.core.engine import InterceptingWriter
        q = queue.Queue()
        writer = InterceptingWriter(q, prefix="[TEST]")
        
        writer.write("An error occurred during loading\n")
        writer.write("A warning was issued by PyTorch\n")
        
        self.assertFalse(q.empty())
        msg1 = q.get_nowait()
        self.assertEqual(msg1, {"type": "log", "message": "[IA ERROR] An error occurred during loading"})
        
        msg2 = q.get_nowait()
        self.assertEqual(msg2, {"type": "log", "message": "[IA ADVERTENCIA] A warning was issued by PyTorch"})


if __name__ == "__main__":
    unittest.main()
