# SDD + Engram Workflow For LiveAudio

This document applies only to the `liveaudio` project.

## Required Order

1. Recover Engram context with project `liveaudio` before planning or coding.
2. Search Engram when the task references previous work, decisions, bugs, branches, agents, OBS, ASR, or config behavior.
3. Use Conductor artifacts in `conductor/` for non-trivial feature, bugfix, refactor, or review tracks.
4. Use LiveAudio specialist skills when changes affect their areas.
5. Run the closest available validation after code changes.
6. Save important outcomes back to Engram before closing the session.

## Engram Privacy Rule

- Do not save raw audio, raw transcripts, session contents, secrets, API keys, private filesystem paths, or PII to Engram.
- Save sanitized technical summaries only.

## OpenCode Tool Mapping

- `ask_user` means OpenCode `question`.
- `write_file` and `replace` mean `apply_patch`.
- `run_shell_command` means `bash`.
- Plan Mode instructions are procedural guidance when the current OpenCode runtime has no plan-mode tool.

## Universal File Resolution

- Product Definition: `conductor/product.md`.
- Product Guidelines: `conductor/product-guidelines.md`.
- Tech Stack: `conductor/tech-stack.md`.
- Workflow: `conductor/workflow.md`.
- Tracks Registry: `conductor/tracks.md`.
- Tracks Directory: `conductor/tracks/`.

## LiveAudio Validation Baseline

```powershell
python -m compileall main.py core utils
```

## Specialist Review Mapping

- Architecture/security/privacy: `liveaudio-architecture-security-deepseek`.
- QA/product/docs/regression: `liveaudio-qa-qwen36plus`.
- Performance/resilience/latency: `liveaudio-performance-minimax-m27`.
- Research/API compatibility/traceability: `liveaudio-research-gemini25pro`.

## Save To Engram When

- A bug is fixed.
- An architecture or design decision is made.
- A config or environment setup changes.
- A non-obvious discovery is found.
- A feature track is completed or committed.
