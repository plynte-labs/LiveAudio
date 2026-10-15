# LiveAudio — Project Instructions

## Model Policy (overrides global Model Assignments for this project)

- All implementation, design, and code-review delegation MUST use Fable 5: pass `model: "fable"` in every Agent tool call for sdd-apply, sdd-design, sdd-propose, jd-judge-a/b, jd-fix-agent, and general implementation delegation.
- Lightweight phases keep cheaper models: sdd-explore/spec/tasks/verify → sonnet, sdd-archive/onboard → haiku.

## Project Facts

- Open source (MIT) — no IP-protection constraints.
- Architecture: Rust workspace (`crates/`) + Tauri 2 desktop app (`desktop/`), with a hybrid Python ASR worker.
  - `liveaudio-audio`: capture via `cpal`. `liveaudio-vad`: Silero VAD v6 via ONNX Runtime (`ort`, model embedded).
  - `liveaudio-network`: WebSocket server (`tokio-tungstenite`) for OBS subtitles and integrations; protocol in `docs/WEBSOCKET_OBS.md`.
  - `liveaudio-core`: pipeline and runtime provisioning; `liveaudio-cli`: CLI, also bundled as the Tauri sidecar.
  - Python is ASR only: `liveaudio/service/asr_worker.py` (faster-whisper + ctranslate2), provisioned into a managed `.venv` with a vendored `uv` (`liveaudio setup-runtime --backend cpu|cu121`).
- The legacy Python GUI (`liveaudio/app.py`, CustomTkinter) is not the shipped app.
- CUDA backend must use the cu121 torch index (bundles cublas64_12.dll required by ctranslate2 4.x).
- Release: build the `liveaudio-cli` sidecar, then `npx @tauri-apps/cli@2.12.1 build` with `tauri.release.conf.json`.
- Do not commit built artifacts (`dist/` is gitignored) or `desktop/src-tauri/gen/schemas/*`.

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
