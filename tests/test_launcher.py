# SPDX-License-Identifier: MIT
"""Tests for the bootstrapper launcher (packaging/launcher.py).

The launcher lives outside the liveaudio package, so it is loaded via an
importlib file spec. Only pure logic is tested here — no network, no uv
subprocesses, no GUI.
"""

import importlib.util
import json
import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import MagicMock, patch

_LAUNCHER_PATH = Path(__file__).resolve().parents[1] / "packaging" / "launcher.py"
_spec = importlib.util.spec_from_file_location("liveaudio_launcher", _LAUNCHER_PATH)
launcher = importlib.util.module_from_spec(_spec)
sys.modules["liveaudio_launcher"] = launcher
_spec.loader.exec_module(launcher)


class TestDetectDevice(unittest.TestCase):
    """The hardware detection ladder for choosing the torch extra."""

    def test_cli_flag_cuda_wins_over_everything(self):
        """--device cuda short-circuits to cu121 without probing hardware."""
        with patch.object(launcher, "_nvml_available") as mock_nvml:
            result = launcher.detect_device(cli_device="cuda", persisted="cpu")
        self.assertEqual(result, "cu121")
        mock_nvml.assert_not_called()

    def test_cli_flag_cpu_wins_over_everything(self):
        """--device cpu short-circuits to cpu without probing hardware."""
        with patch.object(launcher, "_nvml_available") as mock_nvml:
            result = launcher.detect_device(cli_device="cpu", persisted="cu121")
        self.assertEqual(result, "cpu")
        mock_nvml.assert_not_called()

    def test_persisted_preference_used_when_no_flag(self):
        """installed.json extra is reused without probing hardware."""
        with patch.object(launcher, "_nvml_available") as mock_nvml:
            result = launcher.detect_device(cli_device=None, persisted="cu121")
        self.assertEqual(result, "cu121")
        mock_nvml.assert_not_called()

    def test_no_nvml_falls_back_to_cpu(self):
        """Missing NVML library means no NVIDIA driver -> cpu."""
        with patch.object(launcher, "_nvml_available", return_value=False):
            with patch.object(launcher, "_query_nvidia_smi") as mock_smi:
                result = launcher.detect_device()
        self.assertEqual(result, "cpu")
        mock_smi.assert_not_called()

    def test_smi_failure_falls_back_to_cpu(self):
        """nvidia-smi failing (or timing out) -> cpu."""
        with patch.object(launcher, "_nvml_available", return_value=True):
            with patch.object(launcher, "_query_nvidia_smi", return_value=None):
                result = launcher.detect_device()
        self.assertEqual(result, "cpu")

    def test_old_driver_falls_back_to_cpu_with_reason(self):
        """Driver below 525 -> cpu, and the reason is surfaced via notify."""
        notes = []
        with patch.object(launcher, "_nvml_available", return_value=True):
            with patch.object(launcher, "_query_nvidia_smi", return_value=("470.82", 8192)):
                result = launcher.detect_device(notify=notes.append)
        self.assertEqual(result, "cpu")
        self.assertEqual(notes, [launcher.GPU_FALLBACK_MESSAGE])

    def test_low_vram_falls_back_to_cpu_with_reason(self):
        """VRAM below 4096 MiB -> cpu, and the reason is surfaced."""
        notes = []
        with patch.object(launcher, "_nvml_available", return_value=True):
            with patch.object(launcher, "_query_nvidia_smi", return_value=("552.44", 2048)):
                result = launcher.detect_device(notify=notes.append)
        self.assertEqual(result, "cpu")
        self.assertEqual(notes, [launcher.GPU_FALLBACK_MESSAGE])

    def test_good_gpu_selects_cu121(self):
        """Driver >= 525 and VRAM >= 4096 MiB -> cu121."""
        with patch.object(launcher, "_nvml_available", return_value=True):
            with patch.object(launcher, "_query_nvidia_smi", return_value=("552.44", 8192)):
                result = launcher.detect_device()
        self.assertEqual(result, "cu121")

    def test_boundary_driver_and_vram_qualify(self):
        """Exactly driver 525 / 4096 MiB still qualifies for cu121."""
        with patch.object(launcher, "_nvml_available", return_value=True):
            with patch.object(launcher, "_query_nvidia_smi", return_value=("525.00", 4096)):
                result = launcher.detect_device()
        self.assertEqual(result, "cu121")

    def test_unparseable_driver_falls_back_to_cpu(self):
        """Garbage driver strings never crash detection."""
        with patch.object(launcher, "_nvml_available", return_value=True):
            with patch.object(launcher, "_query_nvidia_smi", return_value=("garbage", 8192)):
                result = launcher.detect_device()
        self.assertEqual(result, "cpu")


