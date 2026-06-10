# SPDX-License-Identifier: MIT
"""Tests for dependency version pinning (REQ-6), now sourced from pyproject.toml."""

import os
import re
import unittest

try:
    import tomllib
except ImportError:  # Python < 3.11
    import tomli as tomllib

PYPROJECT_PATH = os.path.join(os.path.dirname(__file__), "..", "pyproject.toml")


def _has_lower_bound(spec):
    return ">=" in spec or "==" in spec or "~=" in spec


def _has_upper_bound(spec):
    return "<" in spec or "~=" in spec or "==" in spec


def _strip_marker(requirement):
    return requirement.split(";")[0].strip()


class TestDependencyPinning(unittest.TestCase):
    """Tests that pyproject.toml dependencies have lower and upper bounds."""

    @classmethod
    def setUpClass(cls):
        with open(PYPROJECT_PATH, "rb") as f:
            cls.pyproject = tomllib.load(f)
        cls.dependencies = cls.pyproject["project"]["dependencies"]
        cls.extras = cls.pyproject["project"].get("optional-dependencies", {})

    def _find(self, requirements, name):
        pattern = re.compile(rf"^{re.escape(name)}\s*[><=~!\[;]", re.IGNORECASE)
        matches = [r for r in requirements if pattern.match(_strip_marker(r) + ";")]
        self.assertTrue(matches, f"{name} not found in {requirements}")
        return _strip_marker(matches[0])

    def test_all_runtime_deps_have_bounds(self):
        """Every runtime dependency should have lower and upper version bounds."""
        for req in self.dependencies:
            spec = _strip_marker(req)
            self.assertTrue(_has_lower_bound(spec), f"Missing lower bound: {req}")
            self.assertTrue(_has_upper_bound(spec), f"Missing upper bound: {req}")

    def test_all_extra_deps_have_bounds(self):
        """Every optional (extra) dependency should have lower and upper bounds."""
        for extra, requirements in self.extras.items():
            for req in requirements:
                spec = _strip_marker(req)
                self.assertTrue(_has_lower_bound(spec), f"[{extra}] missing lower bound: {req}")
                self.assertTrue(_has_upper_bound(spec), f"[{extra}] missing upper bound: {req}")

    def test_websockets_pinned(self):
        """websockets should be pinned to a known compatible range."""
        spec = self._find(self.dependencies, "websockets")
        self.assertIn("<", spec, "websockets should have upper bound")

    def test_faster_whisper_pinned(self):
        """faster-whisper should be pinned to a known compatible range."""
        spec = self._find(self.dependencies, "faster-whisper")
        self.assertIn("<", spec, "faster-whisper should have upper bound")

    def test_numpy_pinned(self):
        """numpy should be pinned to a known compatible range."""
        spec = self._find(self.dependencies, "numpy")
        self.assertIn("<", spec, "numpy should have upper bound")

    def test_sounddevice_pinned(self):
        """sounddevice should be pinned to a known compatible range."""
        spec = self._find(self.dependencies, "sounddevice")
        self.assertIn("<", spec, "sounddevice should have upper bound")

    def test_torch_pinned_per_extra(self):
        """torch and torchaudio should be pinned in both backend extras."""
        for extra in ("cpu", "cu121"):
            self.assertIn(extra, self.extras, f"Missing '{extra}' extra")
            for name in ("torch", "torchaudio"):
                spec = self._find(self.extras[extra], name)
                self.assertTrue(_has_lower_bound(spec), f"[{extra}] {name} missing lower bound")
                self.assertIn("<", spec, f"[{extra}] {name} should have upper bound")

    def test_cu121_torch_range_supports_cudnn9(self):
        """cu121 extra must require torch>=2.4 (cuDNN 9 for ctranslate2>=4.5) and <2.6."""
        spec = self._find(self.extras["cu121"], "torch")
        self.assertIn(">=2.4", spec)
        self.assertIn("<2.6", spec)

    def test_torch_backend_extras_declared_conflicting(self):
        """uv must treat the cpu and cu121 extras as mutually exclusive."""
        conflicts = self.pyproject["tool"]["uv"]["conflicts"]
        flattened = [
            {entry.get("extra") for entry in conflict_set}
            for conflict_set in conflicts
        ]
        self.assertIn({"cpu", "cu121"}, flattened)


if __name__ == "__main__":
    unittest.main()
