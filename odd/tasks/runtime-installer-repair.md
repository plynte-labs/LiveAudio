# Runtime Installer Repair — Organic Work Document

**Goal:** Make a clean Windows desktop install able to prepare its ASR runtime through the app, visibly and cancellably, then start only when the worker is ready.

**Problem:** The Tauri start path preflights Python imports but does not provision the managed runtime. The desktop bundle contains the worker package but not the CLI or `uv`; on a clean machine this leaves no in-app recovery path. New configuration also defaults to CUDA, while model preparation events are not forwarded to the UI until after the worker startup wait.

**Why:** The user authorized four bounded repairs after reviewing the clean-Windows report and confirmed that first preparation may be online if progress is visible and cancellation works.

**Approved design:** Reuse the existing locked `pyproject.toml`/`uv.lock` provisioning into the data-home `asr-runtime` directory. Bundle the existing pinned `uv` binary, let the desktop app offer first-run setup and repair with visible phase/progress and cancellation, default new configurations to CPU unless CUDA is positively verified, and preserve an existing saved device choice. Keep `python.exe` for the worker protocol and suppress Windows console windows while retaining redirected stdio. Forward structured model-download progress before `Ready`; report unknown percentages as indeterminate and do not start audio capture until readiness.

**Architecture:** Rust owns runtime discovery, provisioning, cancellation, process creation, and typed service/progress state. Tauri commands/events expose those operations; the frontend presents preparation, model download, ready, error, and cancel states. Existing ASR worker protocol, lockfile, managed runtime root, and test conventions remain the source of truth.

**Tech stack:** Rust workspace, Tauri 2, JavaScript frontend, Python worker, `uv`, existing `pytest`/Rust/Node test suites.

**Authorized scope:** Four tasks R1–R4 below, sequentially under one writer. The auditor reviewed the complete plan and authorized implementation of all four tasks on the current branch. This document's original planning-only phase has ended; implementation remains bounded to the work units below.

## Constraints and decisions

- Strict TDD is enabled by `AGENTS.md`: each behavior change follows RED → GREEN → REFACTOR with observed results; do not implement before a regression test fails for the intended reason.
- First runtime preparation is online. Bundling `uv` does not make the dependency/Python download offline; do not describe it as offline.
- Runtime setup must not depend on system `python` or `uv` being on `PATH`; use the bundled `uv` and its managed Python provisioning behavior.
- Keep `python.exe`, not `pythonw.exe`, for the worker and preflight because stdout/stderr carry the worker IPC and diagnostics. Apply Windows no-console process creation consistently to Python probe, worker, and `uv` subprocesses without breaking stdio.
- A video-adapter name or NVIDIA vendor string alone is not proof that CUDA can execute. New configurations should safely default to CPU; preserve a previously saved explicit `device` value. Do not silently rewrite an existing user's choice.
- Never fabricate progress totals or percentages. If `uv` or the model backend does not provide a trustworthy denominator, show phase/activity as indeterminate.
- The model worker must emit `Ready` before audio capture begins; preparation/download progress is not a ready signal.
- Preserve the full existing dirty RNNoise/acoustic-events work. Do not checkout/reset, modify its files, or stage it.
- No VM, app launch, installer build/install, real model/dependency download, remote operation, commit, push, PR, or RDD review is authorized by this document-preparation task. Runtime network use is approved only for a later user-visible first preparation, not to be exercised here.
- Do not store VM report paths, names, or other personal details in project artifacts.

## Files and interfaces

