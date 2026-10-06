# LiveAudio v1.2.7rc1 delivery

## Authorized outcome
Owner requested PR, merge after checks, and prerelease with installers using configured GitHub authentication. Single PR size exception explicitly approved. Approved issue: plynte-labs/LiveAudio#19. Historical audio/transcript files and unrelated untracked work are excluded.

## Tasks
- [x] R1: Prepare version 1.2.7rc1 and synchronize release metadata/lockfile with minimal checks.
- [ ] R2: Publish release branch and PR to master, link issue19, record size rationale and wait for CI; merge only passing checks, never admin bypass.
- [ ] R3: Tag the merged commit v1.2.7rc1, wait for source/launcher release build, publish resulting draft as prerelease (not latest).

## Route and constraints
R1 delegated direct: dependency/version preparation requires analysis and multiple generated metadata files. R2/R3 parent remote orchestration, worker read-only monitoring as needed. Strict TDD enabled for behavior; release metadata update is mechanical, verified by version/tag consistency and existing packaging tests. Exact runner: .venv/Scripts/python.exe -m pytest. No new product behavior. Version change is not a full-suite pass.

Source baseline1d62338: dual judge approved corroborated service corrections after2rounds, independent15Windows tests pass; full suites partial locally, POSIX runtime unverified. Remote CI on Windows/Linux is required. Deferred stdout/prewarm GUI/docs warnings remain documented.

## Delivery
Branch chore/release-1.2.7rc1 inherits reviewed feature/configurable-transcription-buffers. Single-PR exception rationale: owner requests consolidated prerelease integration of already-reviewed history; no code removal/minification to shrink diff. No force push, no CI bypass. One attempt per create; ambiguous remote outcome stops mutation until exact identity resolved. Attach every created PR to chat.

## Progress
Issue19 created and approved by explicit owner instruction. R1 complete; R2/R3 pending. Mirror initial synchronization pending. Next: parent-controlled PR and release orchestration.

R1: Updated the authoritative dynamic version source `liveaudio/__init__.py` from `1.2.6` to `1.2.7rc1`; `pyproject.toml` loads version from that file, and `.github/workflows/release.yml` derives its release version and enforces `v<tag> == __version__`. The only other tracked `1.2.6` occurrence is historical `CHANGELOG.md`, left unchanged. `uv.lock` has no editable-project version field; `uv lock --offline --no-upgrade` resolved 78 packages successfully and left the lock unchanged. Focused checks: `tests/test_launcher.py tests/test_dependency_pinning.py -k 'version or dev_mode_reads_version'` — 13 passed, 77 deselected; `tests/test_launcher.py::TestReleaseMeta` — 4 passed. An initial mistyped test-class selector exited 4 before running tests; corrected selector passed. No dependency upgrades, release-workflow edits, commits, or staging. R2/R3 remain pending.
