# Proposal: Installer update notification v1.2.1

## Intent

Make available updates visible and actionable. A v1.2.0 installation can detect v1.2.1 while the welcome screen is active, render the banner into hidden `screen_main`, then lose it when `go_to_main()` destroys/rebuilds that frame.

## Scope

### In Scope
- Preserve pending update state independently from UI widgets.
- Render or re-render the update notice after the main screen exists and after UI rebuilds.
- Ask the user whether to update now or postpone when a newer release is found.
- Check on the first app open of each day, then at most every 6 hours while internet is available, with tests.

### Out of Scope
- Changing release publishing automation.
- Rewriting the launcher update/install mechanism.
- Auto-installing updates without user consent.

## Capabilities

### New Capabilities
- `desktop-update-notification`: Reliable desktop update discovery, notification, user choice, and launcher hand-off.

### Modified Capabilities
- None.

## Approach

Keep `utils/updater.py` as the release-check boundary. In `app.py`, store the latest available update tag on the app instance, defer visual notification until `screen_main` is mapped, and re-render after `go_to_main()`/UI rebuilds. Add a simple prompt path: Update now, Later, and optional release page access. Update checks should run on the first app open of each day, then at most every 6 hours when internet is available; offline users should not be interrupted.

## Affected Areas

| Area | Impact | Description |
|------|--------|-------------|
| `liveaudio/app.py` | Modified | Pending update state, render timing, prompt/update action |
| `liveaudio/utils/updater.py` | Modified | Check interval or periodic-check support if needed |
| `tests/` | Modified | UI lifecycle, updater cadence, and prompt behavior coverage |
| `liveaudio/i18n.py` or locale resources | Modified | User-facing prompt strings if needed |

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Repeated prompts annoy users | Medium | Track dismissal per detected tag/session |
| Update hand-off during active transcription | Medium | Reuse close confirmation before launcher hand-off |
| Network failures remain invisible | Low | Keep non-blocking behavior; only prompt on confirmed newer tag |
| Offline startup delays or noise | Low | Skip visible update handling when internet is unavailable; fail silently |

## Rollback Plan

Revert the change folder implementation and return to the current passive banner plus 24-hour rate-limited check.

## Dependencies

- GitHub release must be published, not only drafted.
- Existing launcher `--update <tag>` path remains the installer hand-off.

## Success Criteria

- [ ] v1.2.0 users see a v1.2.1 update notice after entering the main screen.
- [ ] The notice survives `screen_main` rebuilds and language/layout refreshes.
- [ ] Users can choose update now or postpone.
- [ ] Tests cover the previously lost-banner lifecycle.
- [ ] Update checks run on first daily open and no more than every 6 hours afterward when online.
