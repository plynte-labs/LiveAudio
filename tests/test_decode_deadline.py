# SPDX-License-Identifier: MIT
"""Offline process-boundary tests for lazy ASR decode deadlines."""

import multiprocessing as mp
import queue
import threading
import time
import unittest
from types import SimpleNamespace
from unittest.mock import MagicMock, patch


def _block_while_advancing_lazy_segments(shared, entered):
    """Model a native decoder that blocks only when its iterator is advanced."""
    from liveaudio.core.engine import _transcribe_with_timeout

    attempt = int(shared["asr_attempt"])
    started = time.monotonic()
    marker = {
        "status": "decoding",
        "attempt": attempt,
        "utterance_id": "test-utterance-1",
        "started_monotonic": started,
        "deadline_monotonic": started + 0.05,
    }
    shared["asr_decode"] = marker

    class FakeModel:
        def transcribe(self, _audio, **_kwargs):
            def segments():
                entered.set()
                threading.Event().wait(60)
                yield SimpleNamespace(text="never emitted", no_speech_prob=0.0)

            return segments(), SimpleNamespace()

    decoded, _info = _transcribe_with_timeout(
        FakeModel(), b"cpu-only-test", timeout_sec=0.05,
    )
    # Current implementation returns a lazy iterator; the corrected helper
    # consumes it internally. Keep this explicit for both versions.
    list(decoded)
    shared["asr_decode"] = None
    shared["test_transcript_emitted"] = True


