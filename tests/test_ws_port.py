"""Tests for WebSocket port wiring (REQ-5)."""

import unittest
from unittest.mock import patch, MagicMock, AsyncMock
import asyncio

from core.network import run_ws_server


class TestWSPortWiring(unittest.TestCase):
    """Tests for WebSocket port reading from config."""

    @patch("core.network.serve")
    @patch("core.network._poll_queue")
    def test_default_port_is_8765(self, mock_poll, mock_serve):
        """Default port should be 8765 when not specified."""
        mock_queue = MagicMock()
        mock_log_queue = MagicMock()
        mock_server = MagicMock()
        mock_server.connections = set()

        async def fake_main():
            async with mock_serve.return_value as server:
                pass

        mock_serve.return_value.__aenter__ = AsyncMock(return_value=mock_server)
        mock_serve.return_value.__aexit__ = AsyncMock(return_value=False)
        mock_poll.side_effect = KeyboardInterrupt()

        with self.assertRaises(KeyboardInterrupt):
            run_ws_server(mock_queue, mock_log_queue)

        mock_serve.assert_called_once()
        call_args = mock_serve.call_args
        self.assertEqual(call_args[0][1], "127.0.0.1")
        self.assertEqual(call_args[0][2], 8765)

    @patch("core.network.serve")
    @patch("core.network._poll_queue")
    def test_custom_port_from_config(self, mock_poll, mock_serve):
        """Custom port should be used when passed."""
        mock_queue = MagicMock()
        mock_log_queue = MagicMock()
        mock_server = MagicMock()
        mock_server.connections = set()

        mock_serve.return_value.__aenter__ = AsyncMock(return_value=mock_server)
        mock_serve.return_value.__aexit__ = AsyncMock(return_value=False)
        mock_poll.side_effect = KeyboardInterrupt()

        with self.assertRaises(KeyboardInterrupt):
            run_ws_server(mock_queue, mock_log_queue, port=9999)

        mock_serve.assert_called_once()
        call_args = mock_serve.call_args
        self.assertEqual(call_args[0][2], 9999)

    @patch("core.network.serve")
    @patch("core.network._poll_queue")
    def test_port_in_log_message(self, mock_poll, mock_serve):
        """Log message should include the correct port."""
        mock_queue = MagicMock()
        mock_log_queue = MagicMock()
        mock_server = MagicMock()
        mock_server.connections = set()

        mock_serve.return_value.__aenter__ = AsyncMock(return_value=mock_server)
        mock_serve.return_value.__aexit__ = AsyncMock(return_value=False)
        mock_poll.side_effect = KeyboardInterrupt()

        with self.assertRaises(KeyboardInterrupt):
            run_ws_server(mock_queue, mock_log_queue, port=7777)

        found_port_log = False
        for call in mock_log_queue.put_nowait.call_args_list:
            args = call[0][0]
            if isinstance(args, dict) and args.get("type") == "log":
                if "7777" in args.get("message", ""):
                    found_port_log = True
                    break
        self.assertTrue(found_port_log, "Log message should contain port 7777")

    @patch("core.network.serve")
    @patch("core.network._poll_queue")
    def test_port_in_status_message(self, mock_poll, mock_serve):
        """Status message should include the correct port."""
        mock_queue = MagicMock()
        mock_log_queue = MagicMock()
        mock_server = MagicMock()
        mock_server.connections = set()

        mock_serve.return_value.__aenter__ = AsyncMock(return_value=mock_server)
        mock_serve.return_value.__aexit__ = AsyncMock(return_value=False)
        mock_poll.side_effect = KeyboardInterrupt()

        with self.assertRaises(KeyboardInterrupt):
            run_ws_server(mock_queue, mock_log_queue, port=5555)

        found_port_status = False
        for call in mock_log_queue.put_nowait.call_args_list:
            args = call[0][0]
            if isinstance(args, dict) and args.get("type") == "status":
                if "5555" in args.get("text", ""):
                    found_port_status = True
                    break
        self.assertTrue(found_port_status, "Status message should contain port 5555")


if __name__ == "__main__":
    unittest.main()
