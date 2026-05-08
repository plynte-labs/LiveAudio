# LiveAudio Agent Guide

## Memory First

- Before planning or coding, recover Engram context for project `liveaudio` with `mem_context` first, then `mem_search` when needed.
- Treat recovered memories as constraints, especially previous branch decisions, OBS backlog behavior, specialized-agent workflow, and user preferences.
- Save meaningful decisions, discoveries, bug fixes, and completed-track summaries back to Engram with `mem_save` or `mem_session_summary`.
- Never save raw audio, raw transcripts, session contents, secrets, API keys, private filesystem paths, or PII to Engram. Save sanitized technical summaries only.

## User Preferences

- Keep responses concise when possible.
- Do not commit unless the user explicitly asks.
- Avoid unnecessary code comments.
- Run lint/typecheck or the closest available validation after completing code tasks.

## SDD / Skills Workflow

- Use Conductor skills in `.agents/skills/` for spec-driven work.
- For new features or ambiguous changes, start with `conductor-setup` if the Conductor structure is missing, then use `conductor-newTrack` before implementation.
- Use `conductor-implement` only after the track/spec/tasks are clear.
- Use `conductor-status` to inspect active tracks, `conductor-review` before closing work, and `conductor-revert` only when the user explicitly asks to undo a track.
- Keep tiny fixes direct when a full SDD track would add unnecessary process.

## Existing Specialized Team

- Existing LiveAudio-specific skills remain available and should not be overwritten:
- `liveaudio-architecture-security-deepseek`
- `liveaudio-performance-minimax-m27`
- `liveaudio-qa-qwen36plus`
- `liveaudio-research-gemini25pro`
- `liveaudio-ui-ux-security-architect`

## Known Project Context

- LiveAudio includes OBS subtitle/WebSocket behavior and ASR/GPU freeze resilience work.
- Configurable OBS subtitle backlog policy has been implemented and committed previously.
- Relevant areas include `core/audio.py`, `core/engine.py`, `core/network.py`, `main.py`, `subtitulos_obs.html`, `utils/config.py`, and docs under `docs/`.
