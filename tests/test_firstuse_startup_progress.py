# SPDX-License-Identifier: MIT
"""Track firstuse-startup-progress_20260905 — TDD probes (T1-T4).

Stdlib-first: no mic/GPU/network/service. Engine-level tests reuse the
existing test_engine.py pattern (imports engine with torch available);
supervisor tests use fakes like test_resilience_service_backend.py.
"""

import queue
import unittest

from liveaudio.core.provisioning import (
    ABSOLUTE_STARTUP_SEC,
    DEFAULT_STALL_SEC,
    HONEST_ASR_STATES,
    asr_state_legacy,
    build_progress_event,
    clamp_percent,
    classify_provisioning_error,
    monotonic_percent,
    parse_tqdm_percent,
)


class FakeEmitter:
    def __init__(self):
        self.events = []
        self.service_pid = 1

    def emit(self, event_type, fields=None):
        self.events.append((event_type, dict(fields or {})))

    def write_snapshot(self, snapshot):
        return True


def _make_supervisor(monkey_clock=None):
    from liveaudio.service.supervisor import ProcessSupervisor
    clock = monkey_clock or [1000.0]

    def _clock():
        return clock[0]

    emitter = FakeEmitter()
    sup = ProcessSupervisor(
        {"ws_port": 8765, "output_dir": "", "prewarm": False},
        parent_pid=1234,
        emitter=emitter,
        parent_checker=lambda pid: True,
        clock=_clock,
        sleep=lambda s: None,
        queue_factory=lambda: queue.Queue(),
        manager_factory=None,
        process_factory=None,
    )
    sup.log_queue = queue.Queue()
    return sup, emitter, clock


class TestTqdmParse(unittest.TestCase):
    def test_float_percent(self):
        self.assertAlmostEqual(parse_tqdm_percent(" 42.5%|██| 3/24"), 42.5)

    def test_int_percent(self):
        self.assertAlmostEqual(parse_tqdm_percent("Downloading: 7%"), 7.0)

    def test_unparseable_returns_none(self):
        self.assertIsNone(parse_tqdm_percent("Downloading shards..."))
        self.assertIsNone(parse_tqdm_percent(""))

    def test_clamp_bounds(self):
        self.assertEqual(clamp_percent(142.0), 100.0)
        self.assertEqual(clamp_percent(-3.0), 0.0)

    def test_monotonic_same_attempt(self):
        self.assertEqual(monotonic_percent(40.0, 30.0, attempt_changed=False), 40.0)
        self.assertEqual(monotonic_percent(40.0, 50.0, attempt_changed=False), 50.0)

    def test_reset_once_on_attempt_change(self):
        self.assertEqual(monotonic_percent(80.0, 5.0, attempt_changed=True), 5.0)


class TestLegacyMirror(unittest.TestCase):
    def test_collapse(self):
        for honest in ("downloading", "loading", "transcribing", "stalled"):
            self.assertEqual(asr_state_legacy(honest), "loading", honest)
        self.assertEqual(asr_state_legacy("ready"), "ready")
        self.assertEqual(asr_state_legacy("failed"), "failed")

    def test_honest_states_complete(self):
        for s in ("downloading", "loading", "transcribing", "ready", "stalled", "failed"):
            self.assertIn(s, HONEST_ASR_STATES)


class TestWireFormat(unittest.TestCase):
    def test_event_shape(self):
        ev = build_progress_event(phase="downloading", percent=42.5,
                                  attempt=2, code=None, text="ASR: descargando 42%")
        self.assertEqual(ev["type"], "status")
        self.assertEqual(ev["key"], "asr")
        self.assertEqual(ev["state"], "downloading")
        self.assertEqual(ev["percent"], 42.5)
        self.assertEqual(ev["attempt"], 2)
        self.assertIsNone(ev["code"])
        self.assertTrue(ev["is_download"])
        self.assertEqual(ev["asr_state_legacy"], "loading")


