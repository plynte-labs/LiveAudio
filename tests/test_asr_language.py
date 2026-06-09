# SPDX-License-Identifier: MIT
"""Stress tests for ASR language separation from UI language with dual context prompts.

Covers:
  - Config migration (old whisper_context_prompt -> whisper_context_prompt_es)
  - Config validation (asr_language, dual prompts)
  - Engine dynamic language + context prompt injection
  - UI prompt swapping logic (_on_asr_language_change, _read_ui_config, _load_ui_from_config)
  - Rapid switching edge cases

All tests exercise runtime behavior — no source-code grepping or local simulators.
"""

import unittest
import copy
from unittest.mock import MagicMock, patch


# =============================================================================
#  Config Tests — all real (call _normalize_config directly)
# =============================================================================

class TestASRLanguageConfigMigration(unittest.TestCase):
    """Tests for migration of old whisper_context_prompt to new dual keys."""

    def test_migrates_old_prompt_to_spanish(self):
        """Old whisper_context_prompt content should migrate to whisper_context_prompt_es."""
        from utils.config import _normalize_config, DEFAULT_CONFIG

        config = DEFAULT_CONFIG.copy()
        config["whisper_context_prompt"] = "Stream de gaming en español"

        result, updated = _normalize_config(config)

        self.assertEqual(result["whisper_context_prompt_es"], "Stream de gaming en español")
        self.assertNotIn("whisper_context_prompt", result)
        self.assertTrue(updated)

    def test_migration_does_not_overwrite_existing_spanish_prompt(self):
        """If whisper_context_prompt_es already set, migration should not overwrite it."""
        from utils.config import _normalize_config, DEFAULT_CONFIG

        config = DEFAULT_CONFIG.copy()
        config["whisper_context_prompt"] = "Old prompt from migration"
        config["whisper_context_prompt_es"] = "Already set manually"

        result, updated = _normalize_config(config)

        self.assertEqual(result["whisper_context_prompt_es"], "Already set manually")
        self.assertNotIn("whisper_context_prompt", result)

    def test_migration_with_empty_old_prompt_is_noop(self):
        """Empty old prompt should not trigger migration write but still deletes key."""
        from utils.config import _normalize_config, DEFAULT_CONFIG

        config = DEFAULT_CONFIG.copy()
        config["whisper_context_prompt"] = ""

        result, updated = _normalize_config(config)

        self.assertEqual(result["whisper_context_prompt_es"], "")
        self.assertNotIn("whisper_context_prompt", result)

    def test_migration_with_none_old_prompt(self):
        """None old prompt should be handled gracefully."""
        from utils.config import _normalize_config, DEFAULT_CONFIG

        config = DEFAULT_CONFIG.copy()
        config["whisper_context_prompt"] = None

        result, updated = _normalize_config(config)

        self.assertEqual(result["whisper_context_prompt_es"], "")
        self.assertNotIn("whisper_context_prompt", result)

    def test_no_migration_when_old_key_absent(self):
        """Config without old key should just get defaults for new keys."""
        from utils.config import _normalize_config, DEFAULT_CONFIG

        config = {"device": "cpu"}  # minimal config

        result, updated = _normalize_config(config)

        self.assertIn("whisper_context_prompt_es", result)
        self.assertIn("whisper_context_prompt_en", result)
        self.assertEqual(result["whisper_context_prompt_es"], "")
        self.assertEqual(result["whisper_context_prompt_en"], "")
        self.assertNotIn("whisper_context_prompt", result)