class TestQueryNvidiaSmi(unittest.TestCase):
    """Parsing and failure handling of the nvidia-smi subprocess call."""

    def test_parses_driver_and_vram(self):
        proc = MagicMock(returncode=0, stdout="552.44, 8192 MiB\n", stderr="")
        with patch.object(launcher.subprocess, "run", return_value=proc):
            result = launcher._query_nvidia_smi()
        self.assertEqual(result, ("552.44", 8192))

    def test_timeout_returns_none(self):
        """A hung nvidia-smi (TimeoutExpired) yields None, not an exception."""
        error = subprocess.TimeoutExpired(cmd="nvidia-smi", timeout=10)
        with patch.object(launcher.subprocess, "run", side_effect=error):
            self.assertIsNone(launcher._query_nvidia_smi())

    def test_missing_binary_returns_none(self):
        with patch.object(launcher.subprocess, "run", side_effect=FileNotFoundError()):
            self.assertIsNone(launcher._query_nvidia_smi())

    def test_nonzero_exit_returns_none(self):
        proc = MagicMock(returncode=9, stdout="", stderr="boom")
        with patch.object(launcher.subprocess, "run", return_value=proc):
            self.assertIsNone(launcher._query_nvidia_smi())

    def test_empty_output_returns_none(self):
        proc = MagicMock(returncode=0, stdout="\n", stderr="")
        with patch.object(launcher.subprocess, "run", return_value=proc):
            self.assertIsNone(launcher._query_nvidia_smi())


class TestInstalledJson(unittest.TestCase):
    """Atomic write + read of the install completion marker."""

    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="liveaudio-launcher-test-")
        self.addCleanup(self._cleanup)

    def _cleanup(self):
        import shutil
        shutil.rmtree(self.root, ignore_errors=True)

    def test_write_then_read_roundtrip(self):
        data = {"app_version": "1.1.0", "extra": "cpu", "python": "3.11", "uv_version": "0.7.13"}
        launcher.write_installed(self.root, data)
        self.assertEqual(launcher.read_installed(self.root), data)

    def test_write_leaves_no_temp_files(self):
        """The temp file used for the atomic os.replace must not linger."""
        launcher.write_installed(self.root, {"app_version": "1.1.0"})
        leftovers = [n for n in os.listdir(self.root) if n.endswith(".tmp")]
        self.assertEqual(leftovers, [])

    def test_write_replaces_existing_file(self):
        launcher.write_installed(self.root, {"app_version": "1.0.0", "extra": "cpu"})
        launcher.write_installed(self.root, {"app_version": "1.1.0", "extra": "cu121"})
        installed = launcher.read_installed(self.root)
        self.assertEqual(installed["app_version"], "1.1.0")
        self.assertEqual(installed["extra"], "cu121")

    def test_read_missing_returns_none(self):
        self.assertIsNone(launcher.read_installed(self.root))

    def test_read_corrupt_returns_none(self):
        with open(launcher.installed_json_path(self.root), "w", encoding="utf-8") as fh:
            fh.write("{not json")
        self.assertIsNone(launcher.read_installed(self.root))

    def test_read_non_dict_returns_none(self):
        with open(launcher.installed_json_path(self.root), "w", encoding="utf-8") as fh:
            json.dump(["not", "a", "dict"], fh)
        self.assertIsNone(launcher.read_installed(self.root))


