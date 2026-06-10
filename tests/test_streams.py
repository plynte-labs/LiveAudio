# SPDX-License-Identifier: MIT
"""Tests for liveaudio/utils/streams.py (encoding-safe child process logging).

Child processes (audio producer, ASR consumer, WebSocket server) print
messages containing non-ASCII characters ('→', '🎤'). When stdout uses a
legacy codepage (cp1252 pipe/console), the default strict handler raises
UnicodeEncodeError and kills the child at startup; make_streams_encoding_safe
must degrade characters instead of crashing.
"""

import io
import sys
import unittest
from unittest.mock import patch

from liveaudio.utils.streams import make_streams_encoding_safe


class TestMakeStreamsEncodingSafe(unittest.TestCase):
    """The child-log stdout fallback must survive legacy codepages."""

    def test_cp1252_stdout_would_crash_without_the_fix(self):
        """Sanity check documenting the bug this helper exists to fix."""
        stdout = io.TextIOWrapper(io.BytesIO(), encoding="cp1252")
        with self.assertRaises(UnicodeEncodeError):
            stdout.write("[Productor] callback → buffer 🎤")

    def test_non_ascii_print_does_not_crash_on_cp1252_stdout(self):
        buffer = io.BytesIO()
        stdout = io.TextIOWrapper(buffer, encoding="cp1252")
        stderr = io.TextIOWrapper(io.BytesIO(), encoding="cp1252")
        with patch.object(sys, "stdout", stdout), patch.object(sys, "stderr", stderr):
            make_streams_encoding_safe()
            # Same shape as _log() in liveaudio/core/audio.py: plain print().
            print("[Productor] Arquitectura: callback → buffer → VAD worker 🎤")
            sys.stdout.flush()
        written = buffer.getvalue()
        self.assertIn(b"[Productor] Arquitectura: callback ", written)
        self.assertTrue(written.endswith(b"\n") or written.endswith(b"\r\n"))

    def test_unencodable_characters_are_degraded_not_dropped(self):
        buffer = io.BytesIO()
        stdout = io.TextIOWrapper(buffer, encoding="cp1252")
        with patch.object(sys, "stdout", stdout):
            make_streams_encoding_safe()
            sys.stdout.write("a→b🎤c")
            sys.stdout.flush()
        # '→' and '🎤' become replacement chars; surrounding text survives.
        self.assertEqual(buffer.getvalue(), b"a?b?c")

    def test_none_streams_are_tolerated(self):
        """pythonw runs have sys.stdout/sys.stderr set to None."""
        with patch.object(sys, "stdout", None), patch.object(sys, "stderr", None):
            make_streams_encoding_safe()  # must not raise

    def test_stream_without_reconfigure_is_tolerated(self):
        """StringIO fallbacks and intercepting writers lack reconfigure()."""
        with patch.object(sys, "stdout", io.StringIO()):
            make_streams_encoding_safe()  # must not raise


if __name__ == "__main__":
    unittest.main()