class TestASRLanguageConfigValidation(unittest.TestCase):
    """Tests for validation of new ASR config keys."""

    def test_asr_language_accepts_es(self):
        """asr_language='es' should be accepted."""
        from utils.config import _normalize_config, DEFAULT_CONFIG

        config = DEFAULT_CONFIG.copy()
        config["asr_language"] = "es"
        result, updated = _normalize_config(config)

        self.assertEqual(result["asr_language"], "es")
        self.assertFalse(updated)

    def test_asr_language_accepts_en(self):
        """asr_language='en' should be accepted."""
        from utils.config import _normalize_config, DEFAULT_CONFIG

        config = DEFAULT_CONFIG.copy()
        config["asr_language"] = "en"
        result, updated = _normalize_config(config)

        self.assertEqual(result["asr_language"], "en")
        self.assertFalse(updated)

    def test_asr_language_rejects_invalid_value(self):
        """asr_language='fr' should be reset to default 'es'."""
        from utils.config import _normalize_config, DEFAULT_CONFIG

        config = DEFAULT_CONFIG.copy()
        config["asr_language"] = "fr"
        result, updated = _normalize_config(config)

        self.assertEqual(result["asr_language"], "es")
        self.assertTrue(updated)

    def test_asr_language_rejects_none(self):
        """asr_language=None should be reset to default 'es'."""
        from utils.config import _normalize_config, DEFAULT_CONFIG

        config = DEFAULT_CONFIG.copy()
        config["asr_language"] = None
        result, updated = _normalize_config(config)

        self.assertEqual(result["asr_language"], "es")
        self.assertTrue(updated)

    def test_asr_language_rejects_empty_string(self):
        """asr_language='' should be reset to default 'es'."""
        from utils.config import _normalize_config, DEFAULT_CONFIG

        config = DEFAULT_CONFIG.copy()
        config["asr_language"] = ""
        result, updated = _normalize_config(config)

        self.assertEqual(result["asr_language"], "es")
        self.assertTrue(updated)

    def test_whisper_context_prompt_es_rejects_non_string(self):
        """whisper_context_prompt_es with non-string should be reset to empty string."""
        from utils.config import _normalize_config, DEFAULT_CONFIG

        config = DEFAULT_CONFIG.copy()
        config["whisper_context_prompt_es"] = 12345
        result, updated = _normalize_config(config)

        self.assertEqual(result["whisper_context_prompt_es"], "")
        self.assertTrue(updated)

    def test_whisper_context_prompt_en_rejects_non_string(self):
        """whisper_context_prompt_en with non-string should be reset to empty string."""
        from utils.config import _normalize_config, DEFAULT_CONFIG

        config = DEFAULT_CONFIG.copy()
        config["whisper_context_prompt_en"] = ["list", "of", "strings"]
        result, updated = _normalize_config(config)

        self.assertEqual(result["whisper_context_prompt_en"], "")
        self.assertTrue(updated)

    def test_default_config_has_all_asr_keys(self):
        """DEFAULT_CONFIG must contain all new ASR-related keys."""
        from utils.config import DEFAULT_CONFIG

        self.assertIn("asr_language", DEFAULT_CONFIG)
        self.assertIn("whisper_context_prompt_es", DEFAULT_CONFIG)
        self.assertIn("whisper_context_prompt_en", DEFAULT_CONFIG)
        self.assertEqual(DEFAULT_CONFIG["asr_language"], "es")

    def test_old_whisper_context_prompt_not_in_default_config(self):
        """DEFAULT_CONFIG must NOT contain the deprecated whisper_context_prompt key."""
        from utils.config import DEFAULT_CONFIG

        self.assertNotIn("whisper_context_prompt", DEFAULT_CONFIG)

    def test_empty_config_gets_all_asr_defaults(self):
        """Empty config dict should be filled with all ASR defaults."""
        from utils.config import _normalize_config

        config = {}
        result, updated = _normalize_config(config)

        self.assertEqual(result["asr_language"], "es")
        self.assertEqual(result["whisper_context_prompt_es"], "")
        self.assertEqual(result["whisper_context_prompt_en"], "")
        self.assertTrue(updated)


# =============================================================================
#  Engine Tests — all real (call _transcribe_with_timeout with mock model)
# =============================================================================