class TestInstallRootResolution(unittest.TestCase):
    """Install root precedence: env override > portable.marker > defaults."""

    def setUp(self):
        self.launcher_dir = tempfile.mkdtemp(prefix="liveaudio-ldir-")
        self.addCleanup(self._cleanup)

    def _cleanup(self):
        import shutil
        shutil.rmtree(self.launcher_dir, ignore_errors=True)

    def test_env_override_wins(self):
        environ = {"LIVEAUDIO_INSTALL_ROOT": os.path.join(self.launcher_dir, "custom")}
        root, portable = launcher.resolve_install_root(
            platform="win32", environ=environ, launcher_dir=self.launcher_dir
        )
        self.assertEqual(root, os.path.abspath(environ["LIVEAUDIO_INSTALL_ROOT"]))
        self.assertFalse(portable)

    def test_env_override_beats_portable_marker(self):
        Path(self.launcher_dir, "portable.marker").touch()
        environ = {"LIVEAUDIO_INSTALL_ROOT": os.path.join(self.launcher_dir, "custom")}
        root, portable = launcher.resolve_install_root(
            platform="win32", environ=environ, launcher_dir=self.launcher_dir
        )
        self.assertEqual(root, os.path.abspath(environ["LIVEAUDIO_INSTALL_ROOT"]))
        self.assertFalse(portable)

    def test_portable_marker_uses_launcher_dir_data(self):
        Path(self.launcher_dir, "portable.marker").touch()
        root, portable = launcher.resolve_install_root(
            platform="win32", environ={}, launcher_dir=self.launcher_dir
        )
        self.assertEqual(root, os.path.join(self.launcher_dir, "data"))
        self.assertTrue(portable)

    def test_windows_default_is_localappdata(self):
        environ = {"LOCALAPPDATA": r"C:\Users\test\AppData\Local"}
        root, portable = launcher.resolve_install_root(
            platform="win32", environ=environ, launcher_dir=self.launcher_dir
        )
        self.assertEqual(root, os.path.join(environ["LOCALAPPDATA"], "LiveAudio"))
        self.assertFalse(portable)

    def test_linux_default_is_local_share(self):
        root, portable = launcher.resolve_install_root(
            platform="linux", environ={}, launcher_dir=self.launcher_dir
        )
        expected = os.path.join(os.path.expanduser("~"), ".local", "share", "liveaudio")
        self.assertEqual(root, expected)
        self.assertFalse(portable)


class TestAppHomeResolution(unittest.TestCase):
    """LIVEAUDIO_HOME interplay with the install root."""

    def test_explicit_liveaudio_home_wins(self):
        environ = {"LIVEAUDIO_HOME": r"D:\my-data"}
        self.assertEqual(launcher.resolve_app_home(r"C:\root", environ), r"D:\my-data")

    def test_defaults_to_data_under_install_root(self):
        result = launcher.resolve_app_home(os.path.join("root"), {})
        self.assertEqual(result, os.path.join("root", "data"))


class TestSha256Verification(unittest.TestCase):
    """Checksum verification of downloaded archives."""

    def setUp(self):
        fd, self.path = tempfile.mkstemp(prefix="liveaudio-sha-")
        with os.fdopen(fd, "wb") as fh:
            fh.write(b"hello launcher")
        self.addCleanup(os.unlink, self.path)
        # sha256 of b"hello launcher"
        import hashlib
        self.good = hashlib.sha256(b"hello launcher").hexdigest()

    def test_matching_digest_passes(self):
        launcher.verify_sha256(self.path, self.good)  # must not raise

    def test_matching_digest_is_case_insensitive(self):
        launcher.verify_sha256(self.path, self.good.upper())

    def test_mismatch_raises_checksum_error(self):
        with self.assertRaises(launcher.ChecksumError):
            launcher.verify_sha256(self.path, "0" * 64)


class TestFastPathExeResolution(unittest.TestCase):
    """Per-platform path of the installed gui-script entry point."""

    def test_windows_exe_path(self):
        result = launcher.venv_app_exe(r"C:\root", platform="win32")
        self.assertEqual(
            result, os.path.join(r"C:\root", "app", ".venv", "Scripts", "liveaudio.exe")
        )

    def test_linux_exe_path(self):
        result = launcher.venv_app_exe("/root", platform="linux")
        self.assertEqual(result, os.path.join("/root", "app", ".venv", "bin", "liveaudio"))


