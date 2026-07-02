# Tasks: Installer Update Notification v1.2.1

## Review Workload Forecast

| Field | Value |
|-------|-------|
| Estimated changed lines | 220-330 |
| 400-line budget risk | Low |
| 500-line user budget risk | Low |
| Chained PRs recommended | No |
| Suggested split | Single PR |
| Delivery strategy | ask-on-risk |
| Chain strategy | pending |

Decision needed before apply: No
Chained PRs recommended: No
Chain strategy: pending
400-line budget risk: Low

### Suggested Work Units

| Unit | Goal | Likely PR | Notes |
|------|------|-----------|-------|
| 1 | Reliable update detection and UI prompt | PR 1 | Keep updater, app UI, and tests together. |

## Phase 1: RED - Updater Cadence Tests

- [x] 1.1 Update `tests/test_updater.py` to expect first-open-of-day eligibility when online.
- [x] 1.2 Add coverage that a successful check less than 6 hours ago skips release lookup.
- [x] 1.3 Add coverage that offline/network failure does not call the UI callback and does not block startup.

## Phase 2: GREEN - Updater Cadence Logic

- [x] 2.1 Change `liveaudio/utils/updater.py` interval behavior from 24 hours to 6 hours.
- [x] 2.2 Add first-open-of-day eligibility using existing config timestamps without migration.
- [x] 2.3 Keep release lookup failures silent and non-blocking; persist timestamps only after successful release responses.

## Phase 3: RED - UI Lifecycle Tests

- [x] 3.1 Add or extend app UI tests for update found while the welcome screen is visible.
- [x] 3.2 Test that `go_to_main()` renders the pending update notice after rebuilding `screen_main`.
- [x] 3.3 Test that UI rebuilds do not duplicate notices for the same tag.
- [x] 3.4 Test that choosing Later does not start the launcher update flow for that prompt.

## Phase 4: GREEN - App Update State and Prompt

- [x] 4.1 Add `_pending_update_tag` and `_dismissed_update_tag` state in `liveaudio/app.py`.
- [x] 4.2 Refactor `check_updates()` to store confirmed newer tags and call an idempotent render helper.
- [x] 4.3 Refactor `display_update_alert()` or replacement helper so it only renders when `screen_main` is ready.
- [x] 4.4 Call the render helper after `go_to_main()` and `_rebuild_ui()` rebuild the main screen.
- [x] 4.5 Add a clear user choice: Update now reuses `start_in_app_update(tag)`, Later dismisses the tag for the session.

## Phase 5: Verification and Cleanup

- [x] 5.1 Run `python -m pytest -q tests/test_updater.py` and the new/updated UI tests.
- [x] 5.2 Run full validation: `python -m pytest -q` and compile validation.
- [x] 5.3 Review user-facing strings for English/Spanish consistency and avoid unnecessary comments.
