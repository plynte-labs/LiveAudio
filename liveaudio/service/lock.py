# SPDX-License-Identifier: MIT
"""Single-owner service lock under LIVEAUDIO_HOME (NOT the config lock).

The PID in the persistent lock file is diagnostic metadata. Exclusivity comes
from a non-blocking OS file lock, which is released automatically on process
exit; stale metadata is replaced only after a new owner holds that lock.
"""

import errno
import os
import threading

from liveaudio.service.errors import ServiceError

SERVICE_LOCK_NAME = "service.lock"
_PROCESS_LOCKS = set()
_PROCESS_LOCKS_GUARD = threading.Lock()


def _try_lock(handle):
    handle.seek(0)
    if os.name == "nt":
        import msvcrt
        try:
            msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
        except OSError as exc:
            if exc.errno in (errno.EACCES, errno.EAGAIN, errno.EDEADLK):
                return False
            raise
    else:
        import fcntl
        try:
            fcntl.lockf(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB, 1, 0, os.SEEK_SET)
        except OSError as exc:
            if exc.errno in (errno.EACCES, errno.EAGAIN, errno.EWOULDBLOCK):
                return False
            raise
    return True


def _unlock(handle):
    handle.seek(0)
    if os.name == "nt":
        import msvcrt
        msvcrt.locking(handle.fileno(), msvcrt.LK_UNLCK, 1)
    else:
        import fcntl
        fcntl.lockf(handle.fileno(), fcntl.LOCK_UN, 1, 0, os.SEEK_SET)


def _write_pid(handle, pid):
    handle.seek(0)
    handle.truncate()
    handle.write(("{\"pid\": %d}" % pid).encode("ascii"))
    handle.flush()


class InstanceLock:
    """Persistent lock file held exclusively by a native OS file lock."""

    def __init__(self, home=None, checker=None):
        if home is None:
            from liveaudio.utils.config import get_data_home
            home = get_data_home()
        self.path = os.path.join(home, SERVICE_LOCK_NAME)
        self._lock_key = os.path.normcase(os.path.abspath(self.path))
        self._handle = None
        self._owns = False

    def _close_handle(self):
        handle, self._handle = self._handle, None
        if handle is not None:
            try:
                handle.close()
            except OSError:
                pass

    def _claim(self):
        fd = os.open(self.path, os.O_CREAT | os.O_RDWR, 0o600)
        self._handle = os.fdopen(fd, "r+b")
        if os.fstat(fd).st_size == 0:
            self._handle.write(b"\0")
            self._handle.flush()

        with _PROCESS_LOCKS_GUARD:
            if self._lock_key in _PROCESS_LOCKS:
                self._close_handle()
                return False
            if not _try_lock(self._handle):
                self._close_handle()
                return False
            _PROCESS_LOCKS.add(self._lock_key)
            self._owns = True

        try:
            _write_pid(self._handle, os.getpid())
        except OSError:
            self.release()
            raise
        return True

    def acquire(self):
        """Acquire without deleting or replacing a lock another owner holds."""
        try:
            os.makedirs(os.path.dirname(self.path) or ".", exist_ok=True)
            if self._claim():
                return
        except OSError as exc:
            self._close_handle()
            if getattr(exc, "winerror", None) in (32, 33):
                raise ServiceError("service-already-running")
            raise ServiceError("service-lock-unwritable")
        raise ServiceError("service-already-running")

    def release(self):
        if not self._owns:
            self._close_handle()
            return
        with _PROCESS_LOCKS_GUARD:
            handle = self._handle
            try:
                if handle is not None:
                    _unlock(handle)
            except OSError:
                pass
            finally:
                self._owns = False
                _PROCESS_LOCKS.discard(self._lock_key)
                self._close_handle()

    def __enter__(self):
        self.acquire()
        return self

    def __exit__(self, *exc):
        self.release()
        return False
