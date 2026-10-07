# SPDX-License-Identifier: MIT
"""Allow `python -m liveaudio.service --parent-pid PID` (package execution)."""

import sys

from liveaudio.service import main

if __name__ == "__main__":
    sys.exit(main())
