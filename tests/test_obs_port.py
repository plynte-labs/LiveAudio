"""Tests for OBS HTML port reading from query parameter (REQ-5)."""

import unittest
import os
import re


class TestOBSPortQueryParameter(unittest.TestCase):
    """Tests that subtitulos_obs.html reads port from URL query parameter."""

    def setUp(self):
        """Load subtitulos_obs.html."""
        html_path = os.path.join(os.path.dirname(__file__), "..", "subtitulos_obs.html")
        with open(html_path, "r", encoding="utf-8") as f:
            self.html_content = f.read()

    def test_reads_port_from_query_parameter(self):
        """HTML should read port from URL query parameter."""
        self.assertIn("URLSearchParams", self.html_content)
        self.assertIn("port", self.html_content.lower())

    def test_default_port_is_8765(self):
        """Default port should be 8765 if not specified."""
        self.assertIn("8765", self.html_content)

    def test_uses_dynamic_port_in_websocket_url(self):
        """WebSocket URL should use dynamic port variable."""
        # Should construct URL with the port variable using template literal
        self.assertIn("wsPort", self.html_content)
        self.assertIn("ws://127.0.0.1:", self.html_content)

    def test_strips_dangerous_unicode(self):
        """HTML should strip dangerous Unicode characters."""
        self.assertIn(r"\u202E", self.html_content)
        self.assertIn(r"\u200E", self.html_content)


if __name__ == "__main__":
    unittest.main()
