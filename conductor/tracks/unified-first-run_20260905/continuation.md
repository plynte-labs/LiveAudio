# Continuation checkpoint

## Current state

The checkpoint contains the bounded launcher-progress fix, prior first-use/unified implementation artifacts, and the four-role checkpoint review. It is not closed: implementation gaps, supported-runtime test proof, VM v2, manual E2E-1..E2E-10, and M1-M8 remain pending. The current branch and commit must be reported from Git by the parent; this document intentionally embeds no guessed SHA.

## Evidence boundary

- Latest focused pytest/Ruff commands used available Python 3.13.
- Python 3.11 `.venv` ran compileall only because pytest is unavailable there.
- Therefore no full supported-runtime validation is claimed.
- Native review remains unavailable; PO exception #6471 is not a PASS.
- Read `handoff.md` as the authoritative runtime resumption report.
- Read `checkpoint-review.md`; it records checkpoint safety only, not final feature closure.

## Easy checkpoint work completed

- Removed fixed bootstrap/window percentages; determinate progress remains tied to real byte evidence.
- Updated first-use metadata from stale proposal status to implemented/reviewed with M1-M8 still pending.
- Marked only the T8 percentage-removal subtask complete; composite tasks stay unchecked.
- Reworded user-facing docs/changelog as a partial checkpoint rather than a fully accepted unified UX.
- Retained the sanitized documentation-only proposal tracks `i18n-completeness_20260905`, `longsession-memory_20260905`, and `vad-silence_20260905` for registry/link integrity; they are not implemented features and remain PO-pending proposals.

## Time-consuming continuation

1. Before tests/VM, finish implementation gaps: wire launcher-selected language through `packaging/launcher.py:329-331`; verify app phase 5-7 rendering; complete VAD supervisor ingestion/retry; settle executable-root handoff freshness; and add runtime proof for dead-producer join, same queue, and ASR identity. Revisit the accepted Whisper TLS risk separately.
2. Run supported Python 3.11 tests after installing only approved project dependencies, then compileall/Ruff and record exact commands.
3. Complete final architecture, performance, research, and QA review of the frozen candidate; the checkpoint review is not the final feature review.
4. Rebuild and verify VM v2 without replacing the existing ignored candidate.
5. Execute E2E-1..E2E-10 and M1-M8 with real evidence; CUDA E2E-2 may be marked not run only with a real capability reason.
6. Ask the PO before any delivery action beyond the explicitly authorized present checkpoint commit; do not push or merge.

## Next-agent prompt

Read Engram memories `#6478` (historical context), `#6471` (track-scoped review exception), and `#6480` (checkpoint commit authorization), then read `handoff.md` as authoritative. Report the actual current branch/HEAD from Git. Preserve these unstaged protected files: `.agents/skills/liveaudio-product-strategy-chatgpt/SKILL.md`, `.atl/skill-registry.md`, and `opencode.json`; also preserve `.claude/` and ignored VM/baseline artifacts. Do not infer supported-runtime PASS, manual PASS, native PASS, or completion from source presence. Continue with TDD, final four-role review, VM, and manual evidence before closure.

## Rollback boundary

Conceptually, this checkpoint is one non-destructive work unit: remove only the launcher-progress fix and its focused regression/doc references if that fix is rejected; keep first-use and unified handoff/VAD work intact. Do not use global restore, reset, clean, stash, or destructive rollback to implement that concept.