class TestDecodeDeadline(unittest.TestCase):
    def _start_blocked_child(self, shared):
        entered = mp.Event()
        proc = mp.Process(target=_block_while_advancing_lazy_segments,
                          args=(shared, entered), daemon=True)
        proc.start()
        self.assertTrue(entered.wait(timeout=20), "fake decoder never entered lazy iteration")
        time.sleep(0.08)  # injected test deadline: 50 ms, not production 15 s
        return proc

    @staticmethod
    def _live_asr_process_app(shared, proc):
        from liveaudio.app import LiveASRApp

        class InertProcess:
            def is_alive(self):
                return False

        app = LiveASRApp.__new__(LiveASRApp)
        app.is_running = True
        app._ui_ready = False
        app._asr_attempt = int(shared["asr_attempt"])
        app._shared_config = shared
        app._writer_failure_handled_code = None
        app._decode_timeout_handled_attempt = None
        app._shutdown_loss_counters = {}
        app._asr_decode_timeout_sec = 0.05
        app.log_queue = queue.Queue()
        app.audio_queue = queue.Queue()
        app.text_queue = queue.Queue()
        app.after = MagicMock()
        app.after_cancel = MagicMock()
        app.btn_power = MagicMock()
        app.consola = MagicMock()
        app.print_log = MagicMock()
        app.set_status = MagicMock()
        app.update_session_label = MagicMock()
        app.current_session_dir = None
        app._ws_health_after_id = None
        app.p_audio = InertProcess()
        app.p_ia = proc
        app.p_ws = InertProcess()
        return app

    def test_gui_deadline_kills_and_reaps_child_blocked_in_lazy_iteration(self):
        manager = mp.Manager()
        shared = manager.dict({"asr_attempt": 7})
        proc = None
        try:
            proc = self._start_blocked_child(shared)
            app = self._live_asr_process_app(shared, proc)
            app.handle_event = lambda event: LiveASRApp.handle_event(app, event)
            app.process_logs = lambda: LiveASRApp.process_logs(app)
            from liveaudio.app import LiveASRApp

            started = time.monotonic()
            with patch("liveaudio.app.t", side_effect=lambda key, **kwargs: key), \
                 patch("liveaudio.app.messagebox.showwarning") as alert:
                LiveASRApp.process_logs(app)

            self.assertFalse(proc.is_alive())
            self.assertIsNotNone(proc.exitcode)
            self.assertLess(time.monotonic() - started, 6.0)
            self.assertFalse(app.is_running)
            self.assertEqual(app._shutdown_loss_counters["asr.decode_interrupted"], 1)
            alert.assert_called_once_with("decode_timeout_title", "decode_timeout_msg")
            self.assertFalse(shared.get("test_transcript_emitted", False))
            self.assertEqual(shared.get("asr_decode")["utterance_id"], "test-utterance-1")
        finally:
            if proc is not None and proc.is_alive():
                proc.terminate()
                proc.join(timeout=2)
                if proc.is_alive():
                    proc.kill()
                    proc.join(timeout=2)
            manager.shutdown()

    def test_service_deadline_fails_closed_and_reaps_blocked_child_without_restart(self):
        from tempfile import TemporaryDirectory
        from liveaudio.service.errors import ServiceError
        from tests.test_resilience_service_backend import FakeProcess, make_supervisor

        manager = mp.Manager()
        shared = manager.dict({"asr_attempt": 9})
        proc = None
        try:
            proc = self._start_blocked_child(shared)
            with TemporaryDirectory() as temp_dir:
                supervisor = make_supervisor(temp_dir, prewarm=False)
                supervisor.shared = shared
                supervisor.audio_queue = queue.Queue()
                supervisor.text_queue = queue.Queue()
                supervisor.log_queue = queue.Queue()
                supervisor.audio_started = True
                supervisor.asr_attempt = 9
                supervisor.procs = {
                    "ws": FakeProcess(alive=False),
                    "audio": FakeProcess(alive=False),
                    "asr": proc,
                }
                supervisor.start = lambda: None
                supervisor._clock = time.monotonic
                original_poll_once = supervisor.poll_once

                def stop_if_deadline_was_not_detected():
                    original_poll_once()
                    raise ServiceError("test-stop")

                supervisor.poll_once = stop_if_deadline_was_not_detected

                started = time.monotonic()
                result = supervisor.run()

                fatal = [event for event in supervisor._captured_events if event[0] == "fatal"]
                self.assertEqual(result, 1)
                self.assertEqual([event[1]["code"] for event in fatal], ["asr-decode-timeout"])
                warning = [event for event in supervisor._captured_events if event[0] == "warning"]
                self.assertEqual(warning[0][1]["code"], "asr-decode-timeout")
                self.assertEqual(warning[0][1]["utterance_id"], "test-utterance-1")
                self.assertFalse(proc.is_alive())
                self.assertIsNotNone(proc.exitcode)
                self.assertLess(time.monotonic() - started, 6.0)
                self.assertIsNone(supervisor._pending_restart_at)
                self.assertEqual(supervisor.loss_counters["asr.decode_interrupted"], 1)
                self.assertFalse(shared.get("test_transcript_emitted", False))
        finally:
            if proc is not None and proc.is_alive():
                proc.terminate()
                proc.join(timeout=2)
                if proc.is_alive():
                    proc.kill()
                    proc.join(timeout=2)
            try:
                manager.shutdown()
            except Exception:
                pass

    def test_idle_decode_marker_does_not_trigger_timeout(self):
        from tempfile import TemporaryDirectory
        from tests.test_resilience_service_backend import make_supervisor

        with TemporaryDirectory() as temp_dir:
            supervisor = make_supervisor(temp_dir, prewarm=False)
            supervisor.shared = {
                "asr_decode": {
                    "status": "loading",
                    "attempt": 1,
                    "started_monotonic": time.monotonic() - 100,
                    "deadline_monotonic": time.monotonic() - 99,
                },
            }
            supervisor.asr_attempt = 1
            supervisor.audio_queue = queue.Queue()
            supervisor.text_queue = queue.Queue()
            supervisor.log_queue = queue.Queue()
            supervisor.poll_once()
            self.assertFalse(any(event[0] == "fatal" for event in supervisor._captured_events))

    def test_stale_attempt_decode_marker_does_not_timeout_current_service(self):
        from tempfile import TemporaryDirectory
        from tests.test_resilience_service_backend import make_supervisor

        with TemporaryDirectory() as temp_dir:
            supervisor = make_supervisor(temp_dir, prewarm=False)
            supervisor.shared = {
                "asr_decode": {
                    "status": "decoding",
                    "attempt": 8,
                    "utterance_id": "stale-utterance",
                    "started_monotonic": time.monotonic() - 100,
                    "deadline_monotonic": time.monotonic() - 99,
                },
            }
            supervisor.asr_attempt = 9
            supervisor.audio_queue = queue.Queue()
            supervisor.text_queue = queue.Queue()
            supervisor.log_queue = queue.Queue()
            supervisor.poll_once()
            self.assertFalse(any(event[0] == "fatal" for event in supervisor._captured_events))


if __name__ == "__main__":
    unittest.main()
