# SPDX-License-Identifier: MIT
"""Tests for utils/config.py validation functions (REQ-2)."""

import unittest
import os
import json
import tempfile
import shutil
from unittest.mock import patch

from liveaudio.utils.config import (
    _clamp_number,
    _normalize_config,
    load_config,
    save_config,
    DEFAULT_CONFIG,
    VALID_DEVICES,
    VALID_MODELS,
    VALID_SUBTITLE_STYLES,
    VALID_SUBTITLE_DISPLAY_MODES,
    VALID_BACKLOG_POLICIES,
    VALID_DIAGNOSTICS_LEVELS,
)


class TestClampNumber(unittest.TestCase):
    """Tests for _clamp_number() function."""

    def test_accepts_value_within_range(self):
        """Value within range should be returned unchanged."""
        value, changed = _clamp_number(1.0, 0.5, 0.3, 2.0, float)
        self.assertEqual(value, 1.0)
        self.assertFalse(changed)

    def test_rejects_value_below_min(self):
        """Value below min should be clamped to min and report changed."""
        value, changed = _clamp_number(0.1, 0.5, 0.3, 2.0, float)
        self.assertEqual(value, 0.3)
        self.assertTrue(changed)

    def test_rejects_value_above_max(self):
        """Value above max should be clamped to max and report changed."""
        value, changed = _clamp_number(5.0, 0.5, 0.3, 2.0, float)
        self.assertEqual(value, 2.0)
        self.assertTrue(changed)

    def test_handles_none_value(self):
        """None value should return default and report changed."""
        value, changed = _clamp_number(None, 0.5, 0.3, 2.0, float)
        self.assertEqual(value, 0.5)
        self.assertTrue(changed)

    def test_handles_non_numeric_string(self):
        """Non-numeric string should return default and report changed."""
        value, changed = _clamp_number("abc", 0.5, 0.3, 2.0, float)
        self.assertEqual(value, 0.5)
        self.assertTrue(changed)

    def test_handles_empty_string(self):
        """Empty string should return default and report changed."""
        value, changed = _clamp_number("", 0.5, 0.3, 2.0, float)
        self.assertEqual(value, 0.5)
        self.assertTrue(changed)

    def test_casts_to_int_when_specified(self):
        """Should cast to int when cast=int is specified."""
        value, changed = _clamp_number(3.7, 2, 1, 10, int)
        self.assertEqual(value, 3)
        self.assertFalse(changed)

    def test_rejects_negative_duration(self):
        """Negative durations should be clamped to minimum."""
        value, changed = _clamp_number(-1.0, 0.8, 0.3, 2.0, float)
        self.assertEqual(value, 0.3)
        self.assertTrue(changed)


