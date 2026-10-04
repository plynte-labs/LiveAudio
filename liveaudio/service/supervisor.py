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
import math
import os
import threading
import time

from liveaudio.core.provisioning import (
    ABSOLUTE_STARTUP_SEC,
    DEFAULT_STALL_SEC,
    HONEST_ASR_STATES,
    STALL_MAX_SEC,
    STALL_MIN_SEC,
    asr_state_legacy,
    clamp_percent,
    monotonic_percent,
)
from liveaudio.service.errors import ServiceError
from liveaudio.service.watchdog import is_parent_alive
from liveaudio.utils.config import audio_queue_capacity

MAX_CHILD_FAILURES = 3
CHILD_FAILURE_WINDOW_SEC = 300.0
RESPAWN_BACKOFF_BASE_SEC = 1.0
RESPAWN_BACKOFF_MAX_SEC = 15.0

WATCHDOG_POLL_SEC = 1.0
QUEUE_MAXSIZE = 100
JOIN_TIMEOUT_SEC = 3.0
LOG_PUMP_MAX_PER_TICK = 100
WRITER_FATAL_CODES = frozenset({
    "writer_queue_full", "writer_storage_error", "writer_drain_timeout",
})


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
        self._queue_factory = queue_factory
        self._manager_factory = manager_factory or mp.Manager
        self._process_factory = process_factory or mp.Process
        self.base_port = int(self.config.get("ws_port", 8765))
        self.effective_port = None
        self.prewarm = bool(config.get("prewarm", prewarm))
        self.asr_state = "starting" if self.prewarm else "unavailable"  # pre-first-client ~= stt_unreachable
        # First-use startup progress (T1-T3): honest provisioning state.
        self.asr_phase = None
        self.asr_percent = None
        self.asr_attempt = 1
        self.asr_code = None
        try:
            stall = float(config.get("startup_stall_sec", DEFAULT_STALL_SEC))
        except (TypeError, ValueError):
            stall = DEFAULT_STALL_SEC
        self.stall_sec = max(STALL_MIN_SEC, min(STALL_MAX_SEC, stall))
        try:
            absolute = float(config.get("absolute_startup_sec", ABSOLUTE_STARTUP_SEC))
        except (TypeError, ValueError):
            absolute = ABSOLUTE_STARTUP_SEC
        self.absolute_startup_sec = max(self.stall_sec, absolute)
        self._startup_began_at = None
        self._last_asr_event_at = None
        self._stalled_emitted = False
        self._startup_advisory_emitted = False
        self._last_emitted_percent = None
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
        self.loss_counters = {
            "audio.shutdown_discarded": 0,
            "asr.text_shutdown_discarded": 0,
            "runtime.log_shutdown_discarded": 0,
            "audio.stop_control_rejected": 0,
            "asr.text_stop_control_rejected": 0,
            "asr.decode_interrupted": 0,
        }

    # -- construction ----------------------------------------------------
    def _make_queues(self):
        if self._queue_factory is not None:
            self.audio_queue = self._queue_factory()
            self.text_queue = self._queue_factory()
            self.log_queue = self._queue_factory()
        else:
            self.audio_queue = mp.Queue(maxsize=audio_queue_capacity(self.config))
            self.text_queue = mp.Queue(maxsize=QUEUE_MAXSIZE)
            self.log_queue = mp.Queue(maxsize=QUEUE_MAXSIZE)

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
            self.shared["writer_failure_code"] = None
            self.shared["writer_failure_attempt"] = None
            self.shared["asr_attempt"] = self.asr_attempt
            self.shared["asr_decode"] = None
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
        if self.shared is not None:
            self.shared["asr_attempt"] = self.asr_attempt
            self.shared["asr_decode"] = None
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
        now = self._clock()
        if self._startup_began_at is None:
            self._startup_began_at = now
        self._last_asr_event_at = now
        self.emitter.emit("asr_state", {"asr_state": self.asr_state,
                                        "asr_state_legacy": asr_state_legacy("loading")})

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

    @staticmethod
    def _honest_state_from(msg):
        """Resolve the honest asr_state for a status event (T1/D2).

        Prefers the structured ``phase``; falls back to the legacy
        active/ok/error wire for old child events, using is_download to keep
        in-flight downloads honest instead of collapsing them to loading.
        """
        phase = msg.get("phase")
        if phase == "importing":
            # REQ-6 pre-import heartbeat (workers.run_asr): the child is
            # alive but hasn't reached engine status emission yet. Honest
            # "loading" + watchdog clock refresh; silence since provisioning
            # start still trips stalled-import in _check_provisioning_watchdog.
            return "loading"
        if phase in HONEST_ASR_STATES:
            return phase
        if msg.get("percent") is not None or msg.get("is_download"):
            return "downloading"
        state = msg.get("state")
        if state in HONEST_ASR_STATES:
            return state
        return {"active": "loading", "ok": "ready", "error": "failed"}.get(state)

    def _ingest_asr_status(self, msg):
        honest = self._honest_state_from(msg)
        if not honest:
            return
        # F1 (D3): consume attempt. A delayed event from a previous attempt
        # must never overwrite the fresh attempt's display (no inherited
        # stale % jump, e.g. 0->77 after a manual retry). Missing or
        # unparseable attempt counts as current (applies) — same rule as
        # the GUI guard (_asr_event_superseded in app.py). A stale event
        # touches nothing: no %, no watchdog clock, no emit.
        try:
            ev_attempt = msg.get("attempt", None)
            if ev_attempt is not None:
                ev_attempt = int(ev_attempt)
                current = int(self.asr_attempt or 0)
                if ev_attempt < current:
                    return
                if ev_attempt > current:
                    self.asr_attempt = ev_attempt
        except (TypeError, ValueError, AttributeError):
            pass
        now = self._clock()
        self._last_asr_event_at = now
        raw_percent = msg.get("percent")
        if raw_percent is not None:
            try:
                candidate = clamp_percent(float(raw_percent))
            except (TypeError, ValueError):
                candidate = None
            if candidate is not None:
                previous = self.asr_percent if self.asr_percent is not None else 0.0
                self.asr_percent = monotonic_percent(previous, candidate)
        elif honest == "downloading" and self.asr_percent is None:
            self.asr_percent = None  # indeterminate fallback: no frozen 0%
        if msg.get("code") is not None:
            self.asr_code = msg.get("code")
        elif honest == "failed":
            # Runtime failure without a provisioning code: never show a stale
            # code from an earlier provisioning phase (GUI falls back to
            # provision-unknown hint).
            self.asr_code = None
        elif honest != "stalled":
            self.asr_code = None
        if honest in ("downloading", "loading", "transcribing"):
            # Resumed progress clears a previous stall; ready clears latched flags.
            if self._stalled_emitted and honest == "downloading":
                self._stalled_emitted = False
        if honest == "ready":
            self._stalled_emitted = False
            self._startup_advisory_emitted = False
        if honest != self.asr_state:
            self.asr_state = honest
            self.asr_phase = honest
            self._last_emitted_percent = None
            self._emit_asr_state()
        elif raw_percent is not None:
            bucket = int(self.asr_percent) if self.asr_percent is not None else None
            if bucket != self._last_emitted_percent:
                self._emit_asr_state()

    def _emit_asr_state(self):
        if self.asr_percent is not None:
            self._last_emitted_percent = int(self.asr_percent)
        fields = {
            "asr_state": self.asr_state,
            "asr_state_legacy": asr_state_legacy(self.asr_state),
            "phase": self.asr_phase or self.asr_state,
            "attempt": self.asr_attempt,
            "code": self.asr_code,
        }
        if self.asr_percent is not None:
            fields["percent"] = float(self.asr_percent)
        if self.asr_state == "downloading":
            fields["is_download"] = True
        self.emitter.emit("asr_state", fields)
        self._write_health()

    def _pump_log_queue(self):
        """Drain structured child signals (bounded) without logging transcript text.

        Status events are prioritized over log noise: the tick drains up to
        LOG_PUMP_MAX_PER_TICK messages via get_nowait (never blocks ASR) and
        applies ASR status last-write-wins so a percent is never dropped
        behind a burst of log lines.
        """
        pending_status = None
        writer_fatal_code = None
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
                elif (msg.get("type") == "fatal"
                      and msg.get("code") in WRITER_FATAL_CODES):
                    writer_fatal_code = msg["code"]
                elif msg.get("type") == "status" and msg.get("key") == "asr":
                    pending_status = msg  # status wins over interleaved logs
            except Exception:
                continue
        if pending_status is not None:
            try:
                self._ingest_asr_status(pending_status)
            except Exception:
                pass
        if writer_fatal_code is not None:
            raise ServiceError(writer_fatal_code)

    def _check_provisioning_watchdog(self):
        """Conservative startup/stall watchdog (T2/D3).

        Stall (120-180s, default 150s) with zero events/progress while still
        provisioning -> ``stalled`` + manual retry offer. Slow-but-progressing
        only gets the advisory copy. NEVER kills: no terminate/kill path here,
        independent of the process-liveness watchdog in _check_children.
        """
        if self._startup_began_at is None:
            return
        if self.asr_state not in ("starting", "downloading", "loading",
                                  "transcribing", "stalled"):
            return
        now = self._clock()
        last = self._last_asr_event_at if self._last_asr_event_at is not None else self._startup_began_at
        if self.asr_state != "stalled" and (now - last) >= self.stall_sec:
            self.asr_state = "stalled"
            self.asr_phase = "stalled"
            self.asr_code = "provision-timeout-stalled"
            self._stalled_emitted = True
            self._emit_asr_state()
            return
        if (not self._startup_advisory_emitted
                and (now - self._startup_began_at) >= self.absolute_startup_sec
                and self.asr_state != "stalled"):
            self._startup_advisory_emitted = True
            self.emitter.emit("warning", {"code": "provision-timeout-stalled",
                                          "advisory": True,
                                          "i18n_key": "status_asr_startup_slow"})

    def request_asr_retry(self):
        """Manual retry (D3): new attempt, % resets to 0 exactly once, then monotonic."""
        self.asr_attempt = int(self.asr_attempt or 0) + 1
        self.asr_percent = 0.0
        self.asr_code = None
        self._stalled_emitted = False
        self._startup_advisory_emitted = False
        self._last_emitted_percent = None
        now = self._clock()
        self._startup_began_at = now
        self._last_asr_event_at = now
        if self.asr_state in ("stalled", "failed"):
            self.asr_state = "loading"
            self.asr_phase = "loading"
        self._emit_asr_state()
        return self.asr_attempt

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
            self.asr_attempt = int(self.asr_attempt or 0) + 1
            self._start_audio_asr()

    def _check_asr_decode_deadline(self):
        try:
            marker = self.shared.get("asr_decode") if self.shared is not None else None
        except Exception:
            return
        if not isinstance(marker, dict) or marker.get("status") != "decoding":
            return
        attempt = marker.get("attempt")
        deadline = marker.get("deadline_monotonic")
        if (type(attempt) is not int or attempt != self.asr_attempt
                or type(deadline) not in (int, float) or not math.isfinite(deadline)
                or time.monotonic() < deadline):
            return
        self.loss_counters["asr.decode_interrupted"] += 1
        started = marker.get("started_monotonic")
        elapsed = None
        if type(started) in (int, float) and math.isfinite(started):
            elapsed = max(0.0, time.monotonic() - started)
        timeout_sec = marker.get("timeout_sec", 15)
        if type(timeout_sec) is not int or not 5 <= timeout_sec <= 120:
            timeout_sec = 15
        utterance_id = marker.get("utterance_id")
        warning = {
            "code": "asr-decode-timeout",
            "elapsed_sec": elapsed,
            "timeout_sec": timeout_sec,
            "message": (
                "ASR decode exceeded its deadline. Capture stopped; pending audio or transcript data "
                "may be lost and will not be retried automatically."
            ),
        }
        if isinstance(utterance_id, str) and len(utterance_id) <= 128:
            warning["utterance_id"] = utterance_id
        self.emitter.emit("warning", warning)
        raise ServiceError("asr-decode-timeout")

    def poll_once(self):
        """One supervision tick. Raises ServiceError on fatal conditions."""
        if self._shutdown_done:
            return
        if not is_parent_alive(self.parent_pid, checker=self._checker):
            raise ServiceError("parent-dead")
        try:
            writer_failure_code = (
                self.shared.get("writer_failure_code") if self.shared is not None else None
            )
        except Exception:
            writer_failure_code = None
        if writer_failure_code in WRITER_FATAL_CODES:
            raise ServiceError(writer_failure_code)
        self._check_asr_decode_deadline()
        self._pump_log_queue()
        self._maybe_start_lazy()
        # REQ-6 (decided: option a, PO 2026-09-05): the ASR child emits a
        # pre-import heartbeat (phase "importing" via put_nowait in
        # workers.run_asr) before the heavy torch/faster-whisper import.
        # It resolves to "loading" and refreshes the watchdog clock above;
        # silence since provisioning start still trips stalled-import.
        self._check_provisioning_watchdog()
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
            "asr_state_legacy": asr_state_legacy(self.asr_state),
            "asr_phase": self.asr_phase or self.asr_state,
            "asr_percent": self.asr_percent,
            "asr_attempt": self.asr_attempt,
            "asr_code": self.asr_code,
            "children": {name: self._is_alive(proc) for name, proc in self.procs.items()},
            "error_code": self.error_code,
            "loss_counters": dict(self.loss_counters),
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
        for q, domain, metric in (
            (self.audio_queue, "audio", "audio.stop_control_rejected"),
            (self.text_queue, "text", "asr.text_stop_control_rejected"),
        ):
            if q is not None:
                try:
                    q.put_nowait(None)  # sentinel de apagado limpio
                except Exception:
                    self.loss_counters[metric] += 1
                    self.emitter.emit("warning", {"code": "shutdown-control-rejected", "queue": domain})
        for proc in self.procs.values():
            self._stop_process(proc)
        self.procs = {"ws": None, "audio": None, "asr": None}

    @staticmethod
    def _drain_queue(q):
        discarded = 0
        try:
            while True:
                if q.get_nowait() is not None:
                    discarded += 1
        except Exception:
            return discarded

    def _drain_queues(self):
        discarded_total = 0
        for q, metric in (
            (self.audio_queue, "audio.shutdown_discarded"),
            (self.text_queue, "asr.text_shutdown_discarded"),
            (self.log_queue, "runtime.log_shutdown_discarded"),
        ):
            if q is not None:
                discarded = self._drain_queue(q)
                self.loss_counters[metric] += discarded
                discarded_total += discarded
        if discarded_total:
            self.emitter.emit("warning", {"code": "shutdown-data-discarded"})

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
