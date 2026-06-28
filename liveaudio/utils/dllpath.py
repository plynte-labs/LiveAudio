# SPDX-License-Identifier: MIT
"""Make torch's bundled CUDA/cuDNN DLLs resolvable on Windows.

ctranslate2 (used by faster-whisper) loads cublas64_12.dll & friends via
LoadLibrary, which searches PATH — not the Python DLL directories — so we
both register the directory with os.add_dll_directory() and prepend it to
PATH. No-op on non-Windows platforms or when torch is not installed.
"""

import os
import sys
import importlib.util

_already_done = False


def ensure_torch_dlls():
    """Register torch/lib as a DLL search location (Windows only)."""
    global _already_done
    if _already_done or sys.platform != "win32":
        return

    # Localizamos torch/lib SIN importar torch: find_spec sólo lee los metadatos
    # del paquete (no ejecuta torch/__init__.py), así el proceso GUI no paga el
    # coste de torch sólo para registrar las DLLs de CUDA.
    try:
        spec = importlib.util.find_spec("torch")
    except (ImportError, ValueError):
        return
    if spec is None or not spec.submodule_search_locations:
        return

    torch_lib = os.path.join(spec.submodule_search_locations[0], "lib")
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
