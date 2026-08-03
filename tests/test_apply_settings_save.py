# SPDX-License-Identifier: MIT
"""apply_pending_settings must not report success when the save failed."""

import copy
import os
import shutil
import tempfile
import types
import unittest
from unittest.mock import MagicMock, patch

from liveaudio.app import LiveASRApp
from liveaudio.utils.config import DEFAULT_CONFIG
from liveaudio.utils.i18n import t


class _StubVar:
    def __init__(self, value=""):
        self.value = value

    def get(self):
        return self.value

    def set(self, value):
        self.value = value


def _make_app(config_data, draft):
    """Minimal stand-in exposing only what apply_pending_settings touches."""
    app = types.SimpleNamespace(
        _applying_settings=False,
        is_running=False,
        config_data=copy.deepcopy(config_data),
        draft_config=copy.deepcopy(config_data),
        shared_config=dict(config_data),
        var_profile=_StubVar(t("custom")),
        btn_apply=MagicMock(),
        logs=[],
    )
    app._read_ui_config = lambda: copy.deepcopy(draft)
    app._profile_id_for_current_values = lambda cfg: "balanced"
    app._pending_restart_flags = lambda cfg: (False, False)
    app._validate_draft_config = lambda cfg: None
    app.refresh_profile_status = lambda: None
    app.after = lambda delay, callback: None
    app.print_log = app.logs.append
    return app


class TestApplyPendingSettingsSaveFailure(unittest.TestCase):
    """A failed config save must surface as an error, never as success."""

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        self.config = dict(DEFAULT_CONFIG, output_dir=self.test_dir)
        self.draft = dict(self.config, silence_timeout=1.5)

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def _apply(self, save_result):
        app = _make_app(self.config, self.draft)
        with patch("liveaudio.app.save_config", return_value=save_result) as save:
            with patch("liveaudio.app.messagebox") as messagebox:
                LiveASRApp.apply_pending_settings(app)
        return app, save, messagebox

    def test_successful_save_reports_applied(self):
        """The happy path still logs the applied message."""
        app, save, messagebox = self._apply(True)
        self.assertTrue(save.called)
        self.assertIn(t("log_config_applied"), app.logs)
        self.assertFalse(messagebox.showerror.called)
        self.assertEqual(app.config_data["silence_timeout"], 1.5)

    def test_failed_save_does_not_report_applied(self):
        """A failed save must never log 'configuration applied and saved'."""
        app, _save, _messagebox = self._apply(False)
        self.assertNotIn(t("log_config_applied"), app.logs)

    def test_failed_save_shows_error(self):
        """A failed save must reach the user through the existing error path."""
        app, _save, messagebox = self._apply(False)
        self.assertTrue(messagebox.showerror.called)
        self.assertTrue(any(t("log_config_save_failed") in line for line in app.logs))

    def test_failed_save_rolls_back_in_memory_config(self):
        """In-memory config must not drift away from the on-disk file."""
        app, _save, _messagebox = self._apply(False)
        self.assertEqual(app.config_data["silence_timeout"], self.config["silence_timeout"])
        self.assertEqual(app.shared_config["silence_timeout"], self.config["silence_timeout"])

    def test_failed_save_clears_applying_flag(self):
        """The apply guard must be released even on failure."""
        app, _save, _messagebox = self._apply(False)
        self.assertFalse(app._applying_settings)


if __name__ == "__main__":
    unittest.main()
