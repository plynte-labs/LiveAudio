# Project Workflow: Dream Team SDD Protocol

## Memory-First Rule

- Before planning or coding, recover Engram context for project `liveaudio` with `mem_context`.
- Use `mem_search` when the task references past work, decisions, bugs, branches, OBS behavior, or agent workflow.
- Save meaningful decisions, discoveries, bug fixes, configuration changes, and completed-track summaries back to Engram.
- Do not save raw audio, raw transcripts, session contents, secrets, API keys, private filesystem paths, or PII to Engram. Save sanitized technical summaries only.

## Role Separation

- **Auditor (Principal/Assistant)**: Receives input, plans rapidly, consults agents, presents MCQs to user, delegates tickets, supervises documentation, manages branches/tickets, runs collective reviews, reports status. **Never codes or implements.**
- **Specialized Agents**: Implement assigned tickets, write tests, update documentation, participate in collective reviews, report issues. **Follow auditor delegation strictly.**

## OpenCode Tool Mapping

- Use `question` when Conductor text says `ask_user`.
- Use `edit`/`write` for file edits when Conductor text says `write_file` or `replace`.
- Use `bash` for shell commands when Conductor text says `run_shell_command`.
- Treat Plan Mode instructions as procedural guidance when no plan-mode tool is available.
- Universal File Resolution Protocol: resolve Product Definition to `conductor/product.md`, Product Guidelines to `conductor/product-guidelines.md`, Tech Stack to `conductor/tech-stack.md`, Workflow to `conductor/workflow.md`, Tracks Registry to `conductor/tracks.md`, and Tracks Directory to `conductor/tracks/`.

## Dream Team Phases

### Phase 0: Intake & Rapid Planning
1. Auditor receives feature/fix request.
2. Auditor runs rapid planning: identifies scope, risks, edge cases, required agents.
3. Auditor consults all 4 agents asynchronously to validate assumptions and surface doubts.
4. Agents return findings; auditor consolidates.
5. Auditor presents multiple-choice questions to user for edge cases, boundaries, and acceptance criteria.
6. User responds; auditor confirms requirements and expected cases.
7. **Engram save**: Auditor saves user answers, MCQ outcomes, and confirmed scope.

### Phase 1: Documentation Delegation & Track Scaffolding
1. Auditor invokes `conductor-newTrack` or manually scaffolds `conductor/tracks/<track-id>/` with `spec.md`, `tasks.md`, `plan.md`.
2. Auditor delegates documentation tickets to agents based on scope.
3. Agents draft specs, test plans, idempotency/resilience strategies, and branch plans.
4. Auditor supervises, identifies gaps, requests revisions.
5. Agents update until documentation passes auditor review.
6. Auditor writes confirmed requirements to `conductor/tracks/<track-id>/spec.md`.
7. **Engram save**: Auditor saves spec decisions and test plan outlines.

### Phase 2: Implementation Delegation
1. Auditor creates feature branch `feature/<track-shortname>` (shortname maps to track-id).
2. Auditor updates `conductor/tracks.md` status to `[~] In Progress`.
3. Auditor assigns implementation tickets to agents; each task in `tasks.md` must reference a spec requirement ID (e.g., `REQ-1`, `REQ-2`).
4. Agents implement, write tests, and commit per ticket (only after auditor approval).
5. Auditor tracks branch progress, ensures no unapproved commits.
6. **Engram save**: Auditor saves implementation discoveries and config changes.

### Phase 3: Collective Review & Sign-Off
1. After implementation, auditor summons all 4 agents to review the work together.
2. Agents review code, tests, docs, and branch state. Each agent posts structured sign-off:
   `[Agent Name] SIGN-OFF: PASS | FAIL | PASS-with-notes`
   With 1-3 bullet points of what was reviewed.
3. **Required review areas per agent**:
   - Architecture: WebSocket binding (127.0.0.1 default), config validation, session paths, multiprocessing lifecycle, transcript/audio privacy.
   - Performance: queue backpressure, ASR freeze recovery, OBS burst prevention, VRAM/CPU pressure, VAD throughput, long-session stability.
   - QA: user-facing behavior, regression risk, docs completeness, manual test plan, term consistency, default safety.
   - Research: API compatibility, dependency behavior, traceability (requirements → tests → docs), changelog impact.
