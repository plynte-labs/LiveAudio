# SPDX-License-Identifier: MIT
"""Sonda de disponibilidad de CUDA sin importar torch ni ctranslate2 en la GUI.

El proceso GUI no debe importar torch ni ctranslate2 sólo para decidir si CUDA
es usable: `import ctranslate2` arrastra torch a sys.modules, así que sondear en
proceso contaminaría la GUI con ambos. En su lugar lanzamos un hijo de un solo
disparo que registra las DLLs de CUDA que torch trae empaquetadas (necesarias
para que ctranslate2 pueda hacer LoadLibrary de cublas64_12.dll en Windows),
consulta ctranslate2 y comunica el resultado únicamente por su código de salida.
Así el proceso GUI nunca importa torch ni ctranslate2.
"""

import subprocess
import sys


def cuda_is_available():
    """Devuelve True si hay al menos un dispositivo CUDA usable, sin torch."""
    code = (
        "from liveaudio.utils.dllpath import ensure_torch_dlls; "
        "ensure_torch_dlls(); "
        "import sys, ctranslate2; "
        "sys.exit(0 if ctranslate2.get_cuda_device_count() > 0 else 1)"
    )
    try:
        return subprocess.run([sys.executable, "-c", code], timeout=30).returncode == 0
    except Exception:
        return False