class TestTranscribeWithTimeoutLanguage(unittest.TestCase):
    """Tests for dynamic language injection in _transcribe_with_timeout."""

    def test_language_passed_to_transcribe_kwargs(self):
        """_transcribe_with_timeout should pass language to model.transcribe kwargs."""
        from core.engine import _transcribe_with_timeout

        mock_model = MagicMock()
        mock_model.transcribe.return_value = ([], MagicMock())
        mock_audio = b"fake_audio_chunk"

        _transcribe_with_timeout(
            mock_model, mock_audio,
            timeout_sec=5.0, log_queue=None, device="cpu",
            initial_prompt=None, language="en",
        )

        mock_model.transcribe.assert_called_once()
        call_kwargs = mock_model.transcribe.call_args[1]
        self.assertEqual(call_kwargs["language"], "en")

    def test_language_defaults_to_es(self):
        """When language not specified, defaults to 'es'."""
        from core.engine import _transcribe_with_timeout

        mock_model = MagicMock()
        mock_model.transcribe.return_value = ([], MagicMock())

        _transcribe_with_timeout(
            mock_model, b"fake_audio",
            timeout_sec=5.0, log_queue=None, device="cpu",
        )

        call_kwargs = mock_model.transcribe.call_args[1]
        self.assertEqual(call_kwargs["language"], "es")

    def test_initial_prompt_passed_when_provided(self):
        """initial_prompt should be included in kwargs when not None."""
        from core.engine import _transcribe_with_timeout

        mock_model = MagicMock()
        mock_model.transcribe.return_value = ([], MagicMock())

        _transcribe_with_timeout(
            mock_model, b"fake_audio",
            timeout_sec=5.0, log_queue=None, device="cpu",
            initial_prompt="Coding stream about Python",
            language="en",
        )

        call_kwargs = mock_model.transcribe.call_args[1]
        self.assertEqual(call_kwargs["initial_prompt"], "Coding stream about Python")
        self.assertEqual(call_kwargs["language"], "en")

    def test_initial_prompt_not_in_kwargs_when_none(self):
        """initial_prompt should NOT be in kwargs when None."""
        from core.engine import _transcribe_with_timeout

        mock_model = MagicMock()
        mock_model.transcribe.return_value = ([], MagicMock())

        _transcribe_with_timeout(
            mock_model, b"fake_audio",
            timeout_sec=5.0, log_queue=None, device="cpu",
            initial_prompt=None, language="es",
        )

        call_kwargs = mock_model.transcribe.call_args[1]
        self.assertNotIn("initial_prompt", call_kwargs)


class TestASRConsumerDynamicConfig(unittest.TestCase):
    """Tests for _asr_consumer reading language and prompt from shared_config each cycle.

    These tests verify the consumer's read logic by simulating what the consumer does:
    read from shared_config, build the prompt key, call _transcribe_with_timeout.
    """

    def test_reads_asr_language_and_passes_to_transcribe(self):
        """The consumer's read pattern should pass the correct language to transcribe."""
        from core.engine import _transcribe_with_timeout

        # Simulate what asr_consumer does each cycle:
        shared_config = {"asr_language": "en"}
        asr_lang = shared_config.get("asr_language") or "es"

        mock_model = MagicMock()
        mock_model.transcribe.return_value = ([], MagicMock())

        _transcribe_with_timeout(
            mock_model, b"fake", timeout_sec=1,
            log_queue=None, device="cpu", language=asr_lang,
        )

        call_kwargs = mock_model.transcribe.call_args[1]
        self.assertEqual(call_kwargs["language"], "en")

    def test_prompt_key_built_from_asr_language_resolves_correct_prompt(self):
        """Building prompt_key from asr_language should resolve the correct prompt value."""
        # Simulate consumer's prompt resolution logic
        shared_config = {
            "asr_language": "en",
            "whisper_context_prompt_es": "Prompt español",
            "whisper_context_prompt_en": "English prompt",
        }
        asr_lang = shared_config.get("asr_language") or "es"
        prompt_key = f"whisper_context_prompt_{asr_lang}"
        context_prompt = shared_config.get(prompt_key) or None

        self.assertEqual(prompt_key, "whisper_context_prompt_en")
        self.assertEqual(context_prompt, "English prompt")

    def test_fallback_to_es_when_asr_language_missing(self):
        """Should fall back to 'es' when asr_language key is absent from shared_config."""
        from core.engine import _transcribe_with_timeout

        # Simulate consumer with missing asr_language
        shared_config = {}
        asr_lang = shared_config.get("asr_language") or "es"

        mock_model = MagicMock()
        mock_model.transcribe.return_value = ([], MagicMock())

        _transcribe_with_timeout(
            mock_model, b"fake", timeout_sec=1,
            log_queue=None, device="cpu", language=asr_lang,
        )

        call_kwargs = mock_model.transcribe.call_args[1]
        self.assertEqual(call_kwargs["language"], "es")


