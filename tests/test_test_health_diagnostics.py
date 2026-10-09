# SPDX-License-Identifier: MIT
"""Tests for local test-health diagnostics helpers and export entrypoints."""

import json
import tempfile
import threading
import unittest

from tests.helpers import build_test_health_snapshot, summarize_test_resources


class _FakeProcess:
    def __init__(self, alive, name="proc"):
        self._alive = alive
        self.name = name

    def is_alive(self):
        return self._alive


class _FakeQueue:
    pass


class TestTestHealthHelpers(unittest.TestCase):
    def test_resource_summary_counts_alive_resources(self):
        alive_thread = threading.Thread(target=lambda: None, name="alive-thread")
        summary = summarize_test_resources(
            processes=[_FakeProcess(True, name="worker-a"), _FakeProcess(False, name="worker-b")],
            threads=[alive_thread],
            queues=[_FakeQueue(), _FakeQueue()],
        )

        self.assertEqual(summary["alive_processes"], 1)
        self.assertEqual(summary["queue_count"], 2)
        self.assertIn("alive-thread", summary["thread_names"])

    def test_build_test_health_snapshot_keeps_local_summary(self):
        snapshot = build_test_health_snapshot(
            {"diagnostics_enabled": True, "diagnostics_level": "deep"},
            processes=[_FakeProcess(True, name="worker-a")],
            queues=[_FakeQueue()],
            file_timings={"tests/test_audio.py": 2.3},
            warnings=["teardown warning"],
        )

        self.assertEqual(snapshot["kind"], "test-health")
        self.assertEqual(snapshot["resource_summary"]["alive_processes"], 1)
        self.assertEqual(snapshot["file_timings"]["tests/test_audio.py"], 2.3)


class TestMainDiagnosticsExports(unittest.TestCase):
    def test_build_app_runtime_summary_includes_process_and_queue_state(self):
        from liveaudio.app import build_app_runtime_summary

        summary = build_app_runtime_summary(
            {
                "is_running": True,
                "statuses": {"audio": "ok"},
                "processes": {"audio": _FakeProcess(True), "ws": _FakeProcess(False)},
                "queues": {"audio": 3, "text": 1},
                "session_dir": "sessions\\session_1",
            }
        )

        self.assertTrue(summary["is_running"])
        self.assertEqual(summary["queues"]["audio"], 3)
        self.assertTrue(summary["processes"]["audio"])

    def test_build_app_runtime_summary_includes_fixed_shutdown_loss_counters(self):
        from liveaudio.app import build_app_runtime_summary

        summary = build_app_runtime_summary({
            "loss_counters": {"audio.shutdown_discarded": 2, "asr.text_shutdown_discarded": 1},
        })

        self.assertEqual(summary["loss_counters"], {
            "audio.shutdown_discarded": 2, "asr.text_shutdown_discarded": 1,
        })

    def test_gui_shutdown_counts_discarded_queue_items_and_rejected_stop_control(self):
        import queue
        from unittest.mock import MagicMock
        from liveaudio.app import LiveASRApp

        app = LiveASRApp.__new__(LiveASRApp)
        app._shutdown_loss_counters = {}
        app.print_log = MagicMock()
        audio_queue = queue.Queue()
        audio_queue.put_nowait({"audio": [1]})
        audio_queue.put_nowait(None)

        app.print_log.reset_mock()
        self.assertEqual(app._drain_queue(audio_queue, "audio"), 1)
        self.assertIn("outcome is unknown", app.print_log.call_args[0][0])
        full_queue = queue.Queue(maxsize=1)
        full_queue.put_nowait({"text": "private"})
        app._signal_stop(full_queue, "text")

        self.assertEqual(app._shutdown_loss_counters["audio.shutdown_discarded"], 1)
        self.assertEqual(app._shutdown_loss_counters["asr.text_stop_control_rejected"], 1)
        self.assertIn("pending items may be discarded", app.print_log.call_args[0][0])

    def test_export_local_diagnostics_report_writes_json_locally(self):
        from liveaudio.app import export_local_diagnostics_report

        with tempfile.TemporaryDirectory() as temp_dir:
            path = export_local_diagnostics_report(
                {"runtime": {"ws": 1}, "test_health": {"alive_processes": 0}},
                export_dir=temp_dir,
            )
            with open(path, "r", encoding="utf-8") as handle:
                payload = json.load(handle)

        self.assertTrue(path.endswith(".json"))
        self.assertIn("generated_at", payload)
        self.assertIn("runtime", payload)
