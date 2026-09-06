# SPDX-License-Identifier: MIT
"""TDD coverage for the unified first-run contract; no network, models, or devices."""
import importlib.util
import json
import os
import sys
import tempfile
import time
import unittest
from pathlib import Path

from liveaudio.core.first_run import (
    HANDOFF_FIELDS,
    PHASE_COPY,
    validate_handoff,
)
from liveaudio.core.provisioning import classify_provisioning_error
from liveaudio.utils.config import valid_language

_LAUNCHER_PATH = Path(__file__).resolve().parents[1] / "packaging" / "launcher.py"
_spec = importlib.util.spec_from_file_location("liveaudio_unified_launcher", _LAUNCHER_PATH)
launcher = importlib.util.module_from_spec(_spec)
sys.modules["liveaudio_unified_launcher"] = launcher
_spec.loader.exec_module(launcher)


class TestUnifiedFirstRunVocabulary(unittest.TestCase):
    def test_phase_copy_has_exact_order_and_bilingual_parity(self):
        self.assertEqual(tuple(PHASE_COPY), tuple(range(8)))
        for phase, copy in PHASE_COPY.items():
            self.assertEqual(set(copy), {"es", "en"}, phase)
            self.assertTrue(all(copy[language].strip() for language in ("es", "en")))

    def test_launcher_copy_uses_same_phase_vocabulary(self):
        self.assertEqual(launcher.PHASE_COPY, PHASE_COPY)

    def test_only_supported_languages_are_persistable(self):
        self.assertEqual(valid_language("es"), "es")
        self.assertEqual(valid_language("en"), "en")
        self.assertIsNone(valid_language("ES"))
        self.assertIsNone(valid_language("fr"))


class TestAtomicHandoff(unittest.TestCase):
    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="liveaudio-handoff-")
        self.cache = os.path.join(self.root, "hf-cache")
        os.makedirs(self.cache)
        self.path = os.path.join(self.root, "handoff.json")

    def tearDown(self):
        import shutil
        shutil.rmtree(self.root, ignore_errors=True)

    def test_write_is_exact_and_validates_without_logging_payload(self):
        payload = launcher.write_handoff(
            self.root, self.cache, "cpu", "1.2.3", attempt=1
        )
        self.assertEqual(tuple(payload), HANDOFF_FIELDS)
        with open(self.path, encoding="utf-8") as fh:
            stored = json.load(fh)
        self.assertEqual(stored, payload)
        self.assertEqual(
            validate_handoff(self.path, self.root, "1.2.3", min_attempt=1), payload
        )
        self.assertFalse(os.path.exists(self.path + ".tmp"))

    def test_rejects_extra_field_root_escape_and_stale_attempt(self):
        invalid = {
            "hf_home": os.path.join(self.root, "..", "outside"),
            "install_root": self.root,
            "extra": "cpu",
            "app_version": "1.2.3",
            "attempt": 0,
            "launcher_phases_done": [0, 1, 2, 3],
            "language": "en",
        }
        with open(self.path, "w", encoding="utf-8") as fh:
            json.dump(invalid, fh)
        self.assertIsNone(validate_handoff(self.path, self.root, "1.2.3", min_attempt=1))

    def test_launch_writes_handoff_before_starting_app(self):
        exe = launcher.venv_app_exe(self.root, "linux")
        os.makedirs(os.path.dirname(exe), exist_ok=True)
        Path(exe).touch()
        handoff = {
            "hf_home": self.cache,
            "install_root": self.root,
            "extra": "cpu",
            "app_version": "1.2.3",
            "attempt": 1,
            "launcher_phases_done": [0, 1, 2, 3],
        }
        with unittest.mock.patch.object(launcher, "write_handoff") as write_handoff:
            with unittest.mock.patch.object(launcher.subprocess, "Popen", return_value=object()):
                launcher.launch_app(self.root, False, platform="linux", handoff=handoff)
        write_handoff.assert_called_once_with(**handoff)


class TestVadProvisioning(unittest.TestCase):
    def test_vad_error_codes_reuse_the_provision_catalog(self):
        self.assertEqual(classify_provisioning_error(RuntimeError("TLS certificate failed")), "provision-tls")
        self.assertEqual(classify_provisioning_error(RuntimeError("network unreachable")), "provision-network")
        self.assertEqual(classify_provisioning_error(RuntimeError("invalid cache checksum")), "provision-cache-corrupt")

    def test_vad_heartbeat_is_indeterminate_and_stops(self):
        from liveaudio.core.audio import VadProvisionHeartbeat

        events = []
        heartbeat = VadProvisionHeartbeat(events.append, interval=0.01)
        heartbeat.start()
        time.sleep(0.035)
        heartbeat.stop()
        count = len(events)
        time.sleep(0.025)
        self.assertGreaterEqual(count, 2)
        self.assertEqual(len(events), count)
        self.assertTrue(all(event["phase"] == "provisioning" for event in events))
        self.assertTrue(all("percent" not in event for event in events))

    def test_audio_source_has_no_insecure_tls_override(self):
        source = (Path(__file__).resolve().parents[1] / "liveaudio" / "core" / "audio.py").read_text(encoding="utf-8")
        self.assertNotIn("_create_unverified_context", source)


class TestCacheAndProgressBoundaries(unittest.TestCase):
    def test_reinstall_target_is_app_only_not_hf_cache(self):
        with tempfile.TemporaryDirectory(prefix="liveaudio-reinstall-") as root:
            cache = os.path.join(root, "hf-cache")
            self.assertEqual(launcher.app_dir(root), os.path.join(root, "app"))
            self.assertNotEqual(launcher.app_dir(root), cache)

    def test_uv_sync_does_not_advance_heuristic_percentages(self):
        source = _LAUNCHER_PATH.read_text(encoding="utf-8")
        block = source[source.index("def run_uv_sync"):source.index("# ---------------------------------------------------------------------------\n# App launch")]
        self.assertNotIn("fraction =", block)
        self.assertNotIn("reporter.progress(fraction)", block)

    def test_bootstrap_and_window_barrier_do_not_emit_fixed_phase_percentages(self):
        source = _LAUNCHER_PATH.read_text(encoding="utf-8")
        bootstrap = source[source.index("def run_bootstrap"):source.index("def _bootstrap_headless")]
        barrier = source[source.index("def _await_app_window_gui"):source.index("def _bootstrap_gui")]
        self.assertNotIn("reporter.progress(", bootstrap)
        self.assertNotIn("reporter.progress(", barrier)


if __name__ == "__main__":
    unittest.main()