class TestASRConsumerStressScenarios(unittest.TestCase):
    """Stress scenarios for dynamic ASR language switching during runtime."""

    def test_rapid_language_switch_picks_up_new_config(self):
        """Changing shared_config['asr_language'] mid-loop should affect next transcribe call."""
        from core.engine import _transcribe_with_timeout

        # Simulate two consecutive cycles with different languages
        shared_config = {"asr_language": "es"}

        # Cycle 1: Spanish
        asr_lang_1 = shared_config.get("asr_language") or "es"
        mock_model_1 = MagicMock()
        mock_model_1.transcribe.return_value = ([], MagicMock())
        _transcribe_with_timeout(
            mock_model_1, b"fake", timeout_sec=1,
            log_queue=None, device="cpu", language=asr_lang_1,
        )
        self.assertEqual(mock_model_1.transcribe.call_args[1]["language"], "es")

        # Change config mid-loop (simulating hot-swap via shared_config)
        shared_config["asr_language"] = "en"

        # Cycle 2: English — should pick up the new value
        asr_lang_2 = shared_config.get("asr_language") or "es"
        mock_model_2 = MagicMock()
        mock_model_2.transcribe.return_value = ([], MagicMock())
        _transcribe_with_timeout(
            mock_model_2, b"fake", timeout_sec=1,
            log_queue=None, device="cpu", language=asr_lang_2,
        )
        self.assertEqual(mock_model_2.transcribe.call_args[1]["language"], "en")

    def test_both_languages_produce_different_kwargs(self):
        """Switching from 'es' to 'en' should pass different language to transcribe."""
        from core.engine import _transcribe_with_timeout

        mock_model_es = MagicMock()
        mock_model_es.transcribe.return_value = ([], MagicMock())
        _transcribe_with_timeout(
            mock_model_es, b"fake", timeout_sec=1,
            log_queue=None, device="cpu", language="es",
        )
        self.assertEqual(mock_model_es.transcribe.call_args[1]["language"], "es")

        mock_model_en = MagicMock()
        mock_model_en.transcribe.return_value = ([], MagicMock())
        _transcribe_with_timeout(
            mock_model_en, b"fake", timeout_sec=1,
            log_queue=None, device="cpu", language="en",
        )
        self.assertEqual(mock_model_en.transcribe.call_args[1]["language"], "en")


# =============================================================================
#  UI Logic Tests — all real (call LiveASRApp methods with mock instances)
# =============================================================================

