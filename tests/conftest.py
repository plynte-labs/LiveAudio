# SPDX-License-Identifier: MIT
"""Shared pytest configuration.

The data-home redirect now lives in ``tests/__init__.py`` so that both pytest
and ``python -m unittest discover`` get it. Importing it here fails loudly if
the package bootstrap is ever skipped, instead of letting a test run write to
the developer's real %APPDATA%\\LiveAudio.
"""

from tests import LIVEAUDIO_TEST_HOME  # noqa: F401