| Area | Existing responsibility | Planned touch points |
|---|---|---|
| Runtime setup | `/crates/liveaudio-core/src/runtime.rs` embeds manifests, resolves `uv`, stages the package, runs locked sync, and preflights imports. | Reuse this path; make setup observable/cancellable for desktop while retaining CLI compatibility. |
| Python process discovery and supervision | `/crates/liveaudio-core/src/supervisor/process.rs` selects Python, builds worker config, spawns/reaps workers, and owns bounded respawn policy. | Preserve `python.exe` IPC; suppress Windows console creation for worker/probe/installer child processes. |
| Desktop commands and state | `/desktop/src-tauri/src/commands.rs`, `/desktop/src-tauri/src/state.rs`, `/desktop/src-tauri/src/dto.rs` manage start, service state, and Tauri events. | Add desktop setup/cancel path and typed progress; forward model progress while startup is still waiting for readiness. |
| Desktop package | `/desktop/src-tauri/tauri.conf.json` maps NSIS resources; `/packaging/vendor/uv.exe` is the existing pinned Windows binary. | Bundle the existing `uv` binary at a discoverable resource path; do not add another downloader or unpinned binary. |
| Config/frontend | `/crates/liveaudio-core/src/config/mod.rs` defines config defaults; `/desktop/src/app.js`, `/desktop/src/i18n.js`, `/desktop/src/index.html`, `/desktop/src/types.d.ts` drive the UI. | Safe new-config default, runtime setup/repair controls, determinate/indeterminate progress, cancellation, and readiness states. |
| Existing tests | `/crates/liveaudio-core/tests/runtime_provisioning.rs`, Rust unit tests, `/desktop/src-tauri/src/tests.rs`, `/desktop/test_frontend.js`. | Extend focused fake-process/config/UI tests; never use real network/model acquisition in automated tests. |

Existing package inputs `/pyproject.toml` and `/uv.lock` are embedded by the Rust runtime; retain their lock semantics and selected `cpu`/`cu121` extras. The managed root is `managed_runtime_root()` (`asr-runtime` under the application data home), not the install directory.

## Work units

Route: delegated direct, one writer, four sequential tasks. The cross-file Tauri/process/runtime changes exceed a safe inline mechanical edit; do not parallelize writers. Keep a working change estimate and do not split or trim tests just to meet a line-count heuristic. Delivery strategy: `ask-on-risk`; no commit is authorized until the auditor explicitly approves. If the eventual authored change exceeds the usual ~400-line planning budget, ask the auditor for a chain strategy before a commit rather than silently choosing one.

### R1 — Hide child consoles without breaking worker IPC

**Objective:** Prevent Windows console popups for runtime probes, ASR worker, and bundled `uv` while retaining the existing JSON-lines/stdout protocol and stderr diagnostics.

**Files:** Modify `/crates/liveaudio-core/src/runtime.rs` and `/crates/liveaudio-core/src/supervisor/process.rs`; test in `/crates/liveaudio-core/tests/runtime_provisioning.rs` and relevant process tests in `process.rs`.

**Requirements:** Apply `CREATE_NO_WINDOW` to the Python preflight, worker, and `uv` child process creation paths under `cfg(windows)`. Keep stdin/stdout/stderr wiring intact. Do not switch to `pythonw.exe`; keep the managed/runtime Python executable resolution order and the bounded restart budget unchanged.

- [ ] Write focused regression tests for Windows process setup and for worker/probe stdio still carrying expected data; assert Python selection remains the console-capable `python.exe` path.
- [ ] Run the focused tests and observe the expected RED before changing production code.
- [ ] Implement the smallest shared Windows spawn configuration needed by all three child paths.
- [ ] Re-run focused tests, the liveaudio-core test target, formatting, and diff whitespace checks.

**Checks:** `cargo test -p liveaudio-core --test runtime_provisioning`; `cargo test -p liveaudio-core`; `cargo fmt --all --check`; `git diff --check`. A clean-VM console smoke check remains manual and pending; do not launch an app in this work.

### R2 — Bundle `uv`; add visible, cancellable desktop runtime setup/repair

**Objective:** On first start or explicit repair, provision the managed runtime from the bundled `uv` and included worker package with visible progress and an effective cancel action.

