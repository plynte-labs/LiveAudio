# Model download status

## Final auditor verification
Post-compatibility independent QA: 852 passed and 75 subtests in 57.66s (offline Windows, 120-second watchdog unused); Ruff and whitespace checks passed. Auditor spot check: tests/test_model_preparation.py, 23 passed in 2.80s. Research final PASS-with-notes, architecture PASS-with-notes, QA PASS; writer performance-role PASS (not independent). Actual Linux, older-Hub installation and installed first-use/download/GPU smoke remain pending. Byte counters are live events, not persisted health recovery. No source commits or remote delivery yet. Cohesive candidate exceeds 400 authored lines (writer count 548 before this final recovery note); request owner size exception before committing as one work unit. Preserve all tests/docs and unrelated local work.

## Objective and approved design
Replace the first-use ASR loading black box with truthful cache checking, downloading with observed byte availability when available, device loading, and ready/error states. Never invent percentages or completion. Explain that audio may be discarded while the model is being prepared.

## Problem and evidence
Installed faster-whisper suppresses its download tqdm output. Terminal interception therefore cannot expose transfer progress. Hugging Face aggregate totals can expand; displayed percentages are not a stable overall denominator. Owner approved the bounded design after a 324-second first turbo preparation and audio queue saturation.

## Scope and constraints
Existing ASR provisioning, log/status routing, focused tests and user documentation only. Preserve model aliases, local paths, cache fast path, CUDA/CPU fallback, hot-swap attempt isolation, WS v1 and OBS backlog behavior. No tokens, URLs, private paths or transcripts in progress logs. No dependency upgrades, automatic downloads in tests, pause-capture redesign or remote delivery. Preserve unrelated local registry/track/ADR changes.

## Work unit
- [ ] M1: Implement and verify cache/download/device-load states, bounded truthful progress and preparation warning; include offline failure/cache/hot-swap regressions, independent reviews and user docs; close with one auditor-approved Conventional Commit.

Route: delegated direct; preparation and multi-file writer triggers. Research mapping completed by model_progress_map; one writer owns implementation. Four review areas: research compatibility, performance/resilience, architecture/privacy, QA.

## Acceptance and checks
Given a missing or partial model cache, preparation remains visible without fake percentages; observed download byte availability is reported at bounded intervals (including resumed cache bytes). Cache hits avoid unnecessary network work. Loading failure is not mislabeled as cache miss; CPU fallback remains intact. Ready is emitted only after construction. Non-download states clear stale percentages. Progress never enters subtitle queues. Errors and retries retain existing recovery semantics.

Strict TDD: enabled by AGENTS.md. Runner: `.venv/Scripts/python.exe -m pytest`. Observe RED before implementation, GREEN then refactor. Focused startup/engine/provisioning/supervisor checks, Ruff, compile and one bounded offline full suite. Manual installed first-use download/GPU test remains owner-run; fake dependency/queue harness validates automated runtime boundary.

## Delivery and recovery
Owner explicitly approved retaining this cohesive change as one work unit with a size exception. Delivery strategy: exception-ok. This approval covers local commit only, not push, PR, merge or prerelease. Final post-correction independent verification and role sign-offs are recorded above; no remaining automated blocker. Installed GPU/download smoke and Linux remain pending.
Branch: codex/model-download-status; base cb22514a080515351eb4fd35fefe335ce9d05f6a. Delivery strategy ask-on-risk; forecast about 250–400 authored lines, advisory not a code-minification target. No push/PR/release authorized for this new feature. RDD off (global). Rollback: revert this work unit's progress/provisioning and related tests/docs without unrelated capture/export changes.

## Progress
Implementation is uncommitted and unstaged, pending the Auditor's four independent reviews and commit approval. M1 remains unchecked until those gates and the commit are complete. Engram mirror: odd/model-download-status/tasks.

Implemented: cache-only snapshot resolution before device construction; partial-cache detection (model/config/tokenizer); public snapshot progress callback, byte availability without growing-total percentages; bounded heartbeat and byte events with idempotent reporter cleanup; device loading and CPU fallback without redownload; attempt fencing and stale percentage clearing; concise user docs. No capture, subtitle queue, OBS, dependency, remote or unrelated-file changes.

Truthfulness: byte availability may include resumed cached data; it is not claimed as newly transferred bytes. Future unknown aliases use the public faster-whisper resolver with indeterminate download status. Heartbeats show waiting, not proven network progress. Known aliases retain their current repository mapping without private package internals.

## Verification evidence
- RED: new offline preparation regressions: 5 failed before implementation (missing resolver and retained stale percentage).
- Additional RED: reporter close/final-byte regression, same-state supervisor byte forwarding, missing-tokenizer partial-cache, and four distil repository-mapping regressions each failed before their bounded fixes.
- GREEN/refactor: focused preparation/startup/engine/unified-first-run/idempotent-service/resilience-service/license suite: 175 passed in 24.28s (alias-correction candidate); compatibility-correction candidate: 177 passed in 21.46s.
- Ruff: changed Python files, all checks passed.
- Compile: `python -m compileall -q liveaudio`, exit 0.
- Diff whitespace: `git diff --check`, exit 0; only the pre-existing unrelated registry CRLF warning.
- Initial offline full suite (external 120s deadline): 841 passed, 75 subtests passed, 1 failed in 55.98s. Failure: new test file lacked the required MIT SPDX header. Header corrected; focused license check passes. Auditor-authorized bounded rerun passed: 842 passed, 75 subtests passed in 56.03s (57.91s external elapsed). Subsequent alias self-readback added eight mapping cases, exposed and corrected four distil repository names; final focused checks pass, QA independently verified the alias-correction candidate: 850 passed, 75 subtests passed in 60.42s. That full-suite result predates the subsequent compatibility correction described below.
- Architecture/privacy review: PASS with nonblocking notes retained by the Auditor. Research compatibility review initially FAIL/P2: importing `LocalEntryNotFoundError` from `huggingface_hub.errors` breaks permitted older Hub versions where that module is absent.
- Bounded compatibility correction: use the public `huggingface_hub.utils` export in production/tests, without upgrading dependencies. RED: simulated absent errors module produced ModuleNotFoundError (1 failed, 22 passed); GREEN: final focused compatibility suite 177 passed in 21.46s; Ruff, compile and diff checks passed. A file-count-only progress bar remains indeterminate with no bytes or percentage claimed. Actual legacy package installation was not performed; the regression simulates its missing-module layout.
- Runtime boundary: fake Hub/tqdm, queue and constructor harness proves byte reporting, growing totals, cached/future alias/local paths, failure classification and CPU reuse; no real downloads or GPU load performed.
- Pending: independent final-candidate full-suite verification, four independent reviews, owner-installed first-use/download/GPU smoke test, Auditor-approved Conventional Commit. No source-control delivery claim yet. Current authored scope: 548 additions plus deletions, including tests/docs/task and excluding unrelated registry/track/ADR; one cohesive work unit, not code-minified to meet the advisory forecast.

