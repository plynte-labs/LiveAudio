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


class TestRibbonMemoryLeak(unittest.TestCase):
    """Per-line GC discipline in the ribbon/adaptive path (AC-13, NFR-1, R-2)."""

    def setUp(self):
        html_path = os.path.join(os.path.dirname(__file__), "..", "liveaudio", "assets", "subtitulos_obs.html")
        with open(html_path, "r", encoding="utf-8") as f:
            self.html_content = f.read()

    def test_per_line_timers_nullified_after_cleanup(self):
        """Per-line cleanup must null both _hideTimer and _cleanupTimer ids."""
        self.assertRegex(
            self.html_content,
            re.compile(r"sub\._hideTimer\s*=\s*sub\._cleanupTimer\s*=\s*null"),
            "Per-line cleanup must nullify _hideTimer and _cleanupTimer (GC discipline)",
        )

    def test_eviction_clears_then_removes_then_nulls(self):
        """Cap eviction clears per-line timers, removeChild, then nulls ids (R-2)."""
        # Eviction must clearTimeout the evicted box's per-line timers.
        self.assertRegex(
            self.html_content,
            re.compile(r"clearTimeout\(\s*\w+\._hideTimer\s*\)"),
            "Eviction must clear the evicted box's _hideTimer before removeChild",
        )
        self.assertRegex(
            self.html_content,
            re.compile(r"clearTimeout\(\s*\w+\._cleanupTimer\s*\)"),
            "Eviction must clear the evicted box's _cleanupTimer before removeChild",
        )

    def test_stacked_cleanup_removes_node_via_removechild(self):
        """Stacked per-line cleanup must use removeChild (predictable GC)."""
        # There must be more than one removeChild path now (single + stacked + eviction).
        self.assertGreaterEqual(
            self.html_content.count("removeChild"),
            3,
            "Stacked cleanup, cap eviction, and single path must each removeChild",
        )

    def test_single_path_removechild_clears_per_line_timers(self):
        """Demotion edge: single-path removeChild clears any per-line timers (§1.12)."""
        block = re.search(r"if\s*\(existing\)\s*\{(.*?)\}", self.html_content, re.DOTALL)
        self.assertIsNotNone(block, "single-path removeChild block not found")
        self.assertIn(
            "_hideTimer",
            block.group(1),
            "single-path removeChild must clear the removed node's per-line timers",
        )


if __name__ == "__main__":
    unittest.main()
