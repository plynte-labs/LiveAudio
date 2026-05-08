# SDD / Skills Usage

LiveAudio has Conductor SDD skills installed in:

```text
.agents/skills/
```

## Required Memory Step

Before any SDD work, recover project memory from Engram:

```text
Use Engram memory for project liveaudio. First call mem_context, then mem_search if needed, and treat the recovered memories as constraints.
```

This is required because LiveAudio already has important project memory about branches, OBS subtitle backlog policy, specialized subagents, user preferences, and previous commits.

Never save raw audio, raw transcripts, session contents, secrets, API keys, private filesystem paths, or PII to Engram. Save sanitized technical summaries only.

## OpenCode Tool Mapping

- `ask_user` in Conductor skills maps to OpenCode `question`.
- `write_file` and `replace` map to `apply_patch`.
- `run_shell_command` maps to `bash`.
- Plan Mode instructions are procedural guidance when the current runtime has no plan-mode tool.

## Universal File Resolution

- Product Definition: `conductor/product.md`.
- Product Guidelines: `conductor/product-guidelines.md`.
- Tech Stack: `conductor/tech-stack.md`.
- Workflow: `conductor/workflow.md`.
- Tracks Registry: `conductor/tracks.md`.
- Tracks Directory: `conductor/tracks/`.

## Available Conductor Skills

- `conductor-setup`: initialize the Conductor project structure.
- `conductor-newTrack`: create a new feature/bugfix/refactor track.
- `conductor-implement`: implement a selected track after the spec and tasks are clear.
- `conductor-status`: inspect current tracks and progress.
- `conductor-review`: review a track before closing it.
- `conductor-revert`: revert a track only when explicitly requested.

## Existing LiveAudio Skills

These project-specific skills were already present and remain available:

- `liveaudio-architecture-security-deepseek`
- `liveaudio-performance-minimax-m27`
- `liveaudio-qa-qwen36plus`
- `liveaudio-research-gemini25pro`
- `liveaudio-ui-ux-security-architect`

## Recommended Flow

For a new feature or ambiguous change:

```text
Recover Engram context for liveaudio, then use conductor-newTrack to create the spec, plan, tasks, risks, and acceptance criteria before coding.
```

Then:

```text
Use conductor-implement to implement the approved track one task at a time.
```

Before closing:

```text
Use conductor-review and any relevant LiveAudio-specific specialist skill to verify the implementation against the spec and previous project memory.
```

After meaningful work:

```text
Save the final decisions, discoveries, and completed track summary in Engram with mem_save or mem_session_summary.
```

## Terminal Engram Examples

```powershell
<ENGRAM_BIN>\engram.exe context liveaudio
<ENGRAM_BIN>\engram.exe search "OBS backlog policy" --project liveaudio
<ENGRAM_BIN>\engram.exe save "LiveAudio decision" "Decision/details here." --type decision --project liveaudio
```

For `context`, the positional `liveaudio` argument is the project selector. For commands that support `--project`, always pass `--project liveaudio`.
