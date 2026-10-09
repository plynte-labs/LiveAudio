# SPDX-License-Identifier: MIT
"""Shared service failure type with a sanitized machine-readable code."""


class ServiceError(Exception):
    """Service failure with a sanitized machine-readable code (no PII)."""

    def __init__(self, code, detail=""):
        super().__init__(code if not detail else "%s: %s" % (code, detail))
        self.code = code
