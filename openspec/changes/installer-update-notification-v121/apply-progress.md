# Apply Progress: Installer Update Notification v1.2.1

## Status

All planned tasks are complete.

## Completed Tasks

- [x] 1.1-1.3 Updater cadence tests: first open of day, six-hour skip, offline/network silent behavior.
- [x] 2.1-2.3 Updater cadence logic: 6-hour interval, same-day rate limit, new-day eligibility, no timestamp persistence on failed release lookup.
- [x] 3.1-3.4 UI lifecycle tests: pending tag capture, hidden main screen guard, mapped main render, duplicate guard, Later dismissal, rebuild hook coverage.
- [x] 4.1-4.5 App update state and prompt: pending/dismissed tag state, idempotent render helper, rebuild re-render hooks, Update now/Later choices.
- [x] 5.1-5.3 Targeted and full validation, package compile validation, and string review.

## Files Changed

| File | Action | What Was Done |
|------|--------|---------------|
| `liveaudio/utils/updater.py` | Modified | Changed update cadence to first daily open plus 6-hour same-day rate limit; added local-day helper. |
| `liveaudio/app.py` | Modified | Added pending/dismissed update state, main-screen readiness guard, idempotent update notice rendering, rebuild hooks, and Later dismissal. |
| `liveaudio/utils/i18n.py` | Modified | Added English/Spanish `update_later` strings. |
| `tests/test_updater.py` | Modified | Added updater cadence and app update-notification lifecycle tests. |
| `tests/test_main.py` | Modified | Added coverage that the welcome screen uses the current app version. |
| `liveaudio/__init__.py` | Modified | Bumped app version to `1.2.2` for the next release. |
| `CHANGELOG.md` | Modified | Added v1.2.2 release notes. |
| `openspec/changes/installer-update-notification-v121/tasks.md` | Modified | Marked completed tasks. |
| `openspec/config.yaml` | Modified | Corrected stale compile validation command to the current `liveaudio` package path. |

## TDD Cycle Evidence

| Task Group | RED | GREEN | REFACTOR |
|------------|-----|-------|----------|
| Updater cadence | Added failing tests for six-hour same-day skip, first open of day, elapsed interval, and failed-network persistence behavior. | Implemented `CHECK_INTERVAL_SECONDS = 21600` and `_same_local_day()` eligibility. | Kept config migration-free and preserved existing callback/network semantics. |
| UI lifecycle | Added tests for pending tag capture, hidden/mapped main screen behavior, duplicate prevention, Later dismissal, and rebuild hook presence. | Added `_pending_update_tag`, `_dismissed_update_tag`, `_maybe_show_update_notice()`, and rebuild calls. | Kept launcher hand-off unchanged and confined UI state to `LiveASRApp`. |
| User prompt strings | Existing banner tests covered update hand-off; new Later behavior test covered dismissal. | Added `update_later` localized strings and Later button. | Avoided extra comments and kept existing banner style. |

## Validation

| Command | Result |
|---------|--------|
| `python -m pytest -q tests/test_updater.py` | PASS — 33 passed |
| `python -m pytest -q tests/test_main.py tests/test_updater.py` | PASS — 46 passed |
| `python -m pytest -q` | PASS — 547 passed |
| `python -m pytest -q` after v1.2.2 bump | PASS — 548 passed |
| `python -m compileall main.py core utils` | FAILED — stale configured paths did not exist at repo root (`main.py`, `core`, `utils`) |
| `python -m compileall liveaudio` | PASS — config updated to this command |

## Deviations from Design

- The prompt is implemented as the existing update banner plus explicit `Update now` and `Later` actions, not a modal dialog. This preserves the current non-blocking UX and satisfies user consent.
- The configured compile command in `openspec/config.yaml` was stale for the current package layout; it was corrected to `python -m compileall liveaudio`.

## Issues Found

- The stale OpenSpec compile command was found during validation and corrected.

## Workload / PR Boundary

- Mode: single PR
- Current work unit: reliable update detection and UI prompt
- Boundary: updater cadence + app UI lifecycle + tests + SDD progress
- Estimated review budget impact: implementation diff is about 231 changed code/test lines, below the 500-line user warning budget; OpenSpec planning artifacts are additional review context.
