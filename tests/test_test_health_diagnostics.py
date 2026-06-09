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
        from main import build_app_runtime_summary

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

    def test_export_local_diagnostics_report_writes_json_locally(self):
        from main import export_local_diagnostics_report

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
