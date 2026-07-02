# Design: Installer Update Notification v1.2.1

## Technical Approach

Keep `liveaudio/utils/updater.py` as the network/version/launcher boundary and fix the UI lifecycle in `liveaudio/app.py`. Store detected update state on `LiveASRApp` instead of treating the banner widget as the state. Render the update prompt only when `screen_main` exists and re-render after main-screen rebuilds.

## Architecture Decisions

### Decision: Pending tag is app state, not widget state


**Choice**: Add app-level fields such as `_pending_update_tag` and `_dismissed_update_tag`.
**Alternatives considered**: Keep `frame_update_banner` as the only guard.
**Rationale**: `screen_main` is destroyed by `go_to_main()` and `_rebuild_ui()`, so widget existence cannot represent update availability.

### Decision: Prompt after main screen readiness


**Choice**: Let `check_updates()` save the tag, then call a helper that renders only when `screen_main` is mapped/ready.
**Alternatives considered**: Move initial update check later only.
**Rationale**: Deferring initial check helps one path, but storing state also survives language/layout rebuilds.

### Decision: User consent before launcher hand-off


**Choice**: Add a small prompt or confirmation path with Update now and Later; `Update now` reuses `start_in_app_update()`.
**Alternatives considered**: Auto-start launcher when an update is found.
**Rationale**: Updating closes the app and can interrupt active transcription, so explicit consent is required.

### Decision: Conservative freshness improvement


**Choice**: Check on the first app open of each day, then rate-limit successful release lookups to every 6 hours while internet is available.
**Alternatives considered**: Check on every startup with no cache, or keep the current 24-hour interval.
**Rationale**: Users should see published updates sooner, but startup should not spam GitHub, block offline users, or interrupt transcription.

## Data Flow

```text
LiveASRApp.__init__ / periodic timer
  └─ after(..., check_updates)
       └─ updater checks online/rate-limit eligibility
       └─ updater.check_for_updates_async(callback)
            └─ callback(available, tag)
                 └─ _pending_update_tag = tag
                    └─ _maybe_show_update_notice()
                         ├─ if main screen ready: prompt/banner
                         └─ else: wait for go_to_main/rebuild

User chooses Update now
  └─ start_in_app_update(tag)
       ├─ _confirm_close()
       ├─ updater.start_update(tag)
       └─ _shutdown()
```

## File Changes

| File | Action | Description |
|------|--------|-------------|
| `liveaudio/app.py` | Modify | Add pending/dismissed update state, safe render helper, prompt flow, and re-render after `go_to_main()` / `_rebuild_ui()`. |
| `liveaudio/utils/updater.py` | Modify | Use a 6-hour successful-check interval, support first-open-of-day eligibility, and keep offline checks non-interrupting. |
| `tests/test_updater.py` | Modify | Cover cadence if interval changes. |
| `tests/test_app_update_notification.py` or existing app UI tests | Create/Modify | Cover pending update state surviving main-screen rebuild and postponed prompt behavior. |
| `liveaudio/i18n.py` or locale resource file | Modify | Add prompt labels if no reusable strings exist. |

## Interfaces / Contracts

No external API changes. Internal app state contract:

```python
_pending_update_tag: str | None
_dismissed_update_tag: str | None
```

`_maybe_show_update_notice()` should be idempotent: calling it repeatedly MUST NOT duplicate banners/prompts for the same tag.

## Testing Strategy

| Layer | What to Test | Approach |
|-------|-------------|----------|
| Unit | Version/cadence/online behavior | Existing updater tests with mocked time/config/network: first open of day checks, <6h skips, offline does not interrupt. |
| UI lifecycle | Update found before main screen, then `go_to_main()` | Instantiate app with mocked updater callback or direct pending tag; assert notice is shown after rebuild. |
| UI behavior | Later does not launch update repeatedly | Mock `start_update`/close confirmation and assert no hand-off on postpone. |
| Regression | Existing launcher update hand-off | Keep `tests/test_launcher.py` coverage unchanged. |

## Migration / Rollout

No migration required. Existing `last_update_check` remains valid. If the interval is reduced, users may check sooner after the next eligible successful online check.

## Open Questions

- [ ] Should postponement last only for the session or persist per tag in config?
