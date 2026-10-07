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
- [ ] T3: Prepare and publish stable v1.2.7 as Latest. Owner authorized the stable release after reporting rc4 success on a normal Windows host and VM. Delegated local metadata writer; Auditor owns GitHub build/publication through the configured gh session. Master merge is not authorized. No fresh unseeded VM snapshot or live negative-certificate verification is inferred from the report.
- [x] T2: Prepare 1.2.7rc4 version and release notes, then Auditor builds/publishes through GitHub. Route: delegated release writer for metadata/docs; source logic unchanged. Release-preparation commit `ee1b7c1069ead5f21b93bd05408a09fb49638a58`; GitHub Release run `37702101849` succeeded and prerelease published. Evidence below; frozen clean-VM verification is still pending under T1.
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
Auditor builds and publishes authorized stable v1.2.7 as Latest. Owner reported rc4 success on a Windows host and VM; fresh unseeded snapshot and live invalid-certificate checks remain unproven. Master integration requires separate authorization.

## T2 rc4 release preparation
Owner explicitly authorized the v1.2.7rc4 GitHub prerelease using the configured gh session. This local work unit changes package version and adds an accurate Spanish release entry while retaining rc3 history. Strict version check observed RED: actual 1.2.7rc3 failed the 1.2.7rc4 assertion. Runtime logic is unchanged. No remote operations performed by the writer; Auditor owns exact-commit CI, tag/build/publication. Runtime harness here is offline launcher/version validation; frozen dependency and clean Windows 10 VM checks remain pending. Rollback boundary: only rc4 version/release-note preparation, preserving the native TLS feature and prior releases.

Local GREEN: `uv run python -c "from liveaudio import __version__; assert __version__ == '1.2.7rc4', __version__"` passed. `uv run python -m pytest -q tests/test_launcher.py tests/test_unified_first_run.py`: 97 passed in 3.25 seconds. `uv lock --check`, `uv run ruff check .`, `git diff --check`, and launcher `--self-test --headless` passed. Lockfile requires no new bytes because the project version is dynamic and dependencies are unchanged. Full suite not repeated for version-only preparation; prior feature suite evidence remains above. No remote or frozen VM success claimed.

## T2 final delivery evidence
Auditor confirmed release-preparation commit `ee1b7c1069ead5f21b93bd05408a09fb49638a58`, pushed branch `codex/launcher-native-tls`, and tag `v1.2.7rc4` at that commit. GitHub Release run `37702101849` succeeded for Windows, Linux and source artifacts. Published release state: draft false, prerelease true, latest false, seven nonempty assets. Windows installer size: 35,880,609 bytes; SHA256: `29605ec4f7bba3c1a33302dd056954e64fc3b31248d6a6cd83d81642c2525e06`.

Workflow evidence confirms installation of truststore 0.10.4 and successful executable builds, but does not establish actual frozen backend execution or fresh-VM bootstrap success. T1 stays open. Master was deliberately not merged because integration was not authorized. Earlier local-only/pending publication wording above records historical checkpoints, superseded only for T2 delivery by this section. No further remote operations are part of this evidence update.

## T3 stable release preparation
Owner reported that the published rc4 installer works on the normal Windows host and in the VM, then explicitly authorized v1.2.7 stable publication as Latest. This is owner-reported runtime evidence, not independent inspection or proof of a fresh unseeded snapshot; live negative-certificate verification remains unproven. Preparation changes package version and stable release notes only, preserving rc history and native TLS logic. Version assertion observed RED with actual 1.2.7rc4 before the edit. Auditor owns all remote operations; no master merge authorized. Rollback boundary: stable version and release-note/task metadata only, preserving the rc4 implementation and artifacts.

Local GREEN: version assertion for `1.2.7` passed; `uv lock --check` passed with no dependency/lock bytes changed. `uv run python -m pytest -q tests/test_launcher.py tests/test_unified_first_run.py`: 97 passed in 3.24 seconds. `uv run ruff check .` and `git diff --check` passed. Full suite not repeated for version-only preparation. Prior feature verification and user-reported rc4 runtime success remain distinct evidence; stable workflow build/publication is still pending.
