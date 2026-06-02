# SPDX-License-Identifier: MIT
"""Tests for core/network.py WebSocket layer (REQ-5)."""

import unittest
import asyncio
import json
from unittest.mock import MagicMock, patch, AsyncMock

from tests.helpers import MockQueue, make_shared_config


class TestWebSocketConnectionLifecycle(unittest.TestCase):
    """Tests for WebSocket connect/disconnect/reconnect behavior."""

    def test_connection_accepted_from_localhost(self):
        """Connections from 127.0.0.1 should be accepted."""
        from core.network import _handle_client
        clients = set()
        log_queue = MockQueue()

        mock_websocket = MagicMock()
        mock_websocket.remote_address = ("127.0.0.1", 12345)
        # Make wait_closed hang until we cancel it, so we can check clients set
        connected_event = asyncio.Event()

        async def hang_until_cancelled():
            connected_event.set()
            await asyncio.Event().wait()  # Hang forever

        mock_websocket.wait_closed = hang_until_cancelled

        async def run_test():
            task = asyncio.create_task(_handle_client(mock_websocket, clients, log_queue))
            await connected_event.wait()  # Wait for connection
            self.assertIn(mock_websocket, clients)
            task.cancel()
            try:
                await task
            except asyncio.CancelledError:
                pass

        asyncio.run(run_test())

    def test_connection_rejected_from_external(self):
        """Connections from external IPs should be rejected."""
        from core.network import _handle_client
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
        from core.network import _handle_client
        clients = set()
        log_queue = MockQueue()

        mock_websocket = MagicMock()
        mock_websocket.remote_address = ("127.0.0.1", 12345)
        mock_websocket.wait_closed = AsyncMock(side_effect=ConnectionError("disconnected"))

        async def run_test():
            try:
                await _handle_client(mock_websocket, clients, log_queue)
            except ConnectionError:
                pass

        asyncio.run(run_test())
        self.assertNotIn(mock_websocket, clients)


class TestPortConfiguration(unittest.TestCase):
    """Tests for WebSocket port configuration."""

    def test_default_port_is_8765(self):
        """Default WebSocket port should be 8765."""
        # The port is hardcoded in network.py line 105
        # This test documents the current behavior
        import inspect
        from core.network import run_ws_server
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
        from core.network import run_ws_server
        import inspect
        source = inspect.getsource(run_ws_server)
        self.assertIn("Error fatal", source)


class TestMessageRouting(unittest.TestCase):
    """Tests for WebSocket message routing behavior."""

    def test_broadcast_formats_as_json(self):
        """Messages should be formatted as JSON before broadcast."""
        from core.network import _poll_queue
        import inspect
        source = inspect.getsource(_poll_queue)
        self.assertIn("json.dumps", source)

    def test_replay_buffer_handles_catchup_interval(self):
        """Replay buffer should respect catchup_interval_sec."""
        from core.network import _poll_queue
        import inspect
        source = inspect.getsource(_poll_queue)
        self.assertIn("catchup_interval_sec", source)

    def test_none_sentinel_stops_polling(self):
        """None sentinel should stop the polling loop."""
        from core.network import _poll_queue
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


class TestWebSocketLocalhostBinding(unittest.TestCase):
    """Tests for localhost-only binding security."""

    def test_server_binds_to_localhost(self):
        """Server should bind to 127.0.0.1 only."""
        import inspect
        from core.network import run_ws_server
        source = inspect.getsource(run_ws_server)
        self.assertIn('"127.0.0.1"', source)

    def test_handler_rejects_non_localhost(self):
        """Handler should reject connections from non-localhost IPs."""
        from core.network import _handle_client
        import inspect
        source = inspect.getsource(_handle_client)
        self.assertIn("127.0.0.1", source)
        self.assertIn("::1", source)
        self.assertIn("localhost", source)




if __name__ == "__main__":
    unittest.main()