**Files:** Modify `/desktop/src-tauri/tauri.conf.json`, `/crates/liveaudio-core/src/runtime.rs`, `/desktop/src-tauri/src/commands.rs`, `/desktop/src-tauri/src/dto.rs`, `/desktop/src/types.d.ts`, `/desktop/src/app.js`, `/desktop/src/i18n.js`, `/desktop/src/index.html`, and relevant Rust/frontend tests. Reuse `/packaging/vendor/uv.exe`; do not modify or replace the binary.

**Interfaces:** Add one typed Tauri setup progress payload/event with phase, user-safe text, and optional observed percent. Add a desktop command to start/repair and a separate cancel command/token path that remains callable while setup is active. Reuse `setup_managed_runtime` semantics and the locked manifests; make its long-running execution cancellable and stream progress. The installer resource destination must agree with runtime `uv` discovery and must not fall back to PATH on a clean install.

**Requirements:** First preparation is online and user-visible; report Python/dependency preparation phases and errors. Cancellation stops the active `uv` process, releases the operation state, leaves retry possible, and does not destroy a previously valid managed runtime. Prevent duplicate concurrent setup. The cancel path must not be blocked behind the transition lock held by first start. Remove terminal-only remediation from desktop errors; retain CLI setup for CLI users.

- [x] Add fake-`uv` tests for bundled-path discovery, setup progress, cancellation/child reaping, retry after cancellation, and preservation of an existing valid runtime; add a Tauri/UI test for cancel being available while setup is active.
- [x] Run the focused regressions and observe RED before implementation. RED: `npm test --prefix desktop` reported the expected five missing UI/resource assertions (165 passed, 5 failed); `cargo test -p liveaudio-core --offline --locked cancelling_startup_reaps_worker_while_waiting_for_ready` failed to compile because the cancellable startup API did not yet exist.
- [x] Map the existing vendor binary into Tauri resources and wire desktop setup/repair to the cancellable managed-runtime provisioner.
- [x] Re-run focused tests: `cargo test -p liveaudio-core --offline --locked --test runtime_provisioning` passed 7 tests; `cargo test -p liveaudio-core --offline --locked cancelling_startup_reaps_worker_while_waiting_for_ready` passed 1 test; `cargo test -p liveaudio-desktop --offline --locked` passed 30 tests; `npm test --prefix desktop` passed 170 tests.

**Checks:** `cargo test -p liveaudio-core --test runtime_provisioning`; `cargo test -p liveaudio-desktop`; `node desktop/test_frontend.js`; `cargo fmt --all --check`; `git diff --check`. `uv sync` may access the network only during a later approved runtime setup; do not execute it here. NSIS packaging and installed clean-VM first-run/cancel validation remain manual and pending.

### R3 — Default safely to CPU and preserve saved device choices

**Objective:** Avoid selecting CUDA by default on clean/non-CUDA systems while preserving any existing saved device selection; do not infer CUDA readiness from a display adapter name.

**Files:** Modify `/crates/liveaudio-core/src/config/mod.rs`, desktop device/profile defaults in `/desktop/src/app.js` and `/desktop/src/index.html` as needed, and tests in `/crates/liveaudio-core/src/config/mod.rs`, `/desktop/src-tauri/src/tests.rs`, and `/desktop/test_frontend.js`.

**Requirements:** New/default config selects CPU unless a reliable CUDA execution check is already available. Preserve a stored `device` field exactly when loading existing configuration. If CUDA is not verified, expose CPU safely and do not silently change a saved explicit CUDA choice; report the unavailable backend through the existing user-facing error/setup path. An NVIDIA vendor string, WMI video-controller entry, or `nvcuda.dll` presence alone is insufficient evidence of usable CUDA.

- [x] Add tests proving new/default config is CPU, existing persisted `device: "cuda"` remains unchanged, and UI/profile startup does not overwrite the loaded value.
- [x] Run focused config/frontend tests and observe RED. RED: `cargo test -p liveaudio-core --offline --locked config::tests::test_default_config_fields` failed because current default was `cuda`; `npm test --prefix desktop` had the expected two new missing CPU-default assertions (170 passed, 2 failed).
- [x] Implement only the safe initial default/preservation behavior; no hardware-name detector or new dependency was added. No trustworthy CUDA execution probe exists in the inspected paths; the implementation does not auto-select CUDA and continues preserving an explicit saved selection.
- [x] Re-run focused tests: `cargo test -p liveaudio-core --offline --locked config::tests` passed 9 tests; `npm test --prefix desktop` passed 172 tests.

