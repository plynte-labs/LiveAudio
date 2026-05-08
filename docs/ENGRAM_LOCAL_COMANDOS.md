# Engram Local Commands For LiveAudio

These commands are examples for this project only. Prefer tool calls when working inside OpenCode.

## Context

```powershell
<ENGRAM_BIN>\engram.exe context liveaudio
```

For `context`, the positional `liveaudio` argument is the project selector.

## Search

```powershell
<ENGRAM_BIN>\engram.exe search "OBS backlog policy" --project liveaudio
<ENGRAM_BIN>\engram.exe search "settings profiles apply flow" --project liveaudio
```

## Save A Decision Or Discovery

```powershell
<ENGRAM_BIN>\engram.exe save "LiveAudio decision" "**What**: ...`n**Why**: ...`n**Where**: ..." --type decision --project liveaudio
```

## Session Summary

Use `mem_session_summary` from OpenCode before closing meaningful work. Include:

- Goal
- Instructions
- Discoveries
- Accomplished
- Next Steps
- Relevant Files

## Project Safety

- Always pass `--project liveaudio` when using CLI examples.
- For `context`, use the positional project argument `liveaudio`; for commands that support `--project`, always pass `--project liveaudio`.
- Do not save LiveAudio observations into another project.
- Do not use global setup commands unless the user explicitly requests cross-project changes.