class TestEngineProgress(unittest.TestCase):
    def test_structured_download_event(self):
        from liveaudio.core.engine import InterceptingWriter
        q = queue.Queue()
        w = InterceptingWriter(q)
        w.write(" 42.5%|████████| 3/24 [00:01<00:02, 1.5MB/s]\r")
        statuses = [m for m in list(q.queue)
                    if isinstance(m, dict) and m.get("type") == "status"]
        self.assertTrue(statuses, "expected a structured status event")
        ev = statuses[-1]
        self.assertEqual(ev.get("state"), "downloading")
        self.assertAlmostEqual(float(ev.get("percent")), 42.5)
        self.assertTrue(ev.get("is_download"))
        self.assertEqual(ev.get("asr_state_legacy"), "loading")
        # % travels on log_queue status channel, never via text_queue payload
        self.assertNotIn("text_queue", ev)

    def test_monotonic_no_regress(self):
        from liveaudio.core.engine import InterceptingWriter
        q = queue.Queue()
        w = InterceptingWriter(q)
        w.write(" 80%|██| 1/1\r")
        w.write(" 10%|██| 1/1\r")
        percents = [m.get("percent") for m in list(q.queue)
                    if isinstance(m, dict) and m.get("type") == "status"
                    and m.get("percent") is not None]
        self.assertTrue(percents)
        self.assertGreaterEqual(percents[-1], percents[0])

    def test_indeterminate_fallback(self):
        from liveaudio.core.engine import InterceptingWriter
        q = queue.Queue()
        w = InterceptingWriter(q)
        w.write("Fetching model shards, please wait\r")
        statuses = [m for m in list(q.queue)
                    if isinstance(m, dict) and m.get("type") == "status"
                    and m.get("key") == "asr"]
        self.assertTrue(statuses)
        self.assertEqual(statuses[-1].get("state"), "downloading")


class TestSupervisorProgress(unittest.TestCase):
    def test_preserves_structured_progress(self):
        sup, emitter, _ = _make_supervisor()
        sup.log_queue.put_nowait(build_progress_event(
            phase="downloading", percent=42.5, attempt=1,
            code=None, text="ASR: descargando 42%"))
        sup._pump_log_queue()
        self.assertEqual(sup.asr_state, "downloading")
        kinds = [t for t, _ in emitter.events if t == "asr_state"]
        self.assertTrue(kinds)
        payload = emitter.events[-1][1]
        self.assertAlmostEqual(float(payload.get("percent")), 42.5)
        self.assertEqual(payload.get("asr_state_legacy"), "loading")

    def test_status_prioritized_over_log(self):
        sup, emitter, _ = _make_supervisor()
        for i in range(5):
            sup.log_queue.put_nowait({"type": "log", "message": "noise %d" % i})
        sup.log_queue.put_nowait(build_progress_event(
            phase="downloading", percent=9.0, attempt=1,
            code=None, text="ASR: descargando 9%"))
        sup._pump_log_queue()
        self.assertEqual(sup.asr_state, "downloading")


class TestWatchdog(unittest.TestCase):
    def test_stall_fires_conservative(self):
        sup, emitter, clock = _make_supervisor()
        sup._start_audio_asr = lambda: None  # avoid real procs
        sup._startup_began_at = clock[0]
        sup._last_asr_event_at = clock[0]
        sup.asr_state = "downloading"
        clock[0] += DEFAULT_STALL_SEC + 5.0
        sup.poll_once()
        self.assertEqual(sup.asr_state, "stalled")
        codes = [f.get("code") for t, f in emitter.events]
        self.assertIn("provision-timeout-stalled", codes)

    def test_slow_progress_never_kills(self):
        sup, emitter, clock = _make_supervisor()
        sup._start_audio_asr = lambda: None
        sup.asr_state = "downloading"
        sup._startup_began_at = clock[0]
        sup._last_asr_event_at = clock[0]
        clock[0] += 10.0
        sup.log_queue.put_nowait(build_progress_event(
            phase="downloading", percent=11.0, attempt=1,
            code=None, text="ASR: descargando 11%"))
        sup._pump_log_queue()
        clock[0] += 10.0
        before = dict(sup.procs)
        sup.poll_once()
        self.assertNotEqual(sup.asr_state, "stalled")
        self.assertEqual(sup.procs, before)

    def test_retry_new_attempt_resets_once(self):
        sup, _, _ = _make_supervisor()
        sup.asr_state = "stalled"
        sup.asr_percent = 77.0
        attempt0 = sup.asr_attempt
        sup.request_asr_retry()
        self.assertEqual(sup.asr_attempt, attempt0 + 1)
        self.assertEqual(sup.asr_percent, 0.0)
        self.assertNotEqual(sup.asr_state, "stalled")