class TestUvDiscovery(unittest.TestCase):
    """uv lookup order: bundled > PATH > previously downloaded > None."""

    def setUp(self):
        import shutil
        self.root = tempfile.mkdtemp(prefix="liveaudio-uv-root-")
        self.bundle = tempfile.mkdtemp(prefix="liveaudio-uv-bundle-")
        self.addCleanup(shutil.rmtree, self.root, True)
        self.addCleanup(shutil.rmtree, self.bundle, True)

    def _touch(self, directory, name):
        path = os.path.join(directory, name)
        os.makedirs(directory, exist_ok=True)
        Path(path).touch()
        return path

    def test_bundled_uv_wins_over_path(self):
        bundled = self._touch(self.bundle, "uv.exe")
        which = MagicMock(return_value=r"C:\on\path\uv.exe")
        result = launcher.find_uv(
            self.root, platform="win32", bundled_dirs=[self.bundle], which=which
        )
        self.assertEqual(result, bundled)
        which.assert_not_called()

    def test_path_used_when_no_bundled(self):
        which = MagicMock(return_value=r"C:\on\path\uv.exe")
        result = launcher.find_uv(
            self.root, platform="win32", bundled_dirs=[self.bundle], which=which
        )
        self.assertEqual(result, r"C:\on\path\uv.exe")

    def test_previously_downloaded_used_as_last_resort(self):
        downloaded = self._touch(os.path.join(self.root, "uv"), "uv.exe")
        which = MagicMock(return_value=None)
        result = launcher.find_uv(
            self.root, platform="win32", bundled_dirs=[self.bundle], which=which
        )
        self.assertEqual(result, downloaded)

    def test_returns_none_when_nothing_found(self):
        which = MagicMock(return_value=None)
        result = launcher.find_uv(
            self.root, platform="win32", bundled_dirs=[self.bundle], which=which
        )
        self.assertIsNone(result)

    def test_linux_looks_for_uv_without_extension(self):
        bundled = self._touch(self.bundle, "uv")
        result = launcher.find_uv(
            self.root, platform="linux", bundled_dirs=[self.bundle],
            which=MagicMock(return_value=None),
        )
        self.assertEqual(result, bundled)


class TestChecksumParsing(unittest.TestCase):
    """Parsing of release checksums files for --update."""

    def test_gnu_coreutils_format(self):
        text = "abc123  other.zip\n" + ("f" * 64) + "  liveaudio-src-1.2.0.zip\n"
        result = launcher.parse_checksums(text, "liveaudio-src-1.2.0.zip")
        self.assertEqual(result, "f" * 64)

    def test_binary_marker_format(self):
        text = ("a" * 64) + " *liveaudio-src-1.2.0.zip\n"
        result = launcher.parse_checksums(text, "liveaudio-src-1.2.0.zip")
        self.assertEqual(result, "a" * 64)

    def test_bare_digest_file(self):
        result = launcher.parse_checksums(("b" * 64) + "\n", "liveaudio-src-1.2.0.zip")
        self.assertEqual(result, "b" * 64)

    def test_missing_entry_returns_none(self):
        text = ("c" * 64) + "  unrelated.zip\n"
        self.assertIsNone(launcher.parse_checksums(text, "liveaudio-src-1.2.0.zip"))


class TestReleaseMeta(unittest.TestCase):
    """Release metadata resolution and dev-mode version parsing."""

    def test_dev_mode_reads_version_from_checkout(self):
        repo_root = Path(__file__).resolve().parents[1]
        meta = launcher.load_release_meta(src_dir=str(repo_root))
        self.assertEqual(meta.mode, "dev")
        import liveaudio
        self.assertEqual(meta.version, liveaudio.__version__)

    def test_no_meta_and_no_src_dir_returns_none(self):
        self.assertIsNone(launcher.load_release_meta(src_dir=None))

    def test_invalid_src_dir_raises(self):
        with tempfile.TemporaryDirectory() as empty:
            with self.assertRaises(launcher.LauncherError):
                launcher.load_release_meta(src_dir=empty)

    def test_parse_version_from_init(self):
        with tempfile.TemporaryDirectory() as tmp:
            init = os.path.join(tmp, "__init__.py")
            with open(init, "w", encoding="utf-8") as fh:
                fh.write('__version__ = "9.8.7"\n')
            self.assertEqual(launcher.parse_version_from_init(init), "9.8.7")


