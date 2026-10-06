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
    app._warn_if_ws_port_changed = lambda prev: None
    app.refresh_profile_status = lambda: None
    app.after = lambda delay, callback: None
    app.print_log = app.logs.append
    return app


class TestApplyPendingSettingsSaveFailure(unittest.TestCase):
    """A failed config save must surface as an error, never as success."""

    def setUp(self):
        self.test_dir = tempfile.mkdtemp()
        self.config = dict(DEFAULT_CONFIG, output_dir=self.test_dir)
        self.draft = dict(self.config, silence_timeout=1.5, asr_decode_timeout_sec=60)

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
        self.assertEqual(app.config_data["asr_decode_timeout_sec"], 60)

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
        self.assertEqual(app.config_data["asr_decode_timeout_sec"], self.config["asr_decode_timeout_sec"])
        self.assertEqual(app.shared_config["asr_decode_timeout_sec"], self.config["asr_decode_timeout_sec"])

    def test_failed_save_clears_applying_flag(self):
        """The apply guard must be released even on failure."""
        app, _save, _messagebox = self._apply(False)
        self.assertFalse(app._applying_settings)


class TestSessionTimelineLifecycle(unittest.TestCase):
    def test_gui_session_epoch_is_created_once_and_survives_hot_swap(self):
        import queue

        app = types.SimpleNamespace()
        app.is_running = False
        app._ui_ready = False
        app.shared_config = {
            "ws_port": 8765,
            "output_dir": tempfile.gettempdir(),
            "asr_attempt": 1,
        }
        app._asr_attempt = 1
        app._writer_failure_handled_code = None
        app._decode_timeout_handled_attempt = None
        app.btn_power = MagicMock()
        app._log_lines = []
        app.consola = MagicMock()
        app.print_log = MagicMock()
        app.set_status = MagicMock()
        app.update_session_label = MagicMock()
        app.after = MagicMock()
        app._check_ws_health = MagicMock()
        app.refresh_profile_status = MagicMock()
        app.audio_queue = queue.Queue()
        app.log_queue = queue.Queue()
        app.p_audio = None
        app.p_ia = None
        app.p_ws = None
        app._signal_stop = MagicMock()
        app._stop_process = MagicMock()
        app._drain_queue = MagicMock()
        app._new_audio_queue = lambda _config: queue.Queue()
        app.hot_swap_engine = lambda: LiveASRApp.hot_swap_engine(app)

        with (
            patch("liveaudio.app.port_range_available", return_value=True),
            patch("liveaudio.app.get_language", return_value="en"),
            patch("liveaudio.app.mp.Queue", side_effect=lambda **_kwargs: queue.Queue()),
            patch("liveaudio.app.mp.Process", side_effect=lambda **_kwargs: MagicMock()),
            patch("liveaudio.app.time.monotonic", return_value=123.25),
        ):
            LiveASRApp.toggle_system(app)
            self.assertEqual(app.shared_config["session_started_monotonic"], 123.25)
            app.hot_swap_engine()

        self.assertEqual(app.shared_config["session_started_monotonic"], 123.25)

    def test_gui_new_session_after_apply_resets_epoch(self):
        config = dict(DEFAULT_CONFIG, output_dir=tempfile.gettempdir())
        draft = dict(config, continuous_session=False, max_chunk_duration=30.0)
        app = _make_app(config, draft)
        app.is_running = True
        app.shared_config["session_started_monotonic"] = 100.0
        app.btn_apply.configure = MagicMock()
        app._pending_restart_flags = lambda _draft: (False, True)
        app._validate_draft_config = lambda _draft: None
        app.current_session_dir = None
        app.update_session_label = MagicMock()
        app.print_log = MagicMock()
        app.set_status = MagicMock()
        app.hot_swap_engine = MagicMock(return_value=True)
        app.refresh_profile_status = MagicMock()

        with (
            patch("liveaudio.app.messagebox.askyesno", return_value=True),
            patch("liveaudio.app.time.monotonic", return_value=456.75),
            patch("liveaudio.app.save_config", return_value=True),
        ):
            LiveASRApp.apply_pending_settings(app)

        self.assertEqual(app.shared_config["session_started_monotonic"], 456.75)
        self.assertIn("session_", app.current_session_dir)
        app.hot_swap_engine.assert_called_once()


if __name__ == "__main__":
    unittest.main()
