# SPDX-License-Identifier: MIT
"""Idempotency tests for the headless service backend (track service-backend).

Double application / duplicate signals must converge to identical state:
double shutdown, duplicate ws_port events, duplicate health snapshots,
repeated config reads, and an unchanged hello handshake (proto 1).
"""

import asyncio
import io
import json
import multiprocessing as mp
import queue
import unittest
from contextlib import redirect_stdout
from unittest.mock import MagicMock, patch

from liveaudio.service import (
    FirstClientGate,
    HealthEmitter,
    InstanceLock,
    ProcessSupervisor,
)

from tests.test_resilience_service_backend import (
    FakeManager,
    FakeProcess,
    TEST_CONFIG,
    make_supervisor,
    start_supervisor,
)


class TestDoubleShutdown(unittest.TestCase):
    def test_shutdown_twice_tears_down_exactly_once(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            sup = make_supervisor(tmp)
            start_supervisor(sup)
            manager = sup.manager
            ws_proc = sup.procs["ws"]
            sup.shutdown(reason="first")
            sup.shutdown(reason="second")
            sup.shutdown()
            self.assertEqual(manager.shutdown_calls, 1)
            self.assertEqual(ws_proc.terminate_calls, 1)
            self.assertEqual(sup.state, "stopped")

    def test_shutdown_before_start_is_safe(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            sup = make_supervisor(tmp)
            sup.shutdown(reason="never-started")
            sup.shutdown(reason="never-started-again")
            self.assertEqual(sup.state, "stopped")

    def test_shutdown_after_partial_start_is_safe(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            sup = make_supervisor(tmp, parent_checker=lambda pid: False)
            with patch("liveaudio.core.network.port_range_available", return_value=True):
                with self.assertRaises(Exception):
                    sup.start()  # parent-dead-at-startup, before children
            sup.shutdown(reason="partial")
            sup.shutdown(reason="partial-again")
            self.assertEqual(sup.state, "stopped")


class TestDuplicateSignals(unittest.TestCase):
    def test_duplicate_ws_port_events_converge(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            sup = make_supervisor(tmp)
            start_supervisor(sup)
            try:
                for _ in range(3):
                    sup.log_queue.put_nowait({"type": "ws_port", "port": 8767, "base": 8765})
                sup.poll_once()
                self.assertEqual(sup.effective_port, 8767)
                self.assertEqual(sup.base_port, 8765)
            finally:
                sup.shutdown()

    def test_malformed_log_messages_are_ignored(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            sup = make_supervisor(tmp)
            start_supervisor(sup)
            try:
                sup.log_queue.put_nowait("not-a-dict")
                sup.log_queue.put_nowait({"type": "ws_port"})  # no port
                sup.log_queue.put_nowait({"type": "ws_port", "port": "nan"})
                sup.log_queue.put_nowait({"type": "status", "key": "obs",
                                          "text": "x", "state": "ok"})  # wrong key
                sup.poll_once()  # must not raise, must not change state
                self.assertIsNone(sup.effective_port)
                self.assertEqual(sup.asr_state, "unavailable")
            finally:
                sup.shutdown()

    def test_duplicate_snapshots_are_stable(self):
        import tempfile
        import os
        with tempfile.TemporaryDirectory() as tmp:
            health = os.path.join(tmp, "health.json")
            emitter = HealthEmitter(service_pid=111, parent_pid=222, health_file=health)
            sup = make_supervisor(tmp)
            sup.emitter = emitter
            start_supervisor(sup)
            try:
                first = sup.snapshot()
                sup._write_health()
                snap_a = json.load(open(health, encoding="utf-8"))
                sup._write_health()
                snap_b = json.load(open(health, encoding="utf-8"))
                self.assertEqual(snap_a, snap_b)
                self.assertEqual(first["effective_port"], snap_a["effective_port"])
            finally:
                sup.shutdown()

    def test_repeated_readonly_loads_agree(self):
        import tempfile
        import os
        from liveaudio.utils import config as cfg
        with tempfile.TemporaryDirectory() as tmp:
            target = os.path.join(tmp, "config.json")
            with open(target, "w", encoding="utf-8") as f:
                json.dump({"ws_port": 8800, "subtitle_backlog_policy": "live_only"}, f)
            # Decision: each read-only load is an independent snapshot and must
            # NOT promise immutable hardware — GPU availability can change with
            # the environment between calls. The test pins the detector instead
            # of hiding a real divergence behind retries.
            with patch.dict(os.environ, {"LIVEAUDIO_HOME": tmp}):
                with patch("liveaudio.utils.cuda.cuda_is_available", return_value=False):
                    first, info_a = cfg.load_config_readonly()
                    second, info_b = cfg.load_config_readonly()
                self.assertEqual(first, second)
                self.assertEqual(info_a, info_b)
                self.assertEqual(first["ws_port"], 8800)
                self.assertEqual(first["subtitle_backlog_policy"], "live_only")
                self.assertEqual(first["device"], "cpu")  # pinned detector applies


class TestHelloHandshakeStable(unittest.TestCase):
    def test_hello_payload_unchanged_proto_1_recv_only(self):
        from liveaudio.core.network import _handle_client

        received = []

        class FakeWS:
            def __init__(self):
                self.remote_address = ("127.0.0.1", 5555)
                self.sent = []
                self.closed = False

            async def send(self, msg):
                self.sent.append(msg)

            def __aiter__(self):
                outer = self

                class _Inbound:
                    def __init__(self):
                        self.n = 0

                    async def __anext__(self):
                        self.n += 1
                        if self.n <= 3:
                            received.append("same-text")
                            return "same-text"  # 3 inbound payloads, same text
                        raise StopAsyncIteration

                return _Inbound()

        clients = set()
        ws = FakeWS()
        asyncio.run(_handle_client(ws, clients, MagicMock(), effective_port=8769))
        self.assertEqual(received, ["same-text"] * 3)
        self.assertEqual(clients, set())  # disconnected: recv-only left no state
        # Only the hello handshake was emitted server-side; nothing echoed back.
        self.assertEqual(len(ws.sent), 1)
        hello = json.loads(ws.sent[0])
        self.assertEqual((hello["type"], hello["proto"], hello["port"]),
                         ("hello", 1, 8769))

    def test_three_identical_inbound_payloads_have_no_side_effects(self):
        from liveaudio.core.network import _handle_client

        class FakeWS:
            def __init__(self):
                self.remote_address = ("127.0.0.1", 5556)
                self.sent = []

            async def send(self, msg):
                self.sent.append(msg)

            def __aiter__(self):
                class _Inbound:
                    def __init__(self):
                        self.n = 0

                    async def __anext__(self):
                        self.n += 1
                        if self.n <= 3:
                            return "identical inbound text"
                        raise StopAsyncIteration

                return _Inbound()

        log_queue = MagicMock()
        clients = set()
        ws = FakeWS()
        asyncio.run(_handle_client(ws, clients, log_queue, effective_port=8769))
        # Exactly one hello, no echo/reply to the 3 inbound payloads.
        self.assertEqual(len(ws.sent), 1)
        hello = json.loads(ws.sent[0])
        self.assertEqual((hello["type"], hello["proto"], hello["port"]),
                         ("hello", 1, 8769))
        self.assertEqual(clients, set())


class TestGateAndLockIdempotency(unittest.TestCase):
    def test_gate_shared_across_threads_fires_once(self):
        import threading
        gate = FirstClientGate()
        results = []
        threads = [threading.Thread(target=lambda: results.append(gate.fire()))
                   for _ in range(16)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()
        self.assertEqual(sum(1 for r in results if r), 1)

    def test_emitter_scrubs_sensitive_keys(self):
        buf = io.StringIO()
        emitter = HealthEmitter(service_pid=1, parent_pid=2)
        with redirect_stdout(buf):
            emitter.emit("service_state", {
                "state": "running",
                "text": "hola mundo secreto",
                "transcript": "secreto",
                "segments": [1, 2],
                "output_dir": "C:\\secret",
            })
        line = json.loads(buf.getvalue().strip())
        blob = json.dumps(line)
        self.assertNotIn("secreto", blob)
        self.assertNotIn("secret", blob)
        self.assertEqual(line["state"], "running")

    def test_cli_without_service_flag_does_not_consume_args(self):
        from liveaudio import cli
        with patch("liveaudio.app.main", return_value=0) as gui_main:
            code = cli.main([])
            self.assertEqual(code, 0)
            gui_main.assert_called_once_with()


if __name__ == "__main__":
    unittest.main()
