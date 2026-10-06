# Service Judgment Day corrections

## Authorized scope
Owner approved round-one correction of two HIGH findings confirmed independently by both judges before pushing feature/configurable-transcription-buffers to origin using configured Git authentication. No PR or merge. Base review: cdf0c853cac85bfc9d0f3799bc0fb6fe3a5e6a17 to 74d503635e67a6233e3007de8a1a532aac68055c.

## Frozen ledger and tasks
- [x] JD1: Explicit CLI `--lazy`/`--no-prewarm` and `--prewarm` override normalized persisted configuration; absent an explicit override, normalized configuration/default behavior is retained.
- [x] JD2: Service ownership now uses a persistent lock file plus a non-blocking OS file lock; contenders cannot reclaim during fresh/stale races or incomplete PID publication, release never unlinks another owner's file, and process exit releases the native lock for stale metadata recovery.

Only these severe findings are authorized. Stdout JSON-lines contamination (severity disagreement), GUI prewarm semantics and documentation warnings are deferred INFO. Do not refactor ASR, capture, GUI or exports.

## Route and verification
Delegated direct single bounded fix actor; multiple non-trivial files and concurrency analysis trigger delegation. Strict TDD enabled by AGENTS.md. Runner: .venv/Scripts/python.exe -m pytest. Observe RED before fixes, GREEN and refactor. Use deterministic synthetic CLI/config integration and lock interleavings plus existing service tests. No microphone/model/network runs for verification. Report runtime harness or justified N/A, exact results and independent rollback boundaries. Work-unit commits require auditor approval; no automatic push from worker.

At most two correction rounds and two scoped dual re-judgments; round one now authorized. Re-judges inspect only frozen ledger plus immutable fix delta. No refuter or duplicate native review.

## Delivery
Forecast 150–250 authored lines advisory; default ask-on-risk. Preserve unrelated untracked items. Parent will verify, approve local correction commits and execute ordinary non-force push only after scoped review; never enable RDD. No release-wide verification claim: previous broad suites stalled on multiprocessing/temp restrictions.

## Progress
Strict TDD RED observed for both corrections. JD1: `test_explicit_cli_prewarm_flags_override_normalized_config` failed under the prior constructor logic (`--prewarm` stayed false when normalized config was false); parser absent-default and constructor precedence tests also exposed the prior default/config collision. JD2: deterministic publication interleaving failed because the stale/incomplete lock was removed; fresh/stale multiprocessing races lost the persistent path on release; killed-owner recovery failed while PID metadata remained stale. GREEN: 14 focused prewarm/lock checks passed, including spawned-process fresh and stale contention, stalled PID publication, terminated owner recovery, and release successor protection. `ruff check liveaudio tests` and `git diff --check` passed. Required whole-file attempts `tests/test_resilience_service_backend.py -q` and `tests/test_idempotent_service_backend.py -q` were interrupted after 15 and 7 progress dots respectively because both stalled beyond 60 seconds; no full-file result is claimed. Diff currently 328 insertions/60 deletions (388 authored changed lines), above the initial 150–250 forecast; advisory only. No commit/staging. Runtime harness: offline `service.main` invoked with normalized config and patched lock/emitter plus a local tracking supervisor; native lock behavior exercised across spawned local processes. No Whisper/model, microphone, external network, or real service launch. Independent rollback boundary: none yet; all four files remain one uncommitted scoped delta. Next: auditor readback and approval; no push/PR from this worker.