class TestNormalizeConfig(unittest.TestCase):
    """Tests for _normalize_config() function."""

    def test_adds_missing_keys_with_defaults(self):
        """Missing keys should be filled with defaults."""
        config = {}
        result, updated = _normalize_config(config)
        for key in DEFAULT_CONFIG:
            self.assertIn(key, result)

    def test_normalizes_invalid_device(self):
        """Invalid device should be reset to default."""
        config = DEFAULT_CONFIG.copy()
        config["device"] = "quantum_computer"
        result, updated = _normalize_config(config)
        self.assertEqual(result["device"], "cuda")
        self.assertTrue(updated)

    def test_normalizes_invalid_model(self):
        """Invalid model should be reset to default."""
        config = DEFAULT_CONFIG.copy()
        config["model_size"] = "gigantic (Impossible)"
        result, updated = _normalize_config(config)
        self.assertEqual(result["model_size"], DEFAULT_CONFIG["model_size"])
        self.assertTrue(updated)

    def test_normalizes_invalid_subtitle_style(self):
        """Invalid subtitle style should be reset to default."""
        config = DEFAULT_CONFIG.copy()
        config["subtitle_style"] = "disco"
        result, updated = _normalize_config(config)
        self.assertEqual(result["subtitle_style"], "default")
        self.assertTrue(updated)

    def test_normalizes_invalid_backlog_policy(self):
        """Invalid backlog policy should be reset to default."""
        config = DEFAULT_CONFIG.copy()
        config["subtitle_backlog_policy"] = "spam_everything"
        result, updated = _normalize_config(config)
        self.assertEqual(result["subtitle_backlog_policy"], "auto")
        self.assertTrue(updated)

    def test_clamps_silence_timeout(self):
        """Silence timeout outside range should be clamped."""
        config = DEFAULT_CONFIG.copy()
        config["silence_timeout"] = 5.0
        result, updated = _normalize_config(config)
        self.assertLessEqual(result["silence_timeout"], 2.0)
        self.assertTrue(updated)

    def test_clamps_max_chunk_duration(self):
        """Max chunk duration outside range should be clamped."""
        config = DEFAULT_CONFIG.copy()
        config["max_chunk_duration"] = 1.0
        result, updated = _normalize_config(config)
        self.assertGreaterEqual(result["max_chunk_duration"], 2.0)
        self.assertTrue(updated)

    def test_normalizes_output_dir(self):
        """Output dir should be normalized to absolute path."""
        config = DEFAULT_CONFIG.copy()
        config["output_dir"] = "sessions"
        result, updated = _normalize_config(config)
        self.assertTrue(os.path.isabs(result["output_dir"]))

    def test_handles_none_output_dir(self):
        """None output_dir should be reset to default."""
        config = DEFAULT_CONFIG.copy()
        config["output_dir"] = None
        result, updated = _normalize_config(config)
        self.assertEqual(result["output_dir"], DEFAULT_CONFIG["output_dir"])
        self.assertTrue(updated)

    def test_handles_empty_output_dir(self):
        """Empty output_dir should be reset to default."""
        config = DEFAULT_CONFIG.copy()
        config["output_dir"] = ""
        result, updated = _normalize_config(config)
        self.assertEqual(result["output_dir"], DEFAULT_CONFIG["output_dir"])
        self.assertTrue(updated)

    def test_rejects_negative_duration(self):
        """Negative duration values should be rejected (clamped to min)."""
        config = DEFAULT_CONFIG.copy()
        config["silence_timeout"] = -0.5
        result, updated = _normalize_config(config)
        self.assertGreaterEqual(result["silence_timeout"], 0.3)
        self.assertTrue(updated)

    def test_rejects_invalid_port(self):
        """Port values outside 1-65535 should be rejected."""
        # Note: port validation is not yet implemented in config.py
        # This test will fail until port validation is added
        config = DEFAULT_CONFIG.copy()
        # Add a port field (currently not in DEFAULT_CONFIG)
        config["ws_port"] = 99999
        result, updated = _normalize_config(config)
        # After fix: port should be clamped or rejected
        self.assertIn("ws_port", result)
        self.assertGreaterEqual(result["ws_port"], 1)
        self.assertLessEqual(result["ws_port"], 65535)

    def test_rejects_empty_blacklist(self):
        """Empty blacklist should be reset to default."""
        config = DEFAULT_CONFIG.copy()
        config["blacklist"] = ""
        result, updated = _normalize_config(config)
        # After fix: empty blacklist should be rejected
        self.assertNotEqual(result["blacklist"], "")

    def test_handles_non_string_blacklist(self):
        """Non-string blacklist should be reset to default."""
        config = DEFAULT_CONFIG.copy()
        config["blacklist"] = 12345
        result, updated = _normalize_config(config)
        self.assertEqual(result["blacklist"], DEFAULT_CONFIG["blacklist"])
        self.assertTrue(updated)

    def test_normalizes_audio_device_invalid_type(self):
        """Non-dict audio_device should be reset to None."""
        config = DEFAULT_CONFIG.copy()
        config["audio_device"] = "not_a_dict"
        result, updated = _normalize_config(config)
        self.assertIsNone(result["audio_device"])
        self.assertTrue(updated)

    def test_normalizes_profile_mode_invalid(self):
        """Invalid profile_mode should be reset to default."""
        config = DEFAULT_CONFIG.copy()
        config["profile_mode"] = "custom_made"
        result, updated = _normalize_config(config)
        self.assertEqual(result["profile_mode"], "preset")
        self.assertTrue(updated)


