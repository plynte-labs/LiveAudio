# SPDX-License-Identifier: MIT
"""Tests for local diagnostics helpers."""

import unittest

from core.diagnostics import (
    REDACTED,
    DiagnosticsStore,
    build_diagnostics_report,
    create_store_from_config,
    diagnostics_level,
    should_collect_deep,
)


class TestDiagnosticsLevelGating(unittest.TestCase):
    def test_disabled_config_forces_off_level(self):
        config = {"diagnostics_enabled": False, "diagnostics_level": "deep"}
        self.assertEqual(diagnostics_level(config), "off")
        self.assertFalse(should_collect_deep(config))

    def test_deep_mode_requires_enabled_flag(self):
        config = {"diagnostics_enabled": True, "diagnostics_level": "deep"}
        self.assertEqual(diagnostics_level(config), "deep")
        self.assertTrue(should_collect_deep(config))

    def test_store_created_from_config_respects_level(self):
        store = create_store_from_config({"diagnostics_enabled": True, "diagnostics_level": "minimal"})
        self.assertEqual(store.level, "minimal")


class TestDiagnosticsSanitization(unittest.TestCase):
    def test_report_redacts_sensitive_values(self):
        report = build_diagnostics_report(
            runtime_health={"secret_token": "abc", "path": r"C:\Users\tavo_\secret.txt"},
            test_health={"private_url": "https://internal.example.local/path"},
        )
        self.assertEqual(report["runtime"]["secret_token"], REDACTED)
        self.assertEqual(report["runtime"]["path"], REDACTED)
        self.assertEqual(report["test_health"]["private_url"], REDACTED)

    def test_test_health_redacts_transcript_and_audio_payloads(self):
        store = DiagnosticsStore(level="deep")
        snapshot = store.snapshot_test_health(
            resource_summary={"transcript_preview": "hola", "audio_payload": b"123"},
            warnings=["ok"],
        )
        self.assertEqual(snapshot["resource_summary"]["transcript_preview"], REDACTED)
        self.assertEqual(snapshot["resource_summary"]["audio_payload"], REDACTED)


class TestDiagnosticsSchema(unittest.TestCase):
    def test_runtime_snapshot_contains_expected_sections(self):
        store = DiagnosticsStore(level="minimal")
        store.record_counter("queue.depth", delta=2)
        store.record_duration("engine.asr", 0.25)
        store.record_state("ws.clients", 3)

        snapshot = store.snapshot_runtime_health()

        self.assertEqual(snapshot["schema_version"], 1)
        self.assertEqual(snapshot["kind"], "runtime")
        self.assertIn("counters", snapshot)
        self.assertIn("durations", snapshot)
        self.assertIn("states", snapshot)
        self.assertEqual(snapshot["counters"]["queue.depth"], 2)

    def test_off_mode_drops_runtime_collection(self):
        store = DiagnosticsStore(level="off")
        store.record_counter("queue.depth")
        store.record_duration("engine.asr", 1.0)
        store.record_state("ws.clients", 3)
        snapshot = store.snapshot_runtime_health()
        self.assertEqual(snapshot["counters"], {})
        self.assertEqual(snapshot["durations"], {})
        self.assertEqual(snapshot["states"], {})

    def test_minimal_test_snapshot_hides_thread_names(self):
        store = DiagnosticsStore(level="minimal")
        snapshot = store.snapshot_test_health(resource_summary={"thread_names": ["worker-1"], "alive_threads": 1})
        self.assertNotIn("thread_names", snapshot["resource_summary"])
        self.assertEqual(snapshot["resource_summary"]["alive_threads"], 1)


if __name__ == "__main__":
    unittest.main()
