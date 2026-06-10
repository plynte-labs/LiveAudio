# SPDX-License-Identifier: MIT
"""Tests for the in-app updater (liveaudio/utils/updater.py).

Covers version comparison, the verified-TLS-first / unverified-fallback fetch,
the 24h rate limit, launcher discovery and the detached --update hand-off.
No network and no real subprocesses are involved.
"""

import io
import json
import os
import ssl
import subprocess
import tempfile
import time
import unittest
import urllib.error
from pathlib import Path
from unittest.mock import MagicMock, patch

from liveaudio.utils import updater


class _FakeResponse:
    """Minimal context-manager stand-in for urlopen's response."""

    def __init__(self, payload, status=200):
        self.status = status
        self._body = json.dumps(payload).encode("utf-8")

    def read(self):
        return self._body

    def __enter__(self):
        return self

    def __exit__(self, *exc_info):
        return False


class TestVersionParsing(unittest.TestCase):
    def test_strips_v_prefix(self):
        self.assertEqual(updater.parse_version("v1.2.3"), (1, 2, 3))

    def test_garbage_becomes_zero(self):
        self.assertEqual(updater.parse_version("nightly"), (0, 0, 0))

    def test_newer_version_detected(self):
        self.assertTrue(updater.is_new_version_available("1.1.0", "v1.2.0"))

    def test_same_version_is_not_new(self):
        self.assertFalse(updater.is_new_version_available("1.1.0", "v1.1.0"))

    def test_older_version_is_not_new(self):
        self.assertFalse(updater.is_new_version_available("1.1.0", "v1.0.9"))


class TestFetchLatestReleaseSsl(unittest.TestCase):
    """Verified TLS first; unverified retry only on cert verification failure."""

    def test_verified_context_used_first(self):
        contexts = []

        def fake_urlopen(request, timeout=None, context=None):
            contexts.append(context)
            return _FakeResponse({"tag_name": "v9.9.9"})

        with patch.object(updater.urllib.request, "urlopen", side_effect=fake_urlopen):
            data = updater._fetch_latest_release()

        self.assertEqual(data["tag_name"], "v9.9.9")
        self.assertEqual(len(contexts), 1)
        self.assertTrue(contexts[0].check_hostname)
        self.assertEqual(contexts[0].verify_mode, ssl.CERT_REQUIRED)

    def test_cert_failure_falls_back_to_unverified(self):
        contexts = []
        cert_error = ssl.SSLCertVerificationError("self signed certificate")

        def fake_urlopen(request, timeout=None, context=None):
            contexts.append(context)
            if len(contexts) == 1:
                raise cert_error
            return _FakeResponse({"tag_name": "v9.9.9"})

        with patch.object(updater.urllib.request, "urlopen", side_effect=fake_urlopen):
            data = updater._fetch_latest_release()

        self.assertEqual(data["tag_name"], "v9.9.9")
        self.assertEqual(len(contexts), 2)
        # First attempt verified, second attempt unverified.
        self.assertEqual(contexts[0].verify_mode, ssl.CERT_REQUIRED)
        self.assertEqual(contexts[1].verify_mode, ssl.CERT_NONE)

    def test_urlerror_wrapping_cert_failure_also_falls_back(self):
        """urlopen wraps handshake errors as URLError(reason=SSLCertVerificationError)."""
        contexts = []
        wrapped = urllib.error.URLError(ssl.SSLCertVerificationError("bad cert"))

        def fake_urlopen(request, timeout=None, context=None):
            contexts.append(context)
            if len(contexts) == 1:
                raise wrapped
            return _FakeResponse({"tag_name": "v9.9.9"})

        with patch.object(updater.urllib.request, "urlopen", side_effect=fake_urlopen):
            data = updater._fetch_latest_release()

        self.assertEqual(data["tag_name"], "v9.9.9")
        self.assertEqual(len(contexts), 2)
        self.assertEqual(contexts[1].verify_mode, ssl.CERT_NONE)

    def test_non_ssl_error_propagates_without_retry(self):
        calls = []

        def fake_urlopen(request, timeout=None, context=None):
            calls.append(context)
            raise urllib.error.URLError(OSError("no internet"))

        with patch.object(updater.urllib.request, "urlopen", side_effect=fake_urlopen):
            with self.assertRaises(urllib.error.URLError):
                updater._fetch_latest_release()

        self.assertEqual(len(calls), 1)  # no unverified retry


