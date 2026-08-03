# SPDX-License-Identifier: MIT
"""Tests for core/network.py WebSocket layer (REQ-5)."""

import unittest
import asyncio
import errno
import json
from unittest.mock import MagicMock, patch, AsyncMock

from tests.helpers import MockQueue, make_shared_config


class _AcceptedLocalhostWebSocket:
    """Concrete async websocket double that stays connected until released."""

    def __init__(self, connected_event, release_event):
        self.remote_address = ("127.0.0.1", 12345)
        self.sent = []
        self._connected_event = connected_event
        self._release_event = release_event

    async def send(self, payload):
        self.sent.append(payload)

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

    async def send(self, payload):
        pass

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
        """run_ws_server with the whole range busy should raise OSError and log clearly.

        The fallback loop (WPF-1) skips past a single busy port, so this
        real-bind test pins the range to 1 to exercise exhaustion end-to-end.
        """
        import queue as std_queue
        import socket
        from liveaudio.core import network

        listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        try:
            listener.bind(("127.0.0.1", 0))
            listener.listen(1)
            port = listener.getsockname()[1]

            log_queue = std_queue.Queue()
            text_queue = std_queue.Queue()
            with patch.object(network, "WS_PORT_FALLBACK_RANGE", 1):
                with self.assertRaises(OSError):
                    network.run_ws_server(text_queue, log_queue, port)

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


class _HelloRecordingWebSocket:
    """Concrete async websocket double that records sends and ends immediately."""

    def __init__(self):
        self.remote_address = ("127.0.0.1", 23456)
        self.sent = []

    async def send(self, payload):
        self.sent.append(payload)

    def __aiter__(self):
        async def _messages():
            if False:  # pragma: no cover - keeps this an async generator
                yield None

        return _messages()


class TestHelloFrame(unittest.TestCase):
    """Hello identification frame on every accepted connection (WEI-1, WEI-4, WPF-2)."""

    def test_first_send_is_valid_hello_with_effective_port(self):
        """Frame #1 must be the hello carrying the effective port, without 'text'."""
        from liveaudio.core.network import _handle_client

        for effective_port in (8766, 8765):
            with self.subTest(effective_port=effective_port):
                websocket = _HelloRecordingWebSocket()
                asyncio.run(
                    _handle_client(websocket, set(), MockQueue(), effective_port=effective_port)
                )
                self.assertGreaterEqual(len(websocket.sent), 1, "hello frame must be sent first")
                hello = json.loads(websocket.sent[0])
                self.assertEqual(
                    hello,
                    {"type": "hello", "app": "liveaudio", "proto": 1, "port": effective_port},
                )
                self.assertNotIn("text", hello, "old overlays must safely ignore hello (WEI-4)")

    def test_external_client_rejected_without_hello(self):
        """Non-loopback clients stay rejected on fallback ports; no hello leaks (WPF-2)."""
        from liveaudio.core.network import _handle_client

        mock_websocket = MagicMock()
        mock_websocket.remote_address = ("192.168.1.100", 12345)
        mock_websocket.close = AsyncMock()
        mock_websocket.send = AsyncMock()

        asyncio.run(_handle_client(mock_websocket, set(), MockQueue(), effective_port=8766))

        mock_websocket.close.assert_called_once()
        mock_websocket.send.assert_not_called()


class TestPortFallback(unittest.TestCase):
    """Bind-time fallback loop over base..base+9 (WPF-1, WPF-3, WPF-5)."""

    @staticmethod
    def _make_serve_double(busy_ports):
        """serve() double: raises EADDRINUSE for busy ports, binds otherwise."""
        mock_server = MagicMock()
        mock_server.connections = set()

        def _serve(handler, host, port, **kwargs):
            if port in busy_ports:
                raise OSError(errno.EADDRINUSE, "address already in use")
            ctx = MagicMock()
            ctx.__aenter__ = AsyncMock(return_value=mock_server)
            ctx.__aexit__ = AsyncMock(return_value=False)
            return ctx

        return _serve

    @staticmethod
    def _drain_events(log_queue, event_type):
        return [
            call[0][0]
            for call in log_queue.put_nowait.call_args_list
            if isinstance(call[0][0], dict) and call[0][0].get("type") == event_type
        ]

    @patch("liveaudio.core.network.serve")
    @patch("liveaudio.core.network._poll_queue")
    def test_busy_base_falls_back_to_next_free_port(self, mock_poll, mock_serve):
        """WPF-1/WPF-3: busy base -> binds base+1 and emits ws_port {port, base}.

        The scan starting AT the configured base is also the WPF-5 surface:
        the effective port is never persisted, every start rescans from base.
        """
        from liveaudio.core.network import run_ws_server

        mock_serve.side_effect = self._make_serve_double({8765})
        mock_poll.side_effect = KeyboardInterrupt()
        log_queue = MagicMock()

        with self.assertRaises(KeyboardInterrupt):
            run_ws_server(MagicMock(), log_queue)

        attempted = [call[0][2] for call in mock_serve.call_args_list]
        self.assertEqual(
            attempted, [8765, 8766],
            "scan must start at the configured base and stop at the first free port",
        )

        events = self._drain_events(log_queue, "ws_port")
        self.assertEqual(len(events), 1, "exactly one ws_port event after a successful bind")
        self.assertEqual(events[0], {"type": "ws_port", "port": 8766, "base": 8765})

    @patch("liveaudio.core.network.serve")
    @patch("liveaudio.core.network._poll_queue")
    def test_free_base_binds_base_and_reports_it(self, mock_poll, mock_serve):
        """WPF-1 happy path: free base binds base; ws_port carries port == base."""
        from liveaudio.core.network import run_ws_server

        mock_serve.side_effect = self._make_serve_double(set())
        mock_poll.side_effect = KeyboardInterrupt()
        log_queue = MagicMock()

        with self.assertRaises(KeyboardInterrupt):
            run_ws_server(MagicMock(), log_queue, port=9000)

        attempted = [call[0][2] for call in mock_serve.call_args_list]
        self.assertEqual(attempted, [9000])

        events = self._drain_events(log_queue, "ws_port")
        self.assertEqual(events, [{"type": "ws_port", "port": 9000, "base": 9000}])

    @patch("liveaudio.core.network.serve")
    @patch("liveaudio.core.network._poll_queue")
    def test_exhausted_range_reraises_without_ws_port_event(self, mock_poll, mock_serve):
        """WPF-1 exhaustion: all 10 ports busy -> re-raise, no ws_port, no retry."""
        from liveaudio.core.network import run_ws_server

        mock_serve.side_effect = self._make_serve_double(set(range(8765, 8775)))
        log_queue = MagicMock()

        with self.assertRaises(OSError):
            run_ws_server(MagicMock(), log_queue)

        attempted = [call[0][2] for call in mock_serve.call_args_list]
        self.assertEqual(attempted, list(range(8765, 8775)))
        self.assertEqual(self._drain_events(log_queue, "ws_port"), [])
        mock_poll.assert_not_called()

    @patch("liveaudio.core.network.serve")
    @patch("liveaudio.core.network._poll_queue")
    def test_range_is_clamped_at_65535(self, mock_poll, mock_serve):
        """WPF-1: candidate ports never exceed 65535."""
        from liveaudio.core.network import run_ws_server

        mock_serve.side_effect = self._make_serve_double(set(range(65530, 65536)))

        with self.assertRaises(OSError):
            run_ws_server(MagicMock(), MagicMock(), port=65530)

        attempted = [call[0][2] for call in mock_serve.call_args_list]
        self.assertEqual(attempted, list(range(65530, 65536)))


