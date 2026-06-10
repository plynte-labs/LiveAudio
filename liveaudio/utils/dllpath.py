# SPDX-License-Identifier: MIT
"""Make torch's bundled CUDA/cuDNN DLLs resolvable on Windows.

ctranslate2 (used by faster-whisper) loads cublas64_12.dll & friends via
LoadLibrary, which searches PATH — not the Python DLL directories — so we
both register the directory with os.add_dll_directory() and prepend it to
PATH. No-op on non-Windows platforms or when torch is not installed.
"""

import os
import sys

_already_done = False


def ensure_torch_dlls():
    """Register torch/lib as a DLL search location (Windows only)."""
    global _already_done
    if _already_done or sys.platform != "win32":
        return
    try:
        import torch
    except ImportError:
        return

    torch_lib = os.path.join(os.path.dirname(torch.__file__), "lib")
    if not os.path.isdir(torch_lib):
        return

    try:
        os.add_dll_directory(torch_lib)
    except (AttributeError, OSError):
        pass

    current_path = os.environ.get("PATH", "")
    if torch_lib not in current_path.split(os.pathsep):
        os.environ["PATH"] = torch_lib + os.pathsep + current_path

    _already_done = True
