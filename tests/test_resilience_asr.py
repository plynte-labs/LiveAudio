"""Tests for ASR freeze recovery and resilience (REQ-1)."""

import unittest
import time
import multiprocessing as mp
from unittest.mock import MagicMock, patch, call
from concurrent.futures import TimeoutError as FuturesTimeout

from tests.helpers import MockQueue, make_shared_config, make_mock_transcribe_result


class TestAsrTimeoutRecovery(unittest.TestCase):
    """Tests for ASR transcribe timeout and recovery."""

    @patch("core.engine.ThreadPoolExecutor")
    def test_transcribe_timeout_triggers_after_limit(self, mock_executor_class):
        """Transcribe timeout should trigger after 15s for a 5s chunk."""
        from core.engine import _transcribe_with_timeout
        log_queue = MockQueue()

        # Mock the executor to raise TimeoutError
        mock_future = MagicMock()
        mock_future.result = MagicMock(side_effect=FuturesTimeout())
        mock_executor = MagicMock()
        mock_executor.submit = MagicMock(return_value=mock_future)
        mock_executor.__enter__ = MagicMock(return_value=mock_executor)
        mock_executor.__exit__ = MagicMock(return_value=False)
        mock_executor_class.return_value = mock_executor

        mock_model = MagicMock()

        segments, info = _transcribe_with_timeout(mock_model, b"audio", timeout_sec=15.0, log_queue=log_queue)

        self.assertIsNone(segments)
        self.assertIsNone(info)
        mock_future.result.assert_called_once_with(timeout=15.0)

    @patch("core.engine.ThreadPoolExecutor")
    def test_timeout_emits_warning_status(self, mock_executor_class):
        """Timeout should emit warning status to log_queue."""
        from core.engine import _transcribe_with_timeout
        log_queue = MockQueue()

        mock_future = MagicMock()
        mock_future.result = MagicMock(side_effect=FuturesTimeout())
        mock_executor = MagicMock()
        mock_executor.submit = MagicMock(return_value=mock_future)
        mock_executor.__enter__ = MagicMock(return_value=mock_executor)
        mock_executor.__exit__ = MagicMock(return_value=False)
        mock_executor_class.return_value = mock_executor

        mock_model = MagicMock()

        _transcribe_with_timeout(mock_model, b"audio", timeout_sec=15.0, log_queue=log_queue)

        status_items = [item for item in log_queue.items if item.get("type") == "status"]
        self.assertTrue(any(s.get("key") == "asr" and s.get("state") == "warn" for s in status_items))

    @patch("core.engine.ThreadPoolExecutor")
    def test_timeout_continues_to_next_item(self, mock_executor_class):
        """After timeout, processing should return None to allow continuation."""
        from core.engine import _transcribe_with_timeout
        log_queue = MockQueue()

        mock_future = MagicMock()
        mock_future.result = MagicMock(side_effect=FuturesTimeout())
        mock_executor = MagicMock()
        mock_executor.submit = MagicMock(return_value=mock_future)
        mock_executor.__enter__ = MagicMock(return_value=mock_executor)
        mock_executor.__exit__ = MagicMock(return_value=False)
        mock_executor_class.return_value = mock_executor

        mock_model = MagicMock()

        segments, info = _transcribe_with_timeout(mock_model, b"audio", timeout_sec=15.0, log_queue=log_queue)

        self.assertIsNone(segments)
        self.assertIsNone(info)

    @patch("core.engine.torch")
    def test_cuda_empty_cache_called_after_transcribe(self, mock_torch):
        """torch.cuda.empty_cache() should be called after transcribe on CUDA."""
        from core.engine import _transcribe_with_timeout
        log_queue = MockQueue()

        mock_model = MagicMock()
        mock_model.transcribe = MagicMock(return_value=make_mock_transcribe_result())

        _transcribe_with_timeout(mock_model, b"audio", timeout_sec=15.0, log_queue=log_queue, device="cuda")

        mock_torch.cuda.empty_cache.assert_called_once()

    def test_structured_error_event_sent_on_failure(self):
        """Structured error event should be sent to log_queue on failure."""
        from core.engine import _transcribe_with_timeout
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
        from core.engine import _transcribe_with_timeout
        log_queue = MockQueue()

        mock_model = MagicMock()
        mock_model.transcribe = MagicMock(side_effect=ValueError("test error"))

        _transcribe_with_timeout(mock_model, b"audio", timeout_sec=15.0, log_queue=log_queue)

        error_items = [item for item in log_queue.items if item.get("type") == "error"]
        self.assertTrue(any(e.get("exception_type") == "ValueError" for e in error_items))

    def test_error_event_contains_traceback_summary(self):
        """Error event should include a summary of the traceback."""
        from core.engine import _transcribe_with_timeout
        log_queue = MockQueue()

        mock_model = MagicMock()
        mock_model.transcribe = MagicMock(side_effect=RuntimeError("test"))

        _transcribe_with_timeout(mock_model, b"audio", timeout_sec=15.0, log_queue=log_queue)

        error_items = [item for item in log_queue.items if item.get("type") == "error"]
        self.assertTrue(any("traceback_summary" in e for e in error_items))

    def test_asr_consumer_exits_gracefully_on_fatal_error(self):
        """ASR consumer should emit final error status before exiting."""
        from core.engine import _transcribe_with_timeout
        log_queue = MockQueue()

        mock_model = MagicMock()
        mock_model.transcribe = MagicMock(side_effect=MemoryError("fatal"))

        _transcribe_with_timeout(mock_model, b"audio", timeout_sec=15.0, log_queue=log_queue)

        # Error status should be emitted
        status_items = [item for item in log_queue.items if item.get("type") == "status"]
        self.assertTrue(any(s.get("key") == "asr" and s.get("state") == "error" for s in status_items))


if __name__ == "__main__":
    unittest.main()
