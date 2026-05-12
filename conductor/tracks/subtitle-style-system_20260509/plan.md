# Implementation Plan: Subtitle Style System v2

## Phase 1: Memory Leak Fix (REQ-1)
- [x] Task: Write tests — Red phase (tests fail)
    - [x] Test `container.innerHTML = ''` not used for clearing
    - [x] Test `removeChild()` used for predictable GC
    - [x] Test DOM references nullified after removal
    - [x] Test setTimeout references cleared before new ones
    - [x] Test GC-friendly pattern (removeChild + null)
    - [x] Test no closure capture leak
- [x] Task: Fix `subtitulos_obs.html` — Green phase
    - [x] Replace `innerHTML = ''` with `removeChild()`
    - [x] Nullify references after removal
    - [x] Cancel pending setTimeouts before creating new ones
    - [x] Remove object pooling (rejected per Architecture review)
- [x] Task: Verify all memory leak tests pass
- [x] Task: Conductor - User Manual Verification 'Memory Leak Fix' (Protocol in workflow.md)

## Phase 2: Client-Side Backpressure (REQ-2)
- [x] Task: Write tests — Red phase (tests fail)
    - [x] Test debounce timer prevents rapid re-renders
    - [x] Test max queue discards oldest when full
    - [x] Test no subtitle flash under rapid production
    - [x] Test WebSocket buffer size checked before sending
    - [x] Test pause production when buffer exceeds threshold
- [x] Task: Fix `subtitulos_obs.html` and `core/network.py` — Green phase
    - [x] Add debounce timer (150ms) in JS client
    - [x] Add max queue of 5 messages in JS
    - [x] Add `transport.get_write_buffer_size()` check in Python
    - [x] Add retry_buffer (max 10) instead of single message
    - [x] Add ping_interval=10, ping_timeout=5 for dead connection cleanup
    - [x] Add backpressure duration tracking
    - [x] Add backpressure check to replay buffer
- [x] Task: Verify all backpressure tests pass
- [x] Task: Conductor - User Manual Verification 'Client-Side Backpressure' (Protocol in workflow.md)

## Phase 3: CSS Custom Properties Theme Engine (REQ-3)
- [x] Task: Write tests — Red phase (tests fail)
    - [x] Test CSS custom properties applied to subtitle element
    - [x] Test theme change via WebSocket message updates styles
    - [x] Test invalid theme tokens rejected by schema
- [x] Task: Fix `subtitulos_obs.html` and `core/engine.py` — Green phase
    - [x] Define CSS custom properties in `:root`
    - [x] Refactor existing styles to use variables
    - [x] Add theme message handler in JS
    - [x] Add theme token validation schema in Python
- [x] Task: Verify all theme engine tests pass
- [x] Task: Conductor - User Manual Verification 'CSS Custom Properties Theme Engine' (Protocol in workflow.md)

## Phase 4: 7 Preset Themes (REQ-4)
- [x] Task: Write tests — Red phase (tests fail)
    - [x] Test all 7 presets render with distinct visual identity
    - [x] Test WCAG AA contrast passes for all presets
    - [x] Test preset selection applies correct CSS variables
- [x] Task: Implement presets in `subtitulos_obs.html` — Green phase
    - [x] Define default, karaoke, neon presets (existing styles as CSS vars)
    - [x] Define minimal preset (clean, no bg, subtle fade)
    - [x] Define bold preset (high contrast, thick text, fast animation)
    - [x] Define rgb preset (each word different rainbow color)
    - [x] Define typewriter preset (word-by-word typewriter effect)
    - [x] Add WCAG AA contrast validation for all presets
- [x] Task: Verify all preset tests pass
- [x] Task: Conductor - User Manual Verification '7 Preset Themes' (Protocol in workflow.md)

## Phase 5: Inline Preview Panel (REQ-5)
- [x] Task: Write tests — Red phase (tests fail)
    - [x] Test preview panel shows sample subtitle
    - [x] Test preview updates within 200ms of theme change
    - [x] Test preview includes long words, emojis, mixed content
