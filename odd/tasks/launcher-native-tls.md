# Launcher native Windows TLS

## Objective and problem
Use native Windows certificate-chain validation for launcher-owned HTTPS. A Windows 10 VM failed source downloads with an unavailable issuer, then the unchanged installer succeeded after native certificate updates. This is compatibility hardening, not evidence that every certificate failure is solved.

## Authorized scope and constraints
- Branch: `codex/launcher-native-tls`.
- Launcher HTTPS only; no application SSL mutation, global injection or insecure fallback. Owner subsequently authorized a GitHub-built v1.2.7rc4 prerelease through the configured gh session; Auditor owns all remote operations and publication.
- Bundle the dependency before bootstrap; preserve non-Windows behavior and unrelated work.
- TDD enabled by AGENTS.md strict-tdd-mode; runner: `uv run pytest`.
- Delivery: `ask-on-risk`; forecast below 400 authored changed lines, excluding generated lock data.
- Auditor approved the local source work-unit commit after four scoped reviews and independent focused verification. Owner now authorized installer build/publication; clean Windows VM runtime verification remains pending.

## Tasks
- [ ] T2: Prepare 1.2.7rc4 version and release notes, then Auditor builds/publishes through GitHub. Route: delegated release writer for metadata/docs; source logic unchanged. Local preparation checks and commit recorded below; closure requires observed remote build and publication.
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
Auditor executes the authorized GitHub build/publication for v1.2.7rc4, then owner validates bootstrap in a clean Windows 10 VM. T1 remains open until packaged runtime evidence exists.

## T2 rc4 release preparation
Owner explicitly authorized the v1.2.7rc4 GitHub prerelease using the configured gh session. This local work unit changes package version and adds an accurate Spanish release entry while retaining rc3 history. Strict version check observed RED: actual 1.2.7rc3 failed the 1.2.7rc4 assertion. Runtime logic is unchanged. No remote operations performed by the writer; Auditor owns exact-commit CI, tag/build/publication. Runtime harness here is offline launcher/version validation; frozen dependency and clean Windows 10 VM checks remain pending. Rollback boundary: only rc4 version/release-note preparation, preserving the native TLS feature and prior releases.

Local GREEN: `uv run python -c "from liveaudio import __version__; assert __version__ == '1.2.7rc4', __version__"` passed. `uv run python -m pytest -q tests/test_launcher.py tests/test_unified_first_run.py`: 97 passed in 3.25 seconds. `uv lock --check`, `uv run ruff check .`, `git diff --check`, and launcher `--self-test --headless` passed. Lockfile requires no new bytes because the project version is dynamic and dependencies are unchanged. Full suite not repeated for version-only preparation; prior feature suite evidence remains above. No remote or frozen VM success claimed.
