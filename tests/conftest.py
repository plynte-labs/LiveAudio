# SPDX-License-Identifier: MIT
"""Shared pytest configuration.

Points the LiveAudio data home at a throwaway directory BEFORE any test
imports liveaudio.utils.config, so test runs never touch the real
%APPDATA%\\LiveAudio (or ~/.config/liveaudio) of the developer machine.
"""

import atexit
import json
import os
import shutil
import tempfile

_home = tempfile.mkdtemp(prefix="liveaudio-test-home-")
os.environ["LIVEAUDIO_HOME"] = _home

# Seed an empty config so tests that mock os.path.exists/json.load can still
# open the real file underneath.
with open(os.path.join(_home, "config.json"), "w", encoding="utf-8") as _fh:
    json.dump({}, _fh)

atexit.register(shutil.rmtree, _home, True)
