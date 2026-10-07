# Launcher native Windows TLS

## Objective and problem
Use native Windows certificate-chain validation for launcher-owned HTTPS. A Windows 10 VM failed source downloads with an unavailable issuer, then the unchanged installer succeeded after native certificate updates. This is compatibility hardening, not evidence that every certificate failure is solved.

## Authorized scope and constraints
- Branch: `codex/launcher-native-tls`.
- Launcher HTTPS only; no application SSL mutation, global injection, insecure fallback, installer publication, or remote operations.
- Bundle the dependency before bootstrap; preserve non-Windows behavior and unrelated work.
- TDD enabled by AGENTS.md strict-tdd-mode; runner: `uv run pytest`.
- Delivery: `ask-on-risk`; forecast below 400 authored changed lines, excluding generated lock data.
- Auditor approved the local source work-unit commit after four scoped reviews and independent focused verification. Remote operations and installer builds remain unauthorized.

## Tasks
- [ ] T1: Integrate Windows native TLS context, build/dev dependency, packaging inclusion, regression tests, and documentation.
  - Source implementation approved and automated verification complete; task closure awaits packaged runtime evidence.
  - Route: delegated; preparation and two or more non-trivial files.
  - Acceptance: explicit `truststore.SSLContext(ssl.PROTOCOL_TLS_CLIENT)` for Windows launcher requests; hostname/certificate validation retained; actionable missing-dependency error; Linux unchanged; no global injection.
  - Checks: observe RED before implementation; focused launcher/first-run tests; lint; launcher self-test; Python compilation; full test suite.
  - Runtime checks: frozen dependency inclusion and clean Windows 10 VM bootstrap remain pending until an authorized build and manual run.
  - Source commit: `6c608a2331f4248724659998309b08f8db03c33f` (`fix(launcher): use native Windows certificate validation`).

## Verification and progress
- RED: `uv run python -m pytest -q tests/test_launcher.py -k TestLauncherTls` produced 3 expected failures and 1 pass before implementation (missing explicit native context / missing-dependency fail-closed behavior).
- The configured `uv run pytest` entrypoint failed with `uv trampoline failed to canonicalize script path`; equivalent `uv run python -m pytest` runner used thereafter. No unrelated environment repair.
- GREEN: `uv run python -m pytest -q tests/test_launcher.py tests/test_unified_first_run.py`: 97 passed.
- Full suite: `uv run python -m pytest -q -o faulthandler_timeout=60`: 874 passed, 75 subtests passed, 58.99 seconds.
- `uv run ruff check .`: passed.
- `uv run python packaging/launcher.py --self-test --headless`: passed (local paths/device only, not a TLS/network proof).
- Python compilation of launcher, spec, and tests: passed.
- `uv lock --check` and `git diff --check`: passed.
- Native source-runtime context smoke: Windows backend imported; `CERT_REQUIRED` and hostname validation true; global stdlib SSLContext unchanged.
- Refactor: no additional production refactor required; documentation clarified dependency boundary.
- Authored source/test/docs diff: 106 lines before this progress record; generated lockfile changes excluded from forecast.
- Frozen executable build/inclusion and clean Windows 10 VM bootstrap: NOT RUN; pending explicit build/manual verification. No claim that warmed VM success proves the new backend.
- Independent verification: 97 focused tests passed; four source reviews reported no blockers. RDD off globally.
- Approved local source work-unit committed as `6c608a2331f4248724659998309b08f8db03c33f`; no publication or remote mutation; unrelated dirty files preserved.

## Rollback boundary
Revert only this task's launcher context, dependency/lock additions, packaging inclusion, tests, and associated documentation; preserve unrelated audio and release behavior.

## Next step
Separately authorized frozen build/inclusion and clean VM runtime validation; T1 remains open until that evidence exists.
