# Release Readiness Combo — Organic Work Document

> **Implementation discipline:** Strict RED → GREEN → REFACTOR. One sequential writer. Do not split or trim tests merely to satisfy an advisory line-count forecast.

**Goal:** Resolve the approved clean-checkout, native ONNX Runtime, legacy Python default-device, and RNNoise timing-diagnostic gaps, then hand off for an offline NSIS build after parent review.

**Problem:** The checkout had an ignored local Silero ONNX model, Linux portability gaps, no demonstrated explicit app-owned CPU ONNX Runtime path, legacy Python config mutation of device choice, and an RNNoise ≤64 ms timing gap.

**Why:** The user authorized the combined release-readiness scope. Existing runtime-installer and RNNoise task records remain intact; this document records the additional combination only.

**Approved scope:** RR-1 through RR-5 below. Parent authorized only official Microsoft CPU ONNX Runtime 1.26.0 Windows/Linux assets and the pinned Silero V6 upstream asset. ASR model downloads remain unauthorized. The ≤64 ms RNNoise acceptance remains open absent valid measured proof. Do not claim clean checkout, Linux package execution, hardware, or clean-VM acceptance without evidence.

**Architecture:** Explicitly initialize pinned `ort` from app-owned absolute paths before any ORT call/audio/worker/listener side effect. Tauri uses absolute resource dir; CLI deterministic development or executable-relative package path. Legacy Python uses CPU only for new/missing config and preserves explicit CUDA choice. RNNoise instrumentation retains the existing pacing, timestamp boundary, metric and threshold.

**Tech:** Existing Rust workspace, Tauri 2, frontend, legacy Python and pytest, `ort` 2.0.0-rc.13 with `load-dynamic` and API 17. No tool/dependency installation.

**Related records:** [runtime installer repair](runtime-installer-repair.md), [RNNoise and acoustic events](rnnoise-acoustic-events.md). This document does not reopen prior tasks or close their outstanding acceptance by assertion.

## Constraints and verified evidence