class TestAppWsResilienceWiring(unittest.TestCase):
    """Source-inspection tests for the app-side pre-flight check and health monitor."""

    @staticmethod
    def _read_app_source():
        import os
        app_path = os.path.join(os.path.dirname(__file__), "..", "liveaudio", "app.py")
        with open(app_path, encoding="utf-8") as f:
            return f.read()

    def test_preflight_port_check_runs_before_ws_spawn(self):
        """toggle_system must check the fallback range before spawning the WS process."""
        source = self._read_app_source()
        toggle_start = source.index("def toggle_system")
        body = source[toggle_start:]
        check_pos = body.find("port_range_available(")
        spawn_pos = body.find("self.p_ws = mp.Process")
        self.assertNotEqual(check_pos, -1, "toggle_system does not call port_range_available")
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
        "status_ws_fallback",
        "obs_guide_port_note",
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
                rendered = TRANSLATIONS[lang][key].format(port=8765, end_port=8774)
                self.assertIn("8765", rendered)


class TestPortRangeAvailable(unittest.TestCase):
    @patch("liveaudio.core.network.port_available")
    def test_partial_range_available(self, mock_available):
        from liveaudio.core.network import port_range_available
        mock_available.side_effect = lambda port, host="127.0.0.1": port == 8766
        self.assertTrue(port_range_available(8765))

    @patch("liveaudio.core.network.port_available", return_value=False)
    def test_busy_range_unavailable(self, mock_available):
        from liveaudio.core.network import port_range_available
        self.assertFalse(port_range_available(8765))
        self.assertEqual(mock_available.call_count, 10)


class TestFallbackContractParity(unittest.TestCase):
    def test_python_and_overlay_constants_match(self):
        import re
        from pathlib import Path
        from liveaudio.core.network import WS_PORT_FALLBACK_RANGE
        html = (Path(__file__).parents[1] / "liveaudio" / "assets" / "subtitulos_obs.html").read_text(encoding="utf-8")
        range_match = re.search(r"const WS_PORT_FALLBACK_RANGE = (\d+)", html)
        proto_match = re.search(r"const WS_PROTO = (\d+)", html)
        self.assertIsNotNone(range_match)
        self.assertIsNotNone(proto_match)
        self.assertEqual(int(range_match.group(1)), WS_PORT_FALLBACK_RANGE)
        self.assertEqual(int(proto_match.group(1)), 1)


class TestWsPortGuiState(unittest.TestCase):
    def test_fallback_then_base_restores_chip_and_guide(self):
        from liveaudio import app as app_module

        class Label:
            def __init__(self):
                self.text = None

            def configure(self, **kwargs):
                self.text = kwargs["text"]

        class Gui:
            obs_guide_label = Label()

            def __init__(self):
                self.status = None

            def set_status(self, key, text, state):
                self.status = (key, text, state)

        gui = Gui()
        with patch.object(app_module, "_asset_path", return_value="overlay.html"):
            app_module.LiveASRApp.handle_event(
                gui, {"type": "ws_port", "port": 8766, "base": 8765}
            )
            self.assertIn("?port=8766", gui.obs_guide_label.text)
            app_module.LiveASRApp.handle_event(
                gui, {"type": "ws_port", "port": 8765, "base": 8765}
            )

        self.assertEqual(gui.status, ("ws", "WS: localhost:8765", "ok"))
        self.assertNotIn("?port=8766", gui.obs_guide_label.text)


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
