# SPDX-License-Identifier: MIT
"""Tests for the WebSocket Origin allowlist (WSO).

Browsers do not apply the same-origin policy to WebSocket handshakes: any page
can open ws://127.0.0.1:8765 and read the live transcript. Rejecting foreign
origins is the server's job, and it must be LOUD so a future OBS change is
diagnosable instead of silently killing the overlay.
"""

import asyncio
import json
import unittest
from unittest.mock import AsyncMock, MagicMock, patch

from websockets.asyncio.client import connect
from websockets.asyncio.server import serve
from websockets.datastructures import Headers
from websockets.exceptions import InvalidStatus
from websockets.http11 import Request

from liveaudio.core import network
from tests.helpers import MockQueue

WEBSITE_ORIGIN = "https://microsoft.com"
OBS_ORIGIN = "http://absolute"


def _events(log_queue, event_type):
    return [item for item in log_queue.items if isinstance(item, dict) and item.get("type") == event_type]


def _log_messages(log_queue):
    return [event.get("message", "") for event in _events(log_queue, "log")]


class _OriginServerCase(unittest.TestCase):
    """Spins up a real websockets server wired exactly like run_ws_server."""

    def run_handshake(self, origin=None):
        """Connect once with the given Origin. Returns (result, log_queue, prints).

        result is the parsed hello frame when the handshake succeeds, or the
        InvalidStatus exception when the server rejected it.
        """
        log_queue = MockQueue()

        async def handler(websocket):
            await websocket.send(json.dumps({"type": "hello", "app": "liveaudio"}))
            async for _ in websocket:
                pass

        def check_origin(connection, request):
            return network._reject_foreign_origin(connection, request, log_queue)

        async def scenario():
            async with serve(handler, "127.0.0.1", 0, process_request=check_origin) as server:
                port = server.sockets[0].getsockname()[1]
                kwargs = {"origin": origin} if origin is not None else {}
                try:
                    async with connect(f"ws://127.0.0.1:{port}", **kwargs) as client:
                        return json.loads(await client.recv())
                except InvalidStatus as rejected:
                    return rejected

        with patch("builtins.print") as mock_print:
            result = asyncio.run(scenario())
        prints = [str(call[0][0]) for call in mock_print.call_args_list if call[0]]
        return result, log_queue, prints


class TestOriginAllowlist(_OriginServerCase):
    """WSO-1: allow only a missing Origin header and the OBS browser source."""

    def test_client_without_origin_header_is_accepted(self):
        """Non-browser clients (scripts, native tools, the test suite) send no Origin."""
        result, log_queue, _ = self.run_handshake(origin=None)
        self.assertIsInstance(result, dict, "a client with no Origin must complete the handshake")
        self.assertEqual(result["type"], "hello")
        self.assertEqual(_log_messages(log_queue), [], "an allowed handshake must not log a rejection")

    def test_obs_browser_source_origin_is_accepted(self):
        """OBS CEF sends Origin: http://absolute for local browser sources."""
        result, log_queue, _ = self.run_handshake(origin=OBS_ORIGIN)
        self.assertIsInstance(result, dict, "the OBS browser source must keep working")
        self.assertEqual(result["type"], "hello")
        self.assertEqual(_log_messages(log_queue), [])

    def test_arbitrary_website_origin_is_rejected(self):
        """Any page open in the streamer's browser must not reach the transcript."""
        result, _, _ = self.run_handshake(origin=WEBSITE_ORIGIN)
        self.assertIsInstance(result, InvalidStatus, "a foreign Origin must abort the handshake")
        self.assertEqual(result.response.status_code, 403)

    def test_allowlist_constant_holds_the_obs_origin(self):
        self.assertIn(OBS_ORIGIN, network.WS_ALLOWED_ORIGINS)
        self.assertNotIn(WEBSITE_ORIGIN, network.WS_ALLOWED_ORIGINS)


class TestOriginRejectionIsLoud(_OriginServerCase):
    """WSO-2: a rejection must be visible in the console, the log and the GUI chip."""

    def test_rejection_log_line_contains_the_offending_origin(self):
        """The literal Origin value is what makes a future OBS change diagnosable."""
        _, log_queue, _ = self.run_handshake(origin=WEBSITE_ORIGIN)
        messages = _log_messages(log_queue)
        self.assertTrue(messages, "a rejection must reach the log queue")
        self.assertTrue(
            any(WEBSITE_ORIGIN in message for message in messages),
            f"the rejected Origin must appear verbatim in the log, got {messages}",
        )

    def test_rejection_prints_to_console(self):
        _, _, prints = self.run_handshake(origin=WEBSITE_ORIGIN)
        self.assertTrue(
            any(WEBSITE_ORIGIN in line for line in prints),
            f"the rejection must be printed to the console, got {prints}",
        )

    def test_rejection_emits_obs_status_event(self):
        _, log_queue, _ = self.run_handshake(origin=WEBSITE_ORIGIN)
        statuses = [event for event in _events(log_queue, "status") if event.get("key") == "obs"]
        self.assertEqual(len(statuses), 1, f"exactly one obs status event, got {statuses}")
        self.assertEqual(statuses[0]["state"], "warn")

    def test_status_text_matches_the_es_translation_the_gui_looks_up(self):
        """app.handle_event reverse-maps the raw text against TRANSLATIONS['es']."""
        from liveaudio.utils.i18n import TRANSLATIONS

        _, log_queue, _ = self.run_handshake(origin=WEBSITE_ORIGIN)
        statuses = [event for event in _events(log_queue, "status") if event.get("key") == "obs"]
        self.assertEqual(statuses[0]["text"], TRANSLATIONS["es"]["status_obs_origin_rejected"])


