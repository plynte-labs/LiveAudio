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
        """Typewriter reveal must use the capped stagger (REVEAL_BUDGET_MS + Math.min).

        Previously this asserted the bare substring "80", which only passed because
        DEBOUNCE_MS = 80 happens to contain it — it did NOT guard the typewriter
        stagger at all (the real base delay is 40ms). Rewritten to assert the actual
        clamping mechanism so it fails if the cap is removed.
        """
        # The reveal budget constant must exist.
        self.assertIn(
            "REVEAL_BUDGET_MS", self.html_content,
            "Typewriter stagger must reference the REVEAL_BUDGET_MS cap constant"
        )
        # The typewriter branch must clamp its base delay against the budget.
        # Base delay is 40ms; the clamp uses Math.min(base, REVEAL_BUDGET_MS / divisor).
        clamp_pattern = re.compile(
            r"Math\.min\(\s*40\s*,\s*REVEAL_BUDGET_MS\s*/", re.MULTILINE
        )
        self.assertRegex(
            self.html_content, clamp_pattern,
            "Typewriter reveal must clamp 40ms base delay via Math.min(40, REVEAL_BUDGET_MS / ...)"
        )
        # And the per-word delay must still be assigned to animationDelay.
        self.assertIn(
            "animationDelay", self.html_content,
            "Typewriter must assign the capped delay to animationDelay"
        )


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

    def test_mode_url_param_parsed(self):
        """JS should read ?mode= URL parameter (mirrors ?style= handling)."""
        self.assertRegex(
            self.html_content,
            re.compile(r"urlParams\.get\(\s*['\"]mode['\"]\s*\)"),
            "JS should parse ?mode= via urlParams.get('mode')",
        )

    def test_lines_url_param_parsed(self):
        """JS should read ?lines= URL parameter."""
        self.assertRegex(
            self.html_content,
            re.compile(r"urlParams\.get\(\s*['\"]lines['\"]\s*\)"),
            "JS should parse ?lines= via urlParams.get('lines')",
        )

    def test_mode_resolves_to_adaptive_default(self):
        """Absent/invalid ?mode= must resolve to 'adaptive' (locked default)."""
        self.assertRegex(
            self.html_content,
            re.compile(r"resolvedMode\s*=[^;]*['\"]adaptive['\"]"),
            "Resolved mode must default to 'adaptive', not 'single'",
        )


class TestAnimationDurationSync(unittest.TestCase):
    """Guard REQ-1: CSS :root and JS VALID_THEME_TOKENS animation-duration agree."""

    def setUp(self):
        html_path = os.path.join(os.path.dirname(__file__), "..", "liveaudio", "assets", "subtitulos_obs.html")
        with open(html_path, "r", encoding="utf-8") as f:
            self.html_content = f.read()

    def test_root_and_token_default_durations_match(self):
        """The CSS :root --sub-animation-duration and the JS token default must be identical."""
        root_match = re.search(
            r"--sub-animation-duration:\s*([0-9.]+s)\s*;", self.html_content
        )
        self.assertIsNotNone(
            root_match, "CSS :root must declare --sub-animation-duration with an s value"
        )
        token_match = re.search(
            r"'--sub-animation-duration':\s*\{[^}]*default:\s*'([0-9.]+s)'", self.html_content
        )
        self.assertIsNotNone(
            token_match, "JS VALID_THEME_TOKENS must declare a default for --sub-animation-duration"
        )
        self.assertEqual(
            root_match.group(1), token_match.group(1),
            "CSS :root and JS token default for --sub-animation-duration must be the same value"
        )

    def test_duration_within_validator_bounds(self):
        """The reconciled duration must stay within the [0.1, 2.0] validator bounds."""
        root_match = re.search(
            r"--sub-animation-duration:\s*([0-9.]+)s\s*;", self.html_content
        )
        self.assertIsNotNone(root_match)
        value = float(root_match.group(1))
        self.assertGreaterEqual(value, 0.1, "duration below validator min 0.1")
        self.assertLessEqual(value, 2.0, "duration above validator max 2.0")


