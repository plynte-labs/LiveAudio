# LiveAudio — Project Instructions

## Model Policy (overrides the global Model Assignments for this project)

- Delegate complex work with `model: "sonnet"`: implementation, design, review, exploration, and verification.
- Delegate lightweight phases with `model: "haiku"`: archive and onboarding walkthroughs.
- Do not use `fable` (or any other model) unless the user explicitly asks for it.

## Project Facts

- Open source (MIT). Version 1.6.0. No IP-protection constraints.
- Rust workspace (`crates/`) plus a Tauri 2 desktop app (`desktop/`). The Python runtime has been removed: there is no Python ASR worker, no `uv`, no torch, and no `setup-runtime` command.
  - `liveaudio-audio`: capture via `cpal`; RNNoise noise suppression (off by default, `rnnoise_enabled`).
  - `liveaudio-vad`: Silero VAD v6 via ONNX Runtime (`ort`); the model is embedded.
  - `liveaudio-asr-worker`: native Whisper ASR worker on ONNX Runtime, shipped as a sidecar. Execution provider selection: CUDA, then DirectML (Windows), then CPU fallback; fp16 on GPU, int8 on CPU. Models download to `%LOCALAPPDATA%\LiveAudio\models\onnx`.
  - `liveaudio-core`: pipeline, supervisor (spawns the ASR worker sidecar), runtime discovery of the worker and ONNX Runtime library, and config (`config.json` under `%APPDATA%\LiveAudio`; `LIVEAUDIO_HOME` overrides).
  - `liveaudio-network`: WebSocket server (`tokio-tungstenite`) for OBS and integrations. Serves the subtitle overlay at `/overlay` on port 8765 by default (a fallback port range is tried if it is busy). Protocol: `docs/WEBSOCKET_OBS.md`.
  - `liveaudio-ipc`: shared command and protocol types.
  - `liveaudio-cli`: `doctor` diagnostics and the `mcp` stdio server; also bundled as a Tauri sidecar.
  - `desktop/`: Tauri 2 app (`src-tauri/`) with a vanilla JS frontend in `desktop/src/`.
- ONNX Runtime 1.24.4 and DirectML 1.15.4 are pinned with SHA-256 in `packaging/fetch_onnxruntime.mjs`. Fetch them with `node packaging/fetch_onnxruntime.mjs`, locally and in CI.
- Release: build the `liveaudio-cli` and `liveaudio-asr-worker` sidecars, then run `npx @tauri-apps/cli@2.12.1 build` with `src-tauri/tauri.release.conf.json`. Updates use the Tauri updater with a Minisign key (CI secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`).

## Commands

- `cargo test --workspace --locked`
- `cargo clippy --workspace --all-targets`
- `node --test tests-js/*.mjs`
- `node desktop/test_frontend.js`

## Working Rules

- Do not commit unless the user explicitly approves.
- Never commit built artifacts: `dist/`, `desktop/src-tauri/gen/schemas/*`, ONNX Runtime binaries, or `*.wav`.
- Track substantial implementation in ODD feature documents under `odd/tasks/`.

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
