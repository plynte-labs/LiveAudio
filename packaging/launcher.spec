# -*- mode: python ; coding: utf-8 -*-
# PyInstaller spec for the LiveAudio bootstrapper launcher (onefile).
#
# Build (from the repo root or packaging/):
#   pyinstaller packaging/launcher.spec
#
# Platform notes:
# - Windows: console=False so no console window flashes on the fast path.
#   Consequence: --headless / --self-test output is INVISIBLE on Windows
#   (stdout is detached). That is acceptable — everything is also written to
#   bootstrap.log in the install root, and CI runs --self-test on the
#   unfrozen script.
# - Linux: console=True since the launcher is started from a terminal or a
#   .desktop entry (Terminal=false detaches it anyway), and headless mode
#   needs working stdout.

import os
import sys

IS_WINDOWS = sys.platform == "win32"

# SPECPATH is provided by PyInstaller: the directory containing this spec.
HERE = SPECPATH  # noqa: F821
ASSETS = os.path.join(HERE, "..", "liveaudio", "assets")

APP_NAME = "LiveAudio-Setup" if IS_WINDOWS else "liveaudio-launcher"
ICON_ICO = os.path.join(ASSETS, "LiveAudio-Fran.ico")
ICON_PNG = os.path.join(ASSETS, "LiveAudio-Fran.png")

# Data files bundled into the onefile archive (extracted to sys._MEIPASS).
datas = [
    (os.path.join(HERE, "liveaudio.desktop"), "."),
    (ICON_ICO, "."),
    (ICON_PNG, "."),
]

# CI may drop a vendored uv binary at packaging/vendor/uv(.exe) so the
# launcher never needs to download it. Include it only when present.
vendor_uv = os.path.join(HERE, "vendor", "uv.exe" if IS_WINDOWS else "uv")
if os.path.exists(vendor_uv):
    datas.append((vendor_uv, "."))

# Release builds generate packaging/_release_meta.py (VERSION, SRC_ZIP_URL,
# SRC_ZIP_SHA256). Declare it as a hidden import only when it exists so dev
# builds (no metadata -> --src-dir mode) still work.
hiddenimports = []
if os.path.exists(os.path.join(HERE, "_release_meta.py")):
    hiddenimports.append("_release_meta")

# The launcher is stdlib+tkinter only. Exclude heavyweight packages
# defensively in case the build environment has them installed.
excludes = [
    "torch",
    "torchaudio",
    "numpy",
    "scipy",
    "PIL",
    "customtkinter",
    "websockets",
    "faster_whisper",
    "ctranslate2",
    "sounddevice",
    "matplotlib",
    "pandas",
]

a = Analysis(
    [os.path.join(HERE, "launcher.py")],
    pathex=[HERE],
    binaries=[],
    datas=datas,
    hiddenimports=hiddenimports,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=excludes,
    noarchive=False,
)

pyz = PYZ(a.pure)

# Onefile: pass binaries/zipfiles/datas straight to EXE (no COLLECT).
exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.zipfiles,
    a.datas,
    [],
    name=APP_NAME,
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=False,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=not IS_WINDOWS,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon=ICON_ICO if IS_WINDOWS else None,
)