class TestLaunchAppEnv(unittest.TestCase):
    """Environment forwarded to the app by launch_app."""

    def setUp(self):
        import shutil
        self.root = tempfile.mkdtemp(prefix="liveaudio-launch-env-")
        self.addCleanup(shutil.rmtree, self.root, True)
        exe = launcher.venv_app_exe(self.root, platform="win32")
        os.makedirs(os.path.dirname(exe))
        Path(exe).touch()

    def test_frozen_launcher_exports_its_own_path(self):
        """A frozen launcher exposes LIVEAUDIO_LAUNCHER so the in-app updater can find it."""
        with patch.object(launcher.sys, "frozen", True, create=True):
            with patch.object(launcher.subprocess, "Popen") as popen:
                ok = launcher.launch_app(self.root, portable=False, environ={}, platform="win32")
        self.assertTrue(ok)
        env = popen.call_args.kwargs["env"]
        self.assertEqual(env["LIVEAUDIO_LAUNCHER"], os.path.abspath(launcher.sys.executable))
        self.assertEqual(env["LIVEAUDIO_HOME"], os.path.join(self.root, "data"))

    def test_unfrozen_run_does_not_export_launcher_path(self):
        """Dev runs (plain python) must not claim to be an updatable launcher."""
        with patch.object(launcher.subprocess, "Popen") as popen:
            ok = launcher.launch_app(self.root, portable=False, environ={}, platform="win32")
        self.assertTrue(ok)
        env = popen.call_args.kwargs["env"]
        self.assertNotIn("LIVEAUDIO_LAUNCHER", env)

    def test_returns_popen_handle_on_success(self):
        """The Popen handle feeds proc_alive in wait_for_app_window."""
        with patch.object(launcher.subprocess, "Popen") as popen:
            result = launcher.launch_app(self.root, portable=False, environ={}, platform="win32")
        self.assertIs(result, popen.return_value)

    def test_returns_false_when_exe_missing(self):
        import shutil
        empty_root = tempfile.mkdtemp(prefix="liveaudio-empty-")
        self.addCleanup(shutil.rmtree, empty_root, True)
        with patch.object(launcher.subprocess, "Popen") as popen:
            result = launcher.launch_app(
                empty_root, portable=False, environ={}, platform="win32",
            )
        self.assertIs(result, False)
        popen.assert_not_called()


class TestWaitForAppWindow(unittest.TestCase):
    """Window-detection poll that keeps the splash visible (Windows)."""

    def test_found_immediately_without_sleeping(self):
        sleep = MagicMock()
        result = launcher.wait_for_app_window(
            find_window=MagicMock(return_value=12345),
            proc_alive=MagicMock(return_value=True),
            sleep=sleep,
        )
        self.assertEqual(result, "found")
        sleep.assert_not_called()

    def test_found_after_a_few_ticks(self):
        find = MagicMock(side_effect=[None, None, None, 777])
        sleep = MagicMock()
        result = launcher.wait_for_app_window(
            find_window=find, proc_alive=MagicMock(return_value=True),
            timeout=120.0, interval=0.5, sleep=sleep,
        )
        self.assertEqual(result, "found")
        self.assertEqual(sleep.call_count, 3)

    def test_process_death_reported_before_window(self):
        """proc_alive going False is the 'definitely dead' signal."""
        proc_alive = MagicMock(side_effect=[True, True, False])
        result = launcher.wait_for_app_window(
            find_window=MagicMock(return_value=None),
            proc_alive=proc_alive, sleep=MagicMock(),
        )
        self.assertEqual(result, "died")

    def test_timeout_when_window_never_appears(self):
        sleep = MagicMock()
        result = launcher.wait_for_app_window(
            find_window=MagicMock(return_value=None),
            proc_alive=MagicMock(return_value=True),
            timeout=2.0, interval=0.5, sleep=sleep,
        )
        self.assertEqual(result, "timeout")
        self.assertEqual(sleep.call_count, 4)  # 2.0s / 0.5s ticks, no real waits

    def test_proc_alive_is_optional(self):
        """Without a proc_alive callback only found/timeout are possible."""
        result = launcher.wait_for_app_window(
            find_window=MagicMock(return_value=None),
            timeout=1.0, interval=0.5, sleep=MagicMock(),
        )
        self.assertEqual(result, "timeout")

    def test_on_tick_called_once_per_poll(self):
        ticks = MagicMock()
        launcher.wait_for_app_window(
            find_window=MagicMock(side_effect=[None, None, 1]),
            proc_alive=MagicMock(return_value=True),
            sleep=MagicMock(), on_tick=ticks,
        )
        self.assertEqual(ticks.call_count, 2)


