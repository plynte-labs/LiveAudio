# SPDX-License-Identifier: MIT
"""Tests for ASR freeze recovery and resilience (REQ-1)."""

import unittest
import time
import multiprocessing as mp
from unittest.mock import MagicMock, patch, call

from tests.helpers import MockQueue, make_shared_config, make_mock_transcribe_result


class TestAsrTimeoutRecovery(unittest.TestCase):
    """Tests for full decode consumption and error reporting."""

    def test_transcribe_materializes_the_complete_lazy_segment_iterator(self):
        from liveaudio.core.engine import _transcribe_with_timeout
        log_queue = MockQueue()
        segment = object()
        yielded = []

        def lazy_segments():
            yielded.append("started")
            yield segment
            yielded.append("finished")

        mock_model = MagicMock()
        mock_model.transcribe.return_value = (lazy_segments(), "info")
        segments, info = _transcribe_with_timeout(
            mock_model, b"audio", timeout_sec=15.0, log_queue=log_queue,
        )

        self.assertEqual(segments, [segment])
        self.assertEqual(info, "info")
        self.assertEqual(yielded, ["started", "finished"])

    def test_capture_metadata_and_stage_timings_stay_internal_to_jsonl_and_ws_v1(self):
        import json
        import os
        import queue
        import tempfile
        from types import SimpleNamespace
        from liveaudio.core.diagnostics import DiagnosticsStore
        from liveaudio.core.engine import asr_consumer
        from tests.helpers import make_shared_config

        with tempfile.TemporaryDirectory() as session_dir:
            audio_queue = queue.Queue()
            text_queue = queue.Queue()
            log_queue = queue.Queue()
            audio_queue.put({
                "audio": [], "created_at": time.time() - 0.1, "sequence": 7,
                "attempt": 4,
                "capture_started_monotonic": 100.0,
                "capture_completed_monotonic": 100.2,
            })
            audio_queue.put(None)
            shared = make_shared_config({"asr_attempt": 4, "blacklist": ""})
            diagnostics = DiagnosticsStore(level="deep")

            class FakeModel:
                def transcribe(self, _audio, **_kwargs):
                    return iter([SimpleNamespace(text="safe text", no_speech_prob=0.0)]), SimpleNamespace()

            with patch("liveaudio.core.provisioning.prepare_model", return_value="fake-model"), \
                 patch("liveaudio.core.engine.WhisperModel", return_value=FakeModel()):
                asr_consumer(audio_queue, text_queue, log_queue, shared, session_dir, diagnostics)

            payload = text_queue.get_nowait()
            transcript = json.loads(open(os.path.join(session_dir, "transcript.jsonl"), encoding="utf-8").readline())
            snapshot = diagnostics.snapshot_runtime_health()

        self.assertEqual(transcript["sequence"], 7)
        self.assertEqual(set(transcript), {
            "id", "sequence", "text", "created_at", "processed_at", "queue_delay",
            "latency", "total_delay", "model", "device",
            "capture_offset_start_sec", "capture_offset_end_sec", "audio_duration_sec", "config_snapshot",
        })
        for key in ("capture_offset_start_sec", "capture_offset_end_sec", "audio_duration_sec", "config_snapshot"):
            self.assertNotIn(key, payload)
        self.assertEqual(payload["_telemetry"]["sequence"], 7)
        self.assertEqual(payload["_telemetry"]["attempt"], 4)
        self.assertEqual(payload["_telemetry"]["capture_started_monotonic"], 100.0)
        self.assertEqual(payload["_telemetry"]["capture_completed_monotonic"], 100.2)
        self.assertIn("asr.utterance_formation_sec", snapshot["durations"])
        self.assertIn("asr.queue_wait_sec", snapshot["durations"])
        self.assertIn("asr.decode_sec", snapshot["durations"])
        self.assertIn("asr.jsonl_write_sec", snapshot["durations"])
        self.assertIn("asr.jsonl_capture_to_write_sec", snapshot["durations"])
        self.assertIn("asr.jsonl_saved", snapshot["counters"])
        self.assertNotIn("safe text", json.dumps(snapshot))

    @patch("liveaudio.core.engine.torch")
    def test_cuda_empty_cache_called_after_transcribe(self, mock_torch):
        """torch.cuda.empty_cache() should be called after transcribe on CUDA."""
        from liveaudio.core.engine import _transcribe_with_timeout
        log_queue = MockQueue()

        mock_model = MagicMock()
        mock_model.transcribe = MagicMock(return_value=make_mock_transcribe_result())

        _transcribe_with_timeout(mock_model, b"audio", timeout_sec=15.0, log_queue=log_queue, device="cuda")

        mock_torch.cuda.empty_cache.assert_called_once()

    def test_structured_error_event_sent_on_failure(self):
        """Structured error event should be sent to log_queue on failure."""
        from liveaudio.core.engine import _transcribe_with_timeout
        log_queue = MockQueue()

        mock_model = MagicMock()
        mock_model.transcribe = MagicMock(side_effect=RuntimeError("GPU OOM"))

        _transcribe_with_timeout(mock_model, b"audio", timeout_sec=15.0, log_queue=log_queue)

        error_items = [item for item in log_queue.items if item.get("type") == "error"]
        self.assertTrue(len(error_items) > 0)
        self.assertEqual(error_items[0]["key"], "asr_exception")
        self.assertEqual(error_items[0]["exception_type"], "RuntimeError")


