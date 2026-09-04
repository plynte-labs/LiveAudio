# SPDX-License-Identifier: MIT
"""Headless service runtime for LiveAudio (opencohost backend) — facade.

Stable external contract: `liveaudio.service:main` (used by the
`liveaudio-service` script and `liveaudio --service`), plus the building
blocks re-exported below. Responsibilities live in focused submodules with
directed dependencies:

    errors     — ServiceError (no internal deps)
    watchdog   — owner liveness: parse_parent_pid, is_parent_alive
    health     — HealthEmitter: stdout JSON lines + atomic health file
    lock       — InstanceLock: single owner per data home
    supervisor — candidate_ports, FirstClientGate, ProcessSupervisor

This package imports stdlib only at import time: importing it must NOT pull
CTK or torch (the service parent stays light; heavy imports happen inside
the spawned children via liveaudio.core.workers).
"""

import argparse
import json
import multiprocessing as mp
import os
import sys

from liveaudio.service.errors import ServiceError
from liveaudio.service.health import (
    SERVICE_EVENT_SCHEMA,
    SERVICE_EVENT_VERSION,
    HealthEmitter,
)
from liveaudio.service.lock import SERVICE_LOCK_NAME, InstanceLock
from liveaudio.service.supervisor import (
    CHILD_FAILURE_WINDOW_SEC,
    JOIN_TIMEOUT_SEC,
    LOG_PUMP_MAX_PER_TICK,
    MAX_CHILD_FAILURES,
    QUEUE_MAXSIZE,
    RESPAWN_BACKOFF_BASE_SEC,
    RESPAWN_BACKOFF_MAX_SEC,
    WATCHDOG_POLL_SEC,
    FirstClientGate,
    ProcessSupervisor,
    candidate_ports,
)
from liveaudio.service.watchdog import is_parent_alive, parse_parent_pid

__all__ = [
    "CHILD_FAILURE_WINDOW_SEC",
    "JOIN_TIMEOUT_SEC",
    "LOG_PUMP_MAX_PER_TICK",
    "MAX_CHILD_FAILURES",
    "QUEUE_MAXSIZE",
    "RESPAWN_BACKOFF_BASE_SEC",
    "RESPAWN_BACKOFF_MAX_SEC",
    "SERVICE_EVENT_SCHEMA",
    "SERVICE_EVENT_VERSION",
    "SERVICE_LOCK_NAME",
    "WATCHDOG_POLL_SEC",
    "FirstClientGate",
    "HealthEmitter",
    "InstanceLock",
    "ProcessSupervisor",
    "ServiceError",
    "build_arg_parser",
    "candidate_ports",
    "is_parent_alive",
    "main",
    "parse_parent_pid",
]


def build_arg_parser():
    parser = argparse.ArgumentParser(
        prog="liveaudio-service",
        description="LiveAudio headless service backend (no GUI). "
                    "Spawned by the owner process; exits when the parent dies.")
    parser.add_argument("--parent-pid", required=True,
                        help="PID of the owner process (dies with it).")
    parser.add_argument("--health-file", default=None,
                        help="Optional path for the atomic health snapshot JSON. "
                             "When absent, only stdout JSON lines are emitted.")
    parser.add_argument("--watchdog-interval", type=float, default=WATCHDOG_POLL_SEC,
                        help="Parent-liveness poll interval in seconds.")
    parser.add_argument("--prewarm", dest="prewarm", action=argparse.BooleanOptionalAction, default=True,
                        help="Prewarm ASR model on service startup (default: true).")
    parser.add_argument("--lazy", dest="prewarm", action="store_false",
                        help="Delay audio and ASR child process start until the first WS client connects.")
    return parser


def _print_early_fatal(code, parent_pid=0):
    try:
        print(json.dumps({
            "schema": SERVICE_EVENT_SCHEMA,
            "version": SERVICE_EVENT_VERSION,
            "service_pid": os.getpid(),
            "parent_pid": int(parent_pid) if str(parent_pid).isdigit() else 0,
            "type": "fatal",
            "code": code,
        }), flush=True)
    except Exception:
        pass


def main(argv=None):
    """Service entry point (also used by the `liveaudio-service` script)."""
    mp.freeze_support()
    args = build_arg_parser().parse_args(argv)
    try:
        parent_pid = parse_parent_pid(args.parent_pid)
    except ServiceError as exc:
        _print_early_fatal(exc.code)
        return 2
    try:
        watchdog_interval = float(args.watchdog_interval)
    except (TypeError, ValueError):
        watchdog_interval = WATCHDOG_POLL_SEC
    from liveaudio.utils.config import get_data_home, load_config_readonly
    config, info = load_config_readonly()
    emitter = HealthEmitter(os.getpid(), parent_pid, health_file=args.health_file)
    if info.get("error_code"):
        emitter.emit("warning", {"code": info["error_code"]})
    lock = InstanceLock(home=get_data_home())
    try:
        lock.acquire()
    except ServiceError as exc:
        _print_early_fatal(exc.code, parent_pid)
        return 2
    try:
        supervisor = ProcessSupervisor(config, parent_pid, emitter,
                                       watchdog_interval=watchdog_interval,
                                       prewarm=args.prewarm)
        return supervisor.run()
    finally:
        try:
            lock.release()
        except Exception:
            pass


if __name__ == "__main__":
    sys.exit(main())
