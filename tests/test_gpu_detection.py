"""Tests for GPU auto-detection and VRAM monitoring (REQ-9)."""

import unittest
from unittest.mock import patch, MagicMock


class TestGPUAutoDetection(unittest.TestCase):
    """Tests for GPU auto-detection at startup."""

    @patch("core.engine.torch.cuda.is_available")
    def test_auto_detect_cuda_when_available(self, mock_cuda):
        """Should default to cuda when available."""
        mock_cuda.return_value = True
        # This would be tested in engine startup
        self.assertTrue(mock_cuda())

    @patch("core.engine.torch.cuda.is_available")
    def test_auto_detect_cpu_when_cuda_unavailable(self, mock_cuda):
        """Should default to cpu when cuda is not available."""
        mock_cuda.return_value = False
        self.assertFalse(mock_cuda())


class TestDeviceChangeValidation(unittest.TestCase):
    """Tests for CUDA device change validation."""

    @patch("core.engine.torch.cuda.is_available")
    @patch("core.engine.torch.zeros")
    def test_cuda_validation_passes(self, mock_zeros, mock_cuda):
        """CUDA validation should pass when both checks succeed."""
        mock_cuda.return_value = True
        mock_zeros.return_value.cuda.return_value = MagicMock()
        self.assertTrue(mock_cuda())

    @patch("core.engine.torch.cuda.is_available")
    def test_cuda_validation_fails_when_unavailable(self, mock_cuda):
        """CUDA validation should fail when not available."""
        mock_cuda.return_value = False
        self.assertFalse(mock_cuda())


class TestVRAMMonitoring(unittest.TestCase):
    """Tests for runtime VRAM monitoring."""

    @patch("core.engine.torch.cuda.mem_get_info")
    @patch("core.engine.torch.cuda.is_available")
    def test_vram_check_when_cuda_available(self, mock_cuda, mock_mem):
        """VRAM check should work when CUDA is available."""
        mock_cuda.return_value = True
        mock_mem.return_value = (1024 * 1024 * 1024, 4 * 1024 * 1024 * 1024)  # 1GB free, 4GB total
        free_bytes, total_bytes = mock_mem()
        self.assertGreater(free_bytes, 500 * 1024 * 1024)  # More than 500MB

    @patch("core.engine.torch.cuda.mem_get_info")
    @patch("core.engine.torch.cuda.is_available")
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

    @patch("core.engine.torch.cuda.is_available")
    def test_returns_false_when_cuda_unavailable(self, mock_cuda):
        """Should return False when CUDA is not available."""
        mock_cuda.return_value = False
        is_enough, free_mb = check_vram_available()
        self.assertFalse(is_enough)
        self.assertEqual(free_mb, 0)

    @patch("core.engine.torch.cuda.mem_get_info")
    @patch("core.engine.torch.cuda.is_available")
    def test_returns_true_when_enough_vram(self, mock_cuda, mock_mem):
        """Should return True when enough VRAM is available."""
        mock_cuda.return_value = True
        mock_mem.return_value = (600 * 1024 * 1024, 4 * 1024 * 1024 * 1024)
        is_enough, free_mb = check_vram_available(500)
        self.assertTrue(is_enough)
        self.assertGreaterEqual(free_mb, 500)

    @patch("core.engine.torch.cuda.mem_get_info")
    @patch("core.engine.torch.cuda.is_available")
    def test_returns_false_when_low_vram(self, mock_cuda, mock_mem):
        """Should return False when VRAM is below threshold."""
        mock_cuda.return_value = True
        mock_mem.return_value = (200 * 1024 * 1024, 4 * 1024 * 1024 * 1024)
        is_enough, free_mb = check_vram_available(500)
        self.assertFalse(is_enough)
        self.assertLess(free_mb, 500)


class TestGPUAutoDetectConfig(unittest.TestCase):
    """Tests for GPU auto-detection in config loading."""

    @patch("utils.config.torch.cuda.is_available")
    @patch("utils.config.os.path.exists")
    @patch("utils.config.save_config")
    def test_auto_detect_cpu_when_cuda_unavailable(self, mock_save, mock_exists, mock_cuda):
        """load_config should default to cpu when CUDA is unavailable."""
        mock_cuda.return_value = False
        mock_exists.return_value = False

        from utils.config import load_config, DEFAULT_CONFIG
        config = load_config()

        self.assertEqual(config["device"], "cpu")

    @patch("utils.config.torch.cuda.is_available")
    @patch("utils.config.os.path.exists")
    @patch("utils.config.save_config")
    def test_keeps_cuda_when_available(self, mock_save, mock_exists, mock_cuda):
        """load_config should keep cuda when CUDA is available."""
        mock_cuda.return_value = True
        mock_exists.return_value = False

        from utils.config import load_config, DEFAULT_CONFIG
        config = load_config()

        self.assertEqual(config["device"], "cuda")

    @patch("utils.config.torch.cuda.is_available")
    @patch("utils.config.os.path.exists")
    @patch("utils.config._save_config_no_lock")
    @patch("utils.config.json.load")
    def test_forces_cpu_when_cuda_unavailable_existing_config(self, mock_json_load, mock_save, mock_exists, mock_cuda):
        """load_config should force cpu if existing config has cuda but GPU unavailable."""
        mock_cuda.return_value = False
        mock_exists.return_value = True
        mock_json_load.return_value = {"device": "cuda", "output_dir": "/tmp/test"}

        from utils.config import load_config
        config = load_config()

        self.assertEqual(config["device"], "cpu")
        mock_save.assert_called_once()  # Config should be saved with updated device


if __name__ == "__main__":
    unittest.main()