class TestVadOnsetGraceConfig(unittest.TestCase):
    """Tests for vad_speech_pad_ms / vad_threshold config keys (vad-onset-grace)."""

    def test_default_config_has_vad_speech_pad_ms(self):
        """DEFAULT_CONFIG should include vad_speech_pad_ms defaulting to 200."""
        self.assertIn("vad_speech_pad_ms", DEFAULT_CONFIG)
        self.assertEqual(DEFAULT_CONFIG["vad_speech_pad_ms"], 200)

    def test_default_config_has_vad_threshold(self):
        """DEFAULT_CONFIG should include vad_threshold defaulting to 0.5."""
        self.assertIn("vad_threshold", DEFAULT_CONFIG)
        self.assertEqual(DEFAULT_CONFIG["vad_threshold"], 0.5)

    def test_missing_vad_keys_filled_with_defaults(self):
        """Loading a config without the new keys fills them with defaults (AC-3)."""
        config = {}
        result, _ = _normalize_config(config)
        self.assertEqual(result["vad_speech_pad_ms"], 200)
        self.assertEqual(result["vad_threshold"], 0.5)

    def test_vad_speech_pad_ms_clamps_above_max(self):
        """vad_speech_pad_ms above 500 should clamp to 500 (AC-3)."""
        config = DEFAULT_CONFIG.copy()
        config["vad_speech_pad_ms"] = 9999
        result, updated = _normalize_config(config)
        self.assertEqual(result["vad_speech_pad_ms"], 500)
        self.assertTrue(updated)

    def test_vad_speech_pad_ms_clamps_below_min(self):
        """vad_speech_pad_ms below 0 should clamp to 0 (AC-3)."""
        config = DEFAULT_CONFIG.copy()
        config["vad_speech_pad_ms"] = -5
        result, updated = _normalize_config(config)
        self.assertEqual(result["vad_speech_pad_ms"], 0)
        self.assertTrue(updated)

    def test_vad_speech_pad_ms_non_numeric_resets_to_default(self):
        """Non-numeric vad_speech_pad_ms should reset to default 200 (AC-3)."""
        config = DEFAULT_CONFIG.copy()
        config["vad_speech_pad_ms"] = "abc"
        result, updated = _normalize_config(config)
        self.assertEqual(result["vad_speech_pad_ms"], 200)
        self.assertTrue(updated)

    def test_vad_speech_pad_ms_is_int(self):
        """vad_speech_pad_ms should be stored as an int after normalize."""
        config = DEFAULT_CONFIG.copy()
        config["vad_speech_pad_ms"] = 150.7
        result, _ = _normalize_config(config)
        self.assertIsInstance(result["vad_speech_pad_ms"], int)
        self.assertEqual(result["vad_speech_pad_ms"], 150)

    def test_vad_threshold_clamps_above_max(self):
        """vad_threshold above 0.9 should clamp to 0.9 (AC-6)."""
        config = DEFAULT_CONFIG.copy()
        config["vad_threshold"] = 1.5
        result, updated = _normalize_config(config)
        self.assertEqual(result["vad_threshold"], 0.9)
        self.assertTrue(updated)

    def test_vad_threshold_clamps_below_min(self):
        """vad_threshold below 0.1 should clamp to 0.1 (AC-6)."""
        config = DEFAULT_CONFIG.copy()
        config["vad_threshold"] = 0.0
        result, updated = _normalize_config(config)
        self.assertEqual(result["vad_threshold"], 0.1)
        self.assertTrue(updated)

    def test_vad_threshold_non_numeric_resets_to_default(self):
        """Non-numeric vad_threshold should reset to default 0.5 (AC-6)."""
        config = DEFAULT_CONFIG.copy()
        config["vad_threshold"] = "loud"
        result, updated = _normalize_config(config)
        self.assertEqual(result["vad_threshold"], 0.5)
        self.assertTrue(updated)

    def test_vad_threshold_rounds_to_two_decimals(self):
        """vad_threshold should round to 2 decimals (AC-6)."""
        config = DEFAULT_CONFIG.copy()
        config["vad_threshold"] = 0.666
        result, _ = _normalize_config(config)
        self.assertEqual(result["vad_threshold"], 0.67)