**Checks:** `cargo test -p liveaudio-core`; `cargo test -p liveaudio-desktop`; `node desktop/test_frontend.js`; `cargo fmt --all --check`; `git diff --check`. Hardware/CUDA confirmation on installed NVIDIA machines remains manual and pending.

### R4 — Forward model progress before readiness and gate audio on `Ready`

**Objective:** Show truthful first-use model download progress immediately, with an indeterminate state when no trustworthy percentage exists, and keep capture inactive until the worker reports `Ready`.

**Files:** Modify `/desktop/src-tauri/src/commands.rs`, `/desktop/src-tauri/src/dto.rs`, `/desktop/src/types.d.ts`, `/desktop/src/app.js`, `/desktop/src/i18n.js`, `/desktop/src/index.html`; add/extend `/desktop/src-tauri/src/tests.rs` and `/desktop/test_frontend.js` tests. Worker protocol source `/liveaudio/service/asr_worker.py` and `/liveaudio/service/asr_worker_protocol.py` already emits structured `status`, `download_progress`, and `ready`; preserve that contract unless tests expose a necessary compatibility gap.

**Interfaces:** Extend service status with typed optional ASR progress (`phase`, `text`, optional percent and byte counters) or an equivalent typed event payload. Tauri must forward worker progress/status/ready from a dispatcher active before the `start_internal` readiness wait completes. UI renders known percent as determinate and unknown as indeterminate; it clears stale progress on ready/error/stop.

**Requirements:** Do not estimate total model size or percentages from downloaded bytes without a known total. Keep start/stop/config controls coherent while preparation is pending. Do not begin audio capture or report “listening” until `Ready` is admitted. Preserve the existing worker event protocol and avoid sending progress into subtitle/OBS queues.

- [x] Add Rust tests for optional/unknown progress and ready clearing progress; add attempt-freshness tests and frontend assertions for typed progress, determinate/indeterminate markup, and cancellation during startup/download. Existing command flow already places audio-source startup after the supervisor `Ready` handshake; that gate is preserved.
- [x] Run focused tests and observe RED before implementation. RED: desktop Rust compile failed because the progress mapper/helper was missing; frontend reported the two new model-progress assertions missing (173 passed, 2 failed); focused RED confirmed a `percent` without a known byte total was incorrectly exposed (expected `None`, got `Some(50.0)`); ready-clearing and attempt-freshness tests first failed to compile because their helper APIs were absent.
- [x] Start a dedicated Tauri event-forwarder before the readiness wait; send typed optional progress through existing service status events and render unknown percentages with an indeterminate native progress bar. Clear stale progress on Ready/error/stop, reject older worker attempts, keep cancellation available through the startup handshake, and preserve audio startup after Ready.
- [x] Re-run focused tests: progress mapper tests (2), ready cleanup (1), attempt freshness (1), and frontend suite (178) passed. Full workspace verification is recorded below.

**Checks:** `cargo test -p liveaudio-desktop`; `cargo test -p liveaudio-core`; `node desktop/test_frontend.js`; `cargo fmt --all --check`; `git diff --check`. Installed first-use download and hardware smoke checks remain manual and pending; no model download is permitted during automated verification.

## Acceptance criteria

