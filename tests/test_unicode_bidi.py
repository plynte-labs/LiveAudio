# SPDX-License-Identifier: MIT
"""Tests for Unicode bidi strip (REQ-4)."""

import unittest

from liveaudio.core.engine import _sanitize_text


class TestUnicodeBidiStrip(unittest.TestCase):
    """Tests for Unicode bidi override character stripping."""

    def test_u202e_stripped(self):
        """U+202E RIGHT-TO-LEFT OVERRIDE should be stripped."""
        result = _sanitize_text("hello\u202Eworld")
        self.assertNotIn("\u202E", result)

    def test_u202d_stripped(self):
        """U+202D LEFT-TO-RIGHT OVERRIDE should be stripped."""
        result = _sanitize_text("hello\u202Dworld")
        self.assertNotIn("\u202D", result)

    def test_u200e_stripped(self):
        """U+200E LEFT-TO-RIGHT MARK should be stripped."""
        result = _sanitize_text("hello\u200Eworld")
        self.assertNotIn("\u200E", result)

    def test_u200f_stripped(self):
        """U+200F RIGHT-TO-LEFT MARK should be stripped."""
        result = _sanitize_text("hello\u200Fworld")
        self.assertNotIn("\u200F", result)

    def test_u0000_stripped(self):
        """U+0000 NULL should be stripped."""
        result = _sanitize_text("hello\u0000world")
        self.assertNotIn("\u0000", result)

    def test_u001b_stripped(self):
        """U+001B ESCAPE should be stripped."""
        result = _sanitize_text("hello\u001bworld")
        self.assertNotIn("\u001b", result)

    def test_legitimate_text_preserved(self):
        """Legitimate text (accents, emojis) should be preserved."""
        result = _sanitize_text("hola mundo ñ áéíóú 🎤")
        self.assertIn("ñ", result)
        self.assertIn("áéíóú", result)

    def test_bidi_override_in_middle(self):
        """Bidi override in middle of text should be stripped."""
        result = _sanitize_text("palabra\u202Eotra")
        self.assertEqual(result, "palabraotra")

    def test_multiple_dangerous_chars(self):
        """Multiple dangerous characters should all be stripped."""
        result = _sanitize_text("\u202E\u202D\u200E\u200Fhello")
        self.assertEqual(result, "hello")


if __name__ == "__main__":
    unittest.main()
