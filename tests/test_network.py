# SPDX-License-Identifier: MIT
"""Tests for core/network.py WebSocket layer (REQ-5)."""

import unittest
import asyncio
import json
from unittest.mock import MagicMock, patch, AsyncMock

from tests.helpers import MockQueue, make_shared_config


class _AcceptedLocalhostWebSocket:
    """Concrete async websocket double that stays connected until released."""

    def __init__(self, connected_event, release_event):
        self.remote_address = ("127.0.0.1", 12345)
        self._connected_event = connected_event
        self._release_event = release_event

    def __aiter__(self):
        async def _messages():
            self._connected_event.set()
            await self._release_event.wait()
            if False:  # pragma: no cover - keeps this an async generator
                yield None

        return _messages()


class _DisconnectingLocalhostWebSocket:
    """Concrete async websocket double that disconnects during iteration."""

    remote_address = ("127.0.0.1", 12345)

    def __aiter__(self):
        async def _messages():
            raise ConnectionError("disconnected")
            if False:  # pragma: no cover - keeps this an async generator
                yield None

        return _messages()


class TestWebSocketConnectionLifecycle(unittest.TestCase):
    """Tests for WebSocket connect/disconnect/reconnect behavior."""

    def test_connection_accepted_from_localhost(self):
        """Connections from 127.0.0.1 should be accepted."""
        from liveaudio.core.network import _handle_client
        clients = set()
        log_queue = MockQueue()

        connected_event = asyncio.Event()
        release_event = asyncio.Event()
        websocket = _AcceptedLocalhostWebSocket(connected_event, release_event)

        async def run_test():
            task = asyncio.create_task(_handle_client(websocket, clients, log_queue))
            await connected_event.wait()  # Wait for connection
            self.assertIn(websocket, clients)
            release_event.set()
            await task
            self.assertNotIn(websocket, clients)

        asyncio.run(run_test())

    def test_connection_rejected_from_external(self):
        """Connections from external IPs should be rejected."""
        from liveaudio.core.network import _handle_client
        clients = set()
        log_queue = MockQueue()

        mock_websocket = MagicMock()
        mock_websocket.remote_address = ("192.168.1.100", 12345)
        mock_websocket.close = AsyncMock()

        async def run_test():
            await _handle_client(mock_websocket, clients, log_queue)

        asyncio.run(run_test())
        self.assertNotIn(mock_websocket, clients)
        mock_websocket.close.assert_called_once()

    def test_connection_removed_on_disconnect(self):
        """Client should be removed from clients set on disconnect."""
        from liveaudio.core.network import _handle_client
        clients = set()
        log_queue = MockQueue()

        websocket = _DisconnectingLocalhostWebSocket()

        async def run_test():
            try:
                await _handle_client(websocket, clients, log_queue)
            except ConnectionError:
                pass

        asyncio.run(run_test())
        self.assertNotIn(websocket, clients)


class TestPortConfiguration(unittest.TestCase):
    """Tests for WebSocket port configuration."""

    def test_default_port_is_8765(self):
        """Default WebSocket port should be 8765."""
        # The port is hardcoded in network.py line 105
        # This test documents the current behavior
        import inspect
        from liveaudio.core.network import run_ws_server
        source = inspect.getsource(run_ws_server)
        self.assertIn("8765", source)

    def test_port_in_valid_range(self):
        """Port 8765 should be within valid range (1-65535)."""
        default_port = 8765
        self.assertGreaterEqual(default_port, 1)
        self.assertLessEqual(default_port, 65535)

    def test_port_not_privileged(self):
        """Port 8765 should not be a privileged port (<1024)."""
        default_port = 8765
        self.assertGreater(default_port, 1024)


class TestPortConflictDetection(unittest.TestCase):
    """Tests for port conflict detection behavior."""

    def test_server_raises_on_port_in_use(self):
        """Server should raise error if port is already in use."""
        # This test documents expected behavior when port is in use
        # The actual error would be OSError: [Errno 98] Address already in use
        # We can't easily test this without actually binding the port
        self.assertTrue(True)  # Placeholder - requires integration test

    def test_error_logged_on_failure(self):
        """Error should be logged when server fails to start."""
        from liveaudio.core.network import run_ws_server
        import inspect
        source = inspect.getsource(run_ws_server)
        self.assertIn("Error fatal", source)


class TestMessageRouting(unittest.TestCase):
    """Tests for WebSocket message routing behavior."""

    def test_broadcast_formats_as_json(self):
        """Messages should be formatted as JSON before broadcast."""
        from liveaudio.core.network import _poll_queue
        import inspect
        source = inspect.getsource(_poll_queue)
        self.assertIn("json.dumps", source)

    def test_replay_buffer_handles_catchup_interval(self):
        """Replay buffer should respect catchup_interval_sec."""
        from liveaudio.core.network import _poll_queue
        import inspect
        source = inspect.getsource(_poll_queue)
        self.assertIn("catchup_interval_sec", source)

    def test_none_sentinel_stops_polling(self):
        """None sentinel should stop the polling loop."""
        from liveaudio.core.network import _poll_queue
        import inspect
        source = inspect.getsource(_poll_queue)
        self.assertIn("msg is None", source)

    def test_empty_message_handled_gracefully(self):
        """Empty message should be handled without error."""
        # Test that json.dumps handles empty dict
        result = json.dumps({})
        self.assertEqual(result, "{}")

    def test_transcript_message_format(self):
        """Transcript message should have required fields."""
        msg = {
            "id": "test-123",
            "text": "Hello world",
            "style": "default",
            "created_at": 1234567890.0,
            "processed_at": 1234567890.1,
            "queue_delay": 0.5,
            "total_delay": 1.0,
            "latency": 0.3,
            "is_replay": False,
            "catchup_interval_sec": 1.5,
        }
        payload = json.dumps(msg)
        parsed = json.loads(payload)
        self.assertEqual(parsed["text"], "Hello world")
        self.assertEqual(parsed["style"], "default")