class TestAppWindowTitleConsistency(unittest.TestCase):
    """APP_WINDOW_TITLE must stay in sync with the Tk title in app.py."""

    def test_launcher_constant_matches_app_title(self):
        repo_root = Path(_LAUNCHER_PATH).resolve().parents[1]
        launcher_src = _LAUNCHER_PATH.read_text(encoding="utf-8")
        app_src = (repo_root / "liveaudio" / "app.py").read_text(encoding="utf-8")
        launcher_match = re.search(r'^APP_WINDOW_TITLE\s*=\s*"([^"]+)"', launcher_src, re.M)
        app_match = re.search(r'self\.title\(\s*"([^"]+)"\s*\)', app_src)
        self.assertIsNotNone(launcher_match, "APP_WINDOW_TITLE literal not found in launcher.py")
        self.assertIsNotNone(app_match, "self.title(...) literal not found in app.py")
        self.assertEqual(launcher_match.group(1), app_match.group(1))
        # The imported module agrees with the source literal.
        self.assertEqual(launcher.APP_WINDOW_TITLE, app_match.group(1))


class TestInstalledVersionSatisfies(unittest.TestCase):
    """Fast-path version policy: never downgrade after an in-app update."""

    def test_equal_versions_satisfy(self):
        self.assertTrue(launcher.installed_version_satisfies("1.2.3", "1.2.3"))

    def test_installed_newer_satisfies(self):
        """An in-app update moved installed.json ahead of the baked version."""
        self.assertTrue(launcher.installed_version_satisfies("1.3.0", "1.2.3"))

    def test_installed_older_does_not_satisfy(self):
        self.assertFalse(launcher.installed_version_satisfies("1.2.2", "1.2.3"))

    def test_v_prefix_is_ignored(self):
        self.assertTrue(launcher.installed_version_satisfies("v1.2.3", "1.2.3"))
        self.assertTrue(launcher.installed_version_satisfies("1.2.3", "v1.2.3"))

    def test_malformed_installed_does_not_match_valid_meta(self):
        self.assertFalse(launcher.installed_version_satisfies("nightly", "1.2.3"))

    def test_malformed_but_identical_strings_satisfy(self):
        """String equality is the safe fallback: avoids a re-bootstrap loop."""
        self.assertTrue(launcher.installed_version_satisfies("nightly", "nightly"))

    def test_none_installed_does_not_satisfy(self):
        self.assertFalse(launcher.installed_version_satisfies(None, "1.2.3"))


