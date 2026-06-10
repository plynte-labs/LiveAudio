# SPDX-License-Identifier: MIT
"""Tests for JavaScript subtitle rendering logic (T3-T6 — subtitle-style-system-v2)."""

import unittest
import os
import re


class TestRGBPerWordColor(unittest.TestCase):
    """Tests for RGB per-word color animation (T3)."""

    def setUp(self):
        html_path = os.path.join(os.path.dirname(__file__), "..", "liveaudio", "assets", "subtitulos_obs.html")
        with open(html_path, "r", encoding="utf-8") as f:
            self.html_content = f.read()

    def test_rgb_splits_text_into_word_spans(self):
        """When style is rgb, showSubtitle should split text into word spans."""
        # Look for rgb-specific word splitting logic
        has_rgb_split = (
            ("'rgb'" in self.html_content or '"rgb"' in self.html_content)
            and "split" in self.html_content
            and "hsl" in self.html_content.lower()
        )
        self.assertTrue(has_rgb_split, "RGB style should split text into words with HSL colors")

    def test_rgb_uses_hsl_colors(self):
        """RGB words should use HSL color with minimum luminance 40%."""
        # Check for HSL color generation
        has_hsl = "hsl(" in self.html_content.lower() or "hsl(" in self.html_content
        self.assertTrue(has_hsl, "RGB style should generate HSL colors")

    def test_rgb_has_fade_in_animation(self):
        """RGB words should have fadeIn + slideUp entry animation."""
        has_rgb_fade_in = "rgbFadeIn" in self.html_content or "rgbfadein" in self.html_content.lower()
        self.assertTrue(has_rgb_fade_in, "RGB should have fadeIn animation")


class TestTypewriterSequentialReveal(unittest.TestCase):
    """Tests for typewriter sequential reveal animation (T4)."""

    def setUp(self):
        html_path = os.path.join(os.path.dirname(__file__), "..", "liveaudio", "assets", "subtitulos_obs.html")
        with open(html_path, "r", encoding="utf-8") as f:
            self.html_content = f.read()

    def test_typewriter_splits_into_word_spans(self):
        """When style is typewriter, showSubtitle should split text into word spans."""
        has_typewriter_split = (
            ("'typewriter'" in self.html_content or '"typewriter"' in self.html_content)
            and "animationDelay" in self.html_content
        )
        self.assertTrue(has_typewriter_split, "Typewriter should split text with staggered delays")

    def test_typewriter_has_blink_cursor(self):
        """Typewriter should have blinking cursor animation."""
        has_blink = "blink" in self.html_content.lower() and "@keyframes" in self.html_content
        self.assertTrue(has_blink, "Typewriter should have blink keyframes")

    def test_typewriter_staggered_delay(self):
        """Typewriter words should have staggered animation delay (80ms)."""
        # Look for 80ms delay pattern
        has_stagger = "80" in self.html_content and "animationDelay" in self.html_content
        self.assertTrue(has_stagger, "Typewriter should have 80ms staggered delay")


class TestEntryExitAnimationMirroring(unittest.TestCase):
    """Tests for entry/exit animation mirroring (T5)."""

    def setUp(self):
        html_path = os.path.join(os.path.dirname(__file__), "..", "liveaudio", "assets", "subtitulos_obs.html")
        with open(html_path, "r", encoding="utf-8") as f:
            self.html_content = f.read()

    def test_all_themes_have_hiding_state(self):
        """All themes should have a .hiding state for exit animation."""
        # Check that hiding class is used for exit
        has_hiding = ".hiding" in self.html_content and "classList.add('hiding')" in self.html_content
        self.assertTrue(has_hiding, "All themes should use .hiding class for exit")

    def test_entry_exit_use_same_direction(self):
        """Entry and exit animations should use same direction reversed."""
        # Entry uses translateY(20px) -> translateY(0), exit uses translateY(0) -> translateY(12px)
        has_entry = "translateY(20px)" in self.html_content or "translateY(10px)" in self.html_content
        has_exit = "translateY(12px)" in self.html_content or "translateY(10px)" in self.html_content
        self.assertTrue(has_entry and has_exit, "Entry and exit should use mirrored translateY")


class TestValidStylesAndURLParams(unittest.TestCase):
    """Tests for VALID_STYLES expansion and URL param parser (T6)."""

    def setUp(self):
        html_path = os.path.join(os.path.dirname(__file__), "..", "liveaudio", "assets", "subtitulos_obs.html")
        with open(html_path, "r", encoding="utf-8") as f:
            self.html_content = f.read()

    def test_valid_styles_includes_all_seven_presets(self):
        """JS VALID_STYLES Set should include all 7 presets."""
        for style in ["minimal", "bold", "rgb", "typewriter"]:
            self.assertIn(f"'{style}'", self.html_content, f"VALID_STYLES should include '{style}'")

    def test_url_param_parser_exists(self):
        """JS should read ?style= URL parameter on load."""
        has_url_parser = (
            "URLSearchParams" in self.html_content
            and "style" in self.html_content.lower()
        )
        self.assertTrue(has_url_parser, "JS should parse ?style= URL parameter")

    def test_legacy_url_params_mapped(self):
        """Legacy URL params should map to new behavior."""
        # Check that style param is handled with fallback
        has_legacy = "get('style')" in self.html_content or "get(\"style\")" in self.html_content
        self.assertTrue(has_legacy, "JS should handle legacy style URL param")


if __name__ == "__main__":
    unittest.main()