- [ ] A clean Windows Tauri install with no system Python or `uv` can start managed setup from the UI using bundled `uv` and the included Python package. **Installed clean-VM proof pending.**
- [x] Setup exposes progress and an effective cancel action; fake-process tests verify cancellation/reaping, prior-runtime preservation, and successful retry.
- [x] Python preflight/worker and `uv` receive Windows `CREATE_NO_WINDOW` while preserving `python.exe` and piped IPC; Windows-target tests and mock-worker tests pass.
- [x] New configs safely default to CPU; serialized CUDA remains unchanged through load/normalize; no video-adapter inference was introduced.
- [x] Model preparation forwards typed progress early; unknown totals remain indeterminate, Ready/error/stop clear progress, and audio capture remains sequenced after Ready.
- [x] Automated checks are reported below. NSIS/clean-VM install, real online first preparation/cancellation, and installed hardware checks remain explicitly pending.

## Progress and verification

- [x] Read-only code map and attachment review completed before planning. Confirmed no provisioning call from Tauri start; confirmed managed root, embedded lockfile, existing `uv` vendor asset, CPU/CUDA defaults, and worker download event path.
- [x] Plan was created and read back before implementation; no production code had changed at that planning checkpoint.
- [x] R1: Added Windows `CREATE_NO_WINDOW` to Python preflight, worker, and `uv` command construction while retaining piped stdio and `python.exe`. RED: `cargo test -p liveaudio-core --offline --locked windows_` failed to compile with missing `windows_no_console_flag` (the intended absent policy hook). GREEN: the same command passed 2 Windows-flag tests; focused runtime provisioning passed 7 tests and mock-worker supervision passed 5.
- [x] R2: Bundled the existing `uv.exe` in Tauri resources, added cancellable staged setup/repair and first-start provisioning, and exposed preparation progress/cancel controls. Cancellation uses a token path independent of the transition mutex and can cancel/reap the worker during its initial `Ready` handshake. Fake-process tests preserve the existing runtime on cancellation and pass a retry afterward; NSIS/clean-VM first-run validation remains pending.
- [x] R3: New/default config and pre-load UI default now select CPU. Existing serialized CUDA remains through load/normalize, and user-selected GPU profiles remain explicit selections; no adapter-name-as-CUDA-readiness inference was added. Hardware execution probe/confirmation remains unavailable and no auto CUDA selection is performed.
- [x] R4: Tauri now consumes worker events while startup waits for `Ready`, exposes typed model progress with only trustworthy totals/percentages, clears it on readiness/failure/stop, rejects stale attempt progress, and displays an indeterminate progress bar when totals are unknown. The audio capture path remains sequenced after successful Ready.
- [x] Final verification: `cargo fmt --all -- --check` passed; `cargo test --workspace --offline --locked` passed (exit 0); `npm test --prefix desktop` passed 178 checks; `node --check desktop/src/app.js` exited 0; `git diff --check` exited 0 (Git printed only LF/CRLF normalization warnings).
- [ ] Manual product validation remains pending: test the generated NSIS installer on a clean Windows VM with no Python/uv in PATH, online first preparation, cancel/retry, model progress, and audio capture only after Ready. The installer was built but never launched; no real downloads were performed here.
- [ ] Engram recovery mirror `odd/runtime-installer-repair/tasks` pending: session identity is unavailable in this delegated runtime, so this agent did not perform memory writes.

**Implementation authorization:** Auditor authorized sequential R1–R4 implementation in the current session. Keep the writer on the existing branch, do not commit, and preserve every pre-existing dirty/untracked file.

**Next step:** Auditor review and clean-VM validation; do not treat code or the successful local bundle build as proof of the installed first-run experience.

## Bounded review correction wave

The auditor authorized one correction wave for three scoped findings. These corrections extend R2/R4 plumbing only; they do not expand product scope.

