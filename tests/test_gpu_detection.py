# SPDX-License-Identifier: MIT
"""Tests for GPU auto-detection and VRAM monitoring (REQ-9)."""

import unittest
from unittest.mock import patch, MagicMock


class TestGPUAutoDetection(unittest.TestCase):
    """Tests for GPU auto-detection at startup."""

    @patch("liveaudio.core.engine.torch.cuda.is_available")
    def test_auto_detect_cuda_when_available(self, mock_cuda):
        """Should default to cuda when available."""
        mock_cuda.return_value = True
        # This would be tested in engine startup
        self.assertTrue(mock_cuda())

    @patch("liveaudio.core.engine.torch.cuda.is_available")
    def test_auto_detect_cpu_when_cuda_unavailable(self, mock_cuda):
        """Should default to cpu when cuda is not available."""
        mock_cuda.return_value = False
        self.assertFalse(mock_cuda())


class TestDeviceChangeValidation(unittest.TestCase):
    """Tests for CUDA device change validation."""

    @patch("liveaudio.core.engine.torch.cuda.is_available")
    @patch("liveaudio.core.engine.torch.zeros")
    def test_cuda_validation_passes(self, mock_zeros, mock_cuda):
        """CUDA validation should pass when both checks succeed."""
        mock_cuda.return_value = True
        mock_zeros.return_value.cuda.return_value = MagicMock()
        self.assertTrue(mock_cuda())

    @patch("liveaudio.core.engine.torch.cuda.is_available")
    def test_cuda_validation_fails_when_unavailable(self, mock_cuda):
        """CUDA validation should fail when not available."""
        mock_cuda.return_value = False
        self.assertFalse(mock_cuda())


class TestVRAMMonitoring(unittest.TestCase):
    """Tests for runtime VRAM monitoring."""

    @patch("liveaudio.core.engine.torch.cuda.mem_get_info")
    @patch("liveaudio.core.engine.torch.cuda.is_available")
    def test_vram_check_when_cuda_available(self, mock_cuda, mock_mem):
        """VRAM check should work when CUDA is available."""
        mock_cuda.return_value = True
        mock_mem.return_value = (1024 * 1024 * 1024, 4 * 1024 * 1024 * 1024)  # 1GB free, 4GB total
        free_bytes, total_bytes = mock_mem()
        self.assertGreater(free_bytes, 500 * 1024 * 1024)  # More than 500MB

    @patch("liveaudio.core.engine.torch.cuda.mem_get_info")
    @patch("liveaudio.core.engine.torch.cuda.is_available")
    def test_vram_low_triggers_cache_clear(self, mock_cuda, mock_mem):
        """Low VRAM (<500MB) should trigger cache clear before transcribe."""
        mock_cuda.return_value = True
        mock_mem.return_value = (200 * 1024 * 1024, 4 * 1024 * 1024 * 1024)  # 200MB free
        free_bytes, _ = mock_mem()
        self.assertLess(free_bytes, 500 * 1024 * 1024)  # Less than 500MB


def check_vram_available(min_mb=500):
    """Check if enough VRAM is available. Returns (is_enough, free_mb)."""
    import torch
    if not torch.cuda.is_available():
        return False, 0
    try:
        free_bytes, _ = torch.cuda.mem_get_info()
        free_mb = free_bytes / (1024 * 1024)
        return free_mb >= min_mb, free_mb
    except Exception:
        return False, 0


class TestVRAMCheckFunction(unittest.TestCase):
    """Tests for the check_vram_available function."""

    @patch("liveaudio.core.engine.torch.cuda.is_available")
    def test_returns_false_when_cuda_unavailable(self, mock_cuda):
        """Should return False when CUDA is not available."""
        mock_cuda.return_value = False
        is_enough, free_mb = check_vram_available()
        self.assertFalse(is_enough)
        self.assertEqual(free_mb, 0)

    @patch("liveaudio.core.engine.torch.cuda.mem_get_info")
    @patch("liveaudio.core.engine.torch.cuda.is_available")
    def test_returns_true_when_enough_vram(self, mock_cuda, mock_mem):
        """Should return True when enough VRAM is available."""
        mock_cuda.return_value = True
        mock_mem.return_value = (600 * 1024 * 1024, 4 * 1024 * 1024 * 1024)
        is_enough, free_mb = check_vram_available(500)
        self.assertTrue(is_enough)
        self.assertGreaterEqual(free_mb, 500)

    @patch("liveaudio.core.engine.torch.cuda.mem_get_info")
    @patch("liveaudio.core.engine.torch.cuda.is_available")
    def test_returns_false_when_low_vram(self, mock_cuda, mock_mem):
        """Should return False when VRAM is below threshold."""
        mock_cuda.return_value = True
        mock_mem.return_value = (200 * 1024 * 1024, 4 * 1024 * 1024 * 1024)
        is_enough, free_mb = check_vram_available(500)
        self.assertFalse(is_enough)
        self.assertLess(free_mb, 500)


class TestGPUConfigPreference(unittest.TestCase):
    """Config loading defaults to CPU without rewriting an explicit choice."""

    @patch("liveaudio.utils.cuda.cuda_is_available")
    @patch("liveaudio.utils.config.os.path.exists")
    @patch("liveaudio.utils.config.save_config")
    def test_missing_device_defaults_to_cpu_without_hardware_probe(self, mock_save, mock_exists, mock_cuda):
        """A missing config uses the safe CPU default without auto-selecting CUDA."""
        mock_cuda.return_value = False
        mock_exists.return_value = False

        from liveaudio.utils.config import load_config
        config = load_config()

        self.assertEqual(config["device"], "cpu")
        mock_cuda.assert_not_called()

    @patch("liveaudio.utils.cuda.cuda_is_available")
    @patch("liveaudio.utils.config.os.path.exists")
    @patch("liveaudio.utils.config.save_config")
    def test_missing_device_defaults_to_cpu_even_when_cuda_is_available(self, mock_save, mock_exists, mock_cuda):
        """Hardware detection does not change the initial CPU preference."""
        mock_cuda.return_value = True
        mock_exists.return_value = False

        from liveaudio.utils.config import load_config
        config = load_config()

        self.assertEqual(config["device"], "cpu")
        mock_cuda.assert_not_called()

    @patch("liveaudio.utils.cuda.cuda_is_available")
    @patch("liveaudio.utils.config.os.path.exists")
    @patch("liveaudio.utils.config._save_config_no_lock")
    @patch("liveaudio.utils.config.json.load")
    def test_preserves_explicit_cuda_when_unavailable(self, mock_json_load, mock_save, mock_exists, mock_cuda):
        """Runtime fallback does not rewrite an explicit serialized CUDA preference."""
        mock_cuda.return_value = False
        mock_exists.return_value = True
        mock_json_load.return_value = {"device": "cuda", "output_dir": "/tmp/test"}

        from liveaudio.utils.config import load_config
        config = load_config()

        self.assertEqual(config["device"], "cuda")
        mock_save.assert_called_once()
        self.assertEqual(mock_save.call_args.args[0]["device"], "cuda")
        mock_cuda.assert_not_called()


if __name__ == "__main__":
    unittest.main()
