# SPDX-License-Identifier: MIT
"""Resilience tests for the headless service backend (track service-backend).

Deterministic: no Whisper/Silero/GPU, no real mic, no real sockets except
loopback pre-flight which is always patched. Heavy child targets are faked
via ProcessSupervisor factories.
"""

import asyncio
import io
import json
import multiprocessing as mp
import os
import queue
import sys
import unittest
from contextlib import redirect_stdout
from unittest.mock import AsyncMock, MagicMock, patch

from liveaudio.service import (
    FirstClientGate,
    HealthEmitter,
    InstanceLock,
    ProcessSupervisor,
    ServiceError,
    build_arg_parser,
    candidate_ports,
    is_parent_alive,
    parse_parent_pid,
)


TEST_CONFIG = {
    "output_dir": "",
    "ws_port": 8765,
    "obs_enabled": True,
    "save_transcript_enabled": True,
    "save_vtt_enabled": True,
    "subtitle_backlog_policy": "auto",
}


def _race_service_lock(home, barrier, release_event, results):
    lock = InstanceLock(home=home)
    barrier.wait(10)
    try:
        lock.acquire()
    except ServiceError as exc:
        results.put(("rejected", exc.code))
        return
    results.put(("acquired", os.getpid()))
    release_event.wait(10)
    lock.release()


def _try_service_lock_once(home, results):
    lock = InstanceLock(home=home)
    try:
        lock.acquire()
    except ServiceError as exc:
        results.put(("rejected", exc.code))
        return
    results.put(("acquired", os.getpid()))
    lock.release()


def _hold_service_lock_during_metadata_write(home, truncated, resume):
    import liveaudio.service.lock as lock_module

    original_write = lock_module._write_pid

    def paused_write(handle, pid):
        handle.seek(0)
        handle.truncate()
        handle.flush()
        truncated.set()
        if not resume.wait(10):
            raise RuntimeError("metadata write was not resumed")
        original_write(handle, pid)

    lock_module._write_pid = paused_write
    lock = InstanceLock(home=home)
    try:
        lock.acquire()
    finally:
        lock.release()


def _hold_service_lock_until_terminated(home, acquired_event):
    lock = InstanceLock(home=home)
    lock.acquire()
    acquired_event.set()
    import time
    time.sleep(60)


class FakeManager:
    def __init__(self):
        self.shutdown_calls = 0

    def dict(self, mapping):
        return dict(mapping)

    def shutdown(self):
        self.shutdown_calls += 1


class TestServiceAudioQueueSizing(unittest.TestCase):
    def test_default_audio_queue_uses_configured_phrase_window_budget(self):
        from liveaudio.service import supervisor as supervisor_module

        sizes = []

        def queue_factory(maxsize):
            sizes.append(maxsize)
            return object()

        config = {
            "output_dir": "",
            "ws_port": 8765,
            "max_chunk_duration": 60.0,
            "silence_timeout": 2.0,
            "vad_speech_pad_ms": 500,
        }
        service = ProcessSupervisor(config, parent_pid=1, emitter=MagicMock())
        with patch.object(supervisor_module.mp, "Queue", side_effect=queue_factory):
            service._make_queues()

        self.assertEqual(sizes, [1, 100, 100])


class FakeProcess:
    def __init__(self, target=None, args=(), kwargs=None, name="", daemon=True, alive=True):
        self.target = target
        self.args = args
        self.kwargs = kwargs or {}
        self.name = name
        self.daemon = daemon
        self._alive = alive
        self.start_calls = 0
        self.join_calls = 0
        self.terminate_calls = 0
        self.kill_calls = 0
        self.terminate_kills = True

    def start(self):
        self.start_calls += 1

    def is_alive(self):
        return self._alive

    def join(self, timeout=None):
        self.join_calls += 1

    def terminate(self):
        self.terminate_calls += 1
        if self.terminate_kills:
            self._alive = False

    def kill(self):
        self.kill_calls += 1
        self._alive = False


class FakeQueue(queue.Queue):
    """queue.Queue recording mp.Queue-style teardown calls."""

    def __init__(self):
        super().__init__()
        self.close_calls = 0
        self.cancel_join_thread_calls = 0

    def close(self):
        self.close_calls += 1

    def cancel_join_thread(self):
        self.cancel_join_thread_calls += 1


class FakeClock:
    def __init__(self):
        self.now = 1000.0

    def __call__(self):
        return self.now

    def advance(self, seconds):
        self.now += seconds


def make_supervisor(tmp_dir, **overrides):
    config = dict(TEST_CONFIG)
    config["output_dir"] = tmp_dir
    emitter = HealthEmitter(service_pid=99999, parent_pid=4242)
    events = []
    emitter.emit = lambda event_type, fields=None: events.append((event_type, fields or {}))
    clock = FakeClock()
    params = {
        "config": config,
        "parent_pid": 4242,
        "emitter": emitter,
        "parent_checker": lambda pid: True,
        "clock": clock,
        "sleep": lambda s: None,
        "queue_factory": queue.Queue,
        "manager_factory": FakeManager,
        "process_factory": lambda **kw: FakeProcess(**kw),
        "prewarm": overrides.get("prewarm", False),
    }
    params.update(overrides)
    sup = ProcessSupervisor(**params)
    sup._captured_events = events
    sup._clock_obj = clock
    return sup


def start_supervisor(sup):
    with patch("liveaudio.core.network.port_range_available", return_value=True):
        sup.start()
    return sup