class TestAsrErrorHandling(unittest.TestCase):
    """Tests for ASR error handling and structured events."""

    def test_error_event_contains_exception_type(self):
        """Error event should include exception type for debugging."""
        from liveaudio.core.engine import _transcribe_with_timeout
        log_queue = MockQueue()

        mock_model = MagicMock()
        mock_model.transcribe = MagicMock(side_effect=ValueError("test error"))

        _transcribe_with_timeout(mock_model, b"audio", timeout_sec=15.0, log_queue=log_queue)

        error_items = [item for item in log_queue.items if item.get("type") == "error"]
        self.assertTrue(any(e.get("exception_type") == "ValueError" for e in error_items))

    def test_error_event_contains_traceback_summary(self):
        """Error event should include a summary of the traceback."""
        from liveaudio.core.engine import _transcribe_with_timeout
        log_queue = MockQueue()

        mock_model = MagicMock()
        mock_model.transcribe = MagicMock(side_effect=RuntimeError("test"))

        _transcribe_with_timeout(mock_model, b"audio", timeout_sec=15.0, log_queue=log_queue)

        error_items = [item for item in log_queue.items if item.get("type") == "error"]
        self.assertTrue(any("traceback_summary" in e for e in error_items))

    def test_asr_consumer_exits_gracefully_on_fatal_error(self):
        """ASR consumer should emit final error status before exiting."""
        from liveaudio.core.engine import _transcribe_with_timeout
        log_queue = MockQueue()

        mock_model = MagicMock()
        mock_model.transcribe = MagicMock(side_effect=MemoryError("fatal"))

        _transcribe_with_timeout(mock_model, b"audio", timeout_sec=15.0, log_queue=log_queue)

        # Error status should be emitted
        status_items = [item for item in log_queue.items if item.get("type") == "status"]
        self.assertTrue(any(s.get("key") == "asr" and s.get("state") == "error" for s in status_items))


