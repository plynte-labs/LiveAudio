# SPDX-License-Identifier: MIT
"""Test package bootstrap.

Points the LiveAudio data home at a throwaway directory BEFORE any test
imports liveaudio.utils.config, so test runs never touch the real
%APPDATA%\\LiveAudio (or ~/.config/liveaudio) of the developer machine.

This lives in the package __init__ (not only in conftest.py) because
``python -m unittest discover -s tests`` never loads conftest.py, and several
tests call load_config()/save_config() for real.
"""

import atexit
import json
import os
import shutil
import tempfile

LIVEAUDIO_TEST_HOME = os.environ.get("LIVEAUDIO_TEST_HOME")

if not LIVEAUDIO_TEST_HOME or not os.path.isdir(LIVEAUDIO_TEST_HOME):
    LIVEAUDIO_TEST_HOME = tempfile.mkdtemp(prefix="liveaudio-test-home-")
    os.environ["LIVEAUDIO_TEST_HOME"] = LIVEAUDIO_TEST_HOME
    atexit.register(shutil.rmtree, LIVEAUDIO_TEST_HOME, True)

os.environ["LIVEAUDIO_HOME"] = LIVEAUDIO_TEST_HOME

# Seed an empty config so tests that mock os.path.exists/json.load can still
# open the real file underneath.
_seed = os.path.join(LIVEAUDIO_TEST_HOME, "config.json")
if not os.path.exists(_seed):
    try:
        with open(_seed, "w", encoding="utf-8") as _fh:
            json.dump({}, _fh)
    except OSError:
        pass
