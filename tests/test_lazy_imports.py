# SPDX-License-Identifier: MIT
"""Heavy ML deps (torch, faster_whisper) must stay OUT of the GUI/ws processes.

On Windows multiprocessing uses 'spawn': every child re-imports its target module
tree, so a top-level `import torch` in app.py would multiply torch's RAM across
the GUI, ws and manager processes. Those imports are deferred to the ASR/audio
worker spawn site. Each assertion runs in an ISOLATED subprocess so sys.modules
is pristine (another test importing torch must not mask a real leak).
"""

import subprocess
import sys
import unittest


def _run(code, env=None):
    return subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, env=env)


class TestLazyImports(unittest.TestCase):
    def test_importing_gui_does_not_load_heavy_modules(self):
        """Importing liveaudio.app must not pull torch/faster_whisper/ctranslate2."""
        result = _run(
            "import sys, liveaudio.app; "
            "leaked = [m for m in ('torch', 'faster_whisper', 'ctranslate2') if m in sys.modules]; "
            "print('LEAKED:' + ','.join(leaked)); "
            "sys.exit(1 if leaked else 0)"
        )
        self.assertEqual(
            result.returncode, 0,
            f"GUI import leaked heavy modules.\nstdout: {result.stdout}\nstderr: {result.stderr}",
        )

    def test_device_listing_is_torch_free(self):
        """list_audio_devices must be importable and usable without torch."""
        result = _run(
            "import sys; from liveaudio.core.devices import list_audio_devices; "
            "leaked = [m for m in ('torch',) if m in sys.modules]; "
            "sys.exit(1 if leaked else 0)"
        )
        self.assertEqual(result.returncode, 0, f"devices pulled torch.\nstderr: {result.stderr}")

    def test_cuda_probe_is_torch_free(self):
        """Calling cuda_is_available must not import torch/ctranslate2 in-process."""
        result = _run(
            "import sys; from liveaudio.utils.cuda import cuda_is_available; "
            "cuda_is_available(); "
            "leaked = [m for m in ('torch', 'ctranslate2') if m in sys.modules]; "
            "print('LEAKED:' + ','.join(leaked)); "
            "sys.exit(1 if leaked else 0)"
        )
        self.assertEqual(
            result.returncode, 0,
            f"cuda probe pulled heavy modules.\nstdout: {result.stdout}\nstderr: {result.stderr}",
        )

    def test_load_config_cuda_is_torch_free(self):
        """load_config() on a cuda config must not import torch into the caller."""
        import os
        import json
        import tempfile

        with tempfile.TemporaryDirectory() as tmp:
            with open(os.path.join(tmp, "config.json"), "w", encoding="utf-8") as f:
                json.dump({"device": "cuda"}, f)
            result = _run(
                "import sys; from liveaudio.utils.config import load_config; "
                "load_config(); "
                "leaked = [m for m in ('torch',) if m in sys.modules]; "
                "print('LEAKED:' + ','.join(leaked)); "
                "sys.exit(1 if leaked else 0)",
                env={**os.environ, "LIVEAUDIO_HOME": tmp},
            )
        self.assertEqual(
            result.returncode, 0,
            f"load_config(cuda) pulled torch.\nstdout: {result.stdout}\nstderr: {result.stderr}",
        )

    def test_workers_still_expose_their_callables(self):
        """The ASR/audio workers must remain importable with their entry points."""
        result = _run(
            "from liveaudio.core.audio import audio_producer, list_audio_devices; "
            "from liveaudio.core.engine import asr_consumer; "
            "assert callable(audio_producer) and callable(asr_consumer) and callable(list_audio_devices)"
        )
        self.assertEqual(result.returncode, 0, f"worker exposure failed.\nstderr: {result.stderr}")


if __name__ == "__main__":
    unittest.main()