class TestErrorCatalog(unittest.TestCase):
    def test_each_class_maps(self):
        import ssl
        cases = [
            (ssl.SSLError("tls fail"), "provision-tls"),
            (ConnectionError("net down"), "provision-network"),
            (PermissionError("denied"), "provision-auth"),
            (OSError("no space left on device: disk full"), "provision-disk-full"),
            (TimeoutError("timed out"), "provision-timeout-stalled"),
            (ValueError("cache corrupt checksum"), "provision-cache-corrupt"),
        ]
        for exc, code in cases:
            self.assertEqual(classify_provisioning_error(exc), code, repr(exc))

    def test_unknown_sanitized(self):
        class WeirdBoom(Exception):
            pass
        self.assertEqual(classify_provisioning_error(WeirdBoom("x")), "provision-unknown")

    def test_model_not_found_reserved(self):
        self.assertEqual(classify_provisioning_error(
            FileNotFoundError("model.safetensors")), "model-not-found")
        # A cache read error is NOT "not found"
        self.assertNotEqual(classify_provisioning_error(
            OSError("cache corrupt checksum bad")), "model-not-found")


class TestTlsScope(unittest.TestCase):
    def test_context_restored(self):
        import ssl
        import liveaudio.core.engine as engine_mod
        orig = getattr(ssl, "_create_default_https_context", None)
        engine_mod._scoped_unverified_context_for_provisioning(lambda: None)
        self.assertIs(ssl._create_default_https_context, orig)


class TestI18nKeys(unittest.TestCase):
    def test_new_keys_both_locales(self):
        from liveaudio.utils.i18n import TRANSLATIONS
        keys = ["status_asr_downloading", "status_asr_downloading_indeterminate",
                "status_asr_stalled", "status_asr_startup_slow",
                "provision_cache_corrupt_hint", "provision_network_hint",
                "provision_auth_hint", "provision_disk_full_hint",
                "provision_timeout_stalled_hint", "provision_tls_hint",
                "provision_unknown_hint", "model_not_found_hint",
                "prewarm_toggle_label", "prewarm_toggle_desc", "asr_retry_action"]
        for key in keys:
            self.assertIn(key, TRANSLATIONS["es"], key)
            self.assertIn(key, TRANSLATIONS["en"], key)


class TestPreimportHeartbeat(unittest.TestCase):
    """F1 (REQ-6, option a): ASR child emits importing status BEFORE heavy import."""

    def test_heartbeat_before_heavy_import(self):
        import sys
        import liveaudio.core.workers as workers_mod
        real_engine = sys.modules.get("liveaudio.core.engine")
        q_log = queue.Queue()
        seen = {}

        class FakeEngine:
            @staticmethod
            def asr_consumer(*args):
                # The heartbeat must already be queued when the consumer runs.
                seen["importing_queued"] = any(
                    isinstance(m, dict) and m.get("type") == "status"
                    and m.get("key") == "asr" and m.get("phase") == "importing"
                    for m in list(q_log.queue)
                )
                seen["called"] = True

        sys.modules["liveaudio.core.engine"] = FakeEngine
        try:
            workers_mod.run_asr(queue.Queue(), queue.Queue(), q_log, {}, "session_dir")
        finally:
            if real_engine is not None:
                sys.modules["liveaudio.core.engine"] = real_engine
            else:
                sys.modules.pop("liveaudio.core.engine", None)
        self.assertTrue(seen.get("called"), "asr_consumer must still run")
        self.assertTrue(seen.get("importing_queued"),
                        "importing heartbeat must precede the heavy import")
        first = next(m for m in list(q_log.queue)
                     if isinstance(m, dict) and m.get("type") == "status")
        self.assertEqual(first.get("key"), "asr")
        self.assertEqual(first.get("state"), "active")
        self.assertEqual(first.get("phase"), "importing")

    def test_supervisor_maps_importing_to_loading(self):
        sup, _, clock = _make_supervisor()
        sup._startup_began_at = clock[0]
        sup._last_asr_event_at = clock[0]
        clock[0] += 5.0
        sup.log_queue.put_nowait({"type": "status", "key": "asr",
                                  "state": "active", "phase": "importing"})
        sup._pump_log_queue()
        # Heartbeat proves the child is alive pre-import: honest loading state
        # plus a refreshed watchdog clock (silence since provisioning start
        # still trips stalled-import later).
        self.assertEqual(sup.asr_state, "loading")
        self.assertEqual(sup._last_asr_event_at, clock[0])


