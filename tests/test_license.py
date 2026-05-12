# SPDX-License-Identifier: GPL-3.0-or-later
"""Tests for GPL v3 license file (T14-T15 — subtitle-style-system-v2)."""

import unittest
import os


class TestLicenseFileExists(unittest.TestCase):
    """Tests for LICENSE file presence and content (T14)."""

    def setUp(self):
        self.project_root = os.path.join(os.path.dirname(__file__), "..")
        self.license_path = os.path.join(self.project_root, "LICENSE")

    def test_license_file_exists(self):
        """LICENSE file should exist in project root."""
        self.assertTrue(os.path.isfile(self.license_path), "LICENSE file should exist in project root")

    def test_license_contains_gpl_v3_text(self):
        """LICENSE file should contain GNU GENERAL PUBLIC LICENSE Version 3."""
        with open(self.license_path, "r", encoding="utf-8") as f:
            content = f.read()
        self.assertIn("GNU GENERAL PUBLIC LICENSE", content)
        self.assertIn("Version 3", content)


class TestGPLHeadersInSourceFiles(unittest.TestCase):
    """Tests for GPL v3 headers in all source files (T15)."""

    def setUp(self):
        self.project_root = os.path.join(os.path.dirname(__file__), "..")

    def _get_py_files(self):
        """Get all .py files in the project (excluding tests/helpers)."""
        py_files = []
        for root, dirs, files in os.walk(self.project_root):
            # Skip hidden dirs, __pycache__, .git, .agents, etc.
            dirs[:] = [d for d in dirs if not d.startswith(".") and d not in ("__pycache__", "legacy", "sessions")]
            for f in files:
                if f.endswith(".py"):
                    py_files.append(os.path.join(root, f))
        return py_files

    def test_all_py_files_contain_gpl_header(self):
        """All .py files should contain SPDX-License-Identifier: GPL-3.0-or-later."""
        py_files = self._get_py_files()
        self.assertGreater(len(py_files), 0, "Should find at least one .py file")

        missing = []
        for filepath in py_files:
            with open(filepath, "r", encoding="utf-8", errors="ignore") as f:
                # Check first 20 lines for the header
                first_lines = ""
                for i, line in enumerate(f):
                    if i >= 20:
                        break
                    first_lines += line
            if "SPDX-License-Identifier: GPL-3.0-or-later" not in first_lines:
                missing.append(filepath)

        self.assertEqual(
            missing, [],
            f"These .py files are missing GPL header: {missing}"
        )

    def test_subtitulos_obs_html_contains_gpl_header(self):
        """subtitulos_obs.html should contain SPDX-License-Identifier: GPL-3.0-or-later."""
        html_path = os.path.join(self.project_root, "subtitulos_obs.html")
        self.assertTrue(os.path.isfile(html_path), "subtitulos_obs.html should exist")

        with open(html_path, "r", encoding="utf-8") as f:
            content = f.read()
        self.assertIn(
            "SPDX-License-Identifier: GPL-3.0-or-later",
            content,
            "subtitulos_obs.html should contain GPL-3.0-or-later header"
        )


if __name__ == "__main__":
    unittest.main()