- [x] **C1 — Unique staged-publish backup:** Confirmed `setup_staging_root(root).with_extension("previous")` collapsed the timestamped staging sibling to fixed `<runtime>.previous`. RED: `cargo test -p liveaudio-core --offline --locked --test runtime_provisioning provisioning_refresh_ignores_stale_previous_backup_and_preserves_its_contents` failed with OS error 145 (`DirectoryNotEmpty`) on a stale nonempty backup. GREEN: the same test passed after backup naming retained the unique timestamped staging basename; successful repair leaves the stale backup sentinel untouched and publishes the new runtime.
- [x] **C2 — Desktop-only runtime remediation:** Confirmed the core `DependenciesUnavailable` error intentionally includes CLI setup guidance, and the Tauri desktop path was appending it verbatim despite an in-app Repair action. RED: focused desktop test failed to compile because the UI-specific mapping was absent. GREEN: `cargo test -p liveaudio-desktop --offline --locked desktop_managed_runtime_preflight_error_routes_to_repair_ui_not_cli` passed; desktop maps the structured import-preflight variant to Settings repair guidance, while the core error remains unchanged for CLI callers.
- [x] **C3 — Worker process attempt generations:** Confirmed Python's command parser reads `attempt_id` from the top-level protocol envelope and defaults missing IDs to `1`; Rust previously omitted it and reused the same init payload on respawn. RED: `cargo test -p liveaudio-core --offline --locked --test supervisor_mock_worker respawn_sends_a_new_attempt_id_in_each_worker_init` failed parsing the absent IDs across actual startup/respawn. GREEN: the same test passed with strictly increasing init-envelope IDs, and `cargo test -p liveaudio-desktop --offline --locked respawn_generation_rejects_late_ready_and_progress_from_previous_worker` passed the actual dispatch-generation gate scenario. Startup and background dispatch now reject older generations for Ready/progress/status/error; acoustic speech-attempt IDs remain separate.
- [x] **Progress stream contract:** Worker broadcast channel remains bounded at 1024. Progress is a latest-state stream; intermediate progress samples may be dropped under lag, not a lossless history. UI should present the newest received state and must not claim a continuous byte-complete history.
- [x] Re-normalized with `cargo fmt --all`; final checks passed: `cargo fmt --all -- --check`; `cargo test --workspace --offline --locked` (exit 0); `npm test --prefix desktop` (178 passed, 0 failed); `node --check desktop/src/app.js` (exit 0); `git diff --check` (exit 0; only CRLF normalization warnings).
- [x] Local NSIS package created at `E:\LiveAudio\target\runtime-installer-repair\release\bundle\nsis\LiveAudio_1.5.0_x64-setup.exe` using cached Tauri CLI 2.12.1 with offline Cargo/npm settings, `--offline --locked`, and an isolated `CARGO_TARGET_DIR`; size 19,973,727 bytes; SHA-256 `96417B2D7E4782486FF2C72DD7D99812786FB0E851361E58E08312E276EC9D6B`. The source/staged `uv.exe` hashes matched (`6EB0E7A7BC0A84C3158120D36046B48C30A29E109F718C0D25FEE0701A9FC84C`), as did the worker file (`512F4CCDC0F176992ACD66BEE6F4876DAF3984EE0280B3F12E8CC1E81377E905`). NSIS contents could not be inspected with 7-Zip because it is unavailable; the configured bundle mapping and staged resource copies were checked instead. Both pre-existing dirty generated schemas retained their pre-build SHA-256 values. During the build, Tauri CLI normalized dependency shorthand in the previously clean `desktop/src-tauri/Cargo.toml` into tables with empty `features` lists (`tauri = "=2.12.0"` and `tauri-build = "=2.7.0"` retained equivalent version/default-feature semantics). This was detected after packaging and the exact HEAD bytes for this one clean file were restored; `git diff -- desktop/src-tauri/Cargo.toml` is empty. No other source file was restored, and the installer was not rebuilt because the change was serialization-only.
- [ ] Manual installed clean-VM first-run/repair/cancel validation remains pending. No installer launch, online `uv sync`, real model download, mic use, or app launch occurred.
- [x] Independent review readback: Architecture passed the C1/C3 scope (14 tests); QA passed C2 (178 frontend checks plus one focused Rust regression); Performance passed the worker/event scope (36 desktop tests, six mock-worker tests, and one cancellation regression). CPU remains the intentional new-config default; no automatic CUDA execution probe was implemented. RNNoise performance target/validation remains tracked separately and unchanged.