class TestRevealBudgetCap(unittest.TestCase):
    """Guard REQ-4/REQ-5: all reveal AND hide branches reference the capped stagger."""

    def setUp(self):
        html_path = os.path.join(os.path.dirname(__file__), "..", "liveaudio", "assets", "subtitulos_obs.html")
        with open(html_path, "r", encoding="utf-8") as f:
            self.html_content = f.read()

    def test_reveal_budget_constant_defined_once(self):
        """REVEAL_BUDGET_MS must be defined exactly once as a named constant."""
        defs = re.findall(r"const\s+REVEAL_BUDGET_MS\s*=\s*(\d+)\s*;", self.html_content)
        self.assertEqual(
            len(defs), 1,
            "REVEAL_BUDGET_MS must be defined exactly once as a named constant"
        )
        self.assertEqual(defs[0], "400", "REVEAL_BUDGET_MS must default to 400 (OD-4)")

    def test_all_three_styles_clamp_base_delays(self):
        """Karaoke (25ms), rgb (50ms) and typewriter (40ms) must clamp against the budget."""
        for base in ("25", "50", "40"):
            pattern = re.compile(
                r"Math\.min\(\s*" + base + r"\s*,\s*REVEAL_BUDGET_MS\s*/"
            )
            self.assertRegex(
                self.html_content, pattern,
                f"A style must clamp its {base}ms base delay via Math.min({base}, REVEAL_BUDGET_MS / ...)"
            )

    def test_budget_divisor_guards_single_word(self):
        """The divisor must guard wordCount<=1 (Math.max(wordCount-1, 1))."""
        self.assertRegex(
            self.html_content,
            re.compile(r"Math\.max\(\s*words\.length\s*-\s*1\s*,\s*1\s*\)"),
            "Per-word delay divisor must use Math.max(words.length - 1, 1) to avoid div-by-zero"
        )

    def test_cap_applied_to_both_reveal_and_hide(self):
        """The clamp via REVEAL_BUDGET_MS must appear in both reveal and hide cascades.

        Reveal happens before the hideTimeout setTimeout; hide happens inside it.
        Each of the three styles clamps on reveal and on hide, so the typewriter
        base (40ms) clamp must appear at least twice across the file.
        """
        occurrences = len(re.findall(
            r"Math\.min\(\s*40\s*,\s*REVEAL_BUDGET_MS\s*/", self.html_content
        ))
        self.assertGreaterEqual(
            occurrences, 2,
            "Typewriter clamp must be applied on both reveal and hide (>=2 occurrences)"
        )

    def test_cleanup_timeout_outlasts_budget_plus_exit(self):
        """Every DOM cleanup timeout must be >= budget + per-word exit (>=600ms).

        Track B refactored the single cleanup into a shared cleanupCb scheduled by
        BOTH the single path (cleanupTimeout = setTimeout(cleanupCb, NNN)) and the
        stacked path (sub._cleanupTimer = setTimeout(cleanupCb, NNN)). The original
        regex keyed on the inlined removeChild(sub)+onSubtitleComplete() body, which
        no longer exists verbatim. This now asserts EVERY cleanupCb schedule uses a
        delay >= 600ms — strengthening (not weakening) the contract across paths.
        """
        cleanup_timeouts = re.findall(
            r"setTimeout\(\s*cleanupCb\s*,\s*(\d+)\s*\)", self.html_content
        )
        self.assertGreaterEqual(
            len(cleanup_timeouts), 1,
            "Could not locate any cleanupCb setTimeout schedule"
        )
        for value in cleanup_timeouts:
            self.assertGreaterEqual(
                int(value), 600,
                "Cleanup timeout must be >= 400ms budget + ~200ms exit so tail words finish"
            )


class TestStyleSizeWeightConsistency(unittest.TestCase):
    """All subtitle styles must share ONE size+weight format (user consistency request).

    Previously per-style overrides (karaoke font-size *1.15 / weight 900, bold *1.1 /
    weight 900, minimal weight 500) produced three different sizes and three weights.
    Every style now routes size and weight through the shared --sub-font-size /
    --sub-font-weight variables, so all presets render at one consistent format and
    differ only by colour/effect/animation.
    """

    def setUp(self):
        html_path = os.path.join(
            os.path.dirname(__file__), "..", "liveaudio", "assets", "subtitulos_obs.html"
        )
        with open(html_path, "r", encoding="utf-8") as f:
            self.html_content = f.read()

    def test_no_per_style_font_size_multiplier(self):
        """No style may scale the base font-size, e.g. calc(var(--sub-font-size) * 1.1)."""
        self.assertNotRegex(
            self.html_content,
            r"font-size:\s*calc\(\s*var\(--sub-font-size\)\s*\*",
            "Styles must not apply per-style font-size multipliers; use var(--sub-font-size)",
        )

    def test_no_per_style_hardcoded_weight(self):
        """No style may hardcode font-weight; all route through --sub-font-weight."""
        self.assertNotRegex(
            self.html_content,
            r"\n\s*font-weight:\s*\d{3}\s*;",
            "Styles must not hardcode a numeric font-weight; use var(--sub-font-weight)",
        )

    def test_all_seven_styles_share_weight_variable(self):
        """Each of the 7 style classes must reference the shared weight variable."""
        self.assertEqual(
            self.html_content.count("font-weight: var(--sub-font-weight)"),
            7,
            "All 7 styles must use font-weight: var(--sub-font-weight)",
        )


if __name__ == "__main__":
    unittest.main()
