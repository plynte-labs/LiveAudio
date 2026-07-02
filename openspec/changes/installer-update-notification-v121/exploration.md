# Exploration: Installer update notification v1.2.1

## Current State

LiveAudio checks GitHub releases through `liveaudio/utils/updater.py`. The client uses `APP_VERSION`, fetches `https://api.github.com/repos/plynte-labs/LiveAudio/releases/latest`, compares semantic tags, and rate-limits successful checks with `last_update_check` every 24 hours.

The app schedules `check_updates()` from `LiveASRApp.__init__()` after one second while the welcome screen is visible. If a newer tag is found, `display_update_alert()` renders a passive banner inside `self.screen_main`. However, `go_to_main()` later destroys and rebuilds `screen_main`, which can delete the banner and lose the visible notification. There is no modal or explicit update prompt.

## Affected Areas

- `liveaudio/app.py` — schedules update checks, renders the update banner, starts launcher hand-off, and rebuilds `screen_main` after the welcome screen.
- `liveaudio/utils/updater.py` — owns GitHub latest lookup, 24-hour cadence, version comparison, and launcher hand-off.
- `tests/test_updater.py` — covers update cadence/network/launcher behavior but not UI alert lifecycle.
- `tests/test_launcher.py` — covers launcher update metadata/checksum flow but not app-level visibility of available releases.
- `.github/workflows/release.yml` — creates draft releases; `/releases/latest` only works after the release is published.

## Approaches

1. **Minimal lifecycle fix** — Store pending update state and render the notification only when the main screen exists.
   - Pros: Smallest reliable fix; addresses the likely root cause; easy to test.
   - Cons: Keeps passive banner UX; does not improve long-running re-checks much.
   - Effort: Low

2. **Lifecycle fix plus clearer cadence** — Minimal fix plus a shorter configurable interval or periodic background re-check while the app stays open.
   - Pros: Solves missed visual notification and stale long-running sessions; still contained.
   - Cons: More state and tests around cadence/dismissal.
   - Effort: Medium

3. **Full update UX** — Add a prompt/dialog with Update now, View release notes, and Later, plus persistent dismissal and retry/status copy.
   - Pros: Best user experience; makes updates hard to miss.
   - Cons: Larger UI/test surface; needs careful close/update flow handling.
   - Effort: Medium

## Recommendation

Use approach 2. Preserve detected update state independently from UI widgets, re-render it after `go_to_main()` and UI rebuilds, and add a modest re-check cadence such as 6 hours for long-running sessions. Keep the prompt simple: when a newer version is found, ask whether to update now or postpone.

## Risks

- A check that succeeds before the main screen exists can currently create a banner on a hidden frame and lose it when `go_to_main()` rebuilds the frame.
- Network failures are swallowed, so users may only see no update state rather than an actionable failure.
- A draft GitHub release is invisible to `/releases/latest` until published.
- Prompting too aggressively can annoy users if dismissal state is not respected.

## Ready for Proposal

Yes. The proposal should target a small update-notification reliability change with optional cadence reduction and an explicit user prompt.
