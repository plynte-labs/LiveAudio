# SPDX-License-Identifier: MIT
"""LiveAudio bootstrapper launcher.

A single-file launcher distributed as a small PyInstaller executable. On first
run it downloads the application source, provisions a project-local virtual
environment with ``uv`` (selecting the CPU or CUDA torch extra based on the
detected hardware), and launches the app. On subsequent runs it goes straight
to the installed app ("fast path"); on Windows a minimal "Starting
LiveAudio..." splash stays open until the app window appears, because the
app's module-level torch import can take 60+ seconds on a cold start with no
feedback of its own.

HARD CONSTRAINT: this module may only use the Python standard library plus
tkinter. It must NEVER import torch or any third-party package — it runs
before any of them are installed.

CLI surface:
    --device cpu|cuda        force the torch backend (overrides detection,
                             persisted in installed.json)
    --update [vX.Y.Z]        update to the given (or latest) GitHub release
    --self-test              print detection + resolved paths, exit 0
    --reinstall              wipe app/ + .venv and bootstrap fresh
    --install-desktop-entry  Linux: install a .desktop entry
    --headless               plain stdout progress, no tkinter window
    --src-dir PATH           dev mode: install from a local checkout instead
                             of a GitHub release zip
    --no-launch              (hidden) stop after writing installed.json

Environment overrides:
    LIVEAUDIO_INSTALL_ROOT   use this install root (testing / automation)
    LIVEAUDIO_HOME           forwarded to the app as its data home
"""

import argparse
import ctypes.util
import hashlib
import json
import logging
import os
import platform as _platform_mod
import queue
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import threading
import time
import urllib.error
import urllib.request
import zipfile

LOG = logging.getLogger("liveaudio.launcher")

GITHUB_REPO = "plynte-labs/LiveAudio"
PYTHON_VERSION = "3.11"
MIN_DRIVER_MAJOR = 525
MIN_VRAM_MIB = 4096
NVIDIA_SMI_TIMEOUT = 10  # seconds
DOWNLOAD_RETRIES = 3
UV_RELEASE_BASE = "https://github.com/astral-sh/uv/releases/latest/download"

# Dark theme matching the main app.
COLOR_BG = "#111b1e"
COLOR_PANEL = "#16242a"
COLOR_FG = "#e8f0ee"
COLOR_MUTED = "#8fa6a0"
COLOR_ACCENT = "#3c9e66"

# Must match the Tk title set in liveaudio/app.py (self.title(...)). Window
# detection by title is how the launcher knows the app is actually visible:
# the venv entry point is a uv trampoline, so PID-based checks never see the
# real app process.
APP_WINDOW_TITLE = "Plynte LiveAudio"
APP_WINDOW_TIMEOUT = 120.0  # seconds to wait for the app window to appear

GPU_FALLBACK_MESSAGE = (
    "GPU detected but driver too old / insufficient VRAM — installing CPU "
    "version. Update your NVIDIA driver and re-run with --device cuda"
)
APP_START_FAILED_MESSAGE = (
    "LiveAudio failed to start — see bootstrap.log in the install folder"
)
PORTAUDIO_MESSAGE = (
    "PortAudio runtime missing — install it with: sudo apt install libportaudio2"
)


class LauncherError(Exception):
    """Fatal launcher error with a user-facing message."""


class ChecksumError(LauncherError):
    """Downloaded file failed SHA256 verification."""


class CancelledError(Exception):
    """Bootstrap was cancelled by the user."""


# ---------------------------------------------------------------------------
# Release metadata
# ---------------------------------------------------------------------------

class ReleaseMeta:
    """Where the application source comes from for this launcher build."""

    def __init__(self, version, src_zip_url=None, src_zip_sha256=None, src_dir=None):
        self.version = version
        self.src_zip_url = src_zip_url
        self.src_zip_sha256 = src_zip_sha256
        self.src_dir = src_dir

    @property
    def mode(self):
        if self.src_dir:
            return "dev"
        if self.src_zip_url:
            return "release"
        return "none"


def _launcher_dir():
    """Directory holding the launcher (frozen exe dir or this file's dir)."""
    if getattr(sys, "frozen", False):
        return os.path.dirname(os.path.abspath(sys.executable))
    return os.path.dirname(os.path.abspath(__file__))


def _bundled_data_dirs():
    """Directories that may hold bundled data files (icons, uv, template)."""
    dirs = []
    meipass = getattr(sys, "_MEIPASS", None)
    if meipass:
        dirs.append(meipass)
    dirs.append(_launcher_dir())
    return dirs


def parse_version_from_init(init_path):
    """Extract __version__ from a liveaudio/__init__.py file."""
    with open(init_path, "r", encoding="utf-8") as fh:
        match = re.search(r"__version__\s*=\s*[\"']([^\"']+)[\"']", fh.read())
    if not match:
        raise LauncherError("Could not parse __version__ from %s" % init_path)
    return match.group(1)


def load_release_meta(src_dir=None):
    """Resolve release metadata.

    Order: generated _release_meta.py (release builds) > --src-dir dev mode >
    none (fast path only).
    """
    meta_mod = None
    try:
        import _release_meta as meta_mod  # noqa: F401  (generated at build time)
    except ImportError:
        # Not frozen / not on sys.path: look next to this file.
        meta_path = os.path.join(_launcher_dir(), "_release_meta.py")
        if os.path.isfile(meta_path):
            import importlib.util
            spec = importlib.util.spec_from_file_location("_release_meta", meta_path)
            meta_mod = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(meta_mod)
    if meta_mod is not None:
        return ReleaseMeta(
            version=meta_mod.VERSION,
            src_zip_url=meta_mod.SRC_ZIP_URL,
            src_zip_sha256=meta_mod.SRC_ZIP_SHA256,
        )
    if src_dir:
        src_dir = os.path.abspath(src_dir)
        init_path = os.path.join(src_dir, "liveaudio", "__init__.py")
        if not os.path.isfile(init_path):
            raise LauncherError("--src-dir %s does not look like a LiveAudio checkout" % src_dir)
        return ReleaseMeta(version=parse_version_from_init(init_path), src_dir=src_dir)
    return None


# ---------------------------------------------------------------------------
# Install layout
# ---------------------------------------------------------------------------

def _global_appdata_dir(platform, environ):
    if platform == "win32":
        base = environ.get("APPDATA") or os.path.expanduser("~")
        return os.path.join(base, "LiveAudio")
    else:
        base = environ.get("XDG_CONFIG_HOME") or os.path.expanduser(os.path.join("~", ".config"))
        return os.path.join(base, "liveaudio")

def read_install_location(platform=None, environ=None):
    platform = platform or sys.platform
    environ = os.environ if environ is None else environ
    path = os.path.join(_global_appdata_dir(platform, environ), "install_location.json")
    try:
        with open(path, "r", encoding="utf-8") as fh:
            data = json.load(fh)
            if isinstance(data, dict) and "install_root" in data:
                return data
    except (OSError, ValueError):
        pass
    return None

