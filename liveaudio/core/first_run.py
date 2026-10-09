# SPDX-License-Identifier: MIT
"""Shared, dependency-free first-run contract for the installed application."""
import json
import os
import re


HANDOFF_FIELDS = (
    "hf_home",
    "install_root",
    "extra",
    "app_version",
    "attempt",
    "launcher_phases_done",
)

# This vocabulary is intentionally data-only: launcher has a byte-for-byte
# compatible stdlib copy because it runs before the package is installed.
PHASE_COPY = {
    0: {"es": "Selección de instalación", "en": "Installation selection"},
    1: {"es": "Preparando uv", "en": "Preparing uv"},
    2: {"es": "Código de LiveAudio", "en": "LiveAudio code"},
    3: {"es": "Instalando dependencias", "en": "Installing dependencies"},
    4: {"es": "Abriendo LiveAudio", "en": "Opening LiveAudio"},
    5: {"es": "Preparando VAD", "en": "Preparing VAD"},
    6: {"es": "Descargando/cargando Whisper", "en": "Downloading/loading Whisper"},
    7: {"es": "Listo para iniciar", "en": "Ready to start"},
}

_VERSION_RE = re.compile(r"^v?\d+(?:\.\d+){1,3}(?:[-+][0-9A-Za-z.-]+)?$")
_ALLOWED_EXTRAS = {"cpu", "cu121"}


def handoff_path(install_root):
    return os.path.join(os.path.abspath(install_root), "handoff.json")


def _contained(path, root):
    try:
        path = os.path.realpath(path)
        root = os.path.realpath(root)
        return os.path.commonpath((path, root)) == root
    except (TypeError, ValueError):
        return False


def validate_handoff(path, expected_install_root, expected_version, min_attempt=1):
    """Return a validated snapshot, otherwise ``None`` without logging it."""
    try:
        with open(path, "r", encoding="utf-8") as fh:
            payload = json.load(fh)
    except (OSError, ValueError, TypeError):
        return None
    if not isinstance(payload, dict) or tuple(payload) != HANDOFF_FIELDS:
        return None
    root = os.path.abspath(expected_install_root)
    if not isinstance(payload["install_root"], str) or os.path.abspath(payload["install_root"]) != root:
        return None
    if not isinstance(payload["hf_home"], str) or not _contained(payload["hf_home"], root):
        return None
    if payload["extra"] not in _ALLOWED_EXTRAS:
        return None
    if not isinstance(payload["app_version"], str) or not _VERSION_RE.match(payload["app_version"]):
        return None
    if expected_version and payload["app_version"].lstrip("v") != str(expected_version).lstrip("v"):
        return None
    if isinstance(payload["attempt"], bool) or not isinstance(payload["attempt"], int):
        return None
    if payload["attempt"] < int(min_attempt):
        return None
    if payload["launcher_phases_done"] != [0, 1, 2, 3]:
        return None
    return payload
