# Tech Stack

LiveAudio is a Rust workspace with a Tauri 2 desktop app. The application
runs without Python, uv, PyTorch or CUDA toolkit installs.

## Workspace Crates

- `liveaudio-audio`: capture through `cpal` (WASAPI on Windows), device
  selection, ring buffer, resilient capture with reconnection, optional RNNoise.
- `liveaudio-vad`: Silero VAD v6 through ONNX Runtime (`ort`). The model is
  embedded in the binary. Includes the pre-roll buffer.
- `liveaudio-asr-worker`: native Whisper on ONNX Runtime with DirectML, CUDA or
  CPU execution providers. Runs as a separate supervised process. Downloads
  model weights at runtime.
- `liveaudio-core`: pipeline orchestration, configuration and validation,
  process supervision, Windows job objects, WebVTT and transcript sinks.
- `liveaudio-ipc`: shared IPC types used by the core, CLI and desktop app.
- `liveaudio-network`: local WebSocket server (`tokio-tungstenite`) for OBS
  browser sources and integrations, the `/overlay` HTTP page, and the replay
  buffer for catch-up subtitles.
- `liveaudio-cli`: command-line front end and sidecar: `doctor`, `serve`,
  `devices`, and an MCP stdio server.
- `desktop/src-tauri`: Tauri 2 shell and commands. Frontend assets live in
  `desktop/`.

## Native Dependencies

- ONNX Runtime 1.24.4, fetched by `packaging/fetch_onnxruntime.mjs` (not committed).
- DirectML 1.15.4 on Windows (`DirectML.dll`).
- See `THIRD_PARTY_NOTICES.md` for licenses and `docs/PACKAGING_AND_UPDATES.md`
  for how they ship.

## Validation Commands

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `node --test tests-js/*.mjs`
- `node desktop/test_frontend.js`

## Change Control

- Changes to crate boundaries, IPC or the WebSocket protocol must be documented
  in `docs/WEBSOCKET_OBS.md` (or the relevant doc) before implementation.
- Run the closest available validation after code changes.
