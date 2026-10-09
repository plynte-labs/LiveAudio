# LiveAudio Migration: Historical Benchmark Scope (WU9)

## Status and interpretation

This report preserves measurements recorded in the original WU9 report. They are historical observations, not a fresh run of the corrected migration, a clean-machine validation, or proof of release readiness. The measurements were produced by different implementations and process scopes; deltas below must not be read as controlled end-to-end comparisons. No replacement metrics were collected for this correction.

The migration remains hybrid: Rust owns application control, audio/VAD orchestration, WebSocket behavior, and worker supervision; the existing Python worker still imports Faster-Whisper/PyTorch and performs ASR inference. The Rust executable alone is therefore not the complete ASR runtime.

## Previously recorded measurements

| Dimension | Python baseline recorded in WU9 | Rust migration result recorded in WU9 | Scope qualification |
| --- | ---: | ---: | --- |
| Startup | 2,811.6 ms | 8.0 ms bare CLI; 16.9 ms `doctor` | Bare CLI and diagnostics timings are not full desktop launch or ASR-ready time. `doctor` did not prove dependency imports on a clean install. |
| Resident memory | ~958.6 MB | ~28 MB Rust core; ~73 MB core plus idle worker | Historical estimates describe different component sets. Active ASR runtime/model memory is not represented by the idle figure. |
| VAD frame time | 0.358 ms PyTorch | 0.114 ms Rust ONNX release | Different implementations and runtime stacks; useful as reported component measurements, not a controlled end-to-end speedup. |
| ASR inference (`small`) | ~350–450 ms | 280.6 ms CUDA; 2,416.0 ms CPU int8 | Inference ran in the Python Faster-Whisper worker; the Rust migration does not implement ASR inference. The report used a 7.22-second synthetic phrase on an RTX 3060 for the GPU case. |
| WER | Reference phrase | 13.33% CUDA and CPU | A single reported synthetic phrase is not a general accuracy benchmark. |
| Shutdown | 3,401.0 ms | 6.64 ms (five iterations) | Historical supervisor harness result only; it does not establish a universal shutdown bound or zero-orphan guarantee for every failure mode. |
| WebSocket | Protocol v1 | Golden-schema and security tests | Tests cover the exercised message/origin cases, not every client or deployment configuration. |

The original WU9 test totals (86 Rust and 22 Python tests) and portable-package phases are also historical snapshot results. They do not describe the corrected workspace's current test count, prove a clean-VM installation, or establish that a release artifact contains a working Python/ASR runtime.

## Runtime and distribution caveat

The Rust desktop/CLI requires a compatible Python 3.11 runtime, the staged `liveaudio` worker package, and the ASR dependencies described in [Rust ASR Runtime Requirements](rust-runtime-requirements.md). `setup-runtime` provisions those dependencies only after an explicit backend choice. Startup performs an import preflight and does not download or install packages. A clean-VM installation and first-run provisioning have not been executed as part of this report.

Do not describe the Rust binary or a Tauri bundle as self-contained or offline-ready unless a separate artifact-level check proves that the Python interpreter, worker package, selected dependency backend, and required native libraries are actually present.