class TestNewSubtitleStyles(unittest.TestCase):
    """Tests for expanded subtitle styles (T1 — subtitle-style-system-v2)."""

    def test_valid_subtitle_styles_has_seven_presets(self):
        """VALID_SUBTITLE_STYLES should contain exactly 7 presets."""
        expected = {"default", "karaoke", "neon", "minimal", "bold", "rgb", "typewriter"}
        self.assertEqual(VALID_SUBTITLE_STYLES, expected)

    def test_obs_enabled_in_default_config(self):
        """DEFAULT_CONFIG should include obs_enabled set to True."""
        self.assertIn("obs_enabled", DEFAULT_CONFIG)
        self.assertTrue(DEFAULT_CONFIG["obs_enabled"])

    def test_normalize_accepts_new_style_minimal(self):
        """_normalize_config should accept 'minimal' as valid subtitle style."""
        config = DEFAULT_CONFIG.copy()
        config["subtitle_style"] = "minimal"
        result, updated = _normalize_config(config)
        self.assertEqual(result["subtitle_style"], "minimal")
        self.assertFalse(updated)

    def test_normalize_validates_obs_enabled(self):
        """_normalize_config should validate obs_enabled as boolean."""
        config = DEFAULT_CONFIG.copy()
        config["obs_enabled"] = "not_a_bool"
        result, updated = _normalize_config(config)
        self.assertIsInstance(result["obs_enabled"], bool)


class TestSubtitleDisplayModeConfig(unittest.TestCase):
    """Tests for subtitle_display_mode / subtitle_ribbon_max_lines (subtitle-ribbon-buffer)."""

    def test_valid_display_modes_set(self):
        """VALID_SUBTITLE_DISPLAY_MODES must contain exactly the three modes."""
        self.assertEqual(
            VALID_SUBTITLE_DISPLAY_MODES,
            {"single", "ribbon", "adaptive"},
        )

    def test_default_display_mode_is_adaptive(self):
        """DEFAULT_CONFIG subtitle_display_mode must be 'adaptive' (LOCKED 2026-06-19)."""
        self.assertIn("subtitle_display_mode", DEFAULT_CONFIG)
        self.assertEqual(DEFAULT_CONFIG["subtitle_display_mode"], "adaptive")

    def test_default_ribbon_max_lines_is_three(self):
        """DEFAULT_CONFIG subtitle_ribbon_max_lines must default to 3 (OD-2)."""
        self.assertIn("subtitle_ribbon_max_lines", DEFAULT_CONFIG)
        self.assertEqual(DEFAULT_CONFIG["subtitle_ribbon_max_lines"], 3)

    def test_valid_display_modes_pass_through(self):
        """single/ribbon/adaptive must pass through unchanged (AC-6)."""
        for mode in ("single", "ribbon", "adaptive"):
            config = DEFAULT_CONFIG.copy()
            config["subtitle_display_mode"] = mode
            result, _ = _normalize_config(config)
            self.assertEqual(result["subtitle_display_mode"], mode)

    def test_unknown_display_mode_falls_back_to_adaptive(self):
        """Unknown display mode normalizes to the DEFAULT 'adaptive' (AC-6)."""
        config = DEFAULT_CONFIG.copy()
        config["subtitle_display_mode"] = "tape"
        result, updated = _normalize_config(config)
        self.assertEqual(result["subtitle_display_mode"], "adaptive")
        self.assertTrue(updated)

    def test_missing_display_mode_filled_with_default(self):
        """A config missing the key gets the default 'adaptive'."""
        config = {}
        result, _ = _normalize_config(config)
        self.assertEqual(result["subtitle_display_mode"], "adaptive")

    def test_ribbon_max_lines_clamps_above_max(self):
        """subtitle_ribbon_max_lines above 8 should clamp to 8 (AC-5)."""
        config = DEFAULT_CONFIG.copy()
        config["subtitle_ribbon_max_lines"] = 99
        result, updated = _normalize_config(config)
        self.assertEqual(result["subtitle_ribbon_max_lines"], 8)
        self.assertTrue(updated)

    def test_ribbon_max_lines_clamps_below_min(self):
        """subtitle_ribbon_max_lines of 0 should clamp to 1 (AC-5)."""
        config = DEFAULT_CONFIG.copy()
        config["subtitle_ribbon_max_lines"] = 0
        result, updated = _normalize_config(config)
        self.assertEqual(result["subtitle_ribbon_max_lines"], 1)
        self.assertTrue(updated)

    def test_ribbon_max_lines_coerces_float_to_int(self):
        """A float like 3.7 should coerce to int 3 (AC-5)."""
        config = DEFAULT_CONFIG.copy()
        config["subtitle_ribbon_max_lines"] = 3.7
        result, _ = _normalize_config(config)
        self.assertIsInstance(result["subtitle_ribbon_max_lines"], int)
        self.assertEqual(result["subtitle_ribbon_max_lines"], 3)

    def test_ribbon_max_lines_non_numeric_resets_to_default(self):
        """Non-coercible value resets to the default 3 (AC-5)."""
        config = DEFAULT_CONFIG.copy()
        config["subtitle_ribbon_max_lines"] = "many"
        result, updated = _normalize_config(config)
        self.assertEqual(result["subtitle_ribbon_max_lines"], 3)
        self.assertTrue(updated)

    def test_ribbon_max_lines_valid_passes_through(self):
        """A valid value in range passes through unchanged."""
        config = DEFAULT_CONFIG.copy()
        config["subtitle_ribbon_max_lines"] = 5
        result, _ = _normalize_config(config)
        self.assertEqual(result["subtitle_ribbon_max_lines"], 5)


