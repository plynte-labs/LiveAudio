# Linux PR #37 review fixes

## Objective
Fix three review findings on `feat/linux-v1.6` (PR #37) before it is merged into `develop`.

## Scope
- T1: Restore the stable Windows installer asset name `LiveAudio-Setup-${VERSION}.exe` (and its `.exe.sig`) in `.github/workflows/release.yml`, so the docs and the `latest.json` URL stay consistent.
- T2: Move the blocking `pactl` resolution (`resolve_linux_pulse_source`) off the async runtime thread in `CpalAudioSource::start`.
- T3: Remove the duplicated `PR_SET_PDEATHSIG` helper. Keep one copy in `liveaudio-audio` and reuse it from `liveaudio-core` (core already depends on audio).

Out of scope: any other findings, Linux GPU work, and docs beyond what T1 needs.

## Constraints
- Do not change Windows runtime behavior.
- Strict TDD is enabled (global user config). Runners: `cargo test --workspace --locked` and `node --test tests-js/*.mjs`.
- Linux-only code cannot be executed on the Windows host. Use cfg-independent unit tests where possible and disclose what was not run.

## Checklist
- [x] T1 (b9dd1d4) installer asset name (route: delegated writer, trigger: 2+ non-trivial files across T1–T3)
- [x] T2 (660a4e9) pactl off the async thread (route: delegated writer)
- [x] T3 (refactor(linux): share the PR_SET_PDEATHSIG helper) shared pdeathsig helper (route: delegated writer)

## Checks
`cargo test --workspace --locked`, `cargo clippy --workspace --all-targets`, `node --test tests-js/*.mjs`, `node desktop/test_frontend.js`.

## Progress
- T1: workflow now copies the NSIS installer/.sig to `LiveAudio-Setup-${VERSION}.exe(.sig)` with hard-fail checks. `assemble_latest_manifest.mjs` needed no change (name-agnostic); added a test for the renamed pair (green at once, no RED possible). b9dd1d4.
- T2: `resolve_pulse_off_runtime` (spawn_blocking) resolves the Pulse source in `CpalAudioSource::start` on Linux; `build_and_start_stream` receives it. RED observed by inlining the resolver (off-thread test failed), GREEN with spawn_blocking. 660a4e9.
- T3: single `pub` `configure_linux_parent_death_signal(&mut std::process::Command)` in liveaudio-audio (re-exported at crate root, Linux only); core calls it with `cmd.as_std_mut()`. core keeps `libc` (watchdog, tests). Linux-only: not run on this Windows host (real-SIGKILL test untouched except the call site).
- Checks (Windows host): cargo test --workspace --locked ok; clippy: no new warnings in touched code; node tests 170 pass; frontend 876 pass. Linux target not installed, `cargo check --target` not run.

## Next step
Merged into local develop; push and PR #37 update are the user's decision.
