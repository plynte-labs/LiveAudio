# Rust Style Guide

LiveAudio is a Rust workspace (`crates/`) with a Tauri 2 desktop app
(`desktop/`). The project is Rust-only: there is no Python runtime.

## Required checks

- `cargo fmt --all` before committing.
- `cargo clippy --workspace --all-targets -- -D warnings` must be clean.
- `cargo test --workspace` and `node --test tests-js/*.mjs` must pass for the
  areas a change touches.

## Conventions

- Keep the audio capture path non-blocking: no inference or disk I/O inside
  capture callbacks. Hand audio to the VAD and ASR stages through bounded queues.
- Validate every value that crosses a trust boundary: WebSocket messages,
  config files, IPC from the desktop UI, and paths. Reject out-of-range values
  instead of clamping them silently.
- Never log transcript text, audio samples or secrets. Diagnostics carry
  counts, durations and codes.
- The ASR worker is a separate supervised process. Changes to its IPC messages
  must remain backward-compatible or be versioned.
- Comments explain why, not what. Skip comments that restate the code.
