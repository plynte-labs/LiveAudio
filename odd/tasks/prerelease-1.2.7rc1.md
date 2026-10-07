# LiveAudio v1.2.7rc1 delivery

## Authorized outcome
Owner requested PR, merge after checks, and prerelease with installers using configured GitHub authentication. Single PR size exception explicitly approved. Approved issue: plynte-labs/LiveAudio#19. Historical audio/transcript files and unrelated untracked work are excluded.

## Tasks
- [x] R1: Prepare version 1.2.7rc1 and synchronize release metadata/lockfile with minimal checks.
- [ ] R2: Publish release branch and PR to master, link issue19, record size rationale and wait for CI; merge only passing checks, never admin bypass.
- [ ] R2a: Correct reproduced prerelease CI blockers with deterministic lock RED/GREEN, bounded full-suite verification, and independent auditor verification before commit/push.
- [ ] R2b: Isolate fake capture from missing host hardware, reproduce headless standby deterministically, verify integration/standby and full suite, then require fresh CI before delivery.
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

### R2b headless CI root-cause correction
R2a corrections were committed as `4bd2a8e` by the parent and tested in CI run `37572314328`. Both platforms reached the 20-minute job timeout. Faulthandler showed the same fake capture integration stalled in `audio_producer` hardware standby (`audio.py:518`), called by `_produce_fake_capture` in `tests/test_vad_asr_integration.py`; VAD worker waited for an empty ring. The fake InputStream was never entered because host input enumeration returned zero. Local microphones masked missing test isolation. These stacks identify the repeat CI stall mechanism; the original five-hour run did not retain an equivalent named stack.

Route: delegated direct bounded test-harness writer. Authorized scope: fake capture fixture and regression only, plus this recovery record. Product hardware standby, separate zero-device tests, dependencies, and CI workflow remain unchanged. Strict TDD source and runner remain as above.

RED: new regression fakes real host enumeration to zero (WinMM on Windows and sounddevice), preloads audio before its two-second cancellation deadline, and expects a completed 14-frame phrase. Producer logged missing-microphone standby, returned no phrase after cancellation, and assertion failed (1 failed in 4.82s). Initial RED deadline began before audio import and lacked standby evidence; corrected RED establishes the intended failure without an unbounded hang.

GREEN: `_produce_fake_capture` explicitly fakes input presence as one alongside fake capture/model. Integration plus independent production standby tests: 12 passed in 5.14s. First bounded full suite completed without a hang: 828 passed, 75 subtests, one unrelated public-registry portability failure in 63.31s. During that run an external refresh modified the tracked local registry to contain machine-specific paths; the writer did not invoke refresh. Parent authorized preserving refreshed entries while normalizing local paths to portable home/project-relative form. Readiness RED: 1 expected failure in 0.22s; GREEN: 5 passed in 0.05s. Historical intentional-local registry constraint recovered and parent reaffirmed: `.atl/skill-registry.md` MUST be excluded from this release commit, with refreshed entries preserved; committed registry already portable.

Final bounded offline Windows full suite against current portable local registry: 829 passed, 75 subtests passed in 59.80s, exit 0; stderr empty and external 120-second watchdog unused. Ruff and diff whitespace checks passed. No product code changed. Source/test authored diff: 28 additions plus 2 deletions in the integration test; task record updated separately. Parent independent verification, commit/push, and fresh Windows/Linux CI remain required; no worker commits, staging, remote operations, skips, or new dependencies. POSIX runtime still requires CI proof.

Issue19 created and approved by explicit owner instruction. R1 complete; R2/R3 pending. Mirror initial synchronization pending. Next: parent-controlled PR and release orchestration.

R1: Updated the authoritative dynamic version source `liveaudio/__init__.py` from `1.2.6` to `1.2.7rc1`; `pyproject.toml` loads version from that file, and `.github/workflows/release.yml` derives its release version and enforces `v<tag> == __version__`. The only other tracked `1.2.6` occurrence is historical `CHANGELOG.md`, left unchanged. `uv.lock` has no editable-project version field; `uv lock --offline --no-upgrade` resolved 78 packages successfully and left the lock unchanged. Focused checks: `tests/test_launcher.py tests/test_dependency_pinning.py -k 'version or dev_mode_reads_version'` — 13 passed, 77 deselected; `tests/test_launcher.py::TestReleaseMeta` — 4 passed. An initial mistyped test-class selector exited 4 before running tests; corrected selector passed. No dependency upgrades, release-workflow edits, commits, or staging. R2/R3 remain pending.
