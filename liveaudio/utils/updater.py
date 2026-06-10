# SPDX-License-Identifier: MIT
"""Update check against GitHub releases + hand-off to the bootstrap launcher.

The check itself is rate limited to once per 24 hours and runs on a daemon
thread. When the app was started by the frozen launcher (LiveAudio-Setup.exe /
liveaudio-launcher), ``start_update`` relaunches it with ``--update <tag>`` so
it can swap ``app/`` and re-run ``uv sync`` while the app is closed.
"""
import time
import json
import logging
import os
import ssl
import subprocess
import sys
import urllib.error
import urllib.request
import threading
from liveaudio.utils.config import load_config, save_config

from liveaudio import __version__ as APP_VERSION  # Single source of truth

LOG = logging.getLogger(__name__)

GITHUB_RELEASES_URL = "https://api.github.com/repos/plynte-labs/LiveAudio/releases/latest"
CHECK_INTERVAL_SECONDS = 86400  # 24 hours
REQUEST_TIMEOUT_SECONDS = 5

# Set by the frozen launcher (packaging/launcher.py) in the app's environment.
LAUNCHER_ENV_VAR = "LIVEAUDIO_LAUNCHER"

# Names the frozen launcher executable may have, per platform/build.
_LAUNCHER_EXE_NAMES = (
    "LiveAudio-Setup.exe",
    "liveaudio-launcher.exe",
    "liveaudio-launcher",
)

def parse_version(version_str):
    """Parsea 'v1.2.3' -> (1, 2, 3) para comparación semántica limpia."""
    cleaned = version_str.strip().lower()
    if cleaned.startswith('v'):
        cleaned = cleaned[1:]
    try:
        return tuple(map(int, cleaned.split('.')))
    except ValueError:
        return (0, 0, 0)

def is_new_version_available(current_version, latest_version):
    """Compara si la versión de GitHub es mayor que la local."""
    return parse_version(latest_version) > parse_version(current_version)

def _read_json_response(request, context):
    with urllib.request.urlopen(
        request, timeout=REQUEST_TIMEOUT_SECONDS, context=context
    ) as response:
        if response.status != 200:
            return None
        return json.loads(response.read().decode('utf-8'))

def _fetch_latest_release():
    """Fetch the latest release JSON.

    Tries a properly verified HTTPS request first; only when certificate
    verification itself fails (broken corporate proxies, missing CA bundles
    in frozen environments) does it retry without verification, logging a
    warning. Any other network error propagates to the caller.
    """
    request = urllib.request.Request(
        GITHUB_RELEASES_URL,
        headers={"User-Agent": "Plynte-LiveAudio-Client"}
    )
    try:
        return _read_json_response(request, ssl.create_default_context())
    except ssl.SSLCertVerificationError as exc:
        LOG.warning(
            "TLS certificate verification failed (%s); retrying without verification",
            exc,
        )
    except urllib.error.URLError as exc:
        # urlopen wraps handshake errors: URLError(reason=SSLCertVerificationError)
        if not isinstance(getattr(exc, "reason", None), ssl.SSLCertVerificationError):
            raise
        LOG.warning(
            "TLS certificate verification failed (%s); retrying without verification",
            exc.reason,
        )
    return _read_json_response(request, ssl._create_unverified_context())

def check_for_updates(callback):
    """Synchronous update check (rate limited to once per 24 hours).

    The ``callback`` receives a boolean (update available) and the latest
    release tag string. Network failures are swallowed: no callback is fired.
    """
    config = load_config()
    last_check = config.get("last_update_check", 0)
    current_time = int(time.time())

    if current_time - last_check < CHECK_INTERVAL_SECONDS:
        return

    try:
        data = _fetch_latest_release()
        if data is None:
            return
        latest_tag = data.get("tag_name", "v0.0.0")

        # Persist the timestamp of the successful check.
        config["last_update_check"] = current_time
        save_config(config)

        callback(is_new_version_available(APP_VERSION, latest_tag), latest_tag)
    except Exception as e:
        # Fail silently so a missing connection never hurts the user experience.
        print(f"[Updater] Update check failed: {e}")

def check_for_updates_async(callback):
    """Run the update check on a daemon thread (non-blocking)."""
    thread = threading.Thread(target=check_for_updates, args=(callback,), daemon=True)
    thread.start()
    return thread

def find_launcher():
    """Locate the launcher executable for the current install, or None.

    Strategy:
    1. ``LIVEAUDIO_LAUNCHER`` env var — exported by the frozen launcher when
       it spawns the app (authoritative, works for any install location).
    2. Walk up from the running executable (``<root>/app/.venv/...``) to the
       install root (marked by ``installed.json``); on portable installs the
       launcher sits next to the root (``<launcher dir>/data``).
    3. None — dev runs (``uv run liveaudio``) or non-portable installs started
       without the launcher. Callers must degrade gracefully.
    """
    env_path = os.environ.get(LAUNCHER_ENV_VAR)
    if env_path:
        return env_path

    path = os.path.dirname(os.path.abspath(sys.executable))
    for _ in range(8):
        parent = os.path.dirname(path)
        if parent == path:
            break
        path = parent
        if os.path.isfile(os.path.join(path, "installed.json")):
            launcher_dir = os.path.dirname(path)
            for name in _LAUNCHER_EXE_NAMES:
                candidate = os.path.join(launcher_dir, name)
                if os.path.isfile(candidate):
                    return candidate
            break
    return None

def start_update(tag):
    """Spawn the launcher detached with ``--update <tag>``.

    Returns True when the launcher process was started (the app should then
    shut down so files can be swapped), False when no launcher is available
    or it could not be spawned.
    """
    launcher = find_launcher()
    if not launcher:
        return False

    kwargs = {"close_fds": True}
    if sys.platform == "win32":
        kwargs["creationflags"] = (
            getattr(subprocess, "DETACHED_PROCESS", 0)
            | getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)
        )
    else:
        kwargs["start_new_session"] = True

    try:
        subprocess.Popen([launcher, "--update", tag], **kwargs)
    except OSError as e:
        print(f"[Updater] Could not start the launcher: {e}")
        return False
    return True