- [x] Task: Implement preview in `main.py` UI — Green phase
    - [x] Add preview panel widget in new "Subtítulos" tab
    - [x] Wire preview to theme selection
    - [x] Add sample text with edge cases
- [x] Task: Verify all preview tests pass
- [x] Task: Conductor - User Manual Verification 'Inline Preview Panel' (Protocol in workflow.md)

## Phase 6: Dedicated "Subtítulos" Tab (REQ-6)
- [x] Task: Write tests — Red phase (tests fail)
    - [x] Test tab contains style picker, preview, backlog, blacklist
    - [x] Test advanced settings are collapsible
    - [x] Test tab does not mix with ASR/hardware settings
- [x] Task: Implement tab in `main.py` — Green phase
    - [x] Create new "Subtítulos" tab in settings panel
    - [x] Move style picker, preview, backlog, blacklist to tab
    - [x] Add progressive disclosure (basic/advanced)
- [x] Task: Verify all tab tests pass
- [x] Task: Conductor - User Manual Verification 'Dedicated Subtítulos Tab' (Protocol in workflow.md)

## Phase 7: Backward Compatibility (REQ-7)
- [x] Task: Write tests — Red phase (tests fail)
    - [x] Test `style=default|karaoke|neon` parameter still works
    - [x] Test old OBS browser source URL works without modification
    - [x] Test `style` maps to new theme system automatically
- [x] Task: Implement compatibility in `subtitulos_obs.html` — Green phase
    - [x] Add legacy `style` parameter handler
    - [x] Map old style names to new theme IDs
    - [x] Verify no OBS restart needed for theme changes
- [x] Task: Verify all compatibility tests pass
- [x] Task: Conductor - User Manual Verification 'Backward Compatibility' (Protocol in workflow.md)

## Phase 8: GPL v3 License (REQ-8)
- [x] Task: Add GPL v3 license files
    - [x] Create `LICENSE` file with GPL v3 text
    - [x] Add GPL v3 header to all Python files
    - [x] Add GPL v3 header to `subtitulos_obs.html`
    - [x] Update README with license notice
- [x] Task: Verify license files present
- [x] Task: Conductor - User Manual Verification 'GPL v3 License' (Protocol in workflow.md)

## Phase 9: OBS 30+ Runtime Check (REQ-9)
- [x] Task: Write tests — Red phase (tests fail)
    - [x] Test OBS version detection logic
    - [x] Test warning shows if OBS < 30
    - [x] Test check completes in < 1 second
- [x] Task: Implement check in `main.py` — Green phase
    - [x] Add OBS version detection on startup
    - [x] Show warning if OBS < 30 detected
    - [x] Document requirement in README and docs
- [x] Task: Verify all OBS check tests pass
- [x] Task: Conductor - User Manual Verification 'OBS 30+ Runtime Check' (Protocol in workflow.md)

## Phase 10: Collective Review & Sign-Off
- [x] Task: Run full test suite — all tests must pass
- [x] Task: Dream Team collective review — 5 agents summoned
    - [x] Architecture: CSS vars, backward compat, theme schema
    - [x] Performance: memory leak fix, backpressure, CEF overhead
    - [x] QA: preview accuracy, WCAG contrast, edge cases
    - [x] Research: OBS 30+ compatibility, GPL compliance, docs
    - [x] Product Strategy: preset differentiation, UX flow
- [x] Task: Resolve any issues found during review
- [x] Task: Conductor - User Manual Verification 'Collective Review & Sign-Off' (Protocol in workflow.md)

## Phase 11: Presentation & Merge
- [x] Task: Auditor presents results to user
- [x] Task: User approval for merge
- [x] Task: Merge to `master`, delete feature branch
- [x] Task: Update `conductor/tracks.md` to `[x] Completed`
- [x] Task: Conductor - User Manual Verification 'Presentation & Merge' (Protocol in workflow.md)