class TestFastPathVersionPolicy(unittest.TestCase):
    """main() decision: fast path vs bootstrap based on installed vs baked version."""

    def setUp(self):
        import shutil
        self.root = tempfile.mkdtemp(prefix="liveaudio-fastpath-")
        self.addCleanup(shutil.rmtree, self.root, True)
        exe = launcher.venv_app_exe(self.root)
        os.makedirs(os.path.dirname(exe))
        Path(exe).touch()

    def _run_main(self, installed_version, meta_version):
        """Run launcher.main() headless; returns (exit_code, bootstrap_mock)."""
        launcher.write_installed(
            self.root, {"app_version": installed_version, "extra": "cpu"}
        )
        meta = launcher.ReleaseMeta(
            version=meta_version,
            src_zip_url="https://example.invalid/src.zip",
            src_zip_sha256="0" * 64,
        )
        with patch.dict(os.environ, {"LIVEAUDIO_INSTALL_ROOT": self.root}):
            with patch.object(launcher, "setup_logging"):
                with patch.object(launcher, "load_release_meta", return_value=meta):
                    with patch.object(launcher, "detect_device", return_value="cpu"):
                        with patch.object(
                            launcher, "_bootstrap_headless", return_value=77
                        ) as bootstrap:
                            code = launcher.main(["--headless", "--no-launch"])
        return code, bootstrap

    def test_equal_version_takes_fast_path(self):
        code, bootstrap = self._run_main("1.2.3", "1.2.3")
        self.assertEqual(code, 0)
        bootstrap.assert_not_called()

    def test_installed_newer_takes_fast_path(self):
        """After `--update vX.Y.Z` the old launcher exe must not downgrade."""
        code, bootstrap = self._run_main("1.3.0", "1.2.3")
        self.assertEqual(code, 0)
        bootstrap.assert_not_called()

    def test_installed_older_bootstraps(self):
        code, bootstrap = self._run_main("1.2.2", "1.2.3")
        self.assertEqual(code, 77)
        bootstrap.assert_called_once()

    def test_malformed_installed_version_bootstraps(self):
        code, bootstrap = self._run_main("garbage", "1.2.3")
        self.assertEqual(code, 77)
        bootstrap.assert_called_once()


class TestDirtyInstallMarking(unittest.TestCase):
    """installed.json must be gone before app/ is replaced (M1)."""

    def setUp(self):
        import shutil
        self.root = tempfile.mkdtemp(prefix="liveaudio-dirty-")
        self.addCleanup(shutil.rmtree, self.root, True)

    def test_mark_install_dirty_removes_installed_json(self):
        launcher.write_installed(self.root, {"app_version": "1.0.0"})
        launcher.mark_install_dirty(self.root)
        self.assertIsNone(launcher.read_installed(self.root))

    def test_mark_install_dirty_is_noop_when_missing(self):
        launcher.mark_install_dirty(self.root)  # must not raise
        self.assertIsNone(launcher.read_installed(self.root))

    def test_failed_sync_leaves_install_not_fast_path_eligible(self):
        """Unpack succeeds, uv sync fails -> next run must bootstrap again."""
        launcher.write_installed(self.root, {"app_version": "1.0.0", "extra": "cpu"})
        src = os.path.join(self.root, "checkout")
        os.makedirs(os.path.join(src, "liveaudio"))
        Path(src, "liveaudio", "__init__.py").write_text(
            '__version__ = "1.1.0"\n', encoding="utf-8"
        )
        meta = launcher.ReleaseMeta(version="1.1.0", src_dir=src)
        with patch.object(launcher, "ensure_uv", return_value=("uv", "1.0")):
            with patch.object(
                launcher, "run_uv_sync",
                side_effect=launcher.LauncherError("sync failed"),
            ):
                with self.assertRaises(launcher.LauncherError):
                    launcher.run_bootstrap(
                        meta, self.root, portable=False, device="cpu",
                        reporter=MagicMock(), no_launch=True,
                    )
        # The stale installed.json was removed before app/ was touched.
        self.assertIsNone(launcher.read_installed(self.root))


