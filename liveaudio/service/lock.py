# SPDX-License-Identifier: MIT
"""Single-owner instance lock under LIVEAUDIO_HOME (NOT the config lock).

The file holds {"pid": N}. A lock whose PID is dead is stale and gets
reclaimed; a lock whose PID is alive rejects the second service so two
owners can never serve (not even on different ports). Portable Windows/Linux.
"""

import json
import os

from liveaudio.service.errors import ServiceError
from liveaudio.service.watchdog import is_parent_alive

SERVICE_LOCK_NAME = "service.lock"


class InstanceLock:
    """Filesystem owner lock with PID + stale recovery."""

    def __init__(self, home=None, checker=None):
        if home is None:
            from liveaudio.utils.config import get_data_home
            home = get_data_home()
        self.path = os.path.join(home, SERVICE_LOCK_NAME)
        self._checker = checker
        self._owns = False

    def _holder_alive(self):
        try:
            with open(self.path, "r", encoding="utf-8") as f:
                data = json.load(f)
            holder = int(data.get("pid", -1))
        except (OSError, ValueError, AttributeError, TypeError):
            return False
        if holder <= 0:
            return False
        if holder == os.getpid():
            return True  # our own process: the lock is live, never stale
        return is_parent_alive(holder, checker=self._checker)

    def _claim(self):
        """Create the lock file exclusively. May raise OSError (incl. FileExistsError)."""
        fd = os.open(self.path, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            json.dump({"pid": os.getpid()}, f)
        self._owns = True

    def acquire(self):
        """Claim the lock.

        Creates the data home on fresh installs. Raises only sanitized
        ServiceError codes (no paths, no errno text): the caller maps them
        to the stdout fatal event.
        """
        try:
            os.makedirs(os.path.dirname(self.path) or ".", exist_ok=True)
        except OSError:
            raise ServiceError("service-lock-unwritable")
        try:
            self._claim()
            return
        except FileExistsError:
            pass
        except OSError:
            raise ServiceError("service-lock-unwritable")
        if self._holder_alive():
            raise ServiceError("service-already-running")
        # Stale (or unreadable) lock: reclaim once, then re-check.
        try:
            os.remove(self.path)
        except OSError:
            pass
        try:
            self._claim()
            return
        except FileExistsError:
            raise ServiceError("service-already-running")
        except OSError:
            raise ServiceError("service-lock-unwritable")

    def release(self):
        if self._owns:
            try:
                os.remove(self.path)
            except OSError:
                pass
            self._owns = False

    def __enter__(self):
        self.acquire()
        return self

    def __exit__(self, *exc):
        self.release()
        return False