def write_install_location(install_root, hf_home, platform=None, environ=None):
    platform = platform or sys.platform
    environ = os.environ if environ is None else environ
    config_dir = _global_appdata_dir(platform, environ)
    os.makedirs(config_dir, exist_ok=True)
    path = os.path.join(config_dir, "install_location.json")
    data = {
        "install_root": os.path.abspath(install_root),
        "hf_home": os.path.abspath(hf_home)
    }
    fd, tmp = tempfile.mkstemp(prefix="install_location-", suffix=".tmp", dir=config_dir)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            json.dump(data, fh, indent=2)
            fh.flush()
            os.fsync(fh.fileno())
        os.replace(tmp, path)
    except BaseException:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise

def resolve_install_root(platform=None, environ=None, launcher_dir=None):
    """Return (install_root, portable).

    Precedence:
    1. LIVEAUDIO_INSTALL_ROOT env var (testing/automation override)
    2. portable.marker next to the launcher -> <launcher dir>/data
    3. install_location.json in APPDATA
    4. %LOCALAPPDATA%\\LiveAudio (Windows) / ~/.local/share/liveaudio (Linux)
    """
    platform = platform or sys.platform
    environ = os.environ if environ is None else environ
    launcher_dir = launcher_dir or _launcher_dir()

    override = environ.get("LIVEAUDIO_INSTALL_ROOT")
    if override:
        return os.path.abspath(override), False

    if os.path.isfile(os.path.join(launcher_dir, "portable.marker")):
        return os.path.join(launcher_dir, "data"), True

    location_data = read_install_location(platform, environ)
    if location_data and "install_root" in location_data:
        return location_data["install_root"], False

    if platform == "win32":
        base = environ.get("LOCALAPPDATA") or os.path.expanduser("~")
        default_root = os.path.join(base, "LiveAudio")
        
        if os.path.isdir(default_root) and os.path.isfile(os.path.join(default_root, "installed.json")):
            migrated_root = _prompt_migration(default_root, platform, environ)
            if migrated_root:
                return migrated_root, False
        return default_root, False
    return os.path.join(os.path.expanduser("~"), ".local", "share", "liveaudio"), False

def _prompt_migration(old_root, platform, environ):
    import tkinter as tk
    from tkinter import filedialog
    import threading

    result_path = [None]
    
    def run_dialog():
        root = tk.Tk()
        root.title("LiveAudio - Migración")
        root.geometry("450x180")
        root.eval('tk::PlaceWindow . center')
        
        tk.Label(
            root, text="Se ha detectado una instalación previa en tu disco C:.\nAhora puedes migrar LiveAudio a otra carpeta o disco.",
            font=("Segoe UI", 10), justify="center"
        ).pack(pady=20)
        
        btn_frame = tk.Frame(root)
        btn_frame.pack(pady=10)
        
        def keep_c():
            result_path[0] = old_root
            write_install_location(old_root, os.path.join(old_root, "hf-cache"), platform, environ)
            root.destroy()
            
        def migrate():
            new_dir = filedialog.askdirectory(title="Selecciona la nueva carpeta para LiveAudio", parent=root)
            if new_dir:
                new_root = os.path.join(new_dir, "LiveAudio")
                try:
                    import shutil
                    shutil.move(old_root, new_root)
                    result_path[0] = new_root
                    write_install_location(new_root, os.path.join(new_root, "hf-cache"), platform, environ)
                except Exception as e:
                    import tkinter.messagebox as mb
                    mb.showerror("Error", f"Fallo al migrar: {e}", parent=root)
                    result_path[0] = old_root
                root.destroy()

        tk.Button(btn_frame, text="Mantener en C:", command=keep_c, width=15).pack(side="left", padx=10)
        tk.Button(btn_frame, text="Migrar a nueva ruta", command=migrate, width=20, bg="#3c9e66", fg="white").pack(side="left", padx=10)
        
        root.protocol("WM_DELETE_WINDOW", keep_c)
        root.mainloop()

    run_dialog()
    return result_path[0]


def resolve_app_home(install_root, environ=None):
    """Data home (config.json + sessions/) forwarded to the app.

    An explicit LIVEAUDIO_HOME in the parent environment wins; otherwise the
    app data lives under the install root so portable installs stay
    self-contained.
    """
    environ = os.environ if environ is None else environ
    explicit = environ.get("LIVEAUDIO_HOME")
    if explicit:
        return explicit
    return os.path.join(install_root, "data")


def app_dir(install_root):
    return os.path.join(install_root, "app")


def installed_json_path(install_root):
    return os.path.join(install_root, "installed.json")


def venv_app_exe(install_root, platform=None):
    """Path of the installed gui-script entry point inside app/.venv."""
    platform = platform or sys.platform
    if platform == "win32":
        return os.path.join(app_dir(install_root), ".venv", "Scripts", "liveaudio.exe")
    return os.path.join(app_dir(install_root), ".venv", "bin", "liveaudio")


def _version_tuple(version):
    """Parse 'v1.2.3' / '1.2.3' into an int tuple; None when malformed."""
    if not version or not isinstance(version, str):
        return None
    cleaned = version.strip()
    if cleaned[:1] in ("v", "V"):
        cleaned = cleaned[1:]
    try:
        return tuple(int(part) for part in cleaned.split("."))
    except ValueError:
        return None


def installed_version_satisfies(installed_version, meta_version):
    """True when the installed app is the same or NEWER than ``meta_version``.

    The in-app updater (``--update vX.Y.Z``) moves installed.json ahead of the
    frozen launcher's baked version; a strict equality check would make the
    next normal run re-bootstrap (downgrade to) the old version. Malformed
    version strings fall back to exact string equality, so an unparsable
    mismatch still triggers a bootstrap.
    """
    installed_parsed = _version_tuple(installed_version)
    meta_parsed = _version_tuple(meta_version)
    if installed_parsed is None or meta_parsed is None:
        return installed_version == meta_version
    return installed_parsed >= meta_parsed


def read_installed(install_root):
    """Read installed.json; returns None when missing or unreadable."""
    path = installed_json_path(install_root)
    try:
        with open(path, "r", encoding="utf-8") as fh:
            data = json.load(fh)
    except (OSError, ValueError):
        return None
    if not isinstance(data, dict):
        return None
    return data


def mark_install_dirty(install_root):
    """Delete installed.json right before app/ is cleared or replaced.

    If a bootstrap is interrupted (or ``uv sync`` fails after the new source
    already replaced app/), a stale installed.json would let the fast path
    launch new code on old dependencies. Deleting it first means any failure
    leaves the install in a "needs bootstrap" state; a successful bootstrap
    rewrites the file as its final step (write_installed).
    """
    try:
        os.unlink(installed_json_path(install_root))
    except OSError:
        pass


def write_installed(install_root, data):
    """Write installed.json atomically (temp file + os.replace).

    This is the LAST step of a successful bootstrap: its presence is the
    marker that the install is complete and consistent.
    """
    os.makedirs(install_root, exist_ok=True)
    path = installed_json_path(install_root)
    fd, tmp = tempfile.mkstemp(prefix="installed-", suffix=".tmp", dir=install_root)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            json.dump(data, fh, indent=2, sort_keys=True)
            fh.write("\n")
            fh.flush()
            os.fsync(fh.fileno())
        os.replace(tmp, path)
    except BaseException:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise
    return path


# ---------------------------------------------------------------------------
# Hardware detection
# ---------------------------------------------------------------------------