class TestWaitForAppExit(unittest.TestCase):
    """Update path must not clear app/ while the app still runs (M2)."""

    def setUp(self):
        import shutil
        self.root = tempfile.mkdtemp(prefix="liveaudio-wait-")
        self.addCleanup(shutil.rmtree, self.root, True)

    def _make_exe(self):
        exe = launcher.venv_app_exe(self.root)
        os.makedirs(os.path.dirname(exe))
        Path(exe).touch()
        return exe

    def test_returns_immediately_when_app_not_installed(self):
        launcher.wait_for_app_exit(self.root)  # no exe -> nothing to wait for

    def test_probe_released_when_exe_is_free(self):
        exe = self._make_exe()
        self.assertTrue(launcher._app_exe_released(exe))
        self.assertTrue(os.path.isfile(exe))  # rename round-trip restored it

    def test_probe_locked_when_exe_missing(self):
        exe = launcher.venv_app_exe(self.root)
        self.assertFalse(launcher._app_exe_released(exe))

    def test_proceeds_once_app_exits_during_wait(self):
        self._make_exe()
        with patch.object(
            launcher, "_app_exe_released", side_effect=[False, False, True]
        ):
            launcher.wait_for_app_exit(self.root, timeout=5.0, interval=0.01)

    def test_timeout_raises_launcher_error(self):
        self._make_exe()
        with patch.object(launcher, "_app_exe_released", return_value=False):
            with self.assertRaises(launcher.LauncherError):
                launcher.wait_for_app_exit(self.root, timeout=0.05, interval=0.01)

    def test_update_aborts_without_bootstrap_when_app_never_exits(self):
        """cmd_update must not touch app/ when the wait times out."""
        meta = launcher.ReleaseMeta(
            version="9.9.9",
            src_zip_url="https://example.invalid/src.zip",
            src_zip_sha256="0" * 64,
        )
        args = MagicMock(update="v9.9.9", device=None, no_launch=True)
        with patch.object(launcher, "fetch_release_meta", return_value=meta):
            with patch.object(launcher, "detect_device", return_value="cpu"):
                with patch.object(
                    launcher, "wait_for_app_exit",
                    side_effect=launcher.LauncherError("still running"),
                ):
                    with patch.object(launcher, "_bootstrap_headless") as bootstrap:
                        with self.assertRaises(launcher.LauncherError):
                            launcher.cmd_update(
                                args, self.root, portable=False,
                                installed={"app_version": "1.0.0", "extra": "cpu"},
                                headless=True,
                            )
        bootstrap.assert_not_called()

    def test_update_proceeds_after_app_exits(self):
        meta = launcher.ReleaseMeta(
            version="9.9.9",
            src_zip_url="https://example.invalid/src.zip",
            src_zip_sha256="0" * 64,
        )
        args = MagicMock(update="v9.9.9", device=None, no_launch=True)
        with patch.object(launcher, "fetch_release_meta", return_value=meta):
            with patch.object(launcher, "detect_device", return_value="cpu"):
                with patch.object(launcher, "wait_for_app_exit") as wait:
                    with patch.object(
                        launcher, "_bootstrap_headless", return_value=0
                    ) as bootstrap:
                        code = launcher.cmd_update(
                            args, self.root, portable=False,
                            installed={"app_version": "1.0.0", "extra": "cpu"},
                            headless=True,
                        )
        self.assertEqual(code, 0)
        wait.assert_called_once_with(self.root)
        bootstrap.assert_called_once()


class TestUvSyncCommand(unittest.TestCase):
    """End-user installs must not pull the default dev group (m1)."""

    def test_no_dev_flag_in_uv_sync_command(self):
        proc = MagicMock()
        proc.stdout = []
        proc.returncode = 0
        with patch.object(launcher.subprocess, "Popen", return_value=proc) as popen:
            launcher.run_uv_sync(
                "uv", "/project", "cpu", "/root", reporter=MagicMock()
            )
        command = popen.call_args.args[0]
        self.assertIn("--no-dev", command)
        self.assertIn("--locked", command)
        self.assertEqual(command[command.index("--extra") + 1], "cpu")


class TestDesktopEntry(unittest.TestCase):
    """Rendering of the .desktop template."""

    def test_placeholders_substituted(self):
        template = Path(_LAUNCHER_PATH).parent / "liveaudio.desktop"
        rendered = launcher.render_desktop_entry(
            template.read_text(encoding="utf-8"), "/usr/bin/liveaudio-launcher", "/icons/la.png"
        )
        self.assertIn("Exec=/usr/bin/liveaudio-launcher", rendered)
        self.assertIn("Icon=/icons/la.png", rendered)
        self.assertIn("Categories=AudioVideo;Audio;", rendered)
        self.assertNotIn("{exec}", rendered)
        self.assertNotIn("{icon}", rendered)


if __name__ == "__main__":
    unittest.main()
