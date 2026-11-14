# Contributing to LiveAudio

Thanks for taking the time to contribute! 🎉

This document covers the contribution workflow, code conventions, and how to get your environment running.

---

## Before You Start

- Check [existing issues](https://github.com/plynte-labs/LiveAudio/issues) to avoid duplicates.
- For large changes, open an issue first to discuss the approach before writing code.
- All contributions must comply with the [Code of Conduct](CODE_OF_CONDUCT.md).

---

## Development Setup

### Requirements

- Rust stable toolchain (`rustup`)
- Node.js (for the Tauri CLI and frontend tests)

### Steps

```bash
# 1. Fork and clone
git clone https://github.com/YOUR_USERNAME/LiveAudio.git
cd LiveAudio

# 2. Download the pinned ONNX Runtime / DirectML binaries (not versioned in git)
node packaging/fetch_onnxruntime.mjs

# 3. Build the workspace
cargo build --workspace

# 4. Run the desktop app
cd desktop && npm run dev

# 5. Run tests
cargo test --workspace
node --test tests-js/*.mjs
```

See [docs/PACKAGING_AND_UPDATES.md](docs/PACKAGING_AND_UPDATES.md) for how releases and the installer work.

---

## Contribution Workflow

1. **Create a branch** from `main`:
   ```bash
   git checkout -b feat/my-feature
   # or
   git checkout -b fix/issue-123
   ```

2. **Make your changes.** Keep commits focused and atomic.

3. **Run tests** before pushing:
   ```bash
   cargo test --workspace
   ```

4. **Open a Pull Request** against `main`. Use a clear title and fill out the PR template.

---

## Code Conventions

- **Rust style**: run `cargo fmt` and keep `cargo clippy --workspace --all-targets -- -D warnings` clean.
- **No API keys or secrets** in any file — `config.json` is gitignored for a reason.
- **No external telemetry**: LiveAudio is 100% local by design. Do not add network calls that send user data anywhere.
- **Process boundary**: the ASR worker (`liveaudio-asr-worker`) is a separate supervised process. Keep its IPC messages backward-compatible or versioned, and do not share mutable state across the process boundary.
- **Comments**: write comments only when the *why* is non-obvious. Avoid restating what the code does.

---

## What We Welcome

- Bug fixes with a corresponding test
- Performance improvements (latency, VRAM, CPU usage)
- New Whisper model size support
- OBS/WebSocket integration improvements
- Documentation fixes and translations
- Accessibility improvements in the GUI

## What We Ask You to Avoid

- Adding optional cloud features that require API keys by default
- Breaking changes to `config.json` schema without a migration path
- Large refactors without prior discussion in an issue

---

## Commit Message Format

We use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add support for turbo model
fix: prevent crash when audio device disconnects
docs: update installation steps for Linux
chore: bump ort to 2.0.0
```

---

## Reporting Bugs

Use the [Bug Report issue template](https://github.com/plynte-labs/LiveAudio/issues/new?template=bug_report.yml).

For security vulnerabilities, see [SECURITY.md](SECURITY.md).

---

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