class TestOriginGuardUnit(unittest.TestCase):
    """Direct checks on the hook, including cases a real client cannot send."""

    @staticmethod
    def _call(headers, log_queue=None):
        connection = MagicMock()
        connection.respond.return_value = "REJECTED"
        request = Request("/", Headers(headers))
        result = network._reject_foreign_origin(connection, request, log_queue)
        return result, connection

    def test_missing_origin_returns_none_to_continue_the_handshake(self):
        result, connection = self._call([])
        self.assertIsNone(result)
        connection.respond.assert_not_called()

    def test_duplicate_origin_headers_are_rejected(self):
        """A smuggled second Origin must fail closed, not crash into a 500."""
        log_queue = MockQueue()
        result, connection = self._call(
            [("Origin", OBS_ORIGIN), ("Origin", WEBSITE_ORIGIN)], log_queue
        )
        self.assertEqual(result, "REJECTED")
        self.assertTrue(any(WEBSITE_ORIGIN in message for message in _log_messages(log_queue)))

    def test_rejection_tolerates_a_missing_log_queue(self):
        result, _ = self._call([("Origin", WEBSITE_ORIGIN)], None)
        self.assertEqual(result, "REJECTED")


class TestOriginGuardWiring(unittest.TestCase):
    """WSO-3: run_ws_server must hand the guard to serve() as process_request."""

    @patch("liveaudio.core.network.serve")
    @patch("liveaudio.core.network._poll_queue")
    def test_serve_receives_a_working_process_request_hook(self, mock_poll, mock_serve):
        mock_server = MagicMock()
        mock_server.connections = set()
        mock_serve.return_value.__aenter__ = AsyncMock(return_value=mock_server)
        mock_serve.return_value.__aexit__ = AsyncMock(return_value=False)
        mock_poll.side_effect = KeyboardInterrupt()
        log_queue = MockQueue()

        with self.assertRaises(KeyboardInterrupt):
            network.run_ws_server(MagicMock(), log_queue)

        hook = mock_serve.call_args.kwargs.get("process_request")
        self.assertIsNotNone(hook, "serve() must be called with process_request")

        connection = MagicMock()
        connection.respond.return_value = "REJECTED"
        self.assertIsNone(hook(connection, Request("/", Headers([]))))
        self.assertIsNone(hook(connection, Request("/", Headers([("Origin", OBS_ORIGIN)]))))
        self.assertEqual(
            hook(connection, Request("/", Headers([("Origin", WEBSITE_ORIGIN)]))), "REJECTED"
        )
        self.assertTrue(any(WEBSITE_ORIGIN in message for message in _log_messages(log_queue)))

    @patch("liveaudio.core.network.serve")
    @patch("liveaudio.core.network._poll_queue")
    def test_origin_guard_does_not_disturb_the_bind_fallback(self, mock_poll, mock_serve):
        """The hook is a kwarg; host/port stay positional for the fallback loop."""
        mock_server = MagicMock()
        mock_server.connections = set()
        mock_serve.return_value.__aenter__ = AsyncMock(return_value=mock_server)
        mock_serve.return_value.__aexit__ = AsyncMock(return_value=False)
        mock_poll.side_effect = KeyboardInterrupt()

        with self.assertRaises(KeyboardInterrupt):
            network.run_ws_server(MagicMock(), MockQueue(), port=9321)

        self.assertEqual(mock_serve.call_args[0][1], "127.0.0.1")
        self.assertEqual(mock_serve.call_args[0][2], 9321)


class TestOriginTranslations(unittest.TestCase):
    """i18n key parity for the new user-facing string."""

    def test_status_key_present_in_both_languages(self):
        from liveaudio.utils.i18n import TRANSLATIONS

        for lang in ("es", "en"):
            self.assertIn("status_obs_origin_rejected", TRANSLATIONS[lang], f"missing in {lang}")
            self.assertTrue(TRANSLATIONS[lang]["status_obs_origin_rejected"].strip())

    def test_status_key_is_unique_so_the_gui_reverse_lookup_cannot_alias(self):
        from liveaudio.utils.i18n import TRANSLATIONS

        text = TRANSLATIONS["es"]["status_obs_origin_rejected"]
        collisions = [
            key for key, value in TRANSLATIONS["es"].items()
            if key.startswith("status_") and value == text
        ]
        self.assertEqual(collisions, ["status_obs_origin_rejected"])


if __name__ == "__main__":
    unittest.main()
