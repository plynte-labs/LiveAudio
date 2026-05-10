# Track: Subtitle Style System v2

## Overview
Evolve LiveAudio's subtitle system from a static HTML file to a modular, CSS custom properties-driven theming engine. Fix critical memory leak and backpressure issues first. Add 7 preset themes, inline preview panel, and a dedicated "Subtítulos" tab. Set GPL v3 license. Require OBS 30+ with runtime check.

## Functional Requirements

### REQ-1: Memory Leak Fix (P0)
- Replace `container.innerHTML = ''` with `container.removeChild(sub)` for predictable GC
- Nullify references after removal: `sub = null`
- Cancel all pending `setTimeout` references before creating new ones
- Use object pooling: maintain 3-5 pre-created DOM elements, recycle instead of create/destroy
- Verify no closure captures prevent GC

### REQ-2: Client-Side Backpressure (P0)
- Implement debounce timer in JS client: wait configurable ms between subtitle renders
- Implement max queue of 3 messages: if more arrive while one is displaying, discard oldest
- Add `ws.bufferedAmount` check on Python side: if > 64KB, pause subtitle production
- Subtitles must not flash or interrupt mid-animation

### REQ-3: CSS Custom Properties Theme Engine
- Replace 3 hardcoded CSS classes with CSS custom properties system
- Define tokens: `--sub-bg`, `--sub-color`, `--sub-font-size`, `--sub-radius`, `--sub-shadow`, `--sub-border`, `--sub-animation-duration`, `--sub-font-family`
- All styles reference variables, not hardcoded values
- Theme changes via WebSocket `{ "type": "theme", "tokens": {...} }` message
- Python validates theme tokens against schema before broadcast

### REQ-4: 7 Preset Themes
- `default` — White text, dark bg, shadow (current)
- `karaoke` — Yellow, word-by-word pop animation (current)
- `neon` — Cyan glow, uppercase (current)
- `minimal` — Clean, no bg, subtle fade
- `bold` — High contrast, thick text, fast animation
- `rgb` — Each word gets a different rainbow color
- `typewriter` — Words appear one-by-one with typewriter effect
- All presets must pass WCAG AA contrast (4.5:1 normal text, 3:1 large text)

### REQ-5: Inline Preview Panel
- Preview panel inside new "Subtítulos" tab
- Shows sample subtitle with current theme settings
- Updates in real-time when theme changes
- Includes sample text: long words, emojis, mixed content
- No need to open OBS to preview

### REQ-6: Dedicated "Subtítulos" Tab
- New tab in settings panel for subtitle configuration
- Contains: style picker, preview panel, backlog settings, blacklist
- Does not mix with ASR model/hardware settings
- Progressive disclosure: basic settings visible, advanced collapsible

### REQ-7: Backward Compatibility
- Existing `style=default|karaoke|neon` parameter continues working
- Old OBS browser source URLs work without modification
- `style` parameter maps to new theme system automatically
- No restart of OBS browser source required for theme changes

### REQ-8: GPL v3 License
- Add `LICENSE` file with GPL v3 text to repository root
- Add license header to all Python and JS files
- Update README with license badge and notice

### REQ-9: OBS 30+ Runtime Check
- On app startup, detect OBS version if possible (via `window.obsstudio` or user agent)
- If OBS < 30 detected, show warning: "OBS 30+ recomendado para mejor compatibilidad"
- Do not block functionality, only warn
- Document OBS 30+ requirement in README and docs

## Non-Functional Requirements
- All preset themes must pass WCAG AA contrast minimum
- Preview panel must update within 200ms of theme change
- Memory usage of browser source must not grow > 50MB over 4 hours
- Backpressure must prevent subtitle flash even at 1 subtitle/second
- GPL v3 license headers in all source files
- OBS 30+ runtime check completes in < 1 second

## Acceptance Criteria
- **AC-1**: Browser source memory usage stable after 4 hours (no growth > 50MB)
- **AC-2**: No subtitle flash when ASR produces faster than OBS renders
- **AC-3**: All 7 presets render correctly with distinct visual identity
- **AC-4**: WCAG AA contrast passes for all presets
- **AC-5**: Preview panel shows accurate representation of OBS output
- **AC-6**: Existing OBS browser source URLs work without modification after update
- **AC-7**: Theme changes apply without OBS restart
- **AC-8**: GPL v3 LICENSE file present, headers in all source files
- **AC-9**: OBS version warning shows if < 30 detected
- **AC-10**: All existing tests pass + new tests for style system

## Out of Scope
- Alert overlays (subscriber, donation, follower) — Phase 3
- Multi-speaker auto-detection — Phase 2
- Theme editor visual (sliders, color pickers) — Phase 2
- Theme export/import (.json files) — Phase 2
- Multiple browser sources with per-source styles — Phase 2
- RTL/emoji/long-text edge case handling — Phase 1 handles basic cases, full support Phase 3
- Community theme gallery — Phase 3