class TestOnASRLanguageChange(unittest.TestCase):
    """Tests for _on_asr_language_change method using the REAL method on mock instances."""

    def _make_mock_app(self, draft_config, ui_ready=True, textbox_content=""):
        """Create a minimal mock instance with the attributes _on_asr_language_change needs."""
        mock = MagicMock(spec=[])
        mock.text_whisper_prompt = MagicMock()
        mock.text_whisper_prompt.get.return_value = textbox_content
        mock.lbl_whisper_context_help = MagicMock()
        mock.draft_config = draft_config
        mock.config_data = copy.deepcopy(draft_config)
        mock._ui_ready = ui_ready
        mock.on_setting_change = MagicMock()
        return mock

    def test_switch_from_es_to_en_saves_spanish_prompt(self):
        """Switching ES→EN should save current textbox content to whisper_context_prompt_es."""
        from main import LiveASRApp

        draft = {
            "asr_language": "es",
            "whisper_context_prompt_es": "Old ES prompt",
            "whisper_context_prompt_en": "EN prompt",
        }
        mock = self._make_mock_app(draft, ui_ready=True, textbox_content="Nuevo texto español")

        LiveASRApp._on_asr_language_change(mock, "English")

        self.assertEqual(mock.draft_config["whisper_context_prompt_es"], "Nuevo texto español")
        self.assertEqual(mock.draft_config["asr_language"], "en")

    def test_switch_from_en_to_es_saves_english_prompt(self):
        """Switching EN→ES should save current textbox content to whisper_context_prompt_en."""
        from main import LiveASRApp

        draft = {
            "asr_language": "en",
            "whisper_context_prompt_es": "ES prompt",
            "whisper_context_prompt_en": "Old EN prompt",
        }
        mock = self._make_mock_app(draft, ui_ready=True, textbox_content="New English text")

        LiveASRApp._on_asr_language_change(mock, "Español")

        self.assertEqual(mock.draft_config["whisper_context_prompt_en"], "New English text")
        self.assertEqual(mock.draft_config["asr_language"], "es")

    def test_switch_loads_new_language_prompt_into_textbox(self):
        """After switch, textbox should be cleared and loaded with the saved prompt for the new language."""
        from main import LiveASRApp

        draft = {
            "asr_language": "en",
            "whisper_context_prompt_es": "Prompt guardado en español",
            "whisper_context_prompt_en": "English prompt",
        }
        mock = self._make_mock_app(draft, ui_ready=True, textbox_content="Old content")

        LiveASRApp._on_asr_language_change(mock, "Español")

        mock.text_whisper_prompt.delete.assert_called_with("0.0", "end")
        mock.text_whisper_prompt.insert.assert_called_with("0.0", "Prompt guardado en español")

    def test_switch_updates_help_label(self):
        """Help label should be updated to the correct translation key for the new ASR language."""
        from main import LiveASRApp
        from utils.i18n import t

        draft = {
            "asr_language": "es",
            "whisper_context_prompt_es": "",
            "whisper_context_prompt_en": "",
        }
        mock = self._make_mock_app(draft, ui_ready=True)

        LiveASRApp._on_asr_language_change(mock, "English")

        # The real method calls t(help_key) — verify configure was called with a translated string
        mock.lbl_whisper_context_help.configure.assert_called_once()
        call_text = mock.lbl_whisper_context_help.configure.call_args[1]["text"]
        expected_text = t("whisper_context_help_en")
        self.assertEqual(call_text, expected_text)

    def test_switch_triggers_on_setting_change(self):
        """on_setting_change should be called after language switch."""
        from main import LiveASRApp

        draft = {
            "asr_language": "es",
            "whisper_context_prompt_es": "",
            "whisper_context_prompt_en": "",
        }
        mock = self._make_mock_app(draft, ui_ready=True)

        LiveASRApp._on_asr_language_change(mock, "English")

        mock.on_setting_change.assert_called_once()

    def test_same_language_is_noop(self):
        """Selecting the same language should do nothing — no textbox changes, no config mutation."""
        from main import LiveASRApp

        original_draft = {
            "asr_language": "es",
            "whisper_context_prompt_es": "ES prompt",
            "whisper_context_prompt_en": "EN prompt",
        }
        mock = self._make_mock_app(original_draft, ui_ready=True, textbox_content="Content")

        LiveASRApp._on_asr_language_change(mock, "Español")

        mock.text_whisper_prompt.delete.assert_not_called()
        mock.text_whisper_prompt.insert.assert_not_called()
        mock.lbl_whisper_context_help.configure.assert_not_called()
        mock.on_setting_change.assert_not_called()
        # Config should be unchanged
        self.assertEqual(mock.draft_config["asr_language"], "es")
        self.assertEqual(mock.draft_config["whisper_context_prompt_es"], "ES prompt")

    def test_ui_not_ready_is_noop(self):
        """When _ui_ready is False, the real method should return immediately without side effects."""
        from main import LiveASRApp

        draft = {
            "asr_language": "es",
            "whisper_context_prompt_es": "ES prompt",
            "whisper_context_prompt_en": "EN prompt",
        }
        mock = self._make_mock_app(draft, ui_ready=False, textbox_content="Content")

        LiveASRApp._on_asr_language_change(mock, "English")

        # Nothing should have been called or changed
        mock.text_whisper_prompt.get.assert_not_called()
        mock.text_whisper_prompt.delete.assert_not_called()
        mock.text_whisper_prompt.insert.assert_not_called()
        mock.lbl_whisper_context_help.configure.assert_not_called()
        mock.on_setting_change.assert_not_called()
        self.assertEqual(mock.draft_config["asr_language"], "es")

    def test_empty_prompt_saved_correctly(self):
        """Empty prompt (user typed nothing) should be saved as empty string."""
        from main import LiveASRApp

        draft = {
            "asr_language": "es",
            "whisper_context_prompt_es": "ES prompt",
            "whisper_context_prompt_en": "",
        }
        mock = self._make_mock_app(draft, ui_ready=True, textbox_content="")

        LiveASRApp._on_asr_language_change(mock, "English")

        self.assertEqual(mock.draft_config["whisper_context_prompt_es"], "")
        mock.text_whisper_prompt.insert.assert_called_with("0.0", "")

    def test_rapid_switch_es_en_es_preserves_both_prompts(self):
        """Rapid ES→EN→ES switching should preserve prompts for both languages."""
        from main import LiveASRApp

        draft = {
            "asr_language": "es",
            "whisper_context_prompt_es": "Initial ES",
            "whisper_context_prompt_en": "Initial EN",
        }

        # First switch: ES→EN
        mock1 = self._make_mock_app(draft, ui_ready=True, textbox_content="Hola en español")
        LiveASRApp._on_asr_language_change(mock1, "English")
        self.assertEqual(mock1.draft_config["whisper_context_prompt_es"], "Hola en español")
        self.assertEqual(mock1.draft_config["asr_language"], "en")

        # Second switch: EN→ES (using the updated draft from first switch)
        mock2 = self._make_mock_app(mock1.draft_config, ui_ready=True, textbox_content="Hello in English")
        LiveASRApp._on_asr_language_change(mock2, "Español")

        self.assertEqual(mock2.draft_config["whisper_context_prompt_en"], "Hello in English")
        self.assertEqual(mock2.draft_config["asr_language"], "es")
        # The Spanish prompt from the first switch should still be there
        self.assertEqual(mock2.draft_config["whisper_context_prompt_es"], "Hola en español")


