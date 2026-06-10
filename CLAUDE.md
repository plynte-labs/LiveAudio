# LiveAudio — Project Instructions

## Model Policy (overrides global Model Assignments for this project)

- All implementation, design, and code-review delegation MUST use Fable 5: pass `model: "fable"` in every Agent tool call for sdd-apply, sdd-design, sdd-propose, jd-judge-a/b, jd-fix-agent, and general implementation delegation.
- Lightweight phases keep cheaper models: sdd-explore/spec/tasks/verify → sonnet, sdd-archive/onboard → haiku.

## Active Work

- Packaging & distribution overhaul (uv bootstrapper + frozen Python launcher, Windows + Linux).
  Plan: C:\Users\tavo_\.claude\plans\ok-continua-con-la-refactored-dongarra.md
  Engram topic: architecture/packaging-security (supersedes the Nuitka/portable-Python design).

## Project Facts

- Open source (MIT) — no IP-protection constraints.
- Python desktop app: faster-whisper + ctranslate2 (ASR), torch (Silero VAD), CustomTkinter GUI, websockets (OBS subtitles).
- CUDA builds must use the cu121 torch index (bundles cublas64_12.dll required by ctranslate2 4.x).
- Do not commit built artifacts; dist/ is gitignored.