- Preserve all prior dirty work, especially RNNoise/acoustic events and runtime installer repair. No checkout/reset, commits, pushes, PRs, RDD, remote/SSH/VM access, app/installer launch, mic/OBS/CUDA use, ASR downloads, dependency installation, or `uv sync`.
- Only `.gitignore` and the verified Silero model are staged; all other changes remain unstaged. No Engram mirror is available/authorized for this worker; local document is authoritative, mirror pending.
- TDD enabled by `AGENTS.md`; use existing offline locked Rust suite, desktop npm tests, and Python tests only with installed runner. Do not claim tests not run.
- `ort` is pinned 2.0.0-rc.13, `load-dynamic`, API 17; `ort::init_from(path)` requires explicit commit and initialization before any other ORT APIs. Sources: [init_from](https://docs.rs/ort/2.0.0-rc.13/ort/environment/fn.init_from.html), [environment module](https://docs.rs/ort/2.0.0-rc.13/ort/environment/index.html).
- Prior ignored model digest `4CBF549B8326F60F80F2536D9EEFEB450A9ABE83365A098031C89719F1BE17D2`, Git blob `9febab9bc0aa5ae2b27e69243d813427212bfb36`, 1,245,151 bytes; provenance unknown and bytes preserved in ignored `target/release-readiness-combo/recovery/original-silero_vad_v6.onnx`.
- Verified Silero source is `snakers4/silero-vad` commit `fba061dc5559f696e62171e9a0741782b0fdc23c`, path `src/silero_vad/data/silero_vad_16k_op15.onnx`, expected blob `3036c9e2a0293175d098dbbec63c229c1e0f49fa`, size 1,289,603, MIT license. Bundled file SHA256 `794ED8A51D4F37FAF0555383AA34DBAEEB83E3031A1DF1E0351C457E1142BD3E`; verified actual inference before staging.
- Upstream `utils_vad.py` at the same pinned revision establishes 16 kHz V6 contract: input `[1,576]` context+512, `state` `[2,1,128]`, int64 `sr=16000`; names `input,state,sr`; probability then recurrent state outputs. Old wrapper used `h/c` and failed against V6 with `Invalid input name: h`; minimum internal adaptation made, public VAD interface and transition policy unchanged. Earlier valid-runtime test falsely passed without inference; corrected child-process test explicitly loads and infers.
- Microsoft ORT v1.26.0 official Windows ZIP: 75,675,381 bytes, SHA256 `6ebe99b5564bf4d029b6e93eac9ff423682b6212eade769e9ca3f685eaf500b4`, asset `415503188`. Linux TGZ: 8,590,023 bytes, SHA256 `1254da24fb389cf39dc0ff3451ab48301740ffbfcbaf646849df92f80ee92c57`, asset `415501317`. Source URLs: `https://github.com/microsoft/onnxruntime/releases/download/v1.26.0/onnxruntime-win-x64-1.26.0.zip` and `https://github.com/microsoft/onnxruntime/releases/download/v1.26.0/onnxruntime-linux-x64-1.26.0.tgz`; licenses/notices included. No runtime downloads are added.
- Native init is process-global; independent path tests use child processes. No `ORT_DYLIB_PATH`, `PATH`, CWD, or implicit DLL fallback. Missing/invalid library is an app package/reinstall error before startup side effects, not Python ASR repair; no mock VAD fallback.
- Prior RNNoise paced observations: 61.316 ms pass; 87.333 and 103.072 ms fail (RTF 0.0054 for latter). Preserve original threshold and metric; no repeated rerun-until-green or exclusion of OS waits.
- One later authorized 60-second paced run returned exit 0, RTF 0.0055, 960,000 samples, 32.805 ms, BUT IS INVALID: the initial instrumentation timestamp was before synthetic input chunk generation. The original timestamp boundary is restored. No further paced run is authorized. ≤64 ms remains OPEN.
- Local Linux target/runtime execution, CI, Linux packaged launch, clean Windows VM, hardware, and NSIS package remain unproven. An NSIS build is not clean-VM proof.

## Work units

### RR-1 — Clean-checkout and Rust portability
**Files:** `.gitignore`, model asset, root/core manifests, watchdog, device helper/tests, CI.
- [x] Verify pinned model provenance/hash/blob/size/license; preserve original unknown-provenance bytes.
- [x] Add Unix `libc`, `cfg(unix)` watchdog, Windows-only default-output guard and regressions; add Windows/Ubuntu Rust CI matrix and asset checks.
- [x] Model exception narrowly added; only `.gitignore` and verified model staged.
- [x] Final offline locked workspace Rust test passed on Windows; fmt and whitespace checks passed. Local Linux target unavailable; CI pending.
**Acceptance:** clean checkout/model and Linux CI proof pending CI; local Windows Rust tests passed.

### RR-2 — App-owned CPU ONNX Runtime
**Files:** VAD runtime/Silero/tests; CLI; Tauri startup and platform config; `packaging/onnxruntime`; notices and CI.
- [x] Add explicit `ort::init_from` process init, absolute app-owned library paths, deterministic CLI dev/resource lookup, Tauri resource-dir initialization before side effects; no ambient fallback.
- [x] Add process-isolated missing/invalid/valid runtime tests and constructor guard; correct test to actually infer.
- [x] Add Windows/Linux library, companion, version/license/notices packaging closure.
- [x] Adapt model wrapper minimally to validated V6 API; real Windows subprocess inference validated 512 frames, finite probability, silence and reset. Full workspace passes.
- [ ] Linux native inference/package and clean Windows VM installation remain pending.
**Acceptance:** local Windows CPU process succeeds, missing/invalid fails without panic; Linux and installer/manual proof pending.

### RR-3 — RNNoise timing diagnosis only
**Files:** paced benchmark example/tests and RNNoise docs.
- [x] Add deterministic injected-duration diagnostics for deadline lateness, actual-ready offset and output source age; focused test RED/GREEN.
- [x] Retain pacing, original timestamp boundary, DSP, metric and ≤64 ms gate.
- [x] Record invalid single run and reason; source boundary restored; no second measurement.
- [ ] ≤64 ms gate remains open; any spectral follow-up needs actual reference signal and declared criteria. Do not change DSP/filter state without causal evidence.
**Acceptance:** instrumentation is present, but no valid new sample; performance objective not met or closed by inference.

### RR-4 — Legacy Python safe default
**Files:** `liveaudio/utils/config.py`, `tests/test_config.py`, `tests/test_gpu_detection.py`.
- [x] RED: fresh config was CUDA and saved CUDA was rewritten to CPU when hardware probe false.
- [x] Set shared new-config default CPU; remove config-load hardware rewrite; retain execution fallback independently.
- [x] Focused pytest: 108 passed, 24 subtests passed.
**Acceptance:** new/missing config defaults CPU, saved explicit CUDA is preserved, no adapter-name fake probe.

### RR-5 — Docs, CI, local verification, package handoff
**Files:** workflow, package configs/assets/notices, getting started, changelog, RNNoise docs, this task record.
- [x] Update getting-started/changelog/provenance/notices/RNNoise record and platform package closure CI. CI not run here.
- [x] Final `cargo test --workspace --offline --locked`: exit 0. `cargo test -p liveaudio-cli --offline --locked`: 9/9. `npm test --prefix desktop`: 178/178. Focused pytest: 108 plus 24 subtests. `node --check desktop/src/app.js`: exit 0. `cargo fmt --all -- --check`: exit 0. `git diff --check`: exit 0; only CRLF advisory warnings.
- [ ] NSIS build deferred to parent after terminal handoff/source review. No installer build or launch in this worker; cached tool availability/artifact not claimed.
- [ ] CI, clean VM, Linux package/launch, physical hardware and ≤64 ms RNNoise gate remain pending.

## Final state and handoff

- RR-1 and RR-2 implementation/local Windows checks complete; Linux clean-checkout/runtime evidence pending.
- RR-3 diagnostics complete, one sample invalidated by premature timestamp placement, original boundary restored; ≤64 ms open and no further run authorized.
- RR-4 implementation and focused tests complete.
- RR-5 docs/CI and local checks complete; parent owns any later cached offline NSIS build.
- Only `.gitignore` plus the verified Silero model are staged; model digest/blob/provenance above. Other feature and preexisting RNNoise/runtime changes remain unstaged/preserved. No commit, reset, memory write, GUI, VM, mic or installer launch.
- No Engram writes; memory mirror pending. Local Linux toolchain/CI, Linux package launch, clean Windows VM, hardware and RNNoise ≤64 ms acceptance are unproven. Invalid benchmark result must not be represented as a pass.