class TestWriterFailurePropagation(unittest.TestCase):
    def test_storage_write_failure_emits_transcript_free_fatal_event(self):
        import builtins
        import json
        import os
        import queue
        import tempfile
        from types import SimpleNamespace
        from liveaudio.core.engine import asr_consumer
        from tests.helpers import make_shared_config

        with tempfile.TemporaryDirectory() as session_dir:
            jsonl_path = os.path.join(session_dir, "transcript.jsonl")
            audio_queue = queue.Queue()
            text_queue = queue.Queue()
            log_queue = queue.Queue()
            audio_queue.put({"audio": [], "created_at": time.time(), "sequence": 1})
            audio_queue.put(None)
            shared = make_shared_config({
                "blacklist": "",
                "save_transcript_enabled": True,
                "save_vtt_enabled": False,
                "obs_enabled": False,
            })

            class FakeModel:
                def transcribe(self, audio, **kwargs):
                    return iter([SimpleNamespace(text="safe text", no_speech_prob=0.0)]), SimpleNamespace()

            real_open = builtins.open
            def fail_jsonl(path, *args, **kwargs):
                if path == jsonl_path:
                    raise OSError("private path and error detail")
                return real_open(path, *args, **kwargs)

            with patch("builtins.open", side_effect=fail_jsonl):
                with patch("liveaudio.core.provisioning.prepare_model", return_value="fake-model"), \
                     patch("liveaudio.core.engine.WhisperModel", return_value=FakeModel()):
                    asr_consumer(audio_queue, text_queue, log_queue, shared, session_dir)

            events = []
            while not log_queue.empty():
                events.append(log_queue.get_nowait())

        fatal = next(event for event in events if event.get("type") == "fatal")
        self.assertEqual(fatal, {
            "type": "fatal", "code": "writer_storage_error", "attempt": 1,
        })
        self.assertNotIn("private path", json.dumps(fatal))
        self.assertTrue(text_queue.empty())
        messages = [event.get("message", "") for event in events if event.get("type") == "log"]
        self.assertFalse(any("saved" in message.lower() or "guardad" in message.lower()
                             for message in messages))

    def test_full_log_queue_writer_failure_reaches_gui_and_stops_before_alert(self):
        import builtins
        import os
        import queue
        import tempfile
        from types import SimpleNamespace
        from liveaudio.app import LiveASRApp
        from liveaudio.core.engine import asr_consumer
        from tests.helpers import make_shared_config

        with tempfile.TemporaryDirectory() as session_dir:
            jsonl_path = os.path.join(session_dir, "transcript.jsonl")
            audio_queue = queue.Queue()
            text_queue = queue.Queue()
            log_queue = queue.Queue(maxsize=1)
            log_queue.put_nowait({"type": "log", "message": "occupied"})
            audio_queue.put({"audio": [], "created_at": time.time(), "sequence": 1})
            audio_queue.put(None)
            shared = make_shared_config({
                "blacklist": "", "save_transcript_enabled": True,
                "save_vtt_enabled": False, "obs_enabled": False,
                "diagnostics_enabled": False,
            })

            class FakeModel:
                def transcribe(self, audio, **kwargs):
                    return iter([SimpleNamespace(text="safe text", no_speech_prob=0.0)]), SimpleNamespace()

            real_open = builtins.open

            def fail_jsonl(path, *args, **kwargs):
                if path == jsonl_path:
                    raise OSError("private path and error detail")
                return real_open(path, *args, **kwargs)

            with patch("builtins.open", side_effect=fail_jsonl):
                with patch("liveaudio.core.provisioning.prepare_model", return_value="fake-model"), \
                     patch("liveaudio.core.engine.WhisperModel", return_value=FakeModel()):
                    asr_consumer(audio_queue, text_queue, log_queue, shared, session_dir)

            self.assertEqual(shared.get("writer_failure_code"), "writer_storage_error")
            self.assertEqual(log_queue.qsize(), 1, "the full log queue must not be the only failure route")
            stopped = []
            app = SimpleNamespace(
                log_queue=log_queue, after=MagicMock(), _shared_config=shared,
                _writer_failure_handled_code=None, _asr_attempt=1, is_running=True,
                set_status=MagicMock(), print_log=MagicMock(), set_preview=MagicMock(),
                toggle_system=MagicMock(side_effect=lambda: stopped.append("stop")),
            )
            app.handle_event = lambda event: LiveASRApp.handle_event(app, event)
            app.process_logs = lambda: LiveASRApp.process_logs(app)

            def alert_after_stop(*_args):
                self.assertEqual(stopped, ["stop"])

            with patch("liveaudio.app.t", side_effect=lambda key, **kwargs: key), \
                 patch("liveaudio.app.messagebox.showerror", side_effect=alert_after_stop) as alert:
                LiveASRApp.process_logs(app)

        self.assertEqual(stopped, ["stop"])
        alert.assert_called_once_with("storage_failure_title", "storage_failure_msg")

    def test_backlog_transcript_notice_does_not_claim_disk_save(self):
        import builtins
        import os
        import queue
        import tempfile
        from types import SimpleNamespace
        from liveaudio.core.engine import asr_consumer
        from tests.helpers import make_shared_config

        with tempfile.TemporaryDirectory() as session_dir:
            jsonl_path = os.path.join(session_dir, "transcript.jsonl")
            audio_queue = queue.Queue()
            text_queue = queue.Queue()
            log_queue = queue.Queue()
            audio_queue.put({"audio": [], "created_at": time.time() - 30, "sequence": 1})
            audio_queue.put(None)
            shared = make_shared_config({
                "blacklist": "", "save_transcript_enabled": True,
                "save_vtt_enabled": False, "obs_enabled": True,
                "subtitle_backlog_policy": "live_only",
                "subtitle_max_live_delay_sec": 1.0, "diagnostics_enabled": False,
            })

            class FakeModel:
                def transcribe(self, audio, **kwargs):
                    return iter([SimpleNamespace(text="safe text", no_speech_prob=0.0)]), SimpleNamespace()

            real_open = builtins.open

            def fail_jsonl(path, *args, **kwargs):
                if path == jsonl_path:
                    raise OSError("private path and error detail")
                return real_open(path, *args, **kwargs)

            with patch("builtins.open", side_effect=fail_jsonl):
                with patch("liveaudio.core.provisioning.prepare_model", return_value="fake-model"), \
                     patch("liveaudio.core.engine.WhisperModel", return_value=FakeModel()):
                    asr_consumer(audio_queue, text_queue, log_queue, shared, session_dir)

            events = []
            while not log_queue.empty():
                events.append(log_queue.get_nowait())
            app_logs = []
            app = SimpleNamespace(set_preview=MagicMock(), print_log=app_logs.append)
            from liveaudio.app import LiveASRApp
            for event in events:
                if event.get("type") == "transcript":
                    LiveASRApp.handle_event(app, event)

        messages = [event.get("message", "") for event in events if event.get("type") == "log"]
        self.assertFalse(any("saved" in message.lower() or "guardad" in message.lower()
                             for message in messages + app_logs))

    def test_gui_storage_fatal_alerts_and_uses_existing_stop_path(self):
        from types import SimpleNamespace
        from liveaudio.app import LiveASRApp

        app = SimpleNamespace(
            is_running=True,
            _asr_attempt=1,
            set_status=MagicMock(),
            print_log=MagicMock(),
            toggle_system=MagicMock(),
        )
        stop_order = []
        app.toggle_system.side_effect = lambda: stop_order.append("stop")

        def assert_stopped_before_alert(*_args):
            self.assertEqual(stop_order, ["stop"])

        with patch("liveaudio.app.t", side_effect=lambda key, **kwargs: key), \
             patch("liveaudio.app.messagebox.showerror", side_effect=assert_stopped_before_alert) as showerror:
            LiveASRApp.handle_event(app, {
                "type": "fatal", "code": "writer_storage_error", "attempt": 1,
            })

        app.set_status.assert_called_once_with("asr", "status_asr_storage_failed", "error")
        showerror.assert_called_once_with("storage_failure_title", "storage_failure_msg")
        app.toggle_system.assert_called_once_with()

    def test_explicit_gui_start_clears_session_writer_failure(self):
        import queue
        import tempfile
        from liveaudio.app import LiveASRApp

        class FakeProcess:
            def start(self):
                pass

        with tempfile.TemporaryDirectory() as output_dir:
            app = LiveASRApp.__new__(LiveASRApp)
            app.is_running = False
            app._ui_ready = False
            app._asr_attempt = 1
            app._shared_config = {
                "writer_failure_code": "writer_storage_error",
                "writer_failure_attempt": 1,
                "asr_attempt": 1,
                "ws_port": 8765,
                "output_dir": output_dir,
            }
            app._manager = None
            app.config_data = {}
            app.log_queue = queue.Queue()
            app.btn_power = MagicMock()
            app.consola = MagicMock()
            app.print_log = MagicMock()
            app.set_status = MagicMock()
            app.update_session_label = MagicMock()
            app.after = MagicMock(return_value="health-timer")
            app.hot_swap_engine = MagicMock(return_value=True)
            app._ws_health_after_id = None
            app._writer_failure_handled_code = "writer_storage_error"

            with patch("liveaudio.app.port_range_available", return_value=True), \
                 patch("liveaudio.app.mp.Process", return_value=FakeProcess()), \
                 patch("liveaudio.app.mp.Queue", side_effect=lambda **_kwargs: queue.Queue()):
                LiveASRApp.toggle_system(app)

        self.assertIsNone(app._shared_config.get("writer_failure_code"))
        self.assertIsNone(app._shared_config.get("writer_failure_attempt"))
        self.assertIsNone(app._writer_failure_handled_code)
        self.assertEqual(app._asr_attempt, 2)
        self.assertEqual(app._shared_config.get("asr_attempt"), 2)

    def test_queued_writer_fatal_from_previous_asr_attempt_is_ignored(self):
        import queue
        import tempfile
        from liveaudio.app import LiveASRApp

        class FakeProcess:
            def start(self):
                pass

        with tempfile.TemporaryDirectory() as output_dir:
            app = LiveASRApp.__new__(LiveASRApp)
            app.is_running = False
            app._ui_ready = False
            app._asr_attempt = 1
            app._shared_config = {
                "writer_failure_code": None, "writer_failure_attempt": None,
                "asr_attempt": 1, "ws_port": 8765, "output_dir": output_dir,
            }
            app._manager = None
            app.config_data = {}
            app.log_queue = queue.Queue()
            app.log_queue.put_nowait({
                "type": "fatal", "code": "writer_storage_error", "attempt": 1,
            })
            app.btn_power = MagicMock()
            app.consola = MagicMock()
            app.print_log = MagicMock()
            app.set_status = MagicMock()
            app.update_session_label = MagicMock()
            app.after = MagicMock(return_value="health-timer")
            app.hot_swap_engine = MagicMock(return_value=True)
            app._ws_health_after_id = None
            app._writer_failure_handled_code = None

            with patch("liveaudio.app.port_range_available", return_value=True), \
                 patch("liveaudio.app.mp.Process", return_value=FakeProcess()), \
                 patch("liveaudio.app.mp.Queue", side_effect=lambda **_kwargs: queue.Queue()):
                LiveASRApp.toggle_system(app)

            self.assertTrue(app.is_running)
            self.assertEqual(app._asr_attempt, 2)
            app.set_status.reset_mock()
            app.toggle_system = MagicMock()
            app.handle_event = lambda event: LiveASRApp.handle_event(app, event)
            app.process_logs = lambda: LiveASRApp.process_logs(app)

            with patch("liveaudio.app.t", side_effect=lambda key, **kwargs: key), \
                 patch("liveaudio.app.messagebox.showerror") as showerror:
                LiveASRApp.process_logs(app)

        app.toggle_system.assert_not_called()
        app.set_status.assert_not_called()
        showerror.assert_not_called()

    def test_writer_fatal_requires_allowlisted_code_and_exact_asr_attempt(self):
        import queue
        from types import SimpleNamespace
        from liveaudio.app import LiveASRApp

        rejected_events = (
            {"type": "fatal", "code": "unknown", "attempt": 2},
            {"type": "fatal", "code": "writer_storage_error"},
            {"type": "fatal", "code": "writer_storage_error", "attempt": "invalid"},
            {"type": "fatal", "code": "writer_storage_error", "attempt": 3},
            {"type": "fatal", "code": "writer_storage_error", "attempt": True},
            {"type": "fatal", "code": "writer_storage_error", "attempt": 2.0},
        )
        for event in rejected_events:
            with self.subTest(event=event):
                app = SimpleNamespace(
                    log_queue=queue.Queue(),
                    _shared_config={"writer_failure_code": None, "writer_failure_attempt": None},
                    _writer_failure_handled_code=None,
                    _asr_attempt=2,
                    is_running=True,
                    after=MagicMock(),
                    set_status=MagicMock(),
                    print_log=MagicMock(),
                    toggle_system=MagicMock(),
                )
                app.log_queue.put_nowait(event)
                app.handle_event = lambda item: LiveASRApp.handle_event(app, item)
                app.process_logs = lambda: LiveASRApp.process_logs(app)

                with patch("liveaudio.app.t", side_effect=lambda key, **kwargs: key), \
                     patch("liveaudio.app.messagebox.showerror") as showerror:
                    LiveASRApp.process_logs(app)

                app.toggle_system.assert_not_called()
                app.set_status.assert_not_called()
                showerror.assert_not_called()


if __name__ == "__main__":
    unittest.main()
