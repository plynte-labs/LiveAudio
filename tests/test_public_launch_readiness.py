# SPDX-License-Identifier: MIT
"""Launch-readiness checks for the public LiveAudio repository."""

import os
import re
import unittest


PROJECT_ROOT = os.path.normpath(os.path.join(os.path.dirname(__file__), ".."))
CANONICAL_REPO = "https://github.com/plynte-labs/LiveAudio"


def _read_project_file(*parts):
    path = os.path.join(PROJECT_ROOT, *parts)
    with open(path, "r", encoding="utf-8", errors="ignore") as handle:
        return handle.read()


class TestPublicLaunchReadiness(unittest.TestCase):
    def test_pillow_declared_in_dependencies(self):
        pyproject = _read_project_file("pyproject.toml").lower()
        self.assertIn("pillow", pyproject)

    def test_pillow_pinned_in_lockfile(self):
        lockfile = _read_project_file("uv.lock").lower()
        self.assertIn('name = "pillow"', lockfile)

    def test_public_repo_references_use_canonical_target(self):
        checked_files = (os.path.join("liveaudio", "app.py"), "README.md", "CHANGELOG.md")
        legacy_hits = []
        canonical_hits = []

        for filename in checked_files:
            content = _read_project_file(filename)
            if "github.com/Plynte/LiveAudio" in content or "github.com/FranGuh/LiveAudio" in content:
                legacy_hits.append(filename)
            if CANONICAL_REPO in content:
                canonical_hits.append(filename)

        self.assertEqual(legacy_hits, [], f"Legacy public repo references found in: {legacy_hits}")
        self.assertGreaterEqual(len(canonical_hits), 2, "Canonical repo target should appear in launch-facing files")

    def test_skill_registry_has_no_absolute_windows_paths(self):
        registry = _read_project_file(".atl", "skill-registry.md")
        self.assertIsNone(
            re.search(r"\b[A-Z]:\\", registry),
            "Public skill registry should not expose absolute Windows paths",
        )

    def test_asr_follow_up_is_marked_non_blocking(self):
        tracks = _read_project_file("conductor", "tracks.md").lower()
        self.assertIn("non-blocking", tracks)
        self.assertIn("asr language separation", tracks)


if __name__ == "__main__":
    unittest.main()
