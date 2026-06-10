# SPDX-License-Identifier: MIT
"""Focused runtime diagnostics tests for audio/ASR/WebSocket instrumentation."""

import asyncio
import unittest

from liveaudio.core.diagnostics import DiagnosticsStore


class _DisconnectingWebSocket:
    remote_address = ("127.0.0.1", 9000)

    def __aiter__(self):
        async def _messages():
            raise ConnectionError("bye")
            if False:  # pragma: no cover
                yield None

        return _messages()


class _FakeTransport:
    def __init__(self, buffered):
        if isinstance(buffered, (list, tuple)):
            self._buffered_values = list(buffered)
        else:
            self._buffered_values = [buffered]

    def get_write_buffer_size(self):
        if len(self._buffered_values) > 1:
            return self._buffered_values.pop(0)
        return self._buffered_values[0]


class _FakeConnection:
    def __init__(self, buffered):
        self.transport = _FakeTransport(buffered)


class _FakeServer:
    def __init__(self, buffered=0):
        self.connections = {_FakeConnection(buffered)}


class TestAudioRuntimeDiagnostics(unittest.TestCase):
    def test_audio_runtime_health_records_queue_and_worker_state(self):
        from liveaudio.core.audio import _record_audio_runtime_health

        store = DiagnosticsStore(level="deep")
        _record_audio_runtime_health(
            store,
            ring_buffer_chunks=7,
            callback_age_sec=0.4,
            worker_alive=True,
            stream_active=True,
            reconnecting=False,
        )

        snapshot = store.snapshot_runtime_health()
        self.assertEqual(snapshot["states"]["audio.ring_buffer"]["value"]["chunks"], 7)
        self.assertTrue(snapshot["states"]["audio.worker"]["value"]["alive"])
        self.assertTrue(snapshot["states"]["audio.stream"]["value"]["active"])
        self.assertIn("audio.callback_age_sec", snapshot["durations"])

    def test_audio_runtime_health_counts_reconnects_and_drops(self):
        from liveaudio.core.audio import _record_audio_runtime_health

        store = DiagnosticsStore(level="deep")
        _record_audio_runtime_health(store, reconnecting=True, dropped_phrases=2)

        snapshot = store.snapshot_runtime_health()
        self.assertEqual(snapshot["counters"]["audio.reconnects"], 1)
        self.assertEqual(snapshot["counters"]["audio.queue_full_drops"], 2)


class TestAsrRuntimeDiagnostics(unittest.TestCase):
    def test_asr_runtime_health_records_timings_and_state(self):
        from liveaudio.core.engine import _record_asr_runtime_health

        store = DiagnosticsStore(level="deep")
        _record_asr_runtime_health(
            store,
            model_name="small",
            model_load_sec=1.2,
            queue_delay=0.6,
            latency=0.9,
            total_delay=1.5,
            obs_emitted=False,
            reason="backlog_policy",
            backlog_mode="auto",
        )

        snapshot = store.snapshot_runtime_health()
        self.assertIn("asr.model_load_sec", snapshot["durations"])
        self.assertIn("asr.latency_sec", snapshot["durations"])
        self.assertEqual(snapshot["states"]["asr.last_event"]["value"]["reason"], "backlog_policy")
        self.assertFalse(snapshot["states"]["asr.last_event"]["value"]["obs_emitted"])

    def test_asr_runtime_health_counts_timeouts_and_ws_queue_full(self):
        from liveaudio.core.engine import _record_asr_runtime_health

        store = DiagnosticsStore(level="deep")
        _record_asr_runtime_health(store, timed_out=True, queue_full=True)

        snapshot = store.snapshot_runtime_health()
        self.assertEqual(snapshot["counters"]["asr.timeouts"], 1)
        self.assertEqual(snapshot["counters"]["asr.ws_queue_full"], 1)


class TestNetworkRuntimeDiagnostics(unittest.TestCase):
    def test_handle_client_updates_client_count_diagnostics(self):
        from liveaudio.core.network import _handle_client

        store = DiagnosticsStore(level="deep")
        clients = set()
        websocket = _DisconnectingWebSocket()

        async def run_test():
            await _handle_client(websocket, clients, log_queue=None, diagnostics_store=store)

        asyncio.run(run_test())
        snapshot = store.snapshot_runtime_health()
        self.assertEqual(snapshot["states"]["ws.runtime"]["value"]["client_count"], 0)

    def test_poll_queue_records_backpressure_and_drain_count(self):
        from liveaudio.core.network import _poll_queue
        from tests.helpers import MockQueue
        from unittest.mock import patch

        store = DiagnosticsStore(level="deep")
        text_queue = MockQueue()
        text_queue.put({"text": "hello"})
        text_queue.put(None)
        server = _FakeServer(buffered=[70000, 0, 0, 0])

        with patch("liveaudio.core.network.broadcast", lambda connections, payload: None):
            asyncio.run(_poll_queue(text_queue, server, log_queue=None, diagnostics_store=store))

        snapshot = store.snapshot_runtime_health()
        self.assertGreaterEqual(snapshot["counters"]["ws.backpressure_events"], 1)
        self.assertGreaterEqual(snapshot["counters"]["ws.queue_drained_messages"], 1)
        self.assertEqual(snapshot["states"]["ws.runtime"]["value"]["retry_buffer_size"], 0)


if __name__ == "__main__":
    unittest.main()