class TestConfigValidation(unittest.TestCase):
    """Tests for config validation of unsafe values."""

    def test_rejects_negative_silence_timeout(self):
        """Negative silence_timeout should be clamped to minimum."""
        config = DEFAULT_CONFIG.copy()
        config["silence_timeout"] = -1.0
        result, updated = _normalize_config(config)
        self.assertGreaterEqual(result["silence_timeout"], 0.3)

    def test_rejects_excessive_silence_timeout(self):
        """Excessive silence_timeout should be clamped to maximum."""
        config = DEFAULT_CONFIG.copy()
        config["silence_timeout"] = 100.0
        result, updated = _normalize_config(config)
        self.assertLessEqual(result["silence_timeout"], 2.0)

    def test_rejects_negative_max_chunk_duration(self):
        """Negative max_chunk_duration should be clamped to minimum."""
        config = DEFAULT_CONFIG.copy()
        config["max_chunk_duration"] = -5.0
        result, updated = _normalize_config(config)
        self.assertGreaterEqual(result["max_chunk_duration"], 2.0)

    def test_rejects_excessive_max_chunk_duration(self):
        """Excessive max_chunk_duration should be clamped to maximum."""
        config = DEFAULT_CONFIG.copy()
        config["max_chunk_duration"] = 100.0
        result, updated = _normalize_config(config)
        self.assertLessEqual(result["max_chunk_duration"], 15.0)

    def test_rejects_zero_cpu_threads(self):
        """Zero cpu_threads should be clamped to minimum of 1."""
        config = DEFAULT_CONFIG.copy()
        config["cpu_threads"] = 0
        result, updated = _normalize_config(config)
        self.assertGreaterEqual(result["cpu_threads"], 1)

    def test_rejects_negative_subtitle_delay(self):
        """Negative subtitle_max_live_delay_sec should be clamped to minimum."""
        config = DEFAULT_CONFIG.copy()
        config["subtitle_max_live_delay_sec"] = -5.0
        result, updated = _normalize_config(config)
        self.assertGreaterEqual(result["subtitle_max_live_delay_sec"], 1.0)

    def test_rejects_negative_catchup_interval(self):
        """Negative subtitle_catchup_interval_sec should be clamped to minimum."""
        config = DEFAULT_CONFIG.copy()
        config["subtitle_catchup_interval_sec"] = -1.0
        result, updated = _normalize_config(config)
        self.assertGreaterEqual(result["subtitle_catchup_interval_sec"], 0.0)

    def test_diagnostics_defaults_exist(self):
        """DEFAULT_CONFIG should expose diagnostics defaults."""
        self.assertIn("diagnostics_enabled", DEFAULT_CONFIG)
        self.assertIn("diagnostics_level", DEFAULT_CONFIG)
        self.assertIn("diagnostics_export_dir", DEFAULT_CONFIG)
        self.assertIn(DEFAULT_CONFIG["diagnostics_level"], VALID_DIAGNOSTICS_LEVELS)

    def test_normalizes_invalid_diagnostics_level(self):
        """Invalid diagnostics level should be reset to default."""
        config = DEFAULT_CONFIG.copy()
        config["diagnostics_level"] = "ultra"
        result, updated = _normalize_config(config)
        self.assertEqual(result["diagnostics_level"], DEFAULT_CONFIG["diagnostics_level"])
        self.assertTrue(updated)

    def test_normalizes_diagnostics_export_dir(self):
        """Diagnostics export dir should normalize to an absolute path."""
        config = DEFAULT_CONFIG.copy()
        config["diagnostics_export_dir"] = "reports/diagnostics"
        result, updated = _normalize_config(config)
        self.assertTrue(os.path.isabs(result["diagnostics_export_dir"]))
        self.assertTrue(updated)

    def test_rejects_non_string_diagnostics_export_dir(self):
        """Non-string diagnostics export dir should reset to None."""
        config = DEFAULT_CONFIG.copy()
        config["diagnostics_export_dir"] = 123
        result, updated = _normalize_config(config)
        self.assertIsNone(result["diagnostics_export_dir"])
        self.assertTrue(updated)


