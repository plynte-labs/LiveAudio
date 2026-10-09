# SPDX-License-Identifier: MIT
"""Entry-point dispatcher: `liveaudio [--service ...]` / GUI.

This module is stdlib-only at import time. Service mode never imports CTK;
GUI mode imports the app lazily so `--service --help` stays instant and the
service parent never loads the toolkit (or torch).
"""

import sys


def main(argv=None):
    args = sys.argv[1:] if argv is None else list(argv or [])
    if "--service" in args:
        from liveaudio.service import main as service_main
        return service_main([a for a in args if a != "--service"])
    from liveaudio.app import main as gui_main
    return gui_main()


if __name__ == "__main__":
    sys.exit(main())