class TestRetryAttemptGuard(unittest.TestCase):
    """F2: the GUI consumes attempt so it never inherits a stale percent."""

    def test_older_attempt_is_superseded(self):
        from liveaudio.app import _asr_event_superseded
        self.assertTrue(_asr_event_superseded(3, {"attempt": 2, "phase": "downloading"}))

    def test_same_or_newer_attempt_applies(self):
        from liveaudio.app import _asr_event_superseded
        self.assertFalse(_asr_event_superseded(2, {"attempt": 2, "phase": "downloading"}))
        self.assertFalse(_asr_event_superseded(2, {"attempt": 3, "phase": "downloading"}))

    def test_missing_or_bad_attempt_applies(self):
        from liveaudio.app import _asr_event_superseded
        self.assertFalse(_asr_event_superseded(2, {"phase": "downloading"}))
        self.assertFalse(_asr_event_superseded(2, {"attempt": "n/a"}))


class TestSupervisorStaleAttempt(unittest.TestCase):
    """F1 (QA re-review Engram #6445): the supervisor consumes attempt.

    Mirrors the GUI guard (``_asr_event_superseded`` in app.py): a delayed
    event from a previous attempt must never overwrite the fresh attempt's
    display (no 0->77 % jump in service mode).
    """

    def test_stale_attempt_ignored(self):
        sup, emitter, _ = _make_supervisor()
        sup.asr_state = "loading"
        sup.asr_phase = "loading"
        sup.asr_percent = 0.0
        sup.asr_attempt = 2  # retry sealed a new attempt, % reset once
        self.assertIsNone(sup._last_asr_event_at)
        asr_events_before = len([t for t, _ in emitter.events if t == "asr_state"])
        sup.log_queue.put_nowait(build_progress_event(
            phase="downloading", percent=77.0, attempt=1,
            code=None, text="ASR: descargando 77%"))
        sup._pump_log_queue()
        # Stale event touches nothing: no %, no watchdog clock, no emit.
        self.assertEqual(sup.asr_percent, 0.0)
        self.assertEqual(sup.asr_attempt, 2)
        self.assertEqual(sup.asr_state, "loading")
        self.assertIsNone(sup._last_asr_event_at)
        asr_events_after = len([t for t, _ in emitter.events if t == "asr_state"])
        self.assertEqual(asr_events_after, asr_events_before)

    def test_current_attempt_applies(self):
        sup, _, _ = _make_supervisor()
        sup.asr_state = "loading"
        sup.asr_percent = 0.0
        sup.asr_attempt = 2
        sup.log_queue.put_nowait(build_progress_event(
            phase="downloading", percent=12.0, attempt=2,
            code=None, text="ASR: descargando 12%"))
        sup._pump_log_queue()
        self.assertAlmostEqual(sup.asr_percent, 12.0)

    def test_missing_attempt_applies(self):
        # The REQ-6 pre-import heartbeat carries no attempt: it must still
        # refresh the watchdog clock.
        sup, _, clock = _make_supervisor()
        sup._startup_began_at = clock[0]
        sup._last_asr_event_at = clock[0]
        sup.asr_attempt = 2
        clock[0] += 5.0
        sup.log_queue.put_nowait({"type": "status", "key": "asr",
                                  "state": "active", "phase": "importing"})
        sup._pump_log_queue()
        self.assertEqual(sup._last_asr_event_at, clock[0])


if __name__ == "__main__":
    unittest.main()