class TestCheckForUpdates(unittest.TestCase):
    """Rate limiting + callback semantics of the synchronous check."""

    def test_rate_limited_within_24h(self):
        config = {"last_update_check": int(time.time()) - 60}
        callback = MagicMock()
        with patch.object(updater, "load_config", return_value=config):
            with patch.object(updater, "_fetch_latest_release") as fetch:
                updater.check_for_updates(callback)
        fetch.assert_not_called()
        callback.assert_not_called()

    def test_new_version_fires_true_and_persists_timestamp(self):
        config = {"last_update_check": 0}
        callback = MagicMock()
        saved = {}
        with patch.object(updater, "load_config", return_value=config):
            with patch.object(updater, "save_config", side_effect=saved.update):
                with patch.object(
                    updater, "_fetch_latest_release",
                    return_value={"tag_name": "v999.0.0"},
                ):
                    updater.check_for_updates(callback)
        callback.assert_called_once_with(True, "v999.0.0")
        self.assertGreater(saved.get("last_update_check", 0), 0)

    def test_current_version_fires_false(self):
        callback = MagicMock()
        with patch.object(updater, "load_config", return_value={"last_update_check": 0}):
            with patch.object(updater, "save_config"):
                with patch.object(
                    updater, "_fetch_latest_release",
                    return_value={"tag_name": "v%s" % updater.APP_VERSION},
                ):
                    updater.check_for_updates(callback)
        callback.assert_called_once_with(False, "v%s" % updater.APP_VERSION)

    def test_network_failure_is_silent(self):
        callback = MagicMock()
        with patch.object(updater, "load_config", return_value={"last_update_check": 0}):
            with patch.object(
                updater, "_fetch_latest_release",
                side_effect=urllib.error.URLError(OSError("offline")),
            ):
                with patch("sys.stdout", new_callable=io.StringIO):
                    updater.check_for_updates(callback)  # must not raise
        callback.assert_not_called()


class TestFindLauncher(unittest.TestCase):
    """Launcher discovery: env var > portable install layout walk-up > None."""

    def setUp(self):
        self._saved = os.environ.pop(updater.LAUNCHER_ENV_VAR, None)
        self.addCleanup(self._restore_env)

    def _restore_env(self):
        if self._saved is not None:
            os.environ[updater.LAUNCHER_ENV_VAR] = self._saved
        else:
            os.environ.pop(updater.LAUNCHER_ENV_VAR, None)

    def test_env_var_wins(self):
        os.environ[updater.LAUNCHER_ENV_VAR] = r"C:\fake\LiveAudio-Setup.exe"
        self.assertEqual(updater.find_launcher(), r"C:\fake\LiveAudio-Setup.exe")

    def test_portable_layout_walk_up(self):
        """<dir>/LiveAudio-Setup.exe next to <dir>/data (install root)."""
        with tempfile.TemporaryDirectory(prefix="liveaudio-upd-") as tmp:
            root = os.path.join(tmp, "data")
            scripts = os.path.join(root, "app", ".venv", "Scripts")
            os.makedirs(scripts)
            Path(root, "installed.json").write_text("{}", encoding="utf-8")
            fake_app_exe = os.path.join(scripts, "liveaudio.exe")
            Path(fake_app_exe).touch()
            launcher_exe = os.path.join(tmp, "LiveAudio-Setup.exe")
            Path(launcher_exe).touch()
            with patch.object(updater.sys, "executable", fake_app_exe):
                self.assertEqual(updater.find_launcher(), launcher_exe)

    def test_install_without_launcher_copy_returns_none(self):
        """Non-portable install: installed.json exists but no launcher above it."""
        with tempfile.TemporaryDirectory(prefix="liveaudio-upd-") as tmp:
            root = os.path.join(tmp, "LiveAudio")
            scripts = os.path.join(root, "app", ".venv", "Scripts")
            os.makedirs(scripts)
            Path(root, "installed.json").write_text("{}", encoding="utf-8")
            fake_app_exe = os.path.join(scripts, "liveaudio.exe")
            Path(fake_app_exe).touch()
            with patch.object(updater.sys, "executable", fake_app_exe):
                self.assertIsNone(updater.find_launcher())

    def test_dev_run_returns_none(self):
        with tempfile.TemporaryDirectory(prefix="liveaudio-upd-") as tmp:
            fake_exe = os.path.join(tmp, "a", "b", "python.exe")
            os.makedirs(os.path.dirname(fake_exe))
            Path(fake_exe).touch()
            with patch.object(updater.sys, "executable", fake_exe):
                self.assertIsNone(updater.find_launcher())