class TestLoadSaveConfig(unittest.TestCase):
    """Tests for load_config() and save_config() functions."""

    def setUp(self):
        """Create a temporary directory used as both CWD and data home."""
        self.test_dir = tempfile.mkdtemp()
        self.original_cwd = os.getcwd()
        self.original_home = os.environ.get("LIVEAUDIO_HOME")
        os.environ["LIVEAUDIO_HOME"] = self.test_dir
        os.chdir(self.test_dir)

    def tearDown(self):
        """Clean up temporary directory and restore the data home."""
        os.chdir(self.original_cwd)
        if self.original_home is None:
            os.environ.pop("LIVEAUDIO_HOME", None)
        else:
            os.environ["LIVEAUDIO_HOME"] = self.original_home
        shutil.rmtree(self.test_dir)

    def test_load_config_creates_default_if_missing(self):
        """load_config should create config.json with defaults if missing."""
        config = load_config()
        self.assertTrue(os.path.exists("config.json"))
        for key in DEFAULT_CONFIG:
            self.assertIn(key, config)

    def test_load_config_reads_existing(self):
        """load_config should read existing config.json."""
        test_config = {"device": "cpu", "model_size": "tiny (Más rápido, baja precisión)"}
        save_config(test_config)
        config = load_config()
        self.assertEqual(config["device"], "cpu")

    def test_load_config_fills_missing_keys(self):
        """load_config should fill missing keys with defaults."""
        save_config({"device": "cpu"})
        config = load_config()
        self.assertIn("model_size", config)
        self.assertIn("blacklist", config)

    def test_save_config_writes_valid_json(self):
        """save_config should write valid JSON."""
        save_config(DEFAULT_CONFIG)
        with open("config.json", "r", encoding="utf-8") as f:
            loaded = json.load(f)
        self.assertEqual(loaded["device"], DEFAULT_CONFIG["device"])

    def test_load_config_respects_lock(self):
        """load_config should fallback to read-only if file is locked."""
        save_config(DEFAULT_CONFIG)
        
        with open("config.json", "w", encoding="utf-8") as f:
            broken_config = DEFAULT_CONFIG.copy()
            del broken_config["device"]
            json.dump(broken_config, f)
            
        open("config.json.lock", "w").close()
        
        with patch("liveaudio.utils.config.torch") as mock_torch:
            mock_torch.cuda.is_available.return_value = True
            config = load_config()
            
        self.assertEqual(config["device"], "cuda")
        
        with open("config.json", "r", encoding="utf-8") as f:
            disk_config = json.load(f)
            self.assertNotIn("device", disk_config)
            
        os.remove("config.json.lock")


if __name__ == "__main__":
    unittest.main()
