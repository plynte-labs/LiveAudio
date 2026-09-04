# SPDX-License-Identifier: MIT
"""Child-process supervision with lazy audio/ASR start.

WS starts immediately; audio/ASR start only on the first WS client
(signalled via first_client_event set by the WS child, or on_first_client
in-process). Shutdown is idempotent: sentinels, bounded join, terminate
fallback, queue drain, Manager.shutdown.

Respawn policy (TBD-3, conservative): at most 3 child failures in 5 minutes
with backoff; then fail fast with a non-zero exit. Queues are drained and
recreated on every respawn.
"""

import multiprocessing as mp
import os
import threading
import time

from liveaudio.service.errors import ServiceError
from liveaudio.service.watchdog import is_parent_alive

MAX_CHILD_FAILURES = 3
CHILD_FAILURE_WINDOW_SEC = 300.0
RESPAWN_BACKOFF_BASE_SEC = 1.0
RESPAWN_BACKOFF_MAX_SEC = 15.0

WATCHDOG_POLL_SEC = 1.0
QUEUE_MAXSIZE = 100
JOIN_TIMEOUT_SEC = 3.0
LOG_PUMP_MAX_PER_TICK = 100


def candidate_ports(base, fallback_range=None):
    """Return the bounded WS fallback candidates base..base+N-1 (10 ports).

    WS_PORT_FALLBACK_RANGE=10 means candidates base..base+9, NOT base..base+10.
    """
    if fallback_range is None:
        from liveaudio.core.network import WS_PORT_FALLBACK_RANGE
        fallback_range = WS_PORT_FALLBACK_RANGE
    try:
        base = int(base)
    except (TypeError, ValueError):
        raise ServiceError("ws-base-port-invalid")
    if not 1 <= base <= 65535:
        raise ServiceError("ws-base-port-invalid")
    top = min(base + int(fallback_range) - 1, 65535)
    return list(range(base, top + 1))


class FirstClientGate:
    """One-shot gate: the first WS client starts audio/ASR exactly once."""

    def __init__(self):
        self._lock = threading.Lock()
        self._fired = False

    def fire(self):
        """Return True exactly once (first call), False afterwards."""
        with self._lock:
            if self._fired:
                return False
            self._fired = True
            return True

    @property
    def fired(self):
        with self._lock:
            return self._fired