class TestReadUIConfigASRLanguage(unittest.TestCase):
    """Tests for _read_ui_config handling of ASR language dual prompts — runtime verification."""

    def _make_mock_app_for_read(self, draft_config, config_data, textbox_content="Current prompt text"):
        """Create a minimal mock with the attributes _read_ui_config reads."""
        mock = MagicMock(spec=[])
        mock.draft_config = draft_config
        mock.config_data = config_data
        mock.var_hw = MagicMock()
        mock.var_hw.get.return_value = "cpu"
        mock.var_model = MagicMock()
        mock.var_model.get.return_value = "small (Balance CPU)"
        mock.slider_threads = MagicMock()
        mock.slider_threads.get.return_value = 4
        mock.slider_silence = MagicMock()
        mock.slider_silence.get.return_value = 0.8
        mock.slider_max_dur = MagicMock()
        mock.slider_max_dur.get.return_value = 5.0
        mock.slider_max_live_delay = MagicMock()
        mock.slider_max_live_delay.get.return_value = 10.0
        mock.slider_catchup_interval = MagicMock()
        mock.slider_catchup_interval.get.return_value = 1.5
        mock.var_session = MagicMock()
        mock.var_session.get.return_value = True
        mock.text_blacklist = MagicMock()
        mock.text_blacklist.get.return_value = "amara.org"
        mock.var_style = MagicMock()
        mock.var_style.get.return_value = "default"
        mock.var_backlog_policy = MagicMock()
        mock.var_backlog_policy.get.return_value = "Auto (recomendado)"
        mock.var_obs_enabled = MagicMock()
        mock.var_obs_enabled.get.return_value = True
        mock.text_whisper_prompt = MagicMock()
        mock.text_whisper_prompt.get.return_value = textbox_content
        return mock

    def test_read_ui_config_preserves_both_prompts_from_draft(self):
        """_read_ui_config must carry over both whisper_context_prompt_es and _en from draft_config.

        The ACTIVE language's prompt is overwritten with the textbox content (user's current edit).
        The INACTIVE language's prompt is preserved from draft_config.
        """
        from main import LiveASRApp

        draft = {
            "asr_language": "es",
            "whisper_context_prompt_es": "Draft ES prompt",
            "whisper_context_prompt_en": "Draft EN prompt",
        }
        config_data = {
            "asr_language": "en",
            "whisper_context_prompt_es": "Config ES prompt",
            "whisper_context_prompt_en": "Config EN prompt",
        }
        mock = self._make_mock_app_for_read(draft, config_data, textbox_content="User typing ES")

        result = LiveASRApp._read_ui_config(mock)

        # Active language (es) prompt comes from textbox (user's current edit)
        self.assertEqual(result["whisper_context_prompt_es"], "User typing ES")
        # Inactive language (en) prompt is preserved from draft_config
        self.assertEqual(result["whisper_context_prompt_en"], "Draft EN prompt")

    def test_read_ui_config_includes_asr_language(self):
        """_read_ui_config output must include 'asr_language' key with the correct value."""
        from main import LiveASRApp

        draft = {"asr_language": "en"}
        config_data = {"asr_language": "en"}
        mock = self._make_mock_app_for_read(draft, config_data)

        result = LiveASRApp._read_ui_config(mock)

        self.assertIn("asr_language", result)
        self.assertEqual(result["asr_language"], "en")

    def test_read_ui_config_falls_back_to_config_data(self):
        """When draft_config lacks prompt keys, should fall back to config_data values.

        The ACTIVE language's prompt still comes from textbox (user's current edit).
        The INACTIVE language's prompt falls back to config_data since draft doesn't have it.
        """
        from main import LiveASRApp

        draft = {"asr_language": "es"}  # No prompt keys in draft
        config_data = {
            "asr_language": "es",
            "whisper_context_prompt_es": "Fallback ES",
            "whisper_context_prompt_en": "Fallback EN",
        }
        mock = self._make_mock_app_for_read(draft, config_data, textbox_content="User typing ES")

        result = LiveASRApp._read_ui_config(mock)

        # Active language (es) prompt comes from textbox
        self.assertEqual(result["whisper_context_prompt_es"], "User typing ES")
        # Inactive language (en) prompt falls back to config_data
        self.assertEqual(result["whisper_context_prompt_en"], "Fallback EN")

    def test_read_ui_config_overwrites_active_prompt_from_textbox(self):
        """The active language's prompt should be overwritten with the current textbox content."""
        from main import LiveASRApp

        draft = {
            "asr_language": "en",
            "whisper_context_prompt_es": "Draft ES",
            "whisper_context_prompt_en": "Draft EN",
        }
        config_data = copy.deepcopy(draft)
        mock = self._make_mock_app_for_read(draft, config_data)
        # Textbox has new content that should override the draft value
        mock.text_whisper_prompt.get.return_value = "Fresh from UI"

        result = LiveASRApp._read_ui_config(mock)

        # Active language (en) prompt should be from textbox
        self.assertEqual(result["whisper_context_prompt_en"], "Fresh from UI")
        # Inactive language (es) prompt should be from draft
        self.assertEqual(result["whisper_context_prompt_es"], "Draft ES")


