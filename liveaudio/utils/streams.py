# SPDX-License-Identifier: MIT
"""Encoding-safe stdout/stderr for child processes.

The audio producer, ASR consumer and WebSocket server run in spawned child
processes and print status messages containing non-ASCII characters
('→', '🎤'). When stdout is a pipe or a console using a legacy codepage
(e.g. cp1252 on Windows), the default "strict" error handler makes print()
raise UnicodeEncodeError and kills the child at startup. Under pythonw the
crash is invisible (print is a no-op there), but console runs and CI pipes
die immediately.

Reconfiguring the streams with errors="replace" degrades unencodable
characters instead of crashing. The GUI log path (log_queue) is unaffected:
messages keep their original text there — only the stdout fallback degrades.
"""

import sys


def make_streams_encoding_safe():
    """Best-effort: never let stream encoding crash a child process.

    Must be called at the top of every child process entry point, before the
    first print(). Safe to call when the streams are None (pythonw) or are
    objects without reconfigure() (StringIO fallbacks, intercepting writers).
    """
    for stream in (sys.stdout, sys.stderr):
        if stream is not None and hasattr(stream, "reconfigure"):
            try:
                stream.reconfigure(errors="replace")
            except (OSError, ValueError):
                pass
