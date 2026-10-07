# SPDX-License-Identifier: MIT
"""Health output: stdout JSON-lines events + optional atomic health file.

Health never carries transcripts, logs, audio, or private paths — only
codes, ports, states, and counters. A denylist scrub is applied as defense
in depth on top of the allowlisted snapshot the supervisor builds.
"""

import json
import os
import tempfile

SERVICE_EVENT_SCHEMA = "liveaudio.service.event"
SERVICE_EVENT_VERSION = 1

# Keys that must never reach stdout/health (defense in depth; the snapshot
# builder only passes allowlisted fields anyway).
_SCRUBBED_KEYS = frozenset({
    "text", "transcript", "segments", "message", "log", "logs", "audio",
    "audio_chunk", "path", "output_dir", "session_dir", "export_dir",
    "payload", "blacklist",
})


class HealthEmitter:
    """Stdout JSON-lines events + optional atomic health-file snapshot."""

    def __init__(self, service_pid, parent_pid, health_file=None):
        self.service_pid = int(service_pid)
        self.parent_pid = int(parent_pid)
        self.health_file = health_file
        self._health_file_warned = False

    def _base(self, event_type):
        return {
            "schema": SERVICE_EVENT_SCHEMA,
            "version": SERVICE_EVENT_VERSION,
            "service_pid": self.service_pid,
            "parent_pid": self.parent_pid,
            "type": event_type,
        }

    @staticmethod
    def scrub(fields):
        """Drop keys that could carry transcript/audio/log/path material."""
        return {k: v for k, v in dict(fields).items() if k not in _SCRUBBED_KEYS}

    def emit(self, event_type, fields=None):
        """Print one JSON line to stdout. Never raises, never writes the file."""
        try:
            line = self._base(event_type)
            line.update(self.scrub(fields or {}))
            print(json.dumps(line), flush=True)
        except Exception:
            pass

    def write_snapshot(self, snapshot):
        """Atomically replace the health file (tmp + os.replace + fsync).

        On failure: warn once on stdout and continue serving (degraded, stdout
        remains the source of truth). No rotation in this track.
        """
        if not self.health_file:
            return True
        try:
            payload = self.scrub(snapshot)
            directory = os.path.dirname(os.path.abspath(self.health_file))
            os.makedirs(directory, exist_ok=True)
            fd, tmp_path = tempfile.mkstemp(dir=directory, prefix=".health-", suffix=".tmp")
            try:
                with os.fdopen(fd, "w", encoding="utf-8") as f:
                    json.dump(payload, f)
                    f.flush()
                    os.fsync(f.fileno())
                os.replace(tmp_path, self.health_file)
            except BaseException:
                try:
                    os.remove(tmp_path)
                except OSError:
                    pass
                raise
            return True
        except Exception:
            if not self._health_file_warned:
                self._health_file_warned = True
                self.emit("warning", {"code": "health-file-unwritable"})
            return False
