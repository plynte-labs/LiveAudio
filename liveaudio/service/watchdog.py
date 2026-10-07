# SPDX-License-Identifier: MIT
"""Owner-process watchdog (TBD-1: no TCP control plane).

The parent spawns the service and owns it until the parent dies. Liveness
is checked via parent-PID only: Windows API in-process, POSIX signal 0.
The checker is injectable so supervision stays deterministic in tests.

Limitations (by design, documented — not bugs to fix in this track):

- PID reuse TOCTOU: PIDs are machine-local and recyclable. If the owner
  dies and the OS reassigns the same PID to an unrelated process before
  the next poll (default 1 s), the service briefly considers the parent
  alive. Mitigations: short poll interval, and the instance lock — a new
  owner spawning its own service hits ``service-already-running`` instead
  of silently adopting this one. No cross-process adoption ever happens.
- Local only: the parent PID is meaningful on this machine alone; there
  are no remote owners and no control channel to spoof.
"""

import os

from liveaudio.service.errors import ServiceError


def parse_parent_pid(raw):
    """Validate --parent-pid. Invalid/missing values fail fast, never orphan."""
    try:
        pid = int(raw)
    except (TypeError, ValueError):
        raise ServiceError("parent-pid-invalid")
    if pid <= 0:
        raise ServiceError("parent-pid-invalid")
    return pid


def _default_parent_checker(pid):
    """True if a process with pid exists. Portable Windows/POSIX, stdlib only."""
    try:
        if os.name == "nt":
            import ctypes
            kernel32 = ctypes.windll.kernel32
            # PROCESS_QUERY_LIMITED_INFORMATION: enough to test existence.
            handle = kernel32.OpenProcess(0x1000, False, int(pid))
            if not handle:
                return False
            try:
                return True
            finally:
                kernel32.CloseHandle(handle)
        else:
            os.kill(int(pid), 0)
            return True
    except Exception:
        return False


def is_parent_alive(pid, checker=None):
    """Parent-liveness abstraction (checker injectable for deterministic tests)."""
    check = checker or _default_parent_checker
    try:
        return bool(check(int(pid)))
    except Exception:
        return False