class TestStartUpdate(unittest.TestCase):
    """Detached spawn of `<launcher> --update <tag>`."""

    def test_no_launcher_returns_false(self):
        with patch.object(updater, "find_launcher", return_value=None):
            with patch.object(updater.subprocess, "Popen") as popen:
                self.assertFalse(updater.start_update("v1.2.0"))
        popen.assert_not_called()

    def test_spawns_launcher_detached(self):
        launcher = r"C:\fake\LiveAudio-Setup.exe"
        with patch.object(updater, "find_launcher", return_value=launcher):
            with patch.object(updater.subprocess, "Popen") as popen:
                self.assertTrue(updater.start_update("v1.2.0"))
        popen.assert_called_once()
        args, kwargs = popen.call_args
        self.assertEqual(args[0], [launcher, "--update", "v1.2.0"])
        self.assertTrue(kwargs["close_fds"])
        if updater.sys.platform == "win32":
            expected = (
                getattr(subprocess, "DETACHED_PROCESS", 0)
                | getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)
            )
            self.assertEqual(kwargs["creationflags"], expected)
        else:
            self.assertTrue(kwargs["start_new_session"])

    def test_spawn_failure_returns_false(self):
        with patch.object(updater, "find_launcher", return_value="launcher.exe"):
            with patch.object(updater.subprocess, "Popen", side_effect=OSError("denied")):
                with patch("sys.stdout", new_callable=io.StringIO):
                    self.assertFalse(updater.start_update("v1.2.0"))


class TestStartInAppUpdateOrdering(unittest.TestCase):
    """'Update now' must commit to shutdown BEFORE spawning the launcher.

    Spawning first would let the launcher swap app/ while the app keeps
    running if the user cancels the pending-changes close prompt.
    """

    def _app_mock(self, confirm):
        from liveaudio.app import LiveASRApp
        mock = MagicMock(spec=LiveASRApp)
        mock._confirm_close.return_value = confirm
        return LiveASRApp, mock

    def test_cancelled_close_does_not_spawn_launcher(self):
        LiveASRApp, mock = self._app_mock(confirm=False)
        with patch("liveaudio.app.start_update") as start:
            LiveASRApp.start_in_app_update(mock, "v9.9.9")
        start.assert_not_called()
        mock._shutdown.assert_not_called()

    def test_confirmed_close_spawns_launcher_then_shuts_down(self):
        LiveASRApp, mock = self._app_mock(confirm=True)
        with patch("liveaudio.app.start_update", return_value=True) as start:
            with patch("liveaudio.app.webbrowser") as browser:
                LiveASRApp.start_in_app_update(mock, "v9.9.9")
        start.assert_called_once_with("v9.9.9")
        browser.open_new.assert_not_called()
        mock._shutdown.assert_called_once()

    def test_launcher_unavailable_opens_releases_and_still_closes(self):
        LiveASRApp, mock = self._app_mock(confirm=True)
        with patch("liveaudio.app.start_update", return_value=False):
            with patch("liveaudio.app.webbrowser") as browser:
                LiveASRApp.start_in_app_update(mock, "v9.9.9")
        browser.open_new.assert_called_once()
        mock._shutdown.assert_called_once()


if __name__ == "__main__":
    unittest.main()