class TestParentPidValidation(unittest.TestCase):
    def test_rejects_zero_negative_and_text(self):
        for raw in ("0", "-1", "abc", "", None):
            with self.assertRaises(ServiceError) as ctx:
                parse_parent_pid(raw)
            self.assertEqual(ctx.exception.code, "parent-pid-invalid")

    def test_accepts_positive_pid(self):
        self.assertEqual(parse_parent_pid("1234"), 1234)
        self.assertEqual(parse_parent_pid(4242), 4242)

    def test_is_parent_alive_uses_injected_checker(self):
        self.assertTrue(is_parent_alive(1, checker=lambda pid: True))
        self.assertFalse(is_parent_alive(1, checker=lambda pid: False))

    def test_checker_exception_means_dead(self):
        def boom(pid):
            raise RuntimeError("nope")
        self.assertFalse(is_parent_alive(1, checker=boom))

    def test_startup_with_dead_parent_fails_fast(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            sup = make_supervisor(tmp, parent_checker=lambda pid: False)
            with patch("liveaudio.core.network.port_range_available", return_value=True):
                with self.assertRaises(ServiceError) as ctx:
                    sup.start()
            self.assertEqual(ctx.exception.code, "parent-dead-at-startup")
            # Nothing was spawned: no manager, no procs, no session writes.
            self.assertIsNone(sup.manager)
            self.assertEqual(
                [p for p in sup.procs.values() if p is not None], [])

    def test_run_with_dead_parent_returns_zero_after_graceful_shutdown(self):
        import tempfile
        calls = {"n": 0}

        def checker(pid):
            calls["n"] += 1
            return calls["n"] < 2  # alive for start(), dead on first tick

        with tempfile.TemporaryDirectory() as tmp:
            sup = make_supervisor(tmp, parent_checker=checker)
            with patch("liveaudio.core.network.port_range_available", return_value=True):
                code = sup.run()
        self.assertEqual(code, 0)
        fatal = [e for e in sup._captured_events if e[0] == "fatal"]
        self.assertEqual([e[1]["code"] for e in fatal], ["parent-dead"])

    def test_service_main_rejects_invalid_parent_pid_without_side_effects(self):
        from liveaudio import service
        buf = io.StringIO()
        with redirect_stdout(buf):
            code = service.main(["--parent-pid", "0"])
        self.assertEqual(code, 2)
        line = json.loads(buf.getvalue().strip().splitlines()[-1])
        self.assertEqual(line["type"], "fatal")
        self.assertEqual(line["code"], "parent-pid-invalid")


class TestConfigReadOnly(unittest.TestCase):
    def test_readonly_never_writes_missing_file(self):
        import tempfile
        from liveaudio.utils import config as cfg
        with tempfile.TemporaryDirectory() as tmp:
            with patch.dict(os.environ, {"LIVEAUDIO_HOME": tmp}):
                target = os.path.join(tmp, "config.json")
                self.assertFalse(os.path.exists(target))
                loaded, info = cfg.load_config_readonly()
                self.assertFalse(os.path.exists(target), "read-only load created config.json")
                self.assertEqual(info["error_code"], "config-missing-defaults")
                self.assertEqual(loaded["ws_port"], 8765)
                self.assertEqual(loaded["subtitle_backlog_policy"], "auto")

    def test_readonly_never_writes_corrupt_file(self):
        import tempfile
        from liveaudio.utils import config as cfg
        with tempfile.TemporaryDirectory() as tmp:
            target = os.path.join(tmp, "config.json")
            with open(target, "w", encoding="utf-8") as f:
                f.write("{not valid json")
            before = open(target, "rb").read()
            with patch.dict(os.environ, {"LIVEAUDIO_HOME": tmp}):
                loaded, info = cfg.load_config_readonly()
                self.assertEqual(open(target, "rb").read(), before)
                self.assertEqual(info["error_code"], "config-corrupt-defaults")
                self.assertTrue(loaded["obs_enabled"])

    def test_readonly_normalizes_in_memory_without_persisting(self):
        import tempfile
        from liveaudio.utils import config as cfg
        with tempfile.TemporaryDirectory() as tmp:
            target = os.path.join(tmp, "config.json")
            with open(target, "w", encoding="utf-8") as f:
                json.dump({"ws_port": 999999, "subtitle_backlog_policy": "bogus"}, f)
            before = open(target, "rb").read()
            with patch.dict(os.environ, {"LIVEAUDIO_HOME": tmp}):
                loaded, info = cfg.load_config_readonly()
                self.assertEqual(open(target, "rb").read(), before)
                self.assertIsNone(info["error_code"])
                self.assertEqual(loaded["ws_port"], 65535)  # clamped, memory only
                self.assertEqual(loaded["subtitle_backlog_policy"], "auto")

    def test_service_start_does_not_touch_config_file(self):
        import tempfile
        from liveaudio.utils import config as cfg
        with tempfile.TemporaryDirectory() as tmp:
            target = os.path.join(tmp, "config.json")
            with open(target, "w", encoding="utf-8") as f:
                json.dump({"ws_port": 8765}, f)
            before = open(target, "rb").read()
            with patch.dict(os.environ, {"LIVEAUDIO_HOME": tmp}):
                loaded, _ = cfg.load_config_readonly()
                sup = make_supervisor(tmp)
                sup.config = loaded
                start_supervisor(sup)
                try:
                    self.assertEqual(open(target, "rb").read(), before)
                finally:
                    sup.shutdown()


class TestLazyFirstClient(unittest.TestCase):
    def test_session_capture_origin_uses_monotonic_not_supervision_clock(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            sup = make_supervisor(tmp)
            with patch("liveaudio.core.network.port_range_available", return_value=True), \
                 patch("liveaudio.service.supervisor.time.monotonic", return_value=400.0):
                sup.start()
            try:
                self.assertEqual(sup.shared["session_started_monotonic"], 400.0)
                self.assertEqual(sup._clock(), 1000.0)
            finally:
                sup.shutdown()

    def test_gate_fires_exactly_once(self):
        gate = FirstClientGate()
        self.assertTrue(gate.fire())
        self.assertFalse(gate.fire())
        self.assertFalse(gate.fire())
        self.assertTrue(gate.fired)

    def test_ws_starts_immediately_audio_asr_wait_for_first_client(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            sup = make_supervisor(tmp)
            start_supervisor(sup)
            try:
                self.assertTrue(sup.procs["ws"].start_calls == 1)
                session_origin = sup.shared["session_started_monotonic"]
                self.assertNotEqual(session_origin, sup._clock())
                self.assertIsNone(sup.procs["audio"])
                self.assertIsNone(sup.procs["asr"])
                self.assertEqual(sup.asr_state, "unavailable")
                sup.poll_once()  # no client yet: still lazy
                self.assertIsNone(sup.procs["audio"])
                sup._clock_obj.advance(60.0)
                sup.first_client_event.set()  # first client (probe counts too)
                sup.poll_once()
                self.assertIsNotNone(sup.procs["audio"])
                self.assertIsNotNone(sup.procs["asr"])
                self.assertEqual(sup.shared["session_started_monotonic"], session_origin)
                first_audio, first_asr = sup.procs["audio"], sup.procs["asr"]
                sup.poll_once()
                sup.poll_once()
                self.assertIs(first_audio, sup.procs["audio"])
                self.assertIs(first_asr, sup.procs["asr"])
                self.assertEqual(first_audio.start_calls, 1)
            finally:
                sup.shutdown()

    def test_prewarm_starts_audio_asr_immediately(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            sup = make_supervisor(tmp, prewarm=True)
            start_supervisor(sup)
            try:
                self.assertTrue(sup.procs["ws"].start_calls == 1)
                self.assertIsNotNone(sup.procs["audio"])
                self.assertIsNotNone(sup.procs["asr"])
                self.assertEqual(sup.procs["audio"].start_calls, 1)
                self.assertEqual(sup.procs["asr"].start_calls, 1)
                self.assertEqual(sup.asr_state, "starting")
            finally:
                sup.shutdown()

    def test_cli_prewarm_flags(self):
        self.assertIsNone(build_arg_parser().parse_args(["--parent-pid", "123"]).prewarm)
        self.assertTrue(build_arg_parser().parse_args(["--parent-pid", "123", "--prewarm"]).prewarm)
        self.assertFalse(build_arg_parser().parse_args(["--parent-pid", "123", "--no-prewarm"]).prewarm)
        self.assertFalse(build_arg_parser().parse_args(["--parent-pid", "123", "--lazy"]).prewarm)

    def test_explicit_cli_prewarm_flags_override_normalized_config(self):
        from liveaudio import service
        from liveaudio.utils import config as config_module
        import tempfile

        normalized_config = dict(config_module.DEFAULT_CONFIG, prewarm=False)
        config_module._normalize_config(normalized_config)
        calls = []

        class FakeLock:
            def acquire(self):
                pass

            def release(self):
                pass

        class TrackingSupervisor(ProcessSupervisor):
            def __init__(self, *args, **kwargs):
                super().__init__(*args, **kwargs)
                calls.append(self.prewarm)

            def run(self):
                return 0

        for flags in ([], ["--prewarm"], ["--lazy"], ["--no-prewarm"]):
            with (
                patch.object(service.mp, "freeze_support"),
                patch("liveaudio.utils.config.load_config_readonly",
                      return_value=(normalized_config, {})),
                patch("liveaudio.utils.config.get_data_home", return_value=tempfile.gettempdir()),
                patch.object(service, "InstanceLock", return_value=FakeLock()),
                patch.object(service, "HealthEmitter", return_value=MagicMock()),
                patch.object(service, "ProcessSupervisor", TrackingSupervisor),
            ):
                self.assertEqual(service.main(["--parent-pid", "123", *flags]), 0)

        self.assertEqual(calls, [False, True, False, False])

    def test_precedence_explicit_argument_then_config_then_true_default(self):
        explicit_true = ProcessSupervisor({"prewarm": False}, 123, MagicMock(), prewarm=True)
        explicit_false = ProcessSupervisor({"prewarm": True}, 123, MagicMock(), prewarm=False)
        configured_false = ProcessSupervisor({"prewarm": False}, 123, MagicMock(), prewarm=None)
        default_true = ProcessSupervisor({}, 123, MagicMock(), prewarm=None)

        self.assertTrue(explicit_true.prewarm)
        self.assertFalse(explicit_false.prewarm)
        self.assertFalse(configured_false.prewarm)
        self.assertTrue(default_true.prewarm)

    def test_network_hook_fires_on_connection(self):
        from liveaudio.core.network import _handle_client

        class FakeWS:
            def __init__(self):
                self.remote_address = ("127.0.0.1", 4321)
                self.sent = []

            async def send(self, msg):
                self.sent.append(msg)

            def __aiter__(self):
                outer = self

                class _Empty:
                    async def __anext__(self):
                        raise StopAsyncIteration

                return _Empty()

        fired = []
        event = mp.Event()
        ws = FakeWS()
        asyncio.run(_handle_client(
            ws, set(), MagicMock(), effective_port=8771,
            on_first_client=lambda: fired.append(1),
            first_client_event=event))
        self.assertEqual(fired, [1])
        self.assertTrue(event.is_set())
        hello = json.loads(ws.sent[0])
        self.assertEqual(hello["type"], "hello")
        self.assertEqual(hello["proto"], 1)  # proto unchanged
        self.assertEqual(hello["port"], 8771)  # effective port announced

    def test_server_level_hook_is_one_shot(self):
        from liveaudio.core.network import run_ws_server

        class FakeWS:
            def __init__(self):
                self.remote_address = ("127.0.0.1", 4321)
                self.sent = []

            async def send(self, msg):
                self.sent.append(msg)

            def __aiter__(self):
                class _Empty:
                    async def __anext__(self):
                        raise StopAsyncIteration
                return _Empty()

        captured = {}

        def fake_serve(handler, *args, **kwargs):
            captured["handler"] = handler
            ctx = MagicMock()
            ctx.__aenter__ = AsyncMock(return_value=MagicMock(connections=set()))
            ctx.__aexit__ = AsyncMock(return_value=False)
            return ctx

        fired = []
        event = mp.Event()
        with patch("liveaudio.core.network.serve", side_effect=fake_serve):
            with patch("liveaudio.core.network._poll_queue", side_effect=KeyboardInterrupt()):
                with self.assertRaises(KeyboardInterrupt):
                    run_ws_server(queue.Queue(), MagicMock(), port=8765,
                                  on_first_client=lambda: fired.append(1),
                                  first_client_event=event)
        handler = captured["handler"]
        asyncio.run(handler(FakeWS()))
        asyncio.run(handler(FakeWS()))
        self.assertEqual(fired, [1])
        self.assertTrue(event.is_set())


class TestEffectivePortRange(unittest.TestCase):
    def test_range_is_base_to_base_plus_9(self):
        ports = candidate_ports(8765)
        self.assertEqual(len(ports), 10)
        self.assertEqual(ports[0], 8765)
        self.assertEqual(ports[-1], 8774)  # base+9, NOT base+10
        self.assertNotIn(8775, ports)

    def test_range_clamps_at_65535(self):
        ports = candidate_ports(65530)
        self.assertTrue(all(1 <= p <= 65535 for p in ports))
        self.assertEqual(ports[-1], 65535)

    def test_invalid_base_fails_fast(self):
        for bad in (0, -1, 70000, "abc", None):
            with self.assertRaises(ServiceError) as ctx:
                candidate_ports(bad)
            self.assertEqual(ctx.exception.code, "ws-base-port-invalid")

    def test_exhausted_range_fails_fast_before_spawn(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            sup = make_supervisor(tmp)
            with patch("liveaudio.core.network.port_range_available", return_value=False):
                with self.assertRaises(ServiceError) as ctx:
                    sup.start()
            self.assertEqual(ctx.exception.code, "port-range-exhausted")
            self.assertIsNone(sup.manager)

    def test_ws_port_event_updates_effective_port(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            sup = make_supervisor(tmp)
            start_supervisor(sup)
            try:
                sup.log_queue.put_nowait({"type": "ws_port", "port": 8767, "base": 8765})
                sup.poll_once()
                self.assertEqual(sup.effective_port, 8767)
                ws_events = [e for e in sup._captured_events if e[0] == "ws_port"]
                self.assertTrue(ws_events)
                self.assertEqual(ws_events[-1][1]["effective_port"], 8767)
                self.assertEqual(ws_events[-1][1]["base_port"], 8765)
            finally:
                sup.shutdown()

    def test_asr_status_signals_map_to_asr_state(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            sup = make_supervisor(tmp)
            start_supervisor(sup)
            try:
                sup.first_client_event.set()
                sup.poll_once()
                self.assertEqual(sup.asr_state, "starting")
                sup.log_queue.put_nowait({"type": "status", "key": "asr",
                                          "text": "ASR: cargando", "state": "active"})
                sup.poll_once()
                self.assertEqual(sup.asr_state, "loading")
                sup.log_queue.put_nowait({"type": "status", "key": "asr",
                                          "text": "ASR: listo", "state": "ok"})
                sup.poll_once()
                self.assertEqual(sup.asr_state, "ready")
            finally:
                sup.shutdown()


class TestChildFatalPropagation(unittest.TestCase):
    def test_shutdown_records_queued_audio_discard_count(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            supervisor = make_supervisor(tmp, prewarm=False)
            supervisor.audio_queue = queue.Queue()
            supervisor.text_queue = queue.Queue()
            supervisor.log_queue = queue.Queue()
            supervisor.audio_queue.put_nowait({"audio": [1], "sequence": 1})
            supervisor.audio_queue.put_nowait(None)
            supervisor.text_queue.put_nowait({"text": "private"})
            supervisor._drain_queues()

        self.assertEqual(supervisor.loss_counters["audio.shutdown_discarded"], 1)
        self.assertEqual(supervisor.loss_counters["asr.text_shutdown_discarded"], 1)
        self.assertEqual(supervisor.loss_counters["runtime.log_shutdown_discarded"], 0)
        warnings = [fields for kind, fields in supervisor._captured_events if kind == "warning"]
        self.assertTrue(any(item["code"] == "shutdown-data-discarded" for item in warnings))
        self.assertNotIn("private", json.dumps(warnings))

    def test_full_queue_stop_sentinel_is_counted_and_emits_safe_warning(self):
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            supervisor = make_supervisor(tmp, prewarm=False)
            supervisor.audio_queue = queue.Queue(maxsize=1)
            supervisor.text_queue = queue.Queue(maxsize=1)
            supervisor.audio_queue.put_nowait({"audio": [1]})
            supervisor.text_queue.put_nowait({"text": "private"})
            supervisor._stop_children()

        warnings = [fields for kind, fields in supervisor._captured_events if kind == "warning"]
        self.assertEqual(supervisor.loss_counters["audio.stop_control_rejected"], 1)
        self.assertEqual(supervisor.loss_counters["asr.text_stop_control_rejected"], 1)
        self.assertTrue(warnings)
        self.assertNotIn("private", json.dumps(warnings))

    def test_writer_fatal_stops_service_instead_of_scheduling_restart(self):
        import tempfile
        supervisor = None

        with tempfile.TemporaryDirectory() as tmp:
            supervisor = make_supervisor(tmp, prewarm=False)
            original_poll_once = supervisor.poll_once

            def poll_once_then_stop_if_unhandled():
                supervisor.log_queue.put_nowait({"type": "fatal", "code": "writer_storage_error"})
                original_poll_once()
                raise ServiceError("test-stop")

            supervisor.poll_once = poll_once_then_stop_if_unhandled
            result = supervisor.run()

        fatal = [event for event in supervisor._captured_events if event[0] == "fatal"]
        self.assertEqual(result, 1)
        self.assertEqual([event[1]["code"] for event in fatal], ["writer_storage_error"])
        self.assertEqual(supervisor.state, "stopped")
        self.assertIsNone(supervisor._pending_restart_at)
        self.assertTrue(all(proc is None or not proc.is_alive() for proc in supervisor.procs.values()))

    def test_asr_storage_failure_with_full_log_queue_stops_service_run(self):
        import builtins
        import tempfile
        import time
        from liveaudio.core.engine import asr_consumer
        from tests.helpers import make_shared_config

        with tempfile.TemporaryDirectory() as tmp:
            supervisor = make_supervisor(tmp, prewarm=False)

            def start_and_run_asr_failure():
                supervisor.shared = make_shared_config({
                    "output_dir": tmp, "blacklist": "",
                    "save_transcript_enabled": True, "save_vtt_enabled": False,
                    "obs_enabled": False, "diagnostics_enabled": False,
                    "writer_failure_code": None,
                })
                supervisor.audio_queue = queue.Queue()
                supervisor.text_queue = queue.Queue()
                supervisor.log_queue = queue.Queue(maxsize=1)
                supervisor.log_queue.put_nowait({"type": "log", "message": "occupied"})
                supervisor.session_dir = tmp
                supervisor.state = "running"
                supervisor.audio_queue.put({"audio": [], "created_at": time.time(), "sequence": 1})
                supervisor.audio_queue.put(None)

                class FakeModel:
                    def transcribe(self, audio, **kwargs):
                        from types import SimpleNamespace
                        return iter([SimpleNamespace(text="safe text", no_speech_prob=0.0)]), SimpleNamespace()

                jsonl_path = os.path.join(tmp, "transcript.jsonl")
                real_open = builtins.open

                def fail_jsonl(path, *args, **kwargs):
                    if path == jsonl_path:
                        raise OSError("private path and error detail")
                    return real_open(path, *args, **kwargs)

                with patch("builtins.open", side_effect=fail_jsonl):
                    with patch("liveaudio.core.provisioning.prepare_model", return_value="fake-model"), \
                         patch("liveaudio.core.engine.WhisperModel", return_value=FakeModel()):
                        asr_consumer(
                            supervisor.audio_queue, supervisor.text_queue,
                            supervisor.log_queue, supervisor.shared, tmp,
                        )
                self.assertEqual(supervisor.log_queue.qsize(), 1)

            supervisor.start = start_and_run_asr_failure
            original_poll_once = supervisor.poll_once

            def stop_if_failure_not_observed():
                original_poll_once()
                raise ServiceError("test-stop")

            supervisor.poll_once = stop_if_failure_not_observed
            result = supervisor.run()

        fatal = [event for event in supervisor._captured_events if event[0] == "fatal"]
        self.assertEqual(result, 1)
        self.assertEqual(supervisor.shared.get("writer_failure_code"), "writer_storage_error")
        self.assertEqual([event[1]["code"] for event in fatal], ["writer_storage_error"])
        self.assertEqual(supervisor.state, "stopped")
        self.assertIsNone(supervisor._pending_restart_at)
        self.assertTrue(all(proc is None or not proc.is_alive() for proc in supervisor.procs.values()))


class TestWriterFailureSessionReset(unittest.TestCase):
    def test_new_service_session_clears_sticky_writer_failure(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            supervisor = make_supervisor(tmp, prewarm=False)
            supervisor.config["writer_failure_code"] = "writer_storage_error"
            start_supervisor(supervisor)
            try:
                self.assertIsNone(supervisor.shared.get("writer_failure_code"))
            finally:
                supervisor.shutdown()


class TestChildFailureCeiling(unittest.TestCase):
    def test_three_failures_in_window_raise_fatal(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            sup = make_supervisor(
                tmp,
                process_factory=lambda **kw: FakeProcess(alive=False, **kw))
            start_supervisor(sup)
            try:
                sup.poll_once()  # failure 1 -> restart scheduled with backoff
                got = None
                for _ in range(6):
                    sup._clock_obj.advance(20.0)  # let the pending restart run
                    try:
                        sup.poll_once()
                    except ServiceError as exc:
                        got = exc
                        break
                self.assertIsNotNone(got, "ceiling never tripped for dying children")
                self.assertEqual(got.code, "children-failed-3-in-5m")
            finally:
                sup.shutdown()

    def test_old_failures_slide_out_of_window(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            sup = make_supervisor(
                tmp,
                process_factory=lambda **kw: FakeProcess(alive=False, **kw))
            start_supervisor(sup)
            try:
                sup.poll_once()  # failure 1 at t
                sup._clock_obj.advance(301.0)  # outside the 5min window
                sup.poll_once()  # treated as failure 1 again, not fatal
                self.assertEqual(len(sup.failures), 1)
            finally:
                sup.shutdown()

    def test_respawn_recreates_queues_after_backoff(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            procs = []

            def factory(**kw):
                proc = FakeProcess(**kw)
                procs.append(proc)
                return proc

            sup = make_supervisor(tmp, process_factory=factory)
            start_supervisor(sup)
            try:
                first_audio_queue = sup.audio_queue
                sup.procs["ws"]._alive = False  # simulate WS crash
                sup.poll_once()
                self.assertIsNotNone(sup._pending_restart_at)
                sup._clock_obj.advance(30.0)
                sup.poll_once()
                self.assertIsNone(sup._pending_restart_at)
                self.assertIsNot(sup.audio_queue, first_audio_queue)
            finally:
                sup.shutdown()


class TestReplayBound(unittest.TestCase):
    def test_replay_buffer_drops_oldest_and_counts(self):
        from liveaudio.core.network import REPLAY_BUFFER_MAX, _poll_queue
        text_queue = queue.Queue()
        total = REPLAY_BUFFER_MAX + 44
        for i in range(total):
            text_queue.put_nowait({"text": "msg-%d" % i, "is_replay": True,
                                   "catchup_interval_sec": 1.5})
        text_queue.put_nowait(None)
        server = MagicMock()
        server.connections = set()
        diagnostics = MagicMock()
        log_queue = MagicMock()
        asyncio.run(_poll_queue(text_queue, server, log_queue,
                                diagnostics_store=diagnostics))
        drop_calls = [c for c in diagnostics.record_counter.call_args_list
                      if c[0][0] == "ws.replay_drops"]
        self.assertEqual(len(drop_calls), total - REPLAY_BUFFER_MAX)
        sizes = [c[0][1]["replay_buffer_size"]
                 for c in diagnostics.record_state.call_args_list
                 if "replay_buffer_size" in c[0][1]]
        self.assertTrue(sizes)
        self.assertTrue(all(s <= REPLAY_BUFFER_MAX for s in sizes))
        warned = any("Replay buffer lleno" in (c[0][0].get("message", ""))
                     for c in log_queue.put_nowait.call_args_list
                     if isinstance(c[0][0], dict))
        self.assertTrue(warned)


class TestHealthAndLock(unittest.TestCase):
    def test_snapshot_has_no_sensitive_material(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            health = os.path.join(tmp, "health.json")
            emitter = HealthEmitter(service_pid=111, parent_pid=222, health_file=health)
            ok = emitter.write_snapshot({
                "state": "running", "base_port": 8765, "effective_port": 8766,
                "text": "HOLA SECRETO", "transcript": "SECRETO",
                "output_dir": "C:\\private\\path", "message": "log text",
            })
            self.assertTrue(ok)
            snap = json.load(open(health, encoding="utf-8"))
            blob = json.dumps(snap)
            self.assertNotIn("SECRETO", blob)
            self.assertNotIn("private", blob)
            self.assertNotIn("log text", blob)
            self.assertEqual(snap["base_port"], 8765)

    def test_health_file_written_atomically(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            health = os.path.join(tmp, "sub", "health.json")
            emitter = HealthEmitter(service_pid=111, parent_pid=222, health_file=health)
            self.assertTrue(emitter.write_snapshot({"state": "running"}))
            leftovers = [n for n in os.listdir(os.path.join(tmp, "sub"))
                         if n.endswith(".tmp")]
            self.assertEqual(leftovers, [])
            self.assertEqual(json.load(open(health, encoding="utf-8"))["state"], "running")

    def test_health_failure_warns_once_and_continues(self):
        emitter = HealthEmitter(service_pid=111, parent_pid=222,
                                health_file=os.path.join("\\\\nonexistent-host\\share", "h.json"))
        seen = []
        emitter.emit = lambda t, f=None: seen.append((t, f or {}))
        with patch("liveaudio.service.health.tempfile.mkstemp", side_effect=OSError("disk gone")):
            self.assertFalse(emitter.write_snapshot({"state": "running"}))
            self.assertFalse(emitter.write_snapshot({"state": "running"}))
        warnings = [e for e in seen if e[0] == "warning"]
        self.assertEqual(len(warnings), 1)
        self.assertEqual(warnings[0][1]["code"], "health-file-unwritable")

    def test_stdout_event_schema(self):
        buf = io.StringIO()
        emitter = HealthEmitter(service_pid=111, parent_pid=222)
        with redirect_stdout(buf):
            emitter.emit("service_state", {"state": "running", "base_port": 8765})
        line = json.loads(buf.getvalue().strip())
        self.assertEqual(line["schema"], "liveaudio.service.event")
        self.assertEqual(line["version"], 1)
        self.assertEqual(line["service_pid"], 111)
        self.assertEqual(line["parent_pid"], 222)
        self.assertEqual(line["type"], "service_state")

    def test_second_live_service_is_rejected(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            first = InstanceLock(home=tmp)
            first.acquire()
            try:
                second = InstanceLock(home=tmp, checker=lambda pid: True)
                with self.assertRaises(ServiceError) as ctx:
                    second.acquire()
                self.assertEqual(ctx.exception.code, "service-already-running")
            finally:
                first.release()

    def test_stale_lock_is_reclaimed(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            with open(os.path.join(tmp, "service.lock"), "w", encoding="utf-8") as f:
                json.dump({"pid": 987654321}, f)
            lock = InstanceLock(home=tmp, checker=lambda pid: False)
            lock.acquire()  # stale holder -> reclaimed, no raise
            lock._handle.seek(0)
            self.assertEqual(json.loads(lock._handle.read().decode("utf-8"))["pid"], os.getpid())
            lock.release()
            self.assertTrue(os.path.exists(os.path.join(tmp, "service.lock")))

    def test_lock_release_is_idempotent(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            lock = InstanceLock(home=tmp)
            lock.acquire()
            lock.release()
            lock.release()

    def test_acquire_creates_home_on_fresh_install(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            fresh_home = os.path.join(tmp, "nested", "LIVEAUDIO_HOME")
            self.assertFalse(os.path.exists(fresh_home))
            lock = InstanceLock(home=fresh_home)
            lock.acquire()
            try:
                self.assertTrue(os.path.isfile(os.path.join(fresh_home, "service.lock")))
            finally:
                lock.release()

    def test_acquire_maps_makedirs_failure_to_sanitized_error(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            lock = InstanceLock(home=os.path.join(tmp, "home"))
            with patch("liveaudio.service.lock.os.makedirs",
                       side_effect=PermissionError("denied")):
                with self.assertRaises(ServiceError) as ctx:
                    lock.acquire()
            self.assertEqual(ctx.exception.code, "service-lock-unwritable")
            self.assertNotIn(tmp, str(ctx.exception))

    def test_acquire_maps_open_failure_to_sanitized_error(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            lock = InstanceLock(home=tmp)
            with patch("liveaudio.service.lock.os.open",
                       side_effect=PermissionError("denied")):
                with self.assertRaises(ServiceError) as ctx:
                    lock.acquire()
            self.assertEqual(ctx.exception.code, "service-lock-unwritable")
            self.assertNotIn(tmp, str(ctx.exception))


class TestNativeServiceLockSafety(unittest.TestCase):
    def test_contender_does_not_write_during_owner_metadata_truncation(self):
        import tempfile

        context = mp.get_context("spawn")
        with tempfile.TemporaryDirectory() as home:
            truncated, resume = context.Event(), context.Event()
            results = context.Queue()
            owner = context.Process(target=_hold_service_lock_during_metadata_write,
                                    args=(home, truncated, resume))
            contender = context.Process(target=_try_service_lock_once, args=(home, results))
            processes = [owner, contender]
            owner.start()
            try:
                self.assertTrue(truncated.wait(10), "owner never reached metadata truncation")
                contender.start()
                self.assertEqual(results.get(timeout=10), ("rejected", "service-already-running"))
                self.assertEqual(os.path.getsize(os.path.join(home, "service.lock")), 0)
                resume.set()
                for process in processes:
                    process.join(5)
                    self.assertFalse(process.is_alive())
                    self.assertEqual(process.exitcode, 0)
            finally:
                resume.set()
                for process in processes:
                    if process.pid is not None:
                        process.join(5)
                        if process.is_alive():
                            process.terminate()
                            process.join(5)
                results.close()
                results.join_thread()

    def _race(self, home):
        context = mp.get_context("spawn")
        barrier = context.Barrier(2)
        release_event = context.Event()
        results = context.Queue()
        processes = [context.Process(target=_race_service_lock,
                                     args=(home, barrier, release_event, results))
                     for _ in range(2)]
        for process in processes:
            process.start()
        try:
            outcomes = [results.get(timeout=15), results.get(timeout=15)]
            self.assertCountEqual([outcome[0] for outcome in outcomes], ["acquired", "rejected"])
            self.assertEqual(next(outcome[1] for outcome in outcomes if outcome[0] == "rejected"),
                             "service-already-running")
            release_event.set()
            for process in processes:
                process.join(timeout=10)
                self.assertFalse(process.is_alive())
                self.assertEqual(process.exitcode, 0)
            return next(outcome[1] for outcome in outcomes if outcome[0] == "acquired")
        finally:
            release_event.set()
            for process in processes:
                if process.is_alive():
                    process.terminate()
                    process.join(timeout=5)
            results.close()
            results.join_thread()

    def test_simultaneous_fresh_acquisitions_have_one_owner(self):
        import tempfile

        with tempfile.TemporaryDirectory() as home:
            winner_pid = self._race(home)
            with open(os.path.join(home, "service.lock"), encoding="utf-8") as handle:
                self.assertEqual(json.load(handle)["pid"], winner_pid)

    def test_simultaneous_stale_reclaims_have_one_owner(self):
        import tempfile

        with tempfile.TemporaryDirectory() as home:
            with open(os.path.join(home, "service.lock"), "w", encoding="utf-8") as handle:
                json.dump({"pid": 987654321}, handle)
            winner_pid = self._race(home)
            with open(os.path.join(home, "service.lock"), encoding="utf-8") as handle:
                self.assertEqual(json.load(handle)["pid"], winner_pid)

    def test_incomplete_pid_metadata_does_not_allow_reclaim_or_release_delete(self):
        import tempfile
        import threading

        with tempfile.TemporaryDirectory() as home:
            owner = InstanceLock(home=home)
            contender = InstanceLock(home=home, checker=lambda _pid: False)
            publish_started = threading.Event()
            publish_continue = threading.Event()
            from liveaudio.service.lock import _write_pid
            real_write_pid = _write_pid
            calls = 0
            owner_errors = []

            def pause_first_publication(handle, pid):
                nonlocal calls
                calls += 1
                if calls == 1:
                    publish_started.set()
                    if not publish_continue.wait(timeout=5):
                        raise TimeoutError("test did not release PID publication")
                real_write_pid(handle, pid)

            with patch("liveaudio.service.lock._write_pid", side_effect=pause_first_publication):
                thread = threading.Thread(target=lambda: self._capture_error(owner.acquire, owner_errors))
                thread.start()
                try:
                    self.assertTrue(publish_started.wait(timeout=5))
                    with self.assertRaises(ServiceError) as ctx:
                        contender.acquire()
                    self.assertEqual(ctx.exception.code, "service-already-running")
                finally:
                    publish_continue.set()
                    thread.join(timeout=5)
                    if contender._owns:
                        contender.release()
                    if owner._owns:
                        owner.release()
                    self.assertFalse(thread.is_alive())

            self.assertEqual(owner_errors, [])
            self.assertTrue(os.path.exists(owner.path))

            reclaimed = InstanceLock(home=home)
            reclaimed.acquire()
            try:
                reclaimed._handle.seek(0)
                self.assertEqual(json.loads(reclaimed._handle.read().decode("utf-8"))["pid"], os.getpid())
            finally:
                reclaimed.release()

    def test_released_owner_cannot_delete_successor_lock(self):
        import tempfile

        with tempfile.TemporaryDirectory() as home:
            previous = InstanceLock(home=home)
            previous.acquire()
            previous.release()

            current = InstanceLock(home=home)
            current.acquire()
            try:
                previous.release()
                contender = InstanceLock(home=home)
                with self.assertRaises(ServiceError) as ctx:
                    contender.acquire()
                self.assertEqual(ctx.exception.code, "service-already-running")
            finally:
                current.release()

    @staticmethod
    def _capture_error(operation, errors):
        try:
            operation()
        except Exception as exc:
            errors.append(exc)

    def test_process_crash_releases_kernel_lock_for_stale_metadata(self):
        import tempfile

        context = mp.get_context("spawn")
        with tempfile.TemporaryDirectory() as home:
            acquired = context.Event()
            process = context.Process(target=_hold_service_lock_until_terminated,
                                      args=(home, acquired))
            process.start()
            try:
                self.assertTrue(acquired.wait(timeout=15))
                process.terminate()
                process.join(timeout=10)
                self.assertFalse(process.is_alive())

                successor = InstanceLock(home=home)
                successor.acquire()
                try:
                    successor._handle.seek(0)
                    self.assertEqual(json.loads(successor._handle.read().decode("utf-8"))["pid"], os.getpid())
                finally:
                    successor.release()
            finally:
                if process.is_alive():
                    process.terminate()
                    process.join(timeout=5)

    def test_same_process_contender_does_not_open_fd_or_release_owner_lock(self):
        import tempfile

        context = mp.get_context("spawn")
        with tempfile.TemporaryDirectory() as home:
            owner = InstanceLock(home=home)
            owner.acquire()
            contender = InstanceLock(home=home)
            from liveaudio.service import lock as lock_module
            real_open = lock_module.os.open
            opened = []

            def track_open(*args, **kwargs):
                opened.append(args[0])
                return real_open(*args, **kwargs)

            try:
                with patch("liveaudio.service.lock.os.open", side_effect=track_open):
                    with self.assertRaises(ServiceError) as ctx:
                        contender.acquire()
                self.assertEqual(ctx.exception.code, "service-already-running")
                self.assertEqual(opened, [], "same-process rejection must precede os.open")

                results = context.Queue()
                process = context.Process(target=_try_service_lock_once, args=(home, results))
                process.start()
                try:
                    self.assertEqual(results.get(timeout=15),
                                     ("rejected", "service-already-running"))
                    process.join(timeout=10)
                    self.assertFalse(process.is_alive())
                    self.assertEqual(process.exitcode, 0)
                finally:
                    if process.is_alive():
                        process.terminate()
                        process.join(timeout=5)
                    results.close()
                    results.join_thread()
            finally:
                owner.release()


class TestResourceHygiene(unittest.TestCase):
    def test_stop_process_kill_fallback_when_terminate_insufficient(self):
        from liveaudio.service.supervisor import ProcessSupervisor
        proc = FakeProcess()
        proc.terminate_kills = False  # stubborn child: terminate() doesn't kill
        ProcessSupervisor._stop_process(proc, timeout=0)
        self.assertEqual(proc.terminate_calls, 1)
        self.assertEqual(proc.kill_calls, 1)
        self.assertFalse(proc.is_alive())

    def test_stop_process_skips_kill_when_terminate_suffices(self):
        from liveaudio.service.supervisor import ProcessSupervisor
        proc = FakeProcess()
        ProcessSupervisor._stop_process(proc, timeout=0)
        self.assertEqual(proc.terminate_calls, 1)
        self.assertEqual(proc.kill_calls, 0)

    def test_shutdown_closes_queues(self):
        import tempfile
        created = []

        def factory():
            q = FakeQueue()
            created.append(q)
            return q

        with tempfile.TemporaryDirectory() as tmp:
            sup = make_supervisor(tmp, queue_factory=factory)
            start_supervisor(sup)
            sup.shutdown()
            self.assertEqual(len(created), 3)
            for q in created:
                self.assertEqual(q.close_calls, 1)
                self.assertEqual(q.cancel_join_thread_calls, 1)

    def test_respawn_closes_old_queues_before_recreating(self):
        import tempfile
        created = []

        def factory():
            q = FakeQueue()
            created.append(q)
            return q

        with tempfile.TemporaryDirectory() as tmp:
            sup = make_supervisor(tmp, queue_factory=factory)
            start_supervisor(sup)
            first_gen = list(created)
            sup.procs["ws"]._alive = False
            sup.poll_once()
            sup._clock_obj.advance(30.0)
            sup.poll_once()
            try:
                self.assertEqual(len(created), 6)
                for q in first_gen:
                    self.assertEqual(q.close_calls, 1)
                    self.assertEqual(q.cancel_join_thread_calls, 1)
            finally:
                sup.shutdown()

    def test_start_failure_after_manager_creation_shuts_manager_down(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            blocker = os.path.join(tmp, "blocker")
            with open(blocker, "w", encoding="utf-8") as f:
                f.write("x")
            sup = make_supervisor(tmp)
            sup.config["output_dir"] = blocker  # makedirs fails -> ServiceError
            manager = FakeManager()
            sup._manager_factory = lambda: manager
            with patch("liveaudio.core.network.port_range_available", return_value=True):
                with self.assertRaises(ServiceError) as ctx:
                    sup.start()
            self.assertEqual(ctx.exception.code, "session-dir-unwritable")
            self.assertEqual(manager.shutdown_calls, 1)
            self.assertIsNone(sup.manager)


class TestServiceEntryPoints(unittest.TestCase):
    """Packaging contract: console headless entry + gui entry, pinned from pyproject."""

    def _pyproject(self):
        import tomllib
        root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        with open(os.path.join(root, "pyproject.toml"), "rb") as f:
            return tomllib.load(f)

    def test_headless_console_script_declared(self):
        data = self._pyproject()
        scripts = data["project"]["scripts"]
        self.assertEqual(scripts["liveaudio-service"], "liveaudio.service:main")

    def test_gui_script_uses_dispatcher(self):
        data = self._pyproject()
        gui_scripts = data["project"]["gui-scripts"]
        self.assertEqual(gui_scripts["liveaudio"], "liveaudio.cli:main")

    def test_declared_entries_resolve(self):
        from liveaudio.service import main as service_main
        from liveaudio.cli import main as cli_main
        self.assertTrue(callable(service_main))
        self.assertTrue(callable(cli_main))


class TestModuleHygiene(unittest.TestCase):
    def test_service_and_cli_import_without_heavy_deps(self):
        import subprocess
        code = (
            "import sys; "
            "import liveaudio.service, liveaudio.cli; "
            "leaked = [m for m in ('torch', 'faster_whisper', 'ctranslate2', "
            "'customtkinter', 'tkinter') if m in sys.modules]; "
            "print('LEAKED:' + ','.join(leaked) if leaked else 'CLEAN')"
        )
        result = subprocess.run([sys.executable, "-c", code], capture_output=True,
                                text=True, cwd=os.path.dirname(os.path.dirname(
                                    os.path.abspath(__file__))))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("CLEAN", result.stdout)


if __name__ == "__main__":
    unittest.main()
