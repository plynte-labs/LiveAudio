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


class _StubVar:
    def __init__(self, value=""):
        self.value = value

    def get(self):
        return self.value

    def set(self, value):
        self.value = value


class _StubSlider:
    def __init__(self):
        self.value = None
        self.options = {}

    def configure(self, **kwargs):
        self.options.update(kwargs)

    def set(self, value):
        self.value = value


class _StubLabel:
    def __init__(self):
        self.text = None

    def configure(self, **kwargs):
        self.text = kwargs.get("text", self.text)


class TestTranscriptionPurposeControl(unittest.TestCase):
    def test_purpose_labels_exist_in_both_languages(self):
        keys = (
            "transcription_purpose_label",
            "transcription_purpose_subtitles",
            "transcription_purpose_transcription",
            "transcription_purpose_combined",
            "transcription_purpose_subtitles_help",
            "transcription_purpose_transcription_help",
            "transcription_purpose_combined_help",
        )
        for lang in ("es", "en"):
            for key in keys:
                self.assertIn(key, TRANSLATIONS[lang], f"missing {key} in {lang}")

    def test_selecting_transcription_changes_only_the_draft_window(self):
        config = {
            "transcription_purpose": "subtitles",
            "max_chunk_duration": 5.0,
        }
        app = type("AppStub", (), {})()
        app._ui_ready = True
        app.draft_config = dict(config)
        app.var_transcription_purpose = _StubVar(t("transcription_purpose_subtitles"))
        app.slider_max_dur = _StubSlider()
        app.lbl_max_dur = _StubLabel()
        app.lbl_transcription_purpose_help = _StubLabel()
        app.on_setting_change = lambda: None

        select_purpose = getattr(LiveASRApp, "_on_transcription_purpose_select", None)
        self.assertTrue(callable(select_purpose), "purpose selector handler is missing")
        select_purpose(
            app, t("transcription_purpose_transcription"),
        )

        self.assertEqual(app.draft_config["transcription_purpose"], "transcription")
        self.assertEqual(app.draft_config["max_chunk_duration"], 30.0)
        self.assertEqual(app.slider_max_dur.options["from_"], 1.0)
        self.assertEqual(app.slider_max_dur.options["to"], 60.0)
        self.assertEqual(app.slider_max_dur.value, 30.0)
        self.assertEqual(
            app.lbl_transcription_purpose_help.text,
            t("transcription_purpose_transcription_help"),
        )

    def test_presets_do_not_overwrite_purpose_or_phrase_window(self):
        user_config = {
            "transcription_purpose": "combined",
            "max_chunk_duration": 45.0,
        }
        for profile_id, profile in PROFILE_PRESETS.items():
            self.assertNotIn("max_chunk_duration", profile["values"], profile_id)
            draft = dict(user_config)
            draft.update(profile["values"])
            self.assertEqual(draft["transcription_purpose"], "combined")
            self.assertEqual(draft["max_chunk_duration"], 45.0)


if __name__ == "__main__":
    unittest.main()
