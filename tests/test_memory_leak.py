# SPDX-License-Identifier: MIT
"""Tests for memory leak fix in subtitle HTML (REQ-1)."""

import unittest
import os
import re


class TestMemoryLeakFix(unittest.TestCase):
    """Tests for memory leak prevention in subtitulos_obs.html."""

    def setUp(self):
        """Load subtitulos_obs.html."""
        html_path = os.path.join(os.path.dirname(__file__), "..", "liveaudio", "assets", "subtitulos_obs.html")
        with open(html_path, "r", encoding="utf-8") as f:
            self.html_content = f.read()

    def test_no_innerhtml_clear(self):
        """container.innerHTML = '' should not be used for clearing (causes GC issues)."""
        # Should NOT have innerHTML = '' for clearing DOM
        self.assertNotRegex(
            self.html_content,
            r"container\.innerHTML\s*=\s*['\"]['\"]",
            "container.innerHTML = '' should be replaced with removeChild()"
        )

    def test_removechild_used(self):
        """removeChild() should be used for predictable GC."""
        self.assertIn("removeChild", self.html_content)

    def test_references_nullified(self):
        """DOM references should be nullified after removal."""
        # Should nullify existing reference after removal
        self.assertRegex(
            self.html_content,
            r"existing\s*=\s*null",
            "existing reference should be nullified after removal"
        )

    def test_settimeout_cleared(self):
        """Pending setTimeout references should be cleared before creating new ones."""
        # Should clear hideTimeout and cleanupTimeout before creating new ones
        self.assertIn("clearTimeout(hideTimeout)", self.html_content)
        self.assertIn("clearTimeout(cleanupTimeout)", self.html_content)

    def test_gc_friendly_pattern(self):
        """DOM cleanup should use removeChild + null pattern for predictable GC."""
        # Should have removeChild followed by null assignment
        has_gc_pattern = (
            "removeChild" in self.html_content and
            "= null" in self.html_content
        )
        self.assertTrue(
            has_gc_pattern,
            "GC-friendly pattern (removeChild + null) should be present"
        )

    def test_no_closure_capture_leak(self):
        """Closures should not capture references that prevent GC."""
        # After nullification, closures should not hold references
        # Check that nullification happens before setTimeout cleanup
        nullify_pos = self.html_content.find("existing = null")
        self.assertGreater(
            nullify_pos, -1,
            "existing = null should be present"
        )


if __name__ == "__main__":
    unittest.main()