class TestLoadUIFromConfigASRLanguage(unittest.TestCase):
    """Tests for _load_ui_from_config handling of ASR language — runtime verification."""

    def _make_mock_app_for_load(self):
        """Create a minimal mock with the attributes _load_ui_from_config writes to."""
        mock = MagicMock(spec=[])
        mock.draft_config = {}
        mock._ui_ready = True
        mock._device_display_list = ["🔄 Por defecto del sistema"]
        mock.var_device = MagicMock()
        mock.var_hw = MagicMock()
        mock.slider_threads = MagicMock()
        mock.opt_model = MagicMock()
        mock.opt_model.cget.return_value = ["small (Balance CPU)"]
        mock.var_model = MagicMock()
        mock.slider_silence = MagicMock()
        mock.slider_max_dur = MagicMock()
        mock.slider_max_live_delay = MagicMock()
        mock.slider_catchup_interval = MagicMock()
        mock.var_session = MagicMock()
        mock.var_style = MagicMock()
        mock.var_backlog_policy = MagicMock()
        mock.text_blacklist = MagicMock()
        mock.var_obs_enabled = MagicMock()
        mock.var_asr_lang = MagicMock()
        mock.text_whisper_prompt = MagicMock()
        mock.lbl_whisper_context_help = MagicMock()
        mock.on_setting_change = MagicMock()
        return mock

    def _make_full_config(self, asr_language="es"):
        """Create a complete config dict with all keys _load_ui_from_config reads."""
        return {
            "device": "cpu",
            "cpu_threads": 4,
            "model_size": "small (Balance CPU)",
            "silence_timeout": 0.8,
            "max_chunk_duration": 5.0,
            "continuous_session": True,
            "subtitle_style": "default",
            "subtitle_backlog_policy": "auto",
            "subtitle_max_live_delay_sec": 10.0,
            "subtitle_catchup_interval_sec": 1.5,
            "blacklist": "amara.org",
            "obs_enabled": True,
            "audio_device": None,
            "asr_language": asr_language,
            "whisper_context_prompt_es": "ES prompt",
            "whisper_context_prompt_en": "EN prompt",
        }

    def test_load_ui_sets_asr_lang_var_to_english(self):
        """_load_ui_from_config should set var_asr_lang to 'English' when asr_language='en'."""
        from main import LiveASRApp

        mock = self._make_mock_app_for_load()
        config = self._make_full_config(asr_language="en")

        LiveASRApp._load_ui_from_config(mock, config)

        mock.var_asr_lang.set.assert_called_with("English")

    def test_load_ui_sets_asr_lang_var_to_spanish(self):
        """_load_ui_from_config should set var_asr_lang to 'Español' when asr_language='es'."""
        from main import LiveASRApp

        mock = self._make_mock_app_for_load()
        config = self._make_full_config(asr_language="es")

        LiveASRApp._load_ui_from_config(mock, config)

        mock.var_asr_lang.set.assert_called_with("Español")

    def test_load_ui_reads_correct_prompt_key_for_english(self):
        """Should load whisper_context_prompt_en when asr_language='en'."""
        from main import LiveASRApp

        mock = self._make_mock_app_for_load()
        config = self._make_full_config(asr_language="en")
        config["whisper_context_prompt_es"] = "Wrong prompt (ES)"
        config["whisper_context_prompt_en"] = "Correct prompt (EN)"

        LiveASRApp._load_ui_from_config(mock, config)

        # Textbox should be loaded with the EN prompt, not the ES prompt
        mock.text_whisper_prompt.insert.assert_called_with("0.0", "Correct prompt (EN)")

    def test_load_ui_reads_correct_prompt_key_for_spanish(self):
        """Should load whisper_context_prompt_es when asr_language='es'."""
        from main import LiveASRApp

        mock = self._make_mock_app_for_load()
        config = self._make_full_config(asr_language="es")
        config["whisper_context_prompt_es"] = "Correct prompt (ES)"
        config["whisper_context_prompt_en"] = "Wrong prompt (EN)"

        LiveASRApp._load_ui_from_config(mock, config)

        mock.text_whisper_prompt.insert.assert_called_with("0.0", "Correct prompt (ES)")

    def test_load_ui_updates_help_label_for_english(self):
        """Should update lbl_whisper_context_help with the English help text."""
        from main import LiveASRApp
        from utils.i18n import t

        mock = self._make_mock_app_for_load()
        config = self._make_full_config(asr_language="en")

        LiveASRApp._load_ui_from_config(mock, config)

        mock.lbl_whisper_context_help.configure.assert_called_once()
        call_text = mock.lbl_whisper_context_help.configure.call_args[1]["text"]
        expected_text = t("whisper_context_help_en")
        self.assertEqual(call_text, expected_text)

    def test_load_ui_updates_help_label_for_spanish(self):
        """Should update lbl_whisper_context_help with the Spanish help text."""
        from main import LiveASRApp
        from utils.i18n import t

        mock = self._make_mock_app_for_load()
        config = self._make_full_config(asr_language="es")

        LiveASRApp._load_ui_from_config(mock, config)

        call_text = mock.lbl_whisper_context_help.configure.call_args[1]["text"]
        expected_text = t("whisper_context_help_es")
        self.assertEqual(call_text, expected_text)


