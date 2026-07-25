# SPDX-License-Identifier: MIT
"""The shared-config Manager must be created on first use, not at startup.

mp.Manager() spawns a whole extra Python process, and under Windows 'spawn'
that child re-imports app.py — paying the customtkinter/PIL import cost a
second time on every launch, even when the user never presses Start.
Deferring creation to the first real access moves that cost to Start (where a
5-60s model load already dominates) and removes it entirely otherwise.

The GUI class needs a display, so — as in test_restart_flags.py and
test_asr_language.py — these call unbound methods against light stubs instead
of building a window.
"""

import types
import unittest
from unittest.mock import MagicMock, patch

from liveaudio.app import LiveASRApp


def _app_stub(config_data):
    """Stub carrying the deferred state that LiveASRApp.__init__ leaves behind."""
    return types.SimpleNamespace(
        _manager=None,
        _shared_config=None,
        config_data=config_data,
    )


def _access_shared_config(stub):
    """Read the lazy accessor off the class (it is unbound for a stub)."""
    return LiveASRApp.shared_config.fget(stub)


class TestSharedConfigIsDeferred(unittest.TestCase):
    """The Manager process is not paid for at construction time."""

    def test_init_never_creates_a_manager(self):
        """__init__ must not reference Manager: the cost moves to first use."""
        self.assertNotIn("Manager", LiveASRApp.__init__.__code__.co_names)

    def test_shared_config_is_a_lazy_accessor(self):
        """shared_config resolves through the class, not an eager instance attribute."""
        self.assertIsInstance(LiveASRApp.__dict__.get("shared_config"), property)

    def test_first_access_creates_the_manager_and_the_proxy(self):
        """The first read spawns the Manager and returns its dict proxy."""
        stub = _app_stub({"ws_port": 8765})
        with patch("multiprocessing.Manager") as fake_manager:
            proxy = _access_shared_config(stub)

        fake_manager.assert_called_once_with()
        self.assertIs(proxy, fake_manager.return_value.dict.return_value)
        self.assertIs(stub._manager, fake_manager.return_value)

    def test_seed_uses_the_config_present_at_first_access(self):
        """A key mutated after __init__ (e.g. change_folder) must reach the seed."""
        config = {"output_dir": "old"}
        stub = _app_stub(config)
        with patch("multiprocessing.Manager") as fake_manager:
            config["output_dir"] = "new"
            _access_shared_config(stub)

        fake_manager.return_value.dict.assert_called_once_with({"output_dir": "new"})

    def test_seed_follows_a_rebound_config_data(self):
        """apply_pending_settings rebinds config_data; the seed must follow the new object."""
        stub = _app_stub({"device": "cpu"})
        with patch("multiprocessing.Manager") as fake_manager:
            stub.config_data = {"device": "cuda"}
            _access_shared_config(stub)

        fake_manager.return_value.dict.assert_called_once_with({"device": "cuda"})

    def test_repeated_access_reuses_one_manager(self):
        """Every later read returns the same proxy — no second process, no reseed."""
        stub = _app_stub({"ws_port": 8765})
        with patch("multiprocessing.Manager") as fake_manager:
            first = _access_shared_config(stub)
            second = _access_shared_config(stub)

        self.assertIs(first, second)
        self.assertEqual(fake_manager.call_count, 1)
        self.assertEqual(fake_manager.return_value.dict.call_count, 1)

    def test_writes_reach_the_same_proxy(self):
        """Writing through the accessor mutates the proxy, not a fresh copy."""
        stub = _app_stub({"output_dir": "old"})
        with patch("multiprocessing.Manager"):
            _access_shared_config(stub)["output_dir"] = "new"
            proxy = _access_shared_config(stub)

        proxy.__setitem__.assert_called_once_with("output_dir", "new")


class TestShutdownWithoutManager(unittest.TestCase):
    """Teardown must be safe whether or not the Manager was ever created."""

    @staticmethod
    def _shutdown_stub(manager):
        return types.SimpleNamespace(
            _manager=manager,
            _shared_config=None,
            config_data={},
            is_running=True,
            _ws_health_after_id=None,
            audio_queue=MagicMock(),
            text_queue=MagicMock(),
            log_queue=MagicMock(),
            p_audio=None,
            p_ia=None,
            p_ws=None,
            _stop_process=MagicMock(),
            _drain_queue=MagicMock(),
            destroy=MagicMock(),
        )

    def test_shutdown_without_manager_spawns_nothing(self):
        """Closing an app that never started must not create a Manager just to kill it."""
        stub = self._shutdown_stub(None)
        with patch("multiprocessing.Manager", side_effect=AssertionError("Manager spawned during teardown")):
            LiveASRApp._shutdown(stub)

        self.assertIsNone(stub._manager)
        stub.destroy.assert_called_once_with()

    def test_shutdown_closes_a_created_manager(self):
        """When the Manager exists, teardown still shuts its process down."""
        manager = MagicMock()
        stub = self._shutdown_stub(manager)

        LiveASRApp._shutdown(stub)

        manager.shutdown.assert_called_once_with()
        stub.destroy.assert_called_once_with()


class TestWelcomeScreenDoesNotSpawnAManager(unittest.TestCase):
    """The welcome screen must stay free of the Manager cost.

    settings_navigation_mode is pure UI state: every read goes through
    config_data and no worker process ever consults it. Propagating it to
    shared_config would spawn the Manager from the first screen the user sees,
    which is exactly the startup cost the lazy accessor exists to avoid.
    """

    def test_changing_layout_mode_does_not_touch_shared_config(self):
        stub = types.SimpleNamespace(
            _manager=None,
            _shared_config=None,
            config_data={"settings_navigation_mode": "tabs"},
        )

        with patch("multiprocessing.Manager", side_effect=AssertionError("Manager spawned from welcome screen")), \
                patch("liveaudio.app.save_config"), \
                patch("liveaudio.app.t", side_effect=lambda key, **kw: key):
            LiveASRApp._on_welcome_nav_mode_change(stub, "dropdown_menu")

        self.assertEqual(stub.config_data["settings_navigation_mode"], "dropdown")
        self.assertIsNone(stub._manager)
        self.assertIsNone(stub._shared_config)


if __name__ == "__main__":
    unittest.main()
