import unittest
import os
import tempfile
import time
from core.engine import SessionWriter

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

if __name__ == '__main__':
    unittest.main()
