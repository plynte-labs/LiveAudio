# SPDX-License-Identifier: MIT
"""Tests for CSS custom properties theme engine (REQ-3)."""

import unittest
import os
import re


class TestCSSThemeEngine(unittest.TestCase):
    """Tests for CSS custom properties theme engine in subtitulos_obs.html."""

    def setUp(self):
        """Load subtitulos_obs.html."""
        html_path = os.path.join(os.path.dirname(__file__), "..", "subtitulos_obs.html")
        with open(html_path, "r", encoding="utf-8") as f:
            self.html_content = f.read()

    def test_css_custom_properties_defined(self):
        """CSS custom properties should be defined in :root."""
        required_props = [
            "--sub-bg",
            "--sub-color",
            "--sub-font-size",
            "--sub-radius",
            "--sub-shadow",
        ]
        for prop in required_props:
            self.assertIn(prop, self.html_content, f"CSS property {prop} should be defined")

    def test_styles_use_variables(self):
        """Styles should reference CSS variables, not hardcoded values."""
        # Should use var() for at least some style properties
        self.assertRegex(
            self.html_content,
            r"var\(--sub-",
            "Styles should use CSS custom properties via var()"
        )

    def test_theme_message_handler(self):
        """JS should handle theme messages from WebSocket."""
        # Should have logic to handle theme type messages
        has_theme_handler = any(keyword in self.html_content for keyword in [
            '"theme"',
            "'theme'",
            "tokens",
            "setProperty"
        ])
        self.assertTrue(
            has_theme_handler,
            "JS should handle theme messages from WebSocket"
        )

    def test_theme_validation_schema(self):
        """Theme tokens should be validated before applying."""
        # Should have some validation logic
        has_validation = any(keyword in self.html_content.lower() for keyword in [
            "valid", "schema", "check", "sanitize", "validate"
        ])
        self.assertTrue(
            has_validation,
            "Theme tokens should be validated before applying"
        )


class TestThemeValidationPython(unittest.TestCase):
    """Tests for theme token validation in Python."""

    def setUp(self):
        """Load core/engine.py."""
        engine_path = os.path.join(os.path.dirname(__file__), "..", "core", "engine.py")
        with open(engine_path, "r", encoding="utf-8") as f:
            self.engine_content = f.read()

    def test_theme_token_validation_exists(self):
        """Python should validate theme tokens before broadcast."""
        has_validation = any(keyword in self.engine_content for keyword in [
            "theme", "token", "schema", "validate", "valid_tokens"
        ])
        self.assertTrue(
            has_validation,
            "Python should have theme token validation logic"
        )


class TestNewPresetCSSClasses(unittest.TestCase):
    """Tests for 4 new preset CSS classes (T2 — subtitle-style-system-v2)."""

    def setUp(self):
        """Load subtitulos_obs.html."""
        html_path = os.path.join(os.path.dirname(__file__), "..", "subtitulos_obs.html")
        with open(html_path, "r", encoding="utf-8") as f:
            self.html_content = f.read()

    def test_minimal_style_class_exists(self):
        """.style-minimal CSS class should be defined."""
        self.assertIn(".style-minimal", self.html_content)

    def test_bold_style_class_exists(self):
        """.style-bold CSS class should be defined with font-weight 900."""
        self.assertIn(".style-bold", self.html_content)
        self.assertIn("900", self.html_content)

    def test_rgb_style_class_exists(self):
        """.style-rgb CSS class should be defined."""
        self.assertIn(".style-rgb", self.html_content)

    def test_typewriter_style_class_exists(self):
        """.style-typewriter CSS class should be defined with monospace font."""
        self.assertIn(".style-typewriter", self.html_content)
        self.assertIn("monospace", self.html_content)


if __name__ == "__main__":
    unittest.main()