4. If new issues are found: agents document, update docs, and all review the branch together.
5. Auditor validates that resilience, idempotency, and quality gates are met.
6. **Explicit gates before proceeding**:
   - Resilience/idempotency tests exist in `tests/` with `test_resilience_*.py` / `test_idempotent_*.py` naming.
   - All 4 agents signed off.
   - User-facing docs updated (`README.md`, `docs/GETTING_STARTED.md`, `HISTORIAL_CAMBIOS.md`).
   - Changelog updated if user-facing behavior changed.
7. If issues remain unresolved, auditor escalates to user with options.
8. **Engram save**: Auditor saves review findings and sign-off status.

### Phase 4: Presentation & Feedback
1. Auditor presents completed work, test results, and updated docs to user.
2. Auditor waits for explicit user feedback.
3. **Pre-merge checklist** (auditor verifies before merge):
   - Spec complete and matches implementation.
   - All tests pass (`python -m compileall main.py core utils` + resilience/idempotency tests).
   - Docs updated for user-facing behavior.
   - All 4 agents signed off.
   - Engram session summary saved.
   - `conductor/tracks.md` status ready for `[x]`.
   - Changelog updated.
4. If approved, auditor merges branch, updates track to `[x] Completed`, deletes feature branch, and closes track.
5. If rejected, auditor delegates fixes, updates branch, and repeats Phase 3.
6. **Engram save**: Auditor saves completed track summary.

## Specialized Review Owners

- Architecture/security/privacy: `[Agent Arquitecto Deepseek V4 PRO]`.
- QA/product/docs/regression: `[Agent QA Qwen 3.6Plus]`.
- Performance/resilience/latency/idempotency: `[Agent Performance Minimax M2.7]`.
- Research/API compatibility/traceability: `[Agent Research Gemini 2.5 Pro]`.

## Quality Gates

- Code compiles with `python -m compileall main.py core utils` after Python changes.
- Resilience tests cover: device disconnects, GPU freezes, queue backpressure, OBS reconnection, ASR slower than speech input (60s+), CUDA model load failure, OBS burst after freeze, slow disk during session persistence.
- Idempotency tests verify: duplicate messages, retry loops, config re-applications produce identical state, partial-failure mid-apply, WebSocket reconnection idempotency.
- Documentation is updated for user-facing behavior.
- Config changes include safe defaults and validation.
- Runtime changes consider OBS burst behavior, ASR freezes, queues, device disconnects, and long-session stability.
- Do not commit unless the auditor explicitly approves.
- **QA Decision Rule**: If a feature cannot be manually validated or explained in docs, mark it as not closable.

## Branch And Commit Policy

- Auditor creates and manages feature branches (`feature/<track-shortname>` where shortname maps to track-id).
- Agents commit only to assigned branch after auditor approval.
- Commit messages follow conventional format: `feat:`, `fix:`, `test:`, `docs:`.
- Branch lifecycle: auditor tags branch at each phase gate; after merge, auditor deletes feature branch within 24h or next session.
- If two tracks touch overlapping files, auditor serializes them — no parallel branches on same module.
- Do not rewrite history or revert user changes unless explicitly requested.
- Merge to `master` only after collective review passes and user approves.
- Rollback: if merge introduces regression, auditor uses `conductor-revert` skill to revert logical units of work.

## Phase Completion

- Run automated validation available for the changed area.
- Provide manual verification steps for streamer/OBS workflows when automation is insufficient.
- Save a concise Engram session summary before closing the work.
- Auditor confirms all agents have signed off before proceeding.

## Acceptance Criteria Template

All tickets in `tasks.md` must include:
```
Given: [initial state/preconditions]
When: [action/trigger]
Then: [expected outcome]
Error Path: [what happens on failure]
UI State: [what user sees]
OBS Behavior: [subtitle/WebSocket expected behavior]
```

## Manual Test Matrix

Before Phase 4 presentation, verify:
- OBS WebSocket connect/disconnect/reconnect
- Subtitle backlog behavior (`auto`, `live_only`, `send_all`)
- Burst prevention after freeze recovery
- ASR start/stop/hot-swap
- Device disconnect mid-stream
- Session resume after crash
- Settings profile apply/discard
- Config validation rejects unsafe values
- Long-session stability (>2h simulated)
