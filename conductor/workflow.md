# Project Workflow

## Memory-First Rule

- Before planning or coding, recover Engram context for project `liveaudio` with `mem_context`.
- Use `mem_search` when the task references past work, decisions, bugs, branches, OBS behavior, or agent workflow.
- Save meaningful decisions, discoveries, bug fixes, configuration changes, and completed-track summaries back to Engram.
- Do not save raw audio, raw transcripts, session contents, secrets, API keys, private filesystem paths, or PII to Engram. Save sanitized technical summaries only.

## OpenCode Tool Mapping

- Use `question` when Conductor text says `ask_user`.
- Use `apply_patch` for file edits when Conductor text says `write_file` or `replace`.
- Use `bash` for shell commands when Conductor text says `run_shell_command`.
- Treat Plan Mode instructions as procedural guidance when no plan-mode tool is available.
- Universal File Resolution Protocol: resolve Product Definition to `conductor/product.md`, Product Guidelines to `conductor/product-guidelines.md`, Tech Stack to `conductor/tech-stack.md`, Workflow to `conductor/workflow.md`, Tracks Registry to `conductor/tracks.md`, and Tracks Directory to `conductor/tracks/`.

## Track Workflow

- Use Conductor tracks for new features, ambiguous changes, or multi-step refactors.
- Keep tiny fixes direct when a full track would add unnecessary process.
- Create or update a requirements/spec document before implementation when the behavior is user-facing or risky.
- Implement the smallest correct change and preserve existing project style.

## Specialized Review Owners

- Architecture/security: `[Agent Arquitecto Deepseek V4 PRO]`.
- QA/product: `[Agent QA Qwen 3.6Plus]`.
- Performance/resilience: `[Agent Performance Minimax M2.7]`.
- Research/traceability: `[Agent Research Gemini 2.5 Pro]`.

## Quality Gates

- Code compiles with `python -m compileall main.py core utils` after Python changes.
- Documentation is updated for user-facing behavior.
- Config changes include safe defaults and validation.
- Runtime changes consider OBS burst behavior, ASR freezes, queues, device disconnects, and long-session stability.
- Do not commit unless the user explicitly asks.

## Branch And Commit Policy

- Use feature branches for meaningful work.
- Do not rewrite history or revert user changes unless explicitly requested.
- Commit only after explicit user approval.
- Commit messages should be concise and conventional where practical, for example `feat: add settings profiles`.

## Phase Completion

- Run automated validation available for the changed area.
- Provide manual verification steps for streamer/OBS workflows when automation is insufficient.
- Save a concise Engram session summary before closing the work.
