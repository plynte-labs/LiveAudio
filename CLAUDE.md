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

## Minimalism — Decision Ladder

Write the least code that fully solves the task. Before adding code, walk this ladder and stop at the first hit:

1. Does it need to exist? If not, skip it (YAGNI).
2. Already in the codebase? Reuse it.
3. Covered by the standard library? Use it.
4. Native platform feature? Use it.
5. Already-installed dependency? Use it.
6. One line? Write one line.
7. Only then: the minimum viable solution.

Lazy about the solution, never about reading — analyze the code thoroughly first. Never cut trust-boundary validation, data-loss handling, security, or accessibility. (Adapted from the Ponytail decision ladder.)