class ProcessSupervisor:
    """Owns the WS/audio/ASR child processes with lazy audio/ASR start."""

    def __init__(self, config, parent_pid, emitter, parent_checker=None,
                 clock=time.time, sleep=time.sleep,
                 queue_factory=None, manager_factory=None, process_factory=None,
                 watchdog_interval=None, prewarm=True):
        self.config = dict(config)
        self.parent_pid = int(parent_pid)
        self.emitter = emitter
        self._checker = parent_checker
        self._clock = clock
        self._sleep = sleep
        self._watchdog_interval = watchdog_interval
        if not self._watchdog_interval or self._watchdog_interval <= 0:
            self._watchdog_interval = WATCHDOG_POLL_SEC
        self._queue_factory = queue_factory or (lambda: mp.Queue(maxsize=QUEUE_MAXSIZE))
        self._manager_factory = manager_factory or mp.Manager
        self._process_factory = process_factory or mp.Process
        self.base_port = int(self.config.get("ws_port", 8765))
        self.effective_port = None
        self.prewarm = bool(config.get("prewarm", prewarm))
        self.asr_state = "starting" if self.prewarm else "unavailable"  # pre-first-client ~= stt_unreachable
        self.state = "starting"
        self.error_code = None
        self.manager = None
        self.shared = None
        self.audio_queue = self.text_queue = self.log_queue = None
        self.procs = {"ws": None, "audio": None, "asr": None}
        self.session_dir = None
        self.audio_started = False
        self.failures = []
        self._pending_restart_at = None
        self.first_client_event = None
        self.first_client_gate = FirstClientGate()
        self._shutdown_done = False

    # -- construction ----------------------------------------------------
    def _make_queues(self):
        self.audio_queue = self._queue_factory()
        self.text_queue = self._queue_factory()
        self.log_queue = self._queue_factory()

    def _make_process(self, target, args=(), kwargs=None, name=""):
        return self._process_factory(target=target, args=tuple(args),
                                     kwargs=dict(kwargs or {}), name=name, daemon=True)

    def _build_session_dir(self):
        timestamp = time.strftime("%Y-%m-%d_%H%M%S")
        output_dir = self.config.get("output_dir") or os.getcwd()
        session_dir = os.path.join(output_dir, "session_%s" % timestamp)
        try:
            os.makedirs(session_dir, exist_ok=True)
        except OSError:
            raise ServiceError("session-dir-unwritable")
        return session_dir

    def start(self):
        """Pre-flight checks + WS start. Audio/ASR stay lazy until first client."""
        from liveaudio.core.network import port_range_available
        candidate_ports(self.base_port)  # validates range early
        if not port_range_available(self.base_port):
            raise ServiceError("port-range-exhausted")
        if not is_parent_alive(self.parent_pid, checker=self._checker):
            raise ServiceError("parent-dead-at-startup")
        self.manager = self._manager_factory()
        try:
            # Snapshot semantics: children read this frozen copy; CTK changes need
            # a service restart (documented, no hot reload in this track).
            self.shared = self.manager.dict(dict(self.config))
            self._make_queues()
            self.session_dir = self._build_session_dir()
            self.first_client_event = mp.Event()
            self._start_ws()
            if self.prewarm:
                self._start_audio_asr()
        except Exception:
            # start() failed after the Manager was created: never leak it.
            try:
                self.manager.shutdown()
            except Exception:
                pass
            self.manager = None
            raise
        self.state = "running"
        self.emitter.emit("service_state", self._state_fields())
        self._write_health()

    # -- child startup ----------------------------------------------------
    def _start_ws(self):
        from liveaudio.core.network import run_ws_server
        proc = self._make_process(
            run_ws_server,
            args=(self.text_queue, self.log_queue, self.base_port),
            kwargs={"first_client_event": self.first_client_event},
            name="liveaudio-ws",
        )
        proc.start()
        self.procs["ws"] = proc

    def _start_audio_asr(self):
        from liveaudio.core.workers import run_asr, run_audio
        p_audio = self._make_process(
            run_audio, args=(self.audio_queue, self.shared, self.log_queue),
            name="liveaudio-audio")
        p_asr = self._make_process(
            run_asr, args=(self.audio_queue, self.text_queue, self.log_queue,
                           self.shared, self.session_dir),
            name="liveaudio-asr")
        p_audio.start()
        p_asr.start()
        self.procs["audio"] = p_audio
        self.procs["asr"] = p_asr
        self.audio_started = True
        self.asr_state = "starting"
        self.emitter.emit("asr_state", {"asr_state": self.asr_state})

    def _maybe_start_lazy(self):
        if self.audio_started:
            return
        try:
            signalled = bool(self.first_client_event is not None and self.first_client_event.is_set())
        except Exception:
            signalled = False
        if signalled and self.first_client_gate.fire():
            self._start_audio_asr()

    # -- supervision -------------------------------------------------------
    def _note_failure(self):
        """Record a child failure. True when the 3-in-5min ceiling is hit."""
        now = self._clock()
        cutoff = now - CHILD_FAILURE_WINDOW_SEC
        self.failures = [t for t in self.failures if t >= cutoff] + [now]
        return len(self.failures) >= MAX_CHILD_FAILURES

    def _backoff_delay(self):
        count = len(self.failures)
        return min(RESPAWN_BACKOFF_BASE_SEC * (2 ** max(0, count - 1)), RESPAWN_BACKOFF_MAX_SEC)

    @staticmethod
    def _is_alive(proc):
        try:
            return bool(proc is not None and proc.is_alive())
        except Exception:
            return False

    def _pump_log_queue(self):
        """Drain structured child signals (bounded). Never raises, never logs text."""
        for _ in range(LOG_PUMP_MAX_PER_TICK):
            try:
                msg = self.log_queue.get_nowait()
            except Exception:
                break
            try:
                if not isinstance(msg, dict):
                    continue
                if msg.get("type") == "ws_port" and "port" in msg:
                    try:
                        self.effective_port = int(msg["port"])
                    except (TypeError, ValueError):
                        continue
                    self.emitter.emit("ws_port", {
                        "base_port": self.base_port,
                        "effective_port": self.effective_port,
                    })
                    self._write_health()
                elif msg.get("type") == "status" and msg.get("key") == "asr":
                    state = msg.get("state")
                    mapped = {"active": "loading", "ok": "ready", "error": "failed"}.get(state)
                    if mapped and mapped != self.asr_state:
                        self.asr_state = mapped
                        self.emitter.emit("asr_state", {"asr_state": self.asr_state})
                        self._write_health()
            except Exception:
                continue

    def _check_children(self):
        if self._pending_restart_at is not None:
            return  # restart already scheduled: don't burn the failure budget
        dead = [name for name, proc in self.procs.items()
                if proc is not None and not self._is_alive(proc)
                and (name == "ws" or self.audio_started)]
        if not dead:
            return
        if self._note_failure():
            raise ServiceError("children-failed-3-in-5m")
        self._pending_restart_at = self._clock() + self._backoff_delay()
        self.emitter.emit("warning", {"code": "child-restart-scheduled"})

    def _run_pending_restart(self):
        if self._pending_restart_at is None or self._clock() < self._pending_restart_at:
            return
        self._pending_restart_at = None
        self._stop_children()
        self._drain_queues()
        self._close_queues()
        self._make_queues()  # vaciar y recrear ambas colas en respawn
        self.first_client_event = mp.Event()
        self._start_ws()
        if self.audio_started:
            self._start_audio_asr()

    def poll_once(self):
        """One supervision tick. Raises ServiceError on fatal conditions."""
        if self._shutdown_done:
            return
        if not is_parent_alive(self.parent_pid, checker=self._checker):
            raise ServiceError("parent-dead")
        self._pump_log_queue()
        self._maybe_start_lazy()
        self._check_children()
        self._run_pending_restart()

    def run(self):
        """Block until parent death or fatal failure. Returns exit code."""
        try:
            self.start()
        except ServiceError as exc:
            self.error_code = exc.code
            self.emitter.emit("fatal", {"code": exc.code})
            self.state = "stopped"
            self._write_health()
            return 2
        while True:
            try:
                self.poll_once()
            except ServiceError as exc:
                self.error_code = exc.code
                self.emitter.emit("fatal", {"code": exc.code})
                self.shutdown(reason=exc.code)
                return 0 if exc.code == "parent-dead" else 1
            except KeyboardInterrupt:
                self.shutdown(reason="interrupted")
                return 0
            try:
                self._sleep(self._watchdog_interval)
            except Exception:
                pass

    # -- state --------------------------------------------------------------
    def _state_fields(self):
        return {
            "state": self.state,
            "base_port": self.base_port,
            "effective_port": self.effective_port,
            "asr_state": self.asr_state,
            "children": {name: self._is_alive(proc) for name, proc in self.procs.items()},
            "error_code": self.error_code,
        }

    def snapshot(self):
        from liveaudio.service.health import SERVICE_EVENT_SCHEMA, SERVICE_EVENT_VERSION
        snap = {
            "schema": SERVICE_EVENT_SCHEMA,
            "version": SERVICE_EVENT_VERSION,
            "service_pid": self.emitter.service_pid,
            "parent_pid": self.parent_pid,
        }
        snap.update(self._state_fields())
        return snap

    def _write_health(self):
        self.emitter.write_snapshot(self.snapshot())

    # -- shutdown -------------------------------------------------------------
    @staticmethod
    def _stop_process(proc, timeout=JOIN_TIMEOUT_SEC):
        """Graceful stop: join bounded, then terminate, then kill as fallback."""
        if proc is None:
            return
        try:
            if not proc.is_alive():
                return
        except Exception:
            return
        try:
            proc.join(timeout=timeout)
        except Exception:
            pass
        try:
            if proc.is_alive():
                proc.terminate()
                try:
                    proc.join(timeout=2)
                except Exception:
                    pass
        except Exception:
            pass
        try:
            if proc.is_alive():
                proc.kill()  # last resort: terminate() may not suffice
                try:
                    proc.join(timeout=2)
                except Exception:
                    pass
        except Exception:
            pass

    def _stop_children(self):
        for q in (self.audio_queue, self.text_queue):
            if q is not None:
                try:
                    q.put_nowait(None)  # sentinel de apagado limpio
                except Exception:
                    pass
        for proc in self.procs.values():
            self._stop_process(proc)
        self.procs = {"ws": None, "audio": None, "asr": None}

    @staticmethod
    def _drain_queue(q):
        try:
            while True:
                q.get_nowait()
        except Exception:
            pass

    def _drain_queues(self):
        for q in (self.audio_queue, self.text_queue, self.log_queue):
            if q is not None:
                self._drain_queue(q)

    @staticmethod
    def _close_queue(q):
        """Release queue OS resources (mp.Queue only; duck-typed fakes ignore)."""
        for method in ("close", "cancel_join_thread"):
            try:
                getattr(q, method)()
            except Exception:
                pass

    def _close_queues(self):
        for q in (self.audio_queue, self.text_queue, self.log_queue):
            if q is not None:
                self._close_queue(q)

    def shutdown(self, reason="stopping"):
        """Idempotent teardown: safe to call twice (or after partial start)."""
        if self._shutdown_done:
            return
        self._shutdown_done = True
        self.state = "stopping"
        try:
            self.emitter.emit("service_state", self._state_fields())
        except Exception:
            pass
        try:
            self._stop_children()
        except Exception:
            pass
        try:
            self._drain_queues()
        except Exception:
            pass
        try:
            self._close_queues()
        except Exception:
            pass
        if self.manager is not None:
            try:
                self.manager.shutdown()
            except Exception:
                pass
            self.manager = None
        self.state = "stopped"
        try:
            self.emitter.emit("service_state", self._state_fields())
            self._write_health()
        except Exception:
            pass
