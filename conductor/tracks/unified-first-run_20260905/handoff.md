# Unified first-run — interrupted implementation handoff

## Status

This track is a **partial checkpoint after the implementation interruption**. The bounded launcher percentage fix and collective checkpoint review are recorded, but final integration proof is incomplete. The Product Owner authorized this checkpoint commit; resolve its actual revision from Git. No push, merge, VM rebuild, manual E2E result, or first-use M1–M8 result is claimed here.

- Branch: `feature/firstuse-startup-progress`
- Implementation base: `e004ae3bc5047ecb2f53ba6c9e5c6c37476ad87a` (not the post-checkpoint HEAD).
- Native review: unavailable under the filesystem-unknown/remote validation failure. The PO approved the track-scoped unmanaged exception (Engram #6471); this is **not** a native PASS or review receipt.
- Checkpoint review: Sol Architecture PASS-with-notes; Luna QA PASS-with-notes; Luna Research PASS-with-notes (documentation author disclosed); Terra Performance safe partial-checkpoint self-review (fix author, not independent PASS). Final feature review remains open.
- First-use dependency: implemented/reviewed in the working tree, but M1–M8 remain pending manual evidence.

## Written artifacts and wiring observed

The following is a source readback, not proof of end-to-end behavior:

- `liveaudio/core/first_run.py` is new. It defines the six-field handoff schema, strict root/version/attempt/phase validation, and the bilingual phase vocabulary.
- `packaging/launcher.py` writes `handoff.json` through a same-directory temporary file, flush/fsync, and replace; it validates the cache root and records only phases `[0, 1, 2, 3]` before `Popen`. The fixed bootstrap/window percentage updates were removed in the checkpoint; supported-runtime and packaged-runtime verification remain pending.
- `liveaudio/app.py:366-373` marks phase 4 after the first UI paint, but no verified renderer path proves phases 5–7 remain continuously visible. The handoff path has not been proven in a clean packaged run.
- `packaging/launcher.py`, `liveaudio/utils/config.py`, and `liveaudio/app.py` contain the allowlisted `es|en` persistence primitives, but `packaging/launcher.py:329-331` does not pass the selected language; the bilingual launcher preference is not fully wired.
- `liveaudio/service/supervisor.py:381-388` handles only the ASR key in the relevant ingestion path; VAD service ingestion/retry remains incomplete.
- Handoff expected-root derivation currently uses the same `install_location.json` as the app, and freshness is only a caller attempt floor; the stronger executable-root/freshness design remains pending.
- The dead-producer/join/same-queue/ASR-identity safety boundary has source checks but no runtime proof. The existing bounded Whisper TLS bypass at `liveaudio/core/engine.py:463-485,542-567` remains an accepted prior first-use risk and was not fixed here.
- `liveaudio/core/audio.py` contains an indeterminate VAD heartbeat and removes the insecure TLS fallback detected by the new test. The pre-ready retry path in `app.py` checks that the producer is dead and joined before replacement, but live producer behavior, queue ownership, and full-stop fallback remain unproven.
- `tests/test_unified_first_run.py` is new. The persisted report records 11 tests for schema, phase copy, VAD error classification/heartbeat, and TLS fallback absence.
- `validation.md` is a writer-produced checkpoint report. Latest pytest/Ruff commands used Python 3.13; Python 3.11 compileall was the only supported-runtime check, so this is not full supported-runtime proof.

## Exact candidate delta against the ignored baseline

The baseline manifest contains 19 files. All 19 baseline copies still match their manifest hashes. The comparison below is the state **at interruption before these stabilization-document edits**; the handoff, metadata, and status-only documentation edits are intentionally excluded from the source delta.

`packaging/launcher.py`, `liveaudio/app.py`, `liveaudio/core/audio.py`, `liveaudio/utils/config.py`, `liveaudio/utils/i18n.py`, `README.md`, `HISTORIAL_CAMBIOS.md`, `docs/GETTING_STARTED.md`, `docs/PACKAGING_AND_UPDATES.md`, `conductor/tracks.md`, and this track's `index.md`.

Baseline files unchanged include `liveaudio/core/engine.py`, `liveaudio/core/workers.py`, the first-use/VAD/launcher regression tests, and this track's original `spec.md`, `plan.md`, and `tasks.md`. New relevant files outside that baseline are `liveaudio/core/first_run.py`, `tests/test_unified_first_run.py`, and `validation.md` (the VM/baseline artifacts remain ignored).

## Persisted validation claims — not independently rerun

`validation.md` records the following historical writer output:

- `tests/test_unified_first_run.py`: `11 passed`.
- Focused regression (`test_unified_first_run.py`, `test_launcher.py`, `test_vad_error.py`): `93 passed`, one warning.
- First-use regression: `30 passed`, one warning.
- Ruff on the focused source/tests: all checks passed.
- `python -m compileall liveaudio packaging/launcher.py tests`: completed successfully.

The report also records an ignored `.pytest_cache` permission warning. These commands were not rerun for this handoff, so they do not establish the final state of the current bytes.

## Confirmed gaps before resumption

1. Verify the launcher percentage fix in the supported runtime so only byte/tqdm evidence reaches a determinate progress control; `uv sync`, VAD, post-download loading, and the launcher-to-app boundary must remain indeterminate and truthful.
2. Finish the TDD audit of launcher handoff creation, stale/malformed/path validation, early app paint, language precedence/persistence, and the window-timeout/process-exit paths.
3. Verify VAD error classification (`network`, `tls`, `cache-corrupt`), heartbeat stop/finally behavior, retry attempt semantics, and the safe pre-ready producer replacement boundary without restarting ASR or reusing a queue while a writer may still be alive.
4. Confirm `--reinstall` retains `hf-cache` and add/verify regression coverage; do not add a purge path.
5. Reconcile the planning checkboxes only from completed evidence. Do not mark automated tasks, E2E-1–E2E-10, or M1–M8 complete from source presence or the historical report.

## Required continuation order

1. Resume TDD with RED → GREEN → REFACTOR and complete the implementation gaps listed in the checkpoint review before relying on further test/VM evidence.
2. Run the approved `compileall` command and the exact relevant test set; replace historical claims with current output.
3. Obtain the final feature review after the implementation gaps are resolved; do not represent the checkpoint review or PO exception as a native PASS.
4. Rebuild a new VM v2 candidate in a separate location, preserving the existing ignored candidate until source snapshot, embedded uv, package hashes, and self-test are verified.
5. Execute manual E2E-1…E2E-10 and first-use M1–M8, recording evidence for each row before any closure decision.
6. Ask the PO before any stage/commit/push/merge/revert.

## Safety and preservation

Preserve the three unrelated dirty files, `.claude/`, all existing first-use changes, the ignored VM artifacts, and these protected files: `.agents/skills/liveaudio-product-strategy-chatgpt/SKILL.md`, `.atl/skill-registry.md`, and `opencode.json`. No process was killed: a process listing showed Python processes, but command-line attribution was unavailable even after the elevated query, so runtime cleanliness must not be asserted.