def _nvml_available(platform=None, environ=None):
    """True when the NVIDIA management library is present on this machine."""
    platform = platform or sys.platform
    environ = os.environ if environ is None else environ
    if platform == "win32":
        system_root = environ.get("SystemRoot", r"C:\Windows")
        return os.path.isfile(os.path.join(system_root, "System32", "nvml.dll"))
    return ctypes.util.find_library("nvidia-ml") is not None


def _query_nvidia_smi(timeout=NVIDIA_SMI_TIMEOUT):
    """Return (driver_version_str, vram_mib) from nvidia-smi, or None."""
    try:
        proc = subprocess.run(
            [
                "nvidia-smi",
                "--query-gpu=driver_version,memory.total",
                "--format=csv,noheader",
            ],
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except (OSError, subprocess.TimeoutExpired, subprocess.SubprocessError) as exc:
        LOG.info("nvidia-smi unavailable: %s", exc)
        return None
    if proc.returncode != 0:
        LOG.info("nvidia-smi exited %s: %s", proc.returncode, proc.stderr.strip())
        return None
    first_line = proc.stdout.strip().splitlines()
    if not first_line:
        return None
    parts = [p.strip() for p in first_line[0].split(",")]
    if len(parts) < 2:
        return None
    driver = parts[0]
    vram_match = re.search(r"(\d+)", parts[1])
    if not vram_match:
        return None
    return driver, int(vram_match.group(1))


def detect_device(cli_device=None, persisted=None, platform=None, environ=None, notify=None):
    """Decide the torch extra to install: "cpu" or "cu121".

    Ladder (each step logged):
    1. --device CLI flag wins (cuda -> cu121).
    2. Persisted preference from installed.json.
    3. NVML present + nvidia-smi reports driver >= 525 and VRAM >= 4096 MiB
       -> cu121; otherwise cpu (with a user-facing reason).
    """
    notify = notify or (lambda message: None)
    if cli_device:
        device = "cu121" if cli_device == "cuda" else "cpu"
        LOG.info("Device forced via --device %s -> %s", cli_device, device)
        return device
    if persisted in ("cpu", "cu121"):
        LOG.info("Device taken from persisted preference: %s", persisted)
        return persisted
    if not _nvml_available(platform=platform, environ=environ):
        LOG.info("NVML not found -> cpu")
        return "cpu"
    info = _query_nvidia_smi()
    if info is None:
        LOG.info("NVML present but nvidia-smi failed -> cpu")
        return "cpu"
    driver, vram_mib = info
    try:
        driver_major = int(driver.split(".")[0])
    except ValueError:
        LOG.info("Unparseable driver version %r -> cpu", driver)
        return "cpu"
    LOG.info("nvidia-smi: driver=%s vram=%d MiB", driver, vram_mib)
    if driver_major < MIN_DRIVER_MAJOR or vram_mib < MIN_VRAM_MIB:
        LOG.info(
            "GPU below requirements (driver >= %d, VRAM >= %d MiB) -> cpu",
            MIN_DRIVER_MAJOR,
            MIN_VRAM_MIB,
        )
        notify(GPU_FALLBACK_MESSAGE)
        return "cpu"
    LOG.info("GPU qualifies -> cu121")
    return "cu121"


# ---------------------------------------------------------------------------
# uv discovery / provisioning
# ---------------------------------------------------------------------------

def uv_exe_name(platform=None):
    platform = platform or sys.platform
    return "uv.exe" if platform == "win32" else "uv"


def _bundled_uv_dirs():
    """Where a bundled uv binary may live (next to the frozen launcher)."""
    dirs = list(_bundled_data_dirs())
    if not getattr(sys, "frozen", False):
        # Dev checkout: CI drops vendored binaries in packaging/vendor/.
        dirs.append(os.path.join(_launcher_dir(), "vendor"))
    return dirs


def find_uv(install_root, platform=None, bundled_dirs=None, which=None):
    """Locate uv: bundled > PATH > previously downloaded. None -> download."""
    platform = platform or sys.platform
    which = which or shutil.which
    exe = uv_exe_name(platform)
    dirs = _bundled_uv_dirs() if bundled_dirs is None else bundled_dirs
    for directory in dirs:
        candidate = os.path.join(directory, exe)
        if os.path.isfile(candidate):
            LOG.info("Using bundled uv: %s", candidate)
            return candidate
    on_path = which("uv")
    if on_path:
        LOG.info("Using uv from PATH: %s", on_path)
        return on_path
    downloaded = os.path.join(install_root, "uv", exe)
    if os.path.isfile(downloaded):
        LOG.info("Using previously downloaded uv: %s", downloaded)
        return downloaded
    return None


def _uv_release_asset(platform=None, machine=None):
    platform = platform or sys.platform
    machine = (machine or _platform_mod.machine()).lower()
    arm = machine in ("arm64", "aarch64")
    if platform == "win32":
        return "uv-aarch64-pc-windows-msvc.zip" if arm else "uv-x86_64-pc-windows-msvc.zip"
    if platform == "darwin":
        return "uv-aarch64-apple-darwin.tar.gz" if arm else "uv-x86_64-apple-darwin.tar.gz"
    return "uv-aarch64-unknown-linux-gnu.tar.gz" if arm else "uv-x86_64-unknown-linux-gnu.tar.gz"


def download_uv(install_root, reporter, cancel=None):
    """Download the official uv archive and unpack the binary into <root>/uv."""
    asset = _uv_release_asset()
    url = "%s/%s" % (UV_RELEASE_BASE, asset)
    uv_dir = os.path.join(install_root, "uv")
    os.makedirs(uv_dir, exist_ok=True)
    archive = os.path.join(uv_dir, asset)
    reporter.status("Downloading uv (package manager)...")
    download_with_retries(url, archive, expected_sha256=None, reporter=reporter, cancel=cancel)
    exe = uv_exe_name()
    target = os.path.join(uv_dir, exe)
    if asset.endswith(".zip"):
        with zipfile.ZipFile(archive) as zf:
            member = next(n for n in zf.namelist() if n.endswith(exe))
            with zf.open(member) as src, open(target, "wb") as dst:
                shutil.copyfileobj(src, dst)
    else:
        with tarfile.open(archive, "r:gz") as tf:
            member = next(m for m in tf.getmembers() if m.name.endswith("/uv") or m.name == "uv")
            extracted = tf.extractfile(member)
            with open(target, "wb") as dst:
                shutil.copyfileobj(extracted, dst)
    os.unlink(archive)
    if sys.platform != "win32":
        os.chmod(target, 0o755)
    return target


def uv_version(uv_path):
    """Return the uv version string, or None if it cannot run."""
    try:
        proc = subprocess.run(
            [uv_path, "--version"], capture_output=True, text=True, timeout=30
        )
    except (OSError, subprocess.SubprocessError):
        return None
    if proc.returncode != 0:
        return None
    # Output looks like "uv 0.7.13 (abc123 2025-01-01)".
    parts = proc.stdout.split()
    return parts[1] if len(parts) >= 2 else proc.stdout.strip()


def ensure_uv(install_root, reporter, cancel=None):
    """Find or download uv; verify it runs. Returns (path, version)."""
    uv_path = find_uv(install_root)
    if uv_path is None:
        uv_path = download_uv(install_root, reporter, cancel=cancel)
    version = uv_version(uv_path)
    if version is None:
        raise LauncherError("uv at %s is not runnable" % uv_path)
    LOG.info("uv %s at %s", version, uv_path)
    return uv_path, version


# ---------------------------------------------------------------------------
# Download / verify / unpack
# ---------------------------------------------------------------------------

def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_sha256(path, expected):
    """Raise ChecksumError when the file does not match the expected digest."""
    actual = sha256_file(path)
    if actual.lower() != expected.lower():
        raise ChecksumError(
            "SHA256 mismatch for %s\n  expected: %s\n  actual:   %s"
            % (os.path.basename(path), expected, actual)
        )


def _http_open(url, timeout=60):
    request = urllib.request.Request(url, headers={"User-Agent": "LiveAudio-Launcher"})
    return urllib.request.urlopen(request, timeout=timeout)


def download_with_retries(url, dest, expected_sha256=None, reporter=None, cancel=None,
                          retries=DOWNLOAD_RETRIES):
    """Download url -> dest with MB progress, SHA256 check and retry/backoff."""
    last_error = None
    for attempt in range(1, retries + 1):
        try:
            _download_once(url, dest, reporter=reporter, cancel=cancel)
            if expected_sha256:
                verify_sha256(dest, expected_sha256)
            return dest
        except CancelledError:
            raise
        except (urllib.error.URLError, OSError, ChecksumError) as exc:
            last_error = exc
            LOG.warning("Download attempt %d/%d failed: %s", attempt, retries, exc)
            if attempt < retries:
                delay = 2 ** attempt
                if reporter:
                    reporter.status("Download failed, retrying in %ds..." % delay)
                time.sleep(delay)
    raise LauncherError("Download failed after %d attempts: %s" % (retries, last_error))


def _download_once(url, dest, reporter=None, cancel=None):
    name = os.path.basename(dest)
    with _http_open(url) as response:
        total = int(response.headers.get("Content-Length") or 0)
        done = 0
        last_report = 0.0
        with open(dest, "wb") as fh:
            while True:
                if cancel is not None and cancel.cancelled():
                    raise CancelledError()
                chunk = response.read(256 * 1024)
                if not chunk:
                    break
                fh.write(chunk)
                done += len(chunk)
                now = time.monotonic()
                if reporter and now - last_report > 0.25:
                    last_report = now
                    mb_done = done / (1024.0 * 1024.0)
                    if total:
                        mb_total = total / (1024.0 * 1024.0)
                        reporter.status(
                            "Downloading %s (%.1f / %.1f MB)" % (name, mb_done, mb_total)
                        )
                        reporter.progress(0.05 + 0.25 * (done / total))
                    else:
                        reporter.status("Downloading %s (%.1f MB)" % (name, mb_done))
    LOG.info("Downloaded %s (%d bytes)", url, done)


def _clear_app_dir(directory, keep_venv=True):
    """Remove app/ contents, optionally preserving the .venv directory."""
    if not os.path.isdir(directory):
        return
    for entry in os.listdir(directory):
        if keep_venv and entry == ".venv":
            continue
        path = os.path.join(directory, entry)
        if os.path.isdir(path) and not os.path.islink(path):
            shutil.rmtree(path)
        else:
            os.unlink(path)


def unpack_src_zip(zip_path, target_dir):
    """Unpack the source zip into app/, stripping a single top-level dir."""
    staging = tempfile.mkdtemp(prefix="liveaudio-src-", dir=os.path.dirname(target_dir))
    try:
        with zipfile.ZipFile(zip_path) as zf:
            zf.extractall(staging)
        entries = os.listdir(staging)
        if len(entries) == 1 and os.path.isdir(os.path.join(staging, entries[0])):
            content_root = os.path.join(staging, entries[0])
        else:
            content_root = staging
        os.makedirs(target_dir, exist_ok=True)
        _clear_app_dir(target_dir, keep_venv=True)
        for entry in os.listdir(content_root):
            shutil.move(os.path.join(content_root, entry), os.path.join(target_dir, entry))
    finally:
        shutil.rmtree(staging, ignore_errors=True)


# Files copied from a dev checkout. README.md and LICENSE are required by the
# hatchling build (referenced from pyproject.toml).
_SRC_FILES = ("pyproject.toml", "uv.lock", ".python-version", "README.md", "LICENSE")


def copy_src_tree(src_dir, target_dir):
    """Dev mode: copy the minimal project files from a local checkout."""
    os.makedirs(target_dir, exist_ok=True)
    _clear_app_dir(target_dir, keep_venv=True)
    for name in _SRC_FILES:
        source = os.path.join(src_dir, name)
        if os.path.isfile(source):
            shutil.copy2(source, os.path.join(target_dir, name))
    shutil.copytree(
        os.path.join(src_dir, "liveaudio"),
        os.path.join(target_dir, "liveaudio"),
        ignore=shutil.ignore_patterns("__pycache__", "*.pyc"),
    )


# ---------------------------------------------------------------------------
# uv sync
# ---------------------------------------------------------------------------

def run_uv_sync(uv_path, project_dir, extra, install_root, reporter, cancel=None):
    """Run `uv sync --locked --no-dev --extra <extra> --python 3.11` streaming output."""
    command = [
        uv_path,
        "sync",
        "--locked",
        # End-user installs must not pull the default dev group (pytest, ruff).
        "--no-dev",
        "--extra",
        extra,
        "--python",
        PYTHON_VERSION,
        "--color",
        "never",
    ]
    env = os.environ.copy()
    # Keep the managed CPython inside the install root too.
    env["UV_PYTHON_INSTALL_DIR"] = os.path.join(install_root, "python")
    LOG.info("Running: %s (cwd=%s)", " ".join(command), project_dir)
    reporter.status("Installing dependencies (%s)..." % extra)
    creationflags = 0
    if sys.platform == "win32":
        creationflags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
    proc = subprocess.Popen(
        command,
        cwd=project_dir,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
        creationflags=creationflags,
    )
    if cancel is not None:
        cancel.proc = proc
    fraction = 0.45
    try:
        for line in proc.stdout:
            line = line.rstrip()
            if not line:
                continue
            LOG.info("uv: %s", line)
            reporter.detail(line)
            lowered = line.lower()
            if "torch" in lowered and ("download" in lowered or lowered.startswith("+ torch")):
                reporter.status(
                    "Downloading PyTorch (~2.4 GB on CUDA) — this is the big one"
                )
            if any(key in lowered for key in ("downloading", "downloaded", "prepared", "installed", "audited")) or line.startswith(" + "):
                fraction = min(0.95, fraction + 0.01)
                reporter.progress(fraction)
            if cancel is not None and cancel.cancelled():
                proc.terminate()
                raise CancelledError()
        proc.wait()
    finally:
        if cancel is not None:
            cancel.proc = None
    if cancel is not None and cancel.cancelled():
        raise CancelledError()
    if proc.returncode != 0:
        raise LauncherError("uv sync failed with exit code %d (see bootstrap.log)" % proc.returncode)
    reporter.progress(0.97)


# ---------------------------------------------------------------------------
# App launch
# ---------------------------------------------------------------------------

def check_portaudio(platform=None):
    """Linux preflight: True when the PortAudio runtime is available."""
    platform = platform or sys.platform
    if platform == "win32":
        return True
    return ctypes.util.find_library("portaudio") is not None


def launch_app(install_root, portable, environ=None, platform=None, notify=None):
    """Start the installed app detached.

    Returns the (truthy) Popen handle on success, False when the app
    executable is missing. NOTE: on Windows the handle is a uv trampoline —
    the real interpreter runs as a grandchild — so ``poll()`` going non-None
    is only a "definitely dead" signal, never proof the app window is up.
    """
    environ = os.environ if environ is None else environ
    platform = platform or sys.platform
    notify = notify or (lambda message: None)
    exe = venv_app_exe(install_root, platform)
    if not os.path.isfile(exe):
        LOG.warning("App executable missing: %s", exe)
        return False
    if not check_portaudio(platform):
        LOG.warning(PORTAUDIO_MESSAGE)
        notify(PORTAUDIO_MESSAGE)
    env = dict(environ)
    env["LIVEAUDIO_HOME"] = resolve_app_home(install_root, environ)
    if getattr(sys, "frozen", False):
        # Let the app's in-app updater find us to run `--update <tag>`.
        env["LIVEAUDIO_LAUNCHER"] = os.path.abspath(sys.executable)
    
    location_data = read_install_location(platform, environ)
    if location_data and "hf_home" in location_data:
        env["HF_HOME"] = location_data["hf_home"]
    elif portable:
        env["HF_HOME"] = os.path.join(install_root, "hf-cache")
    os.makedirs(env["LIVEAUDIO_HOME"], exist_ok=True)
    LOG.info("Launching %s (LIVEAUDIO_HOME=%s)", exe, env["LIVEAUDIO_HOME"])
    kwargs = {"env": env, "cwd": os.path.dirname(exe), "close_fds": True}
    if platform == "win32":
        kwargs["creationflags"] = getattr(subprocess, "DETACHED_PROCESS", 0) | getattr(
            subprocess, "CREATE_NEW_PROCESS_GROUP", 0
        )
    else:
        kwargs["start_new_session"] = True
    return subprocess.Popen([exe], **kwargs)


def _find_app_window():
    """Truthy when the app's top-level window exists (Windows only)."""
    if sys.platform != "win32":
        return None
    try:
        import ctypes

        return ctypes.windll.user32.FindWindowW(None, APP_WINDOW_TITLE) or None
    except Exception:
        return None


def wait_for_app_window(find_window=None, proc_alive=None, timeout=APP_WINDOW_TIMEOUT,
                        interval=0.5, sleep=time.sleep, on_tick=None):
    """Poll until the app window appears.

    Returns "found" when the window exists, "died" when ``proc_alive()``
    reports the process gone before the window appeared, "timeout" otherwise.
    The window appearing is the success signal; "timeout" with the process
    still alive usually just means a slow machine.
    """
    if find_window is None:
        find_window = _find_app_window
    elapsed = 0.0
    while True:
        if find_window():
            return "found"
        if proc_alive is not None and not proc_alive():
            return "died"
        if elapsed >= timeout:
            return "timeout"
        if on_tick is not None:
            on_tick()
        sleep(interval)
        elapsed += interval


# ---------------------------------------------------------------------------
# Progress reporters
# ---------------------------------------------------------------------------

class CancelToken:
    """Cooperative cancellation shared between the UI and the worker."""

    def __init__(self):
        self._event = threading.Event()
        self.proc = None

    def cancelled(self):
        return self._event.is_set()

    def cancel(self):
        self._event.set()
        proc = self.proc
        if proc is not None:
            try:
                proc.terminate()
            except OSError:
                pass


class HeadlessReporter:
    """Plain-stdout progress for --headless / no-display environments."""

    def __init__(self):
        self._last_status = None

    def _print(self, text):
        if sys.stdout is not None:
            print(text, flush=True)

    def status(self, text):
        if text != self._last_status:
            self._last_status = text
            self._print("[setup] %s" % text)

    def progress(self, fraction):
        pass  # status lines already carry the useful information

    def detail(self, line):
        self._print("    %s" % line)

    def notify(self, message):
        self._print("[setup] NOTE: %s" % message)


class TkReporter:
    """Tkinter splash window with progress, details pane and cancel/retry."""

    def __init__(self, version):
        import tkinter as tk
        from tkinter import ttk

        self._tk = tk
        self.cancel_token = CancelToken()
        self._queue = queue.Queue()
        self._worker_factory = None
        self.exit_code = 1
        self._details_visible = False

        self.root = tk.Tk()
        self.root.title("LiveAudio Setup")
        self.root.geometry("440x240")
        self.root.minsize(440, 240)
        self.root.configure(bg=COLOR_BG)
        self.root.protocol("WM_DELETE_WINDOW", self._on_cancel)
        _set_window_icon(self.root)

        style = ttk.Style(self.root)
        try:
            style.theme_use("clam")
        except tk.TclError:
            pass
        style.configure(
            "LA.Horizontal.TProgressbar",
            troughcolor=COLOR_PANEL,
            background=COLOR_ACCENT,
            bordercolor=COLOR_BG,
            lightcolor=COLOR_ACCENT,
            darkcolor=COLOR_ACCENT,
        )

        tk.Label(
            self.root, text="LiveAudio Setup", bg=COLOR_BG, fg=COLOR_FG,
            font=("Segoe UI", 14, "bold"),
        ).pack(pady=(18, 0))
        tk.Label(
            self.root, text="version %s" % version, bg=COLOR_BG, fg=COLOR_MUTED,
            font=("Segoe UI", 9),
        ).pack()

        self._status = tk.Label(
            self.root, text="Preparing...", bg=COLOR_BG, fg=COLOR_FG,
            font=("Segoe UI", 10), wraplength=400, justify="center",
        )
        self._status.pack(pady=(12, 6))

        self._bar = ttk.Progressbar(
            self.root, style="LA.Horizontal.TProgressbar", orient="horizontal",
            mode="determinate", maximum=100, length=380,
        )
        self._bar.pack(pady=(0, 10))

        buttons = tk.Frame(self.root, bg=COLOR_BG)
        buttons.pack()
        self._details_btn = tk.Button(
            buttons, text="Details ▸", command=self._toggle_details,
            bg=COLOR_PANEL, fg=COLOR_FG, activebackground=COLOR_PANEL,
            activeforeground=COLOR_FG, relief="flat", padx=10,
        )
        self._details_btn.pack(side="left", padx=6)
        self._action_btn = tk.Button(
            buttons, text="Cancel", command=self._on_cancel,
            bg=COLOR_PANEL, fg=COLOR_FG, activebackground=COLOR_PANEL,
            activeforeground=COLOR_FG, relief="flat", padx=10,
        )
        self._action_btn.pack(side="left", padx=6)
        self._log_btn = None

        self._details = tk.Text(
            self.root, height=9, bg=COLOR_PANEL, fg=COLOR_MUTED,
            insertbackground=COLOR_FG, relief="flat", font=("Consolas", 8),
            state="disabled", wrap="none",
        )
        self._log_path = None
        self.root.after(50, self._poll)

    # -- worker-side API (thread-safe) --------------------------------------

    def status(self, text):
        self._queue.put(("status", text))

    def progress(self, fraction):
        self._queue.put(("progress", fraction))

    def detail(self, line):
        self._queue.put(("detail", line))

    def notify(self, message):
        self._queue.put(("status", message))

    def report_success(self):
        self._queue.put(("success", None))

    def report_error(self, message):
        self._queue.put(("error", message))

    def report_cancelled(self):
        self._queue.put(("close", None))

    # -- UI side -------------------------------------------------------------

    def start(self, worker_factory, log_path=None):
        """worker_factory() -> threading.Thread target callable."""
        self._worker_factory = worker_factory
        self._log_path = log_path
        self._spawn_worker()
        self.root.mainloop()
        return self.exit_code

    def _spawn_worker(self):
        thread = threading.Thread(target=self._worker_factory, daemon=True)
        thread.start()

    def _poll(self):
        try:
            while True:
                kind, payload = self._queue.get_nowait()
                if kind == "status":
                    self._status.config(text=payload)
                elif kind == "progress":
                    self._bar["value"] = max(0.0, min(1.0, payload)) * 100
                elif kind == "detail":
                    self._append_detail(payload)
                elif kind == "success":
                    self.exit_code = 0
                    self.root.destroy()
                    return
                elif kind == "close":
                    self.root.destroy()
                    return
                elif kind == "error":
                    self._show_error(payload)
        except queue.Empty:
            pass
        self.root.after(50, self._poll)

    def _append_detail(self, line):
        self._details.config(state="normal")
        self._details.insert("end", line + "\n")
        self._details.see("end")
        self._details.config(state="disabled")

    def _toggle_details(self):
        if self._details_visible:
            self._details.pack_forget()
            self.root.geometry("440x240")
            self._details_btn.config(text="Details ▸")
        else:
            self._details.pack(fill="both", expand=True, padx=12, pady=(8, 12))
            self.root.geometry("440x420")
            self._details_btn.config(text="Details ▾")
        self._details_visible = not self._details_visible

    def _on_cancel(self):
        self.cancel_token.cancel()
        self.exit_code = 1
        self.root.destroy()

    def _show_error(self, message):
        self._status.config(text="Setup failed: %s" % message, fg="#e07a5f")
        self._action_btn.config(text="Retry", command=self._on_retry)
        if self._log_btn is None and self._log_path:
            self._log_btn = self._tk.Button(
                self._action_btn.master, text="Open log", command=self._open_log,
                bg=COLOR_PANEL, fg=COLOR_FG, activebackground=COLOR_PANEL,
                activeforeground=COLOR_FG, relief="flat", padx=10,
            )
            self._log_btn.pack(side="left", padx=6)

    def _on_retry(self):
        self.cancel_token = CancelToken()
        self._status.config(text="Retrying...", fg=COLOR_FG)
        self._bar["value"] = 0
        self._action_btn.config(text="Cancel", command=self._on_cancel)
        self._spawn_worker()

    def _open_log(self):
        if not self._log_path:
            return
        if sys.platform == "win32":
            os.startfile(self._log_path)  # noqa: S606
        else:
            subprocess.Popen(["xdg-open", self._log_path])


def _find_asset(name):
    """Locate a bundled asset (icon, .desktop template) in dev and frozen runs."""
    candidates = []
    for directory in _bundled_data_dirs():
        candidates.append(os.path.join(directory, name))
    # Dev checkout: packaging/../liveaudio/assets/.
    candidates.append(os.path.join(_launcher_dir(), "..", "liveaudio", "assets", name))
    for candidate in candidates:
        if os.path.isfile(candidate):
            return os.path.abspath(candidate)
    return None


def _set_window_icon(tk_root):
    icon_png = _find_asset("LiveAudio-Fran.png")
    if not icon_png:
        return
    try:
        import tkinter as tk

        image = tk.PhotoImage(file=icon_png)
        tk_root.iconphoto(True, image)
        tk_root._liveaudio_icon = image  # keep a reference alive
    except Exception:  # pragma: no cover - cosmetic only
        LOG.debug("Could not set window icon", exc_info=True)


# ---------------------------------------------------------------------------
# Bootstrap orchestration
# ---------------------------------------------------------------------------

def run_bootstrap(meta, install_root, portable, device, reporter, cancel=None,
                  no_launch=False):
    """Full bootstrap: fetch source, uv sync, write installed.json, launch.

    Returns the Popen handle of the launched app, or None when ``no_launch``
    is set (or the launch failed); GUI callers use it to keep the splash open
    until the app window appears.
    """
    target = app_dir(install_root)
    os.makedirs(install_root, exist_ok=True)
    reporter.progress(0.02)

    if meta.src_dir:
        reporter.status("Copying source from %s..." % meta.src_dir)
        mark_install_dirty(install_root)
        copy_src_tree(meta.src_dir, target)
    else:
        zip_path = os.path.join(install_root, "liveaudio-src.zip")
        download_with_retries(
            meta.src_zip_url, zip_path, expected_sha256=meta.src_zip_sha256,
            reporter=reporter, cancel=cancel,
        )
        reporter.status("Unpacking source...")
        mark_install_dirty(install_root)
        unpack_src_zip(zip_path, target)
        os.unlink(zip_path)
    if cancel is not None and cancel.cancelled():
        raise CancelledError()
    reporter.progress(0.35)

    reporter.status("Preparing uv...")
    uv_path, uv_ver = ensure_uv(install_root, reporter, cancel=cancel)
    reporter.progress(0.45)

    run_uv_sync(uv_path, target, device, install_root, reporter, cancel=cancel)

    exe = venv_app_exe(install_root)
    if not os.path.isfile(exe):
        raise LauncherError("uv sync succeeded but app executable missing: %s" % exe)

    write_installed(
        install_root,
        {
            "app_version": meta.version,
            "extra": device,
            "python": PYTHON_VERSION,
            "uv_version": uv_ver,
        },
    )
    reporter.progress(0.99)
    LOG.info("Bootstrap complete: version=%s extra=%s", meta.version, device)

    popen = None
    if not no_launch:
        reporter.status("Launching LiveAudio...")
        popen = launch_app(install_root, portable, notify=reporter.notify) or None
    reporter.progress(1.0)
    return popen


def _bootstrap_headless(meta, install_root, portable, device, no_launch, notes=()):
    reporter = HeadlessReporter()
    for note in notes:
        reporter.notify(note)
    try:
        run_bootstrap(meta, install_root, portable, device, reporter, no_launch=no_launch)
    except CancelledError:
        reporter.status("Cancelled.")
        return 1
    except LauncherError as exc:
        LOG.error("Bootstrap failed: %s", exc)
        reporter.status("ERROR: %s" % exc)
        reporter.status("Log: %s" % os.path.join(install_root, "bootstrap.log"))
        return 1
    reporter.status("Done.")
    return 0


def _await_app_window_gui(reporter, popen):
    """Keep the splash informative until the app window appears (Windows).

    Returns True when the splash should close normally, False when an error
    was already reported (the splash stays open showing it). On non-Windows
    platforms window detection is unavailable, so this is a no-op.
    """
    if popen is None or sys.platform != "win32":
        return True
    reporter.status("Starting LiveAudio...")
    reporter.progress(1.0)
    result = wait_for_app_window(proc_alive=lambda: popen.poll() is None)
    if result == "died":
        LOG.error("App process exited before its window appeared")
        reporter.report_error(APP_START_FAILED_MESSAGE)
        return False
    if result == "timeout":
        # Process still alive: slow machine, the window will appear; close
        # the splash quietly rather than scaring the user with an error.
        LOG.warning(
            "App window not seen within %.0fs; assuming slow start", APP_WINDOW_TIMEOUT
        )
    else:
        LOG.info("App window detected")
    return True


def _bootstrap_gui(meta, install_root, portable, device, no_launch, notes=()):
    try:
        reporter = TkReporter(meta.version)
    except Exception as exc:  # tkinter missing or no display
        LOG.info("tkinter unavailable (%s), falling back to headless", exc)
        return _bootstrap_headless(meta, install_root, portable, device, no_launch, notes)
    for note in notes:
        reporter.detail("NOTE: %s" % note)

    def worker():
        try:
            popen = run_bootstrap(
                meta, install_root, portable, device, reporter,
                cancel=reporter.cancel_token, no_launch=no_launch,
            )
        except CancelledError:
            reporter.report_cancelled()
        except Exception as exc:
            LOG.exception("Bootstrap failed")
            reporter.report_error(str(exc))
        else:
            if _await_app_window_gui(reporter, popen):
                reporter.report_success()

    log_path = os.path.join(install_root, "bootstrap.log")
    return reporter.start(worker, log_path=log_path)


def _fast_path_gui(version, install_root, portable):
    """Windows fast path: minimal splash until the app window appears.

    A cold start still spends seconds on interpreter and GUI-toolkit imports
    before the first window paints, with zero feedback. Reuse the existing
    splash (TkReporter) so users know something is happening.
    """
    try:
        reporter = TkReporter(version or "")
    except Exception as exc:  # tkinter missing or no display
        LOG.info("tkinter unavailable (%s), launching without splash", exc)
        return 0 if launch_app(install_root, portable) else 1

    def worker():
        try:
            reporter.status("Starting LiveAudio...")
            popen = launch_app(install_root, portable, notify=reporter.notify) or None
        except Exception:
            LOG.exception("Fast-path launch failed")
            reporter.report_error(APP_START_FAILED_MESSAGE)
            return
        if popen is None:
            reporter.report_error(APP_START_FAILED_MESSAGE)
            return
        if _await_app_window_gui(reporter, popen):
            reporter.report_success()

    log_path = os.path.join(install_root, "bootstrap.log")
    return reporter.start(worker, log_path=log_path)


# ---------------------------------------------------------------------------
# Update via GitHub releases
# ---------------------------------------------------------------------------

def _github_api_json(url):
    with _http_open(url) as response:
        return json.loads(response.read().decode("utf-8"))


def fetch_release_meta(tag="latest", repo=GITHUB_REPO):
    """Build a ReleaseMeta from a GitHub release (src zip + checksums asset)."""
    if tag == "latest":
        url = "https://api.github.com/repos/%s/releases/latest" % repo
    else:
        url = "https://api.github.com/repos/%s/releases/tags/%s" % (repo, tag)
    release = _github_api_json(url)
    assets = release.get("assets", [])
    src_asset = None
    sums_asset = None
    for asset in assets:
        name = asset.get("name", "")
        if name.startswith("liveaudio-src") and name.endswith(".zip"):
            src_asset = asset
        elif name.endswith(".sha256") or "checksums" in name.lower() or "sha256sums" in name.lower():
            sums_asset = asset
    if src_asset is None:
        raise LauncherError("Release %s has no liveaudio-src-*.zip asset" % release.get("tag_name"))
    expected = None
    if sums_asset is not None:
        with _http_open(sums_asset["browser_download_url"]) as response:
            sums_text = response.read().decode("utf-8")
        expected = parse_checksums(sums_text, src_asset["name"])
    if expected is None:
        raise LauncherError(
            "Could not find a SHA256 checksum for %s in release assets" % src_asset["name"]
        )
    version = (release.get("tag_name") or "").lstrip("v")
    return ReleaseMeta(
        version=version,
        src_zip_url=src_asset["browser_download_url"],
        src_zip_sha256=expected,
    )


def parse_checksums(text, filename):
    """Extract the SHA256 hex digest for filename from a checksums file."""
    for line in text.splitlines():
        parts = line.split()
        if not parts:
            continue
        if len(parts) == 1 and re.fullmatch(r"[0-9a-fA-F]{64}", parts[0]):
            return parts[0]  # bare digest file (e.g. <asset>.sha256)
        if len(parts) >= 2 and parts[1].lstrip("*") == filename:
            return parts[0]
    return None


APP_EXIT_TIMEOUT = 60  # seconds to wait for the running app before an update


def _app_exe_released(exe):
    """Probe whether the installed app executable is no longer locked.

    Windows locks the executable of every running process, so a rename
    round-trip only succeeds once the app has exited. On Linux the rename
    succeeds even while the app runs, which is acceptable: replacing files
    under a live process is safe there.
    """
    probe = exe + ".update-probe"
    try:
        os.replace(exe, probe)
        os.replace(probe, exe)
    except OSError:
        return False
    return True


def wait_for_app_exit(install_root, timeout=APP_EXIT_TIMEOUT, interval=1.0):
    """Wait until the running app releases its executable; raise on timeout.

    The in-app updater spawns the launcher while the app is still shutting
    down; clearing app/ at that point would corrupt the install, so updates
    must not proceed while the app executable is still locked.
    """
    exe = venv_app_exe(install_root)
    if not os.path.isfile(exe):
        return
    deadline = time.monotonic() + timeout
    announced = False
    while not _app_exe_released(exe):
        if not announced:
            LOG.info("Waiting for LiveAudio to close...")
            print("Waiting for LiveAudio to close...")
            announced = True
        if time.monotonic() >= deadline:
            raise LauncherError(
                "LiveAudio is still running after %d seconds; close it and "
                "retry the update." % int(timeout)
            )
        time.sleep(interval)
    if announced:
        LOG.info("LiveAudio closed; continuing with the update.")


def cmd_update(args, install_root, portable, installed, headless):
    tag = args.update
    LOG.info("Update requested: %s", tag)
    meta = fetch_release_meta(tag)
    if installed and meta.version == installed.get("app_version") and tag == "latest":
        LOG.info("Already at version %s", meta.version)
        print("Already up to date (version %s)." % meta.version)
        return 0
    persisted = installed.get("extra") if installed else None
    device = detect_device(args.device, persisted)
    wait_for_app_exit(install_root)
    if headless:
        return _bootstrap_headless(meta, install_root, portable, device, args.no_launch)
    return _bootstrap_gui(meta, install_root, portable, device, args.no_launch)


# ---------------------------------------------------------------------------
# Desktop entry (Linux)
# ---------------------------------------------------------------------------

def render_desktop_entry(template_text, exec_cmd, icon_path):
    return template_text.format(exec=exec_cmd, icon=icon_path)


def cmd_install_desktop_entry(install_root):
    if sys.platform == "win32":
        print("--install-desktop-entry is only supported on Linux.")
        return 2
    template_path = _find_asset("liveaudio.desktop") or os.path.join(
        _launcher_dir(), "liveaudio.desktop"
    )
    if not os.path.isfile(template_path):
        print("Desktop entry template not found: %s" % template_path)
        return 2
    with open(template_path, "r", encoding="utf-8") as fh:
        template_text = fh.read()
    if getattr(sys, "frozen", False):
        exec_cmd = os.path.abspath(sys.executable)
    else:
        exec_cmd = "%s %s" % (sys.executable, os.path.abspath(__file__))
    icon = os.path.join(app_dir(install_root), "liveaudio", "assets", "LiveAudio-Fran.png")
    if not os.path.isfile(icon):
        bundled = _find_asset("LiveAudio-Fran.png")
        if bundled:
            os.makedirs(install_root, exist_ok=True)
            icon = os.path.join(install_root, "LiveAudio-Fran.png")
            shutil.copy2(bundled, icon)
    applications = os.path.join(
        os.path.expanduser("~"), ".local", "share", "applications"
    )
    os.makedirs(applications, exist_ok=True)
    target = os.path.join(applications, "liveaudio.desktop")
    with open(target, "w", encoding="utf-8") as fh:
        fh.write(render_desktop_entry(template_text, exec_cmd, icon))
    os.chmod(target, 0o755)
    print("Installed desktop entry: %s" % target)
    LOG.info("Installed desktop entry: %s", target)
    return 0


# ---------------------------------------------------------------------------
# Self-test
# ---------------------------------------------------------------------------

def cmd_self_test(args, meta, install_root, portable):
    installed = read_installed(install_root)
    exe = venv_app_exe(install_root)
    uv_path = find_uv(install_root)
    uv_ver = uv_version(uv_path) if uv_path else None
    persisted = installed.get("extra") if installed else None
    device = detect_device(args.device, persisted)

    lines = [
        "LiveAudio launcher self-test",
        "  launcher mode:    %s" % (meta.mode if meta else "none (no metadata, no --src-dir)"),
        "  target version:   %s" % (meta.version if meta else "n/a"),
        "  platform:         %s (%s)" % (sys.platform, _platform_mod.machine()),
        "  install root:     %s" % install_root,
        "  portable:         %s" % ("yes" if portable else "no"),
        "  app home:         %s" % resolve_app_home(install_root),
        "  app executable:   %s (exists: %s)" % (exe, "yes" if os.path.isfile(exe) else "no"),
        "  installed.json:   %s" % (json.dumps(installed) if installed else "missing"),
        "  detected device:  %s" % device,
        "  uv:               %s" % (
            "%s (version %s)" % (uv_path, uv_ver or "NOT RUNNABLE")
            if uv_path else "not found (would download %s)" % _uv_release_asset()
        ),
        "  portaudio:        %s" % ("ok" if check_portaudio() else "MISSING"),
    ]
    output = "\n".join(lines)
    print(output)
    # DEBUG so the stderr handler (INFO) does not duplicate the print output.
    LOG.debug("Self-test:\n%s", output)
    return 0


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def setup_logging(install_root):
    LOG.setLevel(logging.DEBUG)
    formatter = logging.Formatter("%(asctime)s %(levelname)s %(message)s")
    try:
        os.makedirs(install_root, exist_ok=True)
        file_handler = logging.FileHandler(
            os.path.join(install_root, "bootstrap.log"), encoding="utf-8"
        )
        file_handler.setFormatter(formatter)
        LOG.addHandler(file_handler)
    except OSError:
        pass
    # stderr is None when frozen with console=False on Windows.
    if sys.stderr is not None:
        stream_handler = logging.StreamHandler()
        stream_handler.setLevel(logging.INFO)
        stream_handler.setFormatter(formatter)
        LOG.addHandler(stream_handler)


def parse_args(argv=None):
    parser = argparse.ArgumentParser(
        prog="liveaudio-launcher",
        description="LiveAudio bootstrapper: installs and launches the app.",
    )
    parser.add_argument(
        "--device", choices=["cpu", "cuda"], default=None,
        help="force the torch backend (persisted for future runs)",
    )
    parser.add_argument(
        "--update", nargs="?", const="latest", default=None, metavar="vX.Y.Z",
        help="update to the given (or latest) GitHub release",
    )
    parser.add_argument(
        "--self-test", action="store_true",
        help="print detection results and resolved paths, then exit",
    )
    parser.add_argument(
        "--reinstall", action="store_true",
        help="wipe app/ and .venv, then bootstrap fresh",
    )
    parser.add_argument(
        "--install-desktop-entry", action="store_true",
        help="Linux: install a .desktop launcher entry",
    )
    parser.add_argument(
        "--headless", action="store_true",
        help="plain stdout progress instead of the tkinter window",
    )
    parser.add_argument(
        "--src-dir", default=None, metavar="PATH",
        help="dev mode: install from a local checkout (no GitHub release)",
    )
    parser.add_argument("--no-launch", action="store_true", help=argparse.SUPPRESS)
    return parser.parse_args(argv)


def main(argv=None):
    args = parse_args(argv)
    install_root, portable = resolve_install_root()
    setup_logging(install_root)
    LOG.info("Launcher start: argv=%s root=%s portable=%s", argv or sys.argv[1:], install_root, portable)

    try:
        meta = load_release_meta(src_dir=args.src_dir)
    except LauncherError as exc:
        LOG.error("%s", exc)
        print("ERROR: %s" % exc)
        return 2

    if args.install_desktop_entry:
        return cmd_install_desktop_entry(install_root)
    if args.self_test:
        return cmd_self_test(args, meta, install_root, portable)

    installed = read_installed(install_root)

    if args.reinstall:
        LOG.info("Reinstall requested: wiping %s", app_dir(install_root))
        shutil.rmtree(app_dir(install_root), ignore_errors=True)
        try:
            os.unlink(installed_json_path(install_root))
        except OSError:
            pass
        # Keep the previous extra as a persisted preference for re-detection.
        installed = {"extra": installed.get("extra")} if installed else None

    if args.update is not None:
        try:
            return cmd_update(args, install_root, portable, installed, args.headless)
        except (LauncherError, urllib.error.URLError) as exc:
            LOG.error("Update failed: %s", exc)
            print("ERROR: update failed: %s" % exc)
            return 1

    requested_device = (
        {"cpu": "cpu", "cuda": "cu121"}[args.device] if args.device else None
    )
    is_installed = (
        installed is not None
        and installed.get("app_version")
        and os.path.isfile(venv_app_exe(install_root))
    )
    version_matches = meta is None or (
        is_installed
        and installed_version_satisfies(installed.get("app_version"), meta.version)
    )
    extra_matches = requested_device is None or (
        is_installed and installed.get("extra") == requested_device
    )

    if is_installed and version_matches and extra_matches:
        # Fast path: straight to the app. On Windows a minimal splash stays
        # open until the app window appears; Linux/headless launch with no UI.
        LOG.info("Fast path: launching installed version %s", installed.get("app_version"))
        if args.no_launch:
            return 0
        if sys.platform == "win32" and not args.headless:
            return _fast_path_gui(installed.get("app_version"), install_root, portable)
        LOG.info("Launching app...")
        return 0 if launch_app(install_root, portable) else 1

    if meta is None:
        message = (
            "No release metadata bundled and no --src-dir given; cannot bootstrap."
        )
        LOG.error(message)
        print("ERROR: %s" % message)
        return 2

    persisted = installed.get("extra") if installed else None
    notes = []
    device = detect_device(args.device, persisted, notify=notes.append)
    LOG.info("Bootstrapping version=%s device=%s", meta.version, device)
    if args.headless:
        return _bootstrap_headless(meta, install_root, portable, device, args.no_launch, notes)
    return _bootstrap_gui(meta, install_root, portable, device, args.no_launch, notes)


if __name__ == "__main__":
    sys.exit(main())