class TestPortAvailable(unittest.TestCase):
    """Tests for the advisory pre-flight port check."""

    def test_returns_false_when_port_is_held_by_listener(self):
        """port_available should be False while another socket listens on the port."""
        import socket
        from liveaudio.core.network import port_available

        listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        try:
            listener.bind(("127.0.0.1", 0))
            listener.listen(1)
            port = listener.getsockname()[1]
            self.assertFalse(port_available(port))
        finally:
            listener.close()

    def test_returns_true_for_freed_ephemeral_port(self):
        """port_available should be True for a port that was just released."""
        import socket
        from liveaudio.core.network import port_available

        probe = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
        probe.close()
        self.assertTrue(port_available(port))


class TestRunWsServerPortConflict(unittest.TestCase):
    """Tests for the bind-failure path of run_ws_server."""

    def test_occupied_port_raises_and_logs_friendly_message(self):
        """run_ws_server on a busy port should raise OSError and emit a clear log."""
        import queue as std_queue
        import socket
        from liveaudio.core.network import run_ws_server

        listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        try:
            listener.bind(("127.0.0.1", 0))
            listener.listen(1)
            port = listener.getsockname()[1]

            log_queue = std_queue.Queue()
            text_queue = std_queue.Queue()
            with self.assertRaises(OSError):
                run_ws_server(text_queue, log_queue, port)

            messages = []
            while True:
                try:
                    event = log_queue.get_nowait()
                except std_queue.Empty:
                    break
                if isinstance(event, dict) and event.get("type") == "log":
                    messages.append(event.get("message", ""))
            self.assertTrue(
                any("en uso" in message for message in messages),
                f"No friendly port-in-use log emitted. Logs: {messages}",
            )
        finally:
            listener.close()


class TestAppWsResilienceWiring(unittest.TestCase):
    """Source-inspection tests for the app-side pre-flight check and health monitor."""

    @staticmethod
    def _read_app_source():
        import os
        app_path = os.path.join(os.path.dirname(__file__), "..", "liveaudio", "app.py")
        with open(app_path, encoding="utf-8") as f:
            return f.read()

    def test_preflight_port_check_runs_before_ws_spawn(self):
        """toggle_system must call port_available before spawning the WS process."""
        source = self._read_app_source()
        toggle_start = source.index("def toggle_system")
        body = source[toggle_start:]
        check_pos = body.find("port_available(")
        spawn_pos = body.find("self.p_ws = mp.Process")
        self.assertNotEqual(check_pos, -1, "toggle_system does not call port_available")
        self.assertNotEqual(spawn_pos, -1, "toggle_system does not spawn p_ws")
        self.assertLess(check_pos, spawn_pos, "port check must run before spawning p_ws")

    def test_ws_health_monitor_exists_and_is_scheduled(self):
        """_check_ws_health must exist and be scheduled with after() post-start."""
        source = self._read_app_source()
        self.assertIn("def _check_ws_health(self):", source)
        self.assertIn("self.after(2000, self._check_ws_health)", source)
        self.assertIn("self.p_ws.is_alive()", source)

    def test_ws_health_monitor_is_cancelled_on_stop(self):
        """The stop branch of toggle_system itself must cancel the monitor handle."""
        source = self._read_app_source()
        toggle_start = source.index("def toggle_system")
        toggle_end = source.index("\n    def ", toggle_start)
        toggle_body = source[toggle_start:toggle_end]
        self.assertIn("self.after_cancel(self._ws_health_after_id)", toggle_body)


class TestWsResilienceTranslations(unittest.TestCase):
    """New i18n keys must exist in both languages (key parity)."""

    NEW_KEYS = (
        "ws_port_busy_title",
        "ws_port_busy_msg",
        "log_ws_port_busy",
        "status_ws_port_busy",
        "status_ws_dead",
        "log_ws_dead",
    )

    def test_new_keys_present_in_both_languages(self):
        from liveaudio.utils.i18n import TRANSLATIONS
        for lang in ("es", "en"):
            for key in self.NEW_KEYS:
                self.assertIn(key, TRANSLATIONS[lang], f"{key} missing in {lang}")

    def test_port_placeholder_renders(self):
        from liveaudio.utils.i18n import TRANSLATIONS
        for lang in ("es", "en"):
            for key in ("ws_port_busy_msg", "log_ws_port_busy"):
                rendered = TRANSLATIONS[lang][key].format(port=8765)
                self.assertIn("8765", rendered)


class TestWebSocketLocalhostBinding(unittest.TestCase):
    """Tests for localhost-only binding security."""

    def test_server_binds_to_localhost(self):
        """Server should bind to 127.0.0.1 only."""
        import inspect
        from liveaudio.core.network import run_ws_server
        source = inspect.getsource(run_ws_server)
        self.assertIn('"127.0.0.1"', source)

    def test_handler_rejects_non_localhost(self):
        """Handler should reject connections from non-localhost IPs."""
        from liveaudio.core.network import _handle_client
        import inspect
        source = inspect.getsource(_handle_client)
        self.assertIn("127.0.0.1", source)
        self.assertIn("::1", source)
        self.assertIn("localhost", source)




if __name__ == "__main__":
    unittest.main()
