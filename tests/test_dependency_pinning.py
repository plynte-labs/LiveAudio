# SPDX-License-Identifier: MIT
"""Tests for dependency version pinning (REQ-6)."""

import unittest
import os
import re


class TestDependencyPinning(unittest.TestCase):
    """Tests that requirements.txt has upper bounds pinned."""

    def setUp(self):
        """Load requirements.txt."""
        req_path = os.path.join(os.path.dirname(__file__), "..", "requirements.txt")
        with open(req_path, "r") as f:
            self.requirements = f.read().strip().splitlines()

    def test_all_deps_have_upper_bounds(self):
        """All dependencies should have upper version bounds."""
        for req in self.requirements:
            req = req.strip()
            if not req or req.startswith("#"):
                continue
            # Should contain both >= and < or ~=
            has_lower = ">=" in req or "==" in req or "~=" in req
            has_upper = "<" in req or "~=" in req
            self.assertTrue(has_upper, f"Missing upper bound: {req}")

    def test_websockets_pinned(self):
        """websockets should be pinned to known compatible range."""
        websockets_req = [r for r in self.requirements if "websockets" in r.lower()]
        self.assertTrue(websockets_req, "websockets not found in requirements")
        req = websockets_req[0]
        self.assertIn("<", req, "websockets should have upper bound")

    def test_torch_pinned(self):
        """torch should be pinned to known compatible range."""
        torch_req = [r for r in self.requirements if "torch" in r.lower() and "faster" not in r.lower()]
        self.assertTrue(torch_req, "torch not found in requirements")
        req = torch_req[0]
        self.assertIn("<", req, "torch should have upper bound")

    def test_faster_whisper_pinned(self):
        """faster-whisper should be pinned to known compatible range."""
        fw_req = [r for r in self.requirements if "faster-whisper" in r.lower()]
        self.assertTrue(fw_req, "faster-whisper not found in requirements")
        req = fw_req[0]
        self.assertIn("<", req, "faster-whisper should have upper bound")

    def test_numpy_pinned(self):
        """numpy should be pinned to known compatible range."""
        numpy_req = [r for r in self.requirements if "numpy" in r.lower()]
        self.assertTrue(numpy_req, "numpy not found in requirements")
        req = numpy_req[0]
        self.assertIn("<", req, "numpy should have upper bound")

    def test_sounddevice_pinned(self):
        """sounddevice should be pinned to known compatible range."""
        sd_req = [r for r in self.requirements if "sounddevice" in r.lower()]
        self.assertTrue(sd_req, "sounddevice not found in requirements")
        req = sd_req[0]
        self.assertIn("<", req, "sounddevice should have upper bound")


if __name__ == "__main__":
    unittest.main()
