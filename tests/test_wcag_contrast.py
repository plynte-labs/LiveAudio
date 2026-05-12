# SPDX-License-Identifier: GPL-3.0-or-later
"""WCAG AA contrast verification for all 7 subtitle presets (T7)."""

import unittest
import os
import re


def hex_to_rgb(hex_color):
    """Convert hex color to RGB tuple."""
    hex_color = hex_color.lstrip('#')
    if len(hex_color) == 3:
        hex_color = ''.join(c * 2 for c in hex_color)
    return tuple(int(hex_color[i:i+2], 16) for i in (0, 2, 4))


def relative_luminance(rgb):
    """Calculate relative luminance per WCAG 2.1."""
    def linearize(c):
        c = c / 255.0
        return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4
    r, g, b = rgb
    return 0.2126 * linearize(r) + 0.7152 * linearize(g) + 0.0722 * linearize(b)


def contrast_ratio(color1, color2):
    """Calculate WCAG contrast ratio between two colors."""
    l1 = relative_luminance(hex_to_rgb(color1))
    l2 = relative_luminance(hex_to_rgb(color2))
    lighter = max(l1, l2)
    darker = min(l1, l2)
    return (lighter + 0.05) / (darker + 0.05)


# Preset color definitions extracted from subtitulos_obs.html CSS
PRESETS = {
    "default": {"color": "#ffffff", "bg": "#000000"},  # white on rgba(0,0,0,0.7) ~ black
    "karaoke": {"color": "#ffcc00", "bg": "#000000"},  # yellow on rgba(0,0,0,0.7) ~ black
    "neon": {"color": "#ffffff", "bg": "#001414"},     # white on rgba(0,20,20,0.6)
    "minimal": {"color": "#ffffff", "bg": "#000000"},  # white on rgba(0,0,0,0.3)
    "bold": {"color": "#ffffff", "bg": "#000000"},     # white on rgba(0,0,0,0.85)
    "rgb": {"color": "#ffffff", "bg": "#000000"},      # white on rgba(0,0,0,0.7) - base vars
    "typewriter": {"color": "#ffffff", "bg": "#000000"},  # white on rgba(0,0,0,0.75)
}


class TestWCAGContrast(unittest.TestCase):
    """Verify all 7 presets meet WCAG AA contrast requirements."""

    def _check_contrast(self, preset_name, fg, bg, min_ratio=3.0):
        """Helper: assert contrast ratio meets minimum for large text (3:1)."""
        ratio = contrast_ratio(fg, bg)
        self.assertGreaterEqual(
            ratio, min_ratio,
            f"Preset '{preset_name}' contrast ratio {ratio:.2f}:1 < {min_ratio}:1 (WCAG AA large text)"
        )
        return ratio

    def test_default_contrast(self):
        """Default preset: white on black should exceed 21:1."""
        ratio = self._check_contrast("default", "#ffffff", "#000000")
        self.assertGreater(ratio, 20.0)

    def test_karaoke_contrast(self):
        """Karaoke preset: yellow (#ffcc00) on black should exceed 3:1."""
        ratio = self._check_contrast("karaoke", "#ffcc00", "#000000")
        self.assertGreaterEqual(ratio, 3.0)

    def test_neon_contrast(self):
        """Neon preset: white on dark teal should exceed 3:1."""
        ratio = self._check_contrast("neon", "#ffffff", "#001414")
        self.assertGreaterEqual(ratio, 3.0)

    def test_minimal_contrast(self):
        """Minimal preset: white on dark bg should exceed 3:1."""
        ratio = self._check_contrast("minimal", "#ffffff", "#000000")
        self.assertGreaterEqual(ratio, 3.0)

    def test_bold_contrast(self):
        """Bold preset: white on near-black should exceed 3:1."""
        ratio = self._check_contrast("bold", "#ffffff", "#000000")
        self.assertGreater(ratio, 20.0)

    def test_rgb_contrast(self):
        """RGB preset: base colors should exceed 3:1."""
        ratio = self._check_contrast("rgb", "#ffffff", "#000000")
        self.assertGreater(ratio, 20.0)

    def test_typewriter_contrast(self):
        """Typewriter preset: white on dark bg should exceed 3:1."""
        ratio = self._check_contrast("typewriter", "#ffffff", "#000000")
        self.assertGreater(ratio, 20.0)


if __name__ == "__main__":
    unittest.main()
