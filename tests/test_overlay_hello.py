# SPDX-License-Identifier: MIT
import json
import shutil
import subprocess
import unittest
from pathlib import Path


@unittest.skipUnless(shutil.which("node"), "Node.js is required for overlay runtime tests")
class TestOverlayHello(unittest.TestCase):
    def run_scenario(self, name):
        harness = Path(__file__).with_name("ribbon_harness.js")
        result = subprocess.run(
            ["node", str(harness), name], capture_output=True, text=True, check=True
        )
        return json.loads(result.stdout)

    def test_discards_frames_until_valid_hello(self):
        result = self.run_scenario("helloGate")
        self.assertEqual(result["beforeHello"], 0)
        self.assertEqual(result["afterHello"], ["accepted"])

    def test_timeout_closes_foreign_socket_and_advances(self):
        result = self.run_scenario("helloTimeout")
        self.assertTrue(result["firstClosed"])
        self.assertEqual(result["urls"][:2], ["ws://127.0.0.1:8765", "ws://127.0.0.1:8766"])

    def test_active_disconnect_rescans_from_base(self):
        result = self.run_scenario("activeReconnect")
        self.assertEqual(result["urls"][:2], ["ws://127.0.0.1:8765", "ws://127.0.0.1:8765"])

    def test_stale_socket_callbacks_cannot_mutate_current_attempt(self):
        result = self.run_scenario("staleCallbacks")
        self.assertEqual(result["before"], 2)
        self.assertEqual(result["after"], 2)
        self.assertEqual(result["rendered"], [])

    def test_full_scan_backoff_is_capped_at_eight_seconds(self):
        result = self.run_scenario("backoffCap")
        self.assertEqual(result["backoffs"], [1000, 2000, 4000, 8000, 8000])


if __name__ == "__main__":
    unittest.main()
