# SPDX-License-Identifier: MIT
"""UI-logic tests for vad-onset-grace sliders, i18n, and preset policy.

These avoid instantiating the CustomTkinter GUI (which needs a display).
They exercise the pure logic: i18n label availability/format, preset-merge
preservation of the new keys (AC-9), and presence of UI wiring methods.
"""

import copy
import unittest

from liveaudio.utils.i18n import TRANSLATIONS, t
from liveaudio.app import LiveASRApp, PROFILE_PRESETS


class TestVadOnsetI18nLabels(unittest.TestCase):
    """New slider labels exist in every UI language with the right format."""

    def test_keys_present_in_all_languages(self):
        """Both new label keys exist in es and en (AC-7, T-10)."""
        for lang in ("es", "en"):
            self.assertIn("vad_speech_pad", TRANSLATIONS[lang], f"missing in {lang}")
            self.assertIn("vad_threshold_label", TRANSLATIONS[lang], f"missing in {lang}")

    def test_pad_label_uses_ms_format(self):
        """Pre-roll label formats as integer ms, not seconds."""
        for lang in ("es", "en"):
            rendered = TRANSLATIONS[lang]["vad_speech_pad"].format(200)
            self.assertIn("200", rendered)
            self.assertIn("ms", rendered)
            self.assertNotIn(".0s", rendered)

    def test_threshold_label_uses_two_decimals(self):
        """Threshold label formats with 2 decimals."""
        for lang in ("es", "en"):
            rendered = TRANSLATIONS[lang]["vad_threshold_label"].format(0.5)
            self.assertIn("0.50", rendered)

    def test_t_helper_resolves_keys(self):
        """t() resolves both keys without falling back to the raw key."""
        self.assertNotEqual(t("vad_speech_pad", 200), "vad_speech_pad")
        self.assertNotEqual(t("vad_threshold_label", 0.5), "vad_threshold_label")


class TestPresetPreservesVadKeys(unittest.TestCase):
    """Applying a preset must not clobber user vad tuning (AC-9, T-11)."""

    def test_new_keys_absent_from_all_presets(self):
        """No preset's values dict contains the new keys (OD-5 default)."""
        for profile_id, profile in PROFILE_PRESETS.items():
            values = profile["values"]
            self.assertNotIn("vad_speech_pad_ms", values, f"{profile_id} should not set pad")
            self.assertNotIn("vad_threshold", values, f"{profile_id} should not set threshold")

    def test_preset_merge_preserves_user_values(self):
        """Mirrors on_profile_select merge: deepcopy(config) then update(preset)."""
        user_config = {
            "device": "cpu",
            "silence_timeout": 1.2,
            "max_chunk_duration": 7.0,
            "vad_speech_pad_ms": 350,
            "vad_threshold": 0.72,
        }
        for profile_id, profile in PROFILE_PRESETS.items():
            draft = copy.deepcopy(user_config)
            draft.update(profile["values"])
            self.assertEqual(draft["vad_speech_pad_ms"], 350, f"{profile_id} clobbered pad")
            self.assertEqual(draft["vad_threshold"], 0.72, f"{profile_id} clobbered threshold")


class TestVadSliderWiring(unittest.TestCase):
    """The app exposes the methods that wire the new sliders."""

    def test_app_has_read_and_load_methods(self):
        self.assertTrue(callable(getattr(LiveASRApp, "_read_ui_config", None)))
        self.assertTrue(callable(getattr(LiveASRApp, "_load_ui_from_config", None)))


if __name__ == "__main__":
    unittest.main()