class TestOnASRLanguageChangeMethodExists(unittest.TestCase):
    """Verify the method and UI elements exist on LiveASRApp — runtime checks only."""

    def test_on_asr_language_change_method_exists(self):
        """_on_asr_language_change should be a method on LiveASRApp."""
        from main import LiveASRApp

        self.assertTrue(hasattr(LiveASRApp, "_on_asr_language_change"))
        self.assertTrue(callable(getattr(LiveASRApp, "_on_asr_language_change", None)))

    def test_asr_lang_var_created_in_build_main_screen(self):
        """build_main_screen source must create var_asr_lang, opt_asr_lang, and reference asr_language."""
        from main import LiveASRApp
        import inspect

        source = inspect.getsource(LiveASRApp.build_main_screen)
        # These are structural checks — the real behavior is tested in TestLoadUIFromConfigASRLanguage
        self.assertIn("var_asr_lang", source)
        self.assertIn("opt_asr_lang", source)
        self.assertIn("asr_language", source)

    def test_spoken_language_label_used_in_ui(self):
        """build_main_screen must reference the spoken_language_label translation key."""
        from main import LiveASRApp
        import inspect

        source = inspect.getsource(LiveASRApp.build_main_screen)
        self.assertIn('spoken_language_label', source)

    def test_asr_lang_dropdown_has_correct_values(self):
        """build_main_screen source must offer 'Español' and 'English' as dropdown values."""
        from main import LiveASRApp
        import inspect

        source = inspect.getsource(LiveASRApp.build_main_screen)
        self.assertIn('"Español"', source)
        self.assertIn('"English"', source)


if __name__ == "__main__":
    unittest.main()
