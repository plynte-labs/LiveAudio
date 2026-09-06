# Unified first-run validation report

## Candidate scope

Automated implementation candidate only. Native review is unavailable; the Product
Owner approved a one-track unmanaged exception. This is **not** a native PASS.
No manual E2E row and no first-use M1–M8 row is complete.

## Execution provenance

The latest focused pytest/Ruff commands used the available `python` interpreter
(Python 3.13), outside the project's supported Python 3.11 runtime. The Python
3.11 `.venv` was used for compileall only because it has no pytest module. This
report therefore does not claim full supported-runtime test validation; the VM,
manual E2E, and review gates remain required.

## TDD evidence

- RED: `python -m pytest tests/test_unified_first_run.py -q` initially failed
  during collection because `liveaudio.core.first_run` did not exist. Subsequent
  RED assertions failed for the absent VAD heartbeat and the VAD TLS override.
- GREEN: `python -m pytest tests/test_unified_first_run.py -q` -> `11 passed`
  (one pytest cache-permission warning only).
- Focused regression: `python -m pytest tests/test_unified_first_run.py
  tests/test_launcher.py tests/test_vad_error.py -q --disable-warnings` ->
  `93 passed, 1 warning`.
- First-use regression: `python -m pytest tests/test_firstuse_startup_progress.py
  -q --disable-warnings` -> `30 passed, 1 warning`.
- Static check: `python -m ruff check liveaudio packaging/launcher.py
  tests/test_unified_first_run.py` -> `All checks passed!`.
- Compile: `python -m compileall liveaudio packaging/launcher.py tests` completed
  successfully before focused validation.

The pytest warning is environmental: this checkout cannot create the ignored
`.pytest_cache` directory. It did not prevent test execution.

## Remaining gates

- [x] Collective checkpoint review: Sol Architecture, Luna QA, Luna Research, and Terra Performance partial-checkpoint self-review. This is not native PASS and final feature review remains open.
- [ ] VM v2 rebuild (environment is prepared; no rebuild was run here).
- [ ] E2E-1 through E2E-10 with real evidence.
- [ ] First-use M1 through M8 with their own real evidence.
- [ ] Parent executes the explicitly authorized checkpoint commit; this report does not claim staging or commit.
- [ ] Push, merge, revert, and any post-checkpoint scope require explicit approval.

## Manual E2E handoff

Use the matrix in `spec.md`: capture the launcher/app boundary, VAD heartbeat
and phase retry, real Whisper progress/stale retry, warm/partial/offline cache,
reinstall hash retention, and OBS no-burst behavior. CUDA E2E-2 may only be
recorded as not run with a real capability reason.

## Truthful launcher-control follow-up

- RED: the fixed-progress assertion in `tests/test_unified_first_run.py` failed
  while `run_bootstrap` and `_await_app_window_gui` still called
  `reporter.progress(0.02/0.35/0.45/0.99/1.0)`.
- GREEN: removed those fixed phase/global fractions. Real byte-ratio reporting
  remains owned by the download path; terminal launcher completion remains a
  status line and is not ASR readiness.
- `python -m pytest tests/test_unified_first_run.py -q --disable-warnings`:
  12 passed in 3.57s.
- `python -m pytest tests/test_launcher.py tests/test_vad_error.py
  tests/test_firstuse_startup_progress.py -q --disable-warnings`: 112 passed
  in 8.60s.
- `python -m pytest tests/test_resilience_service_backend.py -q
  --disable-warnings`: 46 passed in 26.80s.
- `python -m pytest tests/test_idempotent_service_backend.py -q
  --disable-warnings`: 12 passed in 1.58s.
- `.venv\\Scripts\\python.exe -m compileall liveaudio packaging/launcher.py tests`:
  passed. The same Python 3.11 environment has no pytest module, so tests ran
  with the available project test runner (`python`, Python 3.13) without
  installing or changing dependencies.
- `python -m ruff check liveaudio packaging/launcher.py
  tests/test_unified_first_run.py`: passed.
