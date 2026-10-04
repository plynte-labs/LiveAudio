# SPDX-License-Identifier: MIT
import unittest
import os
import tempfile
import time
import queue
import threading
from unittest.mock import patch
from liveaudio.core.engine import SessionWriter

class TestSessionWriter(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.jsonl_path = os.path.join(self.temp_dir.name, "transcript.jsonl")
        self.vtt_path = os.path.join(self.temp_dir.name, "subtitles.vtt")
        self.writer = SessionWriter(self.jsonl_path, self.vtt_path)

    def tearDown(self):
        self.writer.stop()
        self.temp_dir.cleanup()

    def test_writer_is_asynchronous(self):
        """Test that SessionWriter writes asynchronously without blocking."""
        # Write some data
        start_time = time.time()
        self.writer.write_record({"id": "1", "text": "test", "sequence": 0}, "00:00:00.000", "00:00:01.000", "test", 1)
        end_time = time.time()
        
        # Writing should return almost instantly
        self.assertLess(end_time - start_time, 0.1)
        
        # Data shouldn't necessarily be there immediately if async, but we can wait
        self.writer.flush()
        
        # Verify data was written
        with open(self.jsonl_path, "r", encoding="utf-8") as f:
            lines = f.readlines()
            self.assertEqual(len(lines), 1)
            self.assertIn("test", lines[0])
            
        with open(self.vtt_path, "r", encoding="utf-8") as f:
            content = f.read()
            self.assertIn("test", content)

    def test_writer_admission_is_bounded_and_accounts_for_rejection(self):
        started = threading.Event()
        release = threading.Event()
        real_open = open
        failures = []
        try:
            self.writer.stop()
        except TypeError:
            pass
        try:
            writer = SessionWriter(
                self.jsonl_path, self.vtt_path, queue_capacity=1,
                failure_callback=failures.append,
            )
        except TypeError:
            self.fail("SessionWriter must accept a finite queue capacity and failure callback")

        def blocked_open(path, *args, **kwargs):
            if path == self.jsonl_path:
                started.set()
                release.wait(2)
            return real_open(path, *args, **kwargs)

        try:
            with patch("builtins.open", side_effect=blocked_open):
                self.assertTrue(writer.write_record({"id": "1"}, "0", "1", "one", 1, write_vtt=False))
                self.assertTrue(started.wait(1))
                self.assertTrue(writer.write_record({"id": "2"}, "1", "2", "two", 2, write_vtt=False))
                self.assertFalse(writer.write_record({"id": "3"}, "2", "3", "three", 3, write_vtt=False))
                self.assertFalse(writer.write_record({"id": "4"}, "3", "4", "four", 4, write_vtt=False))
                outcomes = writer.outcomes()
                self.assertEqual(outcomes["jsonl"], {"pending": 2, "saved": 0, "rejected": 2, "failed": 0})
                self.assertEqual(failures, ["writer_queue_full"])
        finally:
            release.set()
            try:
                writer.stop(timeout_sec=1)
            except TypeError:
                writer.stop()
            writer.thread.join(timeout=1)

        outcomes = writer.outcomes()
        self.assertEqual(outcomes["jsonl"], {"pending": 0, "saved": 2, "rejected": 2, "failed": 0})

    def test_flush_waits_for_accepted_writes(self):
        started = threading.Event()
        release = threading.Event()
        real_open = open
        try:
            self.writer.stop()
        except TypeError:
            pass
        try:
            writer = SessionWriter(self.jsonl_path, self.vtt_path)
        except TypeError:
            self.fail("SessionWriter construction should remain compatible")

        def blocked_open(path, *args, **kwargs):
            if path == self.jsonl_path:
                started.set()
                release.wait(2)
            return real_open(path, *args, **kwargs)

        result = []
        finished = threading.Event()
        try:
            with patch("builtins.open", side_effect=blocked_open):
                self.assertTrue(writer.write_record({"id": "1"}, "0", "1", "one", 1, write_vtt=False))
                self.assertTrue(started.wait(1))
                def flush_writer():
                    try:
                        result.append(writer.flush(timeout_sec=1))
                    except TypeError:
                        result.append(None)
                    finally:
                        finished.set()
                waiter = threading.Thread(target=flush_writer)
                waiter.start()
                self.assertFalse(finished.wait(0.05))
                release.set()
                self.assertTrue(finished.wait(1))
                waiter.join(1)
            self.assertEqual(result, [True])
        finally:
            release.set()
            try:
                writer.stop(timeout_sec=1)
            except TypeError:
                writer.stop()
            writer.thread.join(timeout=1)

    def test_stop_deadline_covers_stuck_writer_and_keeps_pending_outcome(self):
        started = threading.Event()
        release = threading.Event()
        real_open = open
        try:
            self.writer.stop()
        except TypeError:
            pass
        try:
            writer = SessionWriter(self.jsonl_path, self.vtt_path)
        except TypeError:
            self.fail("SessionWriter construction should remain compatible")

        def blocked_open(path, *args, **kwargs):
            if path == self.jsonl_path:
                started.set()
                release.wait(2)
            return real_open(path, *args, **kwargs)

        try:
            with patch("builtins.open", side_effect=blocked_open):
                self.assertTrue(writer.write_record({"id": "1"}, "0", "1", "one", 1, write_vtt=False))
                self.assertTrue(started.wait(1))
                began = time.monotonic()
                try:
                    stopped = writer.stop(timeout_sec=0.05)
                except TypeError:
                    stopped = None
                elapsed = time.monotonic() - began
                self.assertIs(stopped, False)
                self.assertLess(elapsed, 0.25)
                self.assertEqual(writer.outcomes()["jsonl"]["pending"], 1)
            release.set()
            writer.thread.join(1)
            self.assertFalse(writer.thread.is_alive())
            self.assertEqual(writer.outcomes()["jsonl"]["saved"], 1)
        finally:
            release.set()
            try:
                writer.stop(timeout_sec=1)
            except TypeError:
                writer.stop()
            writer.thread.join(timeout=1)


if __name__ == '__main__':
    unittest.main()
