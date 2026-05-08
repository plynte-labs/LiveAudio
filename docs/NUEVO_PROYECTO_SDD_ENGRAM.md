# Nuevo Proyecto SDD + Engram: LiveAudio Status

This repository is not a new greenfield project. It is a brownfield LiveAudio project with existing code, docs, branches, and Engram memory.

## Project-Local Setup Completed

- `AGENTS.md` defines memory-first and SDD rules for LiveAudio.
- `.agents/skills/` contains Conductor skills and LiveAudio specialist skills.
- `conductor/` now contains project context, workflow, tech stack, style guide, and tracks registry.
- `docs/SDD_SKILLS_USAGE.md` explains how to use the local SDD skills.
- This file and `docs/SDD_ENGRAM_WORKFLOW.md` document the LiveAudio-only SDD/Engram workflow.

## Brownfield Classification

LiveAudio is brownfield because it has:

- Existing Python application code (`main.py`, `core/`, `utils/`).
- Dependency manifest (`requirements.txt`).
- Git history and prior feature branches.
- Product docs and requirements under `docs/`.

## Next New Track Procedure

1. Call Engram `mem_context` for `liveaudio`.
2. Call Engram `mem_search` for related prior work.
3. Create or update a requirement under `docs/requirements/`.
4. Register the track in `conductor/tracks.md` or create `conductor/tracks/<track_id>/` for larger work.
5. Implement, validate, review, and save Engram memory.
