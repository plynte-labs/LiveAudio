# SPDX-License-Identifier: MIT
"""Local-first diagnostics helpers for runtime and test health."""

from __future__ import annotations

from collections import defaultdict
from copy import deepcopy
from datetime import datetime, timezone
import os
import re

VALID_DIAGNOSTICS_LEVELS = {"off", "minimal", "deep"}
REDACTED = "<redacted>"
_WINDOWS_ABS_PATH_RE = re.compile(r"[A-Za-z]:\\[^\\\s]+(?:\\[^\\\s]+)*")
_URL_RE = re.compile(r"https?://[^\s]+")


def _utc_now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


def _normalize_level(level: str | None) -> str:
    if level in VALID_DIAGNOSTICS_LEVELS:
        return level
    return "minimal"


def diagnostics_enabled(config: dict | None) -> bool:
    if not isinstance(config, dict):
        return False
    return bool(config.get("diagnostics_enabled", False))


def diagnostics_level(config: dict | None) -> str:
    if not diagnostics_enabled(config):
        return "off"
    return _normalize_level(config.get("diagnostics_level"))


def should_collect_deep(config: dict | None) -> bool:
    return diagnostics_level(config) == "deep"


def _sanitize_string(value: str) -> str:
    sanitized = _WINDOWS_ABS_PATH_RE.sub(REDACTED, value)
    sanitized = _URL_RE.sub(REDACTED, sanitized)
    return sanitized


def _sanitize_value(value):
    if isinstance(value, str):
        return _sanitize_string(value)
    if isinstance(value, bytes):
        return "<bytes>"
    if isinstance(value, dict):
        return _sanitize_mapping(value)
    if isinstance(value, (list, tuple)):
        return [_sanitize_value(item) for item in value]
    return value


def _sanitize_mapping(data: dict) -> dict:
    sanitized = {}
    for key, value in data.items():
        key_lower = str(key).lower()
        is_path_like_key = any(sep in str(key) for sep in ("/", "\\"))
        if any(token in key_lower for token in ("secret", "token", "password", "transcript", "audio")) and not is_path_like_key:
            sanitized[key] = REDACTED
            continue
        sanitized[key] = _sanitize_value(value)
    return sanitized


class DiagnosticsStore:
    """Collects bounded local-only diagnostics snapshots."""

    def __init__(self, level: str = "off"):
        self.level = _normalize_level(level)
        self.counters = defaultdict(int)
        self.durations = defaultdict(list)
        self.states = {}

    def record_counter(self, name: str, delta: int = 1, tags: dict | None = None):
        if self.level == "off":
            return
        key = self._metric_key(name, tags)
        self.counters[key] += delta

    def record_duration(self, name: str, seconds: float, tags: dict | None = None):
        if self.level == "off":
            return
        key = self._metric_key(name, tags)
        self.durations[key].append(round(float(seconds), 6))

    def record_state(self, name: str, value, tags: dict | None = None):
        if self.level == "off":
            return
        payload = {"value": _sanitize_value(value)}
        if tags and self.level == "deep":
            payload["tags"] = _sanitize_mapping(tags)
        self.states[name] = payload

    def snapshot_runtime_health(self) -> dict:
        return {
            "schema_version": 1,
            "kind": "runtime",
            "level": self.level,
            "generated_at": _utc_now_iso(),
            "counters": dict(self.counters),
            "durations": {key: list(values) for key, values in self.durations.items()},
            "states": deepcopy(self.states),
        }

    def snapshot_test_health(
        self,
        resource_summary: dict | None = None,
        file_timings: dict | None = None,
        warnings: list | None = None,
    ) -> dict:
        payload = {
            "schema_version": 1,
            "kind": "test-health",
            "level": self.level,
            "generated_at": _utc_now_iso(),
            "resource_summary": _sanitize_mapping(resource_summary or {}),
            "file_timings": _sanitize_mapping(file_timings or {}),
            "warnings": _sanitize_value(warnings or []),
        }
        if self.level != "deep":
            payload["resource_summary"].pop("thread_names", None)
        return payload

    @staticmethod
    def _metric_key(name: str, tags: dict | None = None) -> str:
        if not tags:
            return name
        rendered = ",".join(f"{key}={tags[key]}" for key in sorted(tags))
        return f"{name}[{rendered}]"


def create_store_from_config(config: dict | None) -> DiagnosticsStore:
    return DiagnosticsStore(level=diagnostics_level(config))


def build_diagnostics_report(runtime_health: dict | None = None, test_health: dict | None = None) -> dict:
    report = {
        "schema_version": 1,
        "generated_at": _utc_now_iso(),
        "runtime": _sanitize_mapping(runtime_health or {}),
        "test_health": _sanitize_mapping(test_health or {}),
    }
    report["summary"] = {
        "runtime_present": bool(runtime_health),
        "test_health_present": bool(test_health),
    }
    return report


def normalize_export_dir(path: str | None) -> str | None:
    if path is None:
        return None
    text = str(path).strip()
    if not text:
        return None
    return os.path.normpath(os.path.abspath(text))
