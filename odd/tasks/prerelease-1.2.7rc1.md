# LiveAudio v1.2.7rc1 delivery

## Authorized outcome
Owner requested PR, merge after checks, and prerelease with installers using configured GitHub authentication. Single PR size exception explicitly approved. Approved issue: plynte-labs/LiveAudio#19. Historical audio/transcript files and unrelated untracked work are excluded.

## Tasks
- [x] R1: Prepare version 1.2.7rc1 and synchronize release metadata/lockfile with minimal checks.
- [ ] R2: Publish release branch and PR to master, link issue19, record size rationale and wait for CI; merge only passing checks, never admin bypass.
- [ ] R2a: Correct reproduced prerelease CI blockers with deterministic lock RED/GREEN, bounded full-suite verification, and independent auditor verification before commit/push.
- [ ] R3: Tag the merged commit v1.2.7rc1, wait for source/launcher release build, publish resulting draft as prerelease (not latest).

## Route and constraints
R1 delegated direct: dependency/version preparation requires analysis and multiple generated metadata files. R2/R3 parent remote orchestration, worker read-only monitoring as needed. Strict TDD enabled for behavior; release metadata update is mechanical, verified by version/tag consistency and existing packaging tests. Exact runner: .venv/Scripts/python.exe -m pytest. No new product behavior. Version change is not a full-suite pass.

Source baseline1d62338: dual judge approved corroborated service corrections after2rounds, independent15Windows tests pass; full suites partial locally, POSIX runtime unverified. Remote CI on Windows/Linux is required. Deferred stdout/prewarm GUI/docs warnings remain documented.

## Delivery
Branch chore/release-1.2.7rc1 inherits reviewed feature/configurable-transcription-buffers. Single-PR exception rationale: owner requests consolidated prerelease integration of already-reviewed history; no code removal/minification to shrink diff. No force push, no CI bypass. One attempt per create; ambiguous remote outcome stops mutation until exact identity resolved. Attach every created PR to chat.

## Progress
Owner now authorized correcting all reproduced release blockers and completing delivery, still without CI bypass. Recovery checklist: update prerelease-version and additive JSONL contract tests; fix native lock acquisition before any file mutation with deterministic RED/GREEN; handle ignored generated build-artifact license scope without deleting unrelated scripts; add bounded verbose/faulthandler CI diagnostics so remote stalls are actionable. Fresh unrestricted offline reproduction: 822 passed, 4 failed, 75 subtests in 56.72s. Original five-hour CI stall remains unproven; run37545775118 cancelled on both platforms. Route delegated single writer, then independent verification. Existing dual-judge transaction exhausted; do not extend its correction budget or imply it approved new changes.

Delivery update: issue19 is status:approved. Version preparation committed as84c78a7 and pushed on chore/release-1.2.7rc1. PR20 opened and attached to chat: https://github.com/plynte-labs/LiveAudio/pull/20 (master base, type:feature, explicit size-exception rationale). CI run37545775118 completed dependency setup and lint on Windows/Linux, then both Tests steps stalled without a verdict and were cancelled after approximately five hours with owner authorization. No merge, tag, release or installers published. Original remote stall cause remains undiagnosed; local failures do not prove its cause. Next: verify authorized corrections, then parent-approved commit/push and fresh required CI; no bypass.

### R2a recovery evidence
Route: delegated direct single writer; multiple analyzed test/product/CI files trigger delegation. TDD source: AGENTS.md strict mode; runner `.venv/Scripts/python.exe -m pytest`. No commits or staging authorized to worker.

Implemented: prerelease-compatible version test; explicit additive JSONL field contract with fields excluded from outbound payload; native ownership before metadata mutation; generated `build_artifacts` excluded from SPDX scan without touching existing probe scripts; regression still detects missing SPDX in source. CI job bounded to 20 minutes; pytest verbose, short traceback, 60-second faulthandler. No tests skipped and no dependencies added.

RED: deterministic spawned owner paused after truncation; contender expected `service-already-running` but received `service-lock-unwritable`. Generated-artifact regression independently failed because scan included generated fixture. Both failed before corrections (2 failures in 0.97s). Prior full-suite run also proved the version and JSONL stale assertions.

GREEN: focused license/version/metadata/native-lock/health checks: 24 passed in 6.00s. Full unrestricted Windows suite with offline guards, 20-second faulthandler, disabled pytest cache, and 120-second external watchdog: 828 passed, 75 subtests passed in 52.89s, exit 0; stderr empty, watchdog unused. Ruff all checks passed; git diff whitespace check passed. Independent parent verification, commit identity, push, and fresh Windows/Linux CI remain pending. Original CI stall is not claimed fixed; POSIX behavior awaits remote CI. No source cleanup/refactor needed beyond removing the unsafe write.

Issue19 created and approved by explicit owner instruction. R1 complete; R2/R3 pending. Mirror initial synchronization pending. Next: parent-controlled PR and release orchestration.

R1: Updated the authoritative dynamic version source `liveaudio/__init__.py` from `1.2.6` to `1.2.7rc1`; `pyproject.toml` loads version from that file, and `.github/workflows/release.yml` derives its release version and enforces `v<tag> == __version__`. The only other tracked `1.2.6` occurrence is historical `CHANGELOG.md`, left unchanged. `uv.lock` has no editable-project version field; `uv lock --offline --no-upgrade` resolved 78 packages successfully and left the lock unchanged. Focused checks: `tests/test_launcher.py tests/test_dependency_pinning.py -k 'version or dev_mode_reads_version'` — 13 passed, 77 deselected; `tests/test_launcher.py::TestReleaseMeta` — 4 passed. An initial mistyped test-class selector exited 4 before running tests; corrected selector passed. No dependency upgrades, release-workflow edits, commits, or staging. R2/R3 remain pending.
