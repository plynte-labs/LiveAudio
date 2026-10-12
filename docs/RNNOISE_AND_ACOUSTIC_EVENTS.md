# RNNoise and acoustic speech events

RNNoise is optional local noise suppression for captured audio. Acoustic speech events are optional metadata-only VAD start/end notifications. Both settings default off and take effect after the service restarts.

## Enable a feature

1. Open **Settings → Audio** and enable **Local RNNoise noise suppression** and/or **Acoustic speech metadata events**.
2. Confirm the service restart. Cancel leaves the saved and active settings unchanged; the selected option is persisted before restart.
3. Confirm the setting remains enabled after restart. If audio processing fails, capture stops and reports a capture error instead of silently switching back to raw audio.

The CLI reads the same `rnnoise_enabled` value from `config.json`. It can show bundled third-party notices with `liveaudio-cli --licenses`.

## RNNoise behavior

- Processing runs locally in the shared CPAL DSP worker, before the existing VAD and ASR path. Desktop and CLI use the same implementation; there is no second denoising queue or per-app DSP loop.
- The pinned pure-Rust `nnnoiseless` 0.5.2 dependency uses its built-in model. The model is compiled into the application; first run and normal operation do not download a model or start another runtime.
- Captured mono 16 kHz chunks are adapted to RNNoise's 48 kHz, 480-sample frames and returned to the existing 16 kHz VAD path. Frame state is retained across chunks, and the final real tail is drained on stream stop. This frame adaptation can add CPU work and up to the intrinsic 10 ms DSP-frame delay; measure a release build on the target machine before making performance claims.
- This is noise suppression, not acoustic echo cancellation (AEC). It has no playback reference and does not cancel or interrupt ASR requests.
- The capture worker uses nonblocking bounded sends. If processing fails, it marks capture failed and stops the pipeline; it does not bypass RNNoise with raw samples.
- The upsampler currently uses linear interpolation and the downsampler selects every third sample without an explicit low-pass filter. Spectral quality/aliasing has not been demonstrated; characterize it as a bounded follow-up rather than claiming spectral proof.

The bundled crate notice is included in the desktop installer as `THIRD_PARTY_NOTICES.md` and embedded in the CLI binary for `--licenses` output.

## Acoustic event behavior

When enabled, the app sends actual VAD transitions as standalone JSON text frames to every connected local WebSocket client. Events are transient: they are not persisted, logged with payloads, retried, or replayed to late/reconnected clients. A reconnect can therefore observe an end without having observed its start; clients should ignore unmatched ends and clear state when the attempt changes.

Example start:

```json
{"type":"speech_activity_start","event_version":1,"run_id":"<opaque>","attempt_id":1,"utterance_id":"<opaque>","seq":7,"monotonic_ms":1250}
```

Example end:

```json
{"type":"speech_activity_end","event_version":1,"run_id":"<same>","attempt_id":1,"utterance_id":"<same>","seq":8,"monotonic_ms":3420,"reason":"silence"}
```

| Field | Meaning |
| --- | --- |
| `type`, `event_version` | Start/end discriminator and schema version (`1`). |
| `run_id` | Opaque identifier for one service run. |
| `attempt_id` | Capture/VAD attempt; reconnecting a WebSocket does not change it. |
| `utterance_id` | Opaque ID shared by one start and its matching end. |
| `seq` | Run-scoped increasing event sequence. Gaps mean one or more events were not observed. |
| `monotonic_ms` | Attempt-relative monotonic time, not wall-clock time. |
| `reason` | End only: `silence`, `duration_limit`, `service_stopped`, `capture_error`, or `attempt_reset`. |

OpenCohost interruption or cancellation is not implemented. These events provide boundaries only; they do not change subtitle delivery, ASR behavior, or reconnect history.

## Offline and performance checks

Run the synthetic paced release benchmark locally (no microphone or ASR model is used):

```powershell
cargo run --release -p liveaudio-audio --example rnnoise_paced_benchmark --offline --locked
```

It processes 60 seconds of generated audio at 16 kHz in 32 ms paced chunks, reports DSP-processing real-time factor (processor wall time divided by 60 seconds of audio), verifies sample/sequence alignment and bounded output, and reports the maximum added staging delay relative to each bypass chunk's ready boundary. The targets are RTF `< 1` and added DSP staging delay `≤ 64 ms`; report the measured target CPU and command output, not a theoretical frame-delay estimate.

Earlier local release run (2026-10-10): Windows x86_64 on AMD Ryzen 7 5700G with Radeon Graphics; 60 seconds of paced synthetic input; processor wall time 0.370 s (RTF 0.0062), 960,000 output samples, at most one output chunk per call, and 61.316 ms measured against the proposed threshold. Preserve this as a historical local observation only: an independent repeat exceeded the threshold, so performance acceptance is OPEN, not proven.

Independent repeat (2026-10-10): 60 seconds of paced synthetic input, 960,000 samples, processor wall time 0.352 s (RTF 0.0059), maximum measured value 87.333 ms, command exit 1. The current metric includes future-paced input waits and scheduler oversleep; it is not pure algorithmic-stage latency or end-to-end capture latency. Actual CPU load was not measured, so no concurrency/CPU-load cause is established. Investigate the measurement/staging behavior before deciding whether a human-approved metric or threshold change is warranted. Do not report the ≤64 ms target as met based only on the earlier run.

Timing instrumentation now reports each input's lateness relative to its pacing deadline, actual ready offset, and output source age; deterministic tests inject offsets to keep those values distinct. One authorized run on 2026-10-11 reported 32.805 ms, but it sampled the input boundary before synthesizing each chunk and therefore did not preserve the prior metric boundary. That measurement is invalid and does not close the gate. The implementation now records readiness after input construction, matching the original boundary; no second paced run was performed, so performance acceptance remains open.

Final independent checks (2026-10-10): `cargo test --workspace --offline --locked` — 163 passed, 0 failed; `npm test --prefix desktop` — 166 passed; `node --check desktop/src/app.js`; `node --check desktop/src/runtime-settings.js`; `cargo fmt --all -- --check`; and `git diff --check` passed. Architecture and QA reviews returned scoped PASS. These results do not close performance acceptance or manual runtime checks.

Installed CLI and desktop runtime with networking disabled, physical microphone capture, and OBS/browser-source integration remain pending. No application was launched for these checks. LiveAudio functionality is implemented, but overall acceptance remains pending performance evidence and these manual checks.

For offline packaging evidence, build the ordinary CLI and desktop release artifacts without network access, verify the CLI's embedded notice and the Tauri resource mapping, then test the installed artifacts in a network-disconnected environment. Release CLI and desktop Rust targets build offline from the locked cache; `nnnoiseless` 0.5.2 and its built-in model compile into the application, and the notice is embedded in CLI source and mapped as a Tauri resource. A build alone does not prove that an installer runs offline; installed-artifact/network-disconnected execution remains unverified. The next step is to investigate the timing discrepancy, complete these manual runtime checks, then make a human-owned delivery decision. No commit, push, or PR has been created on the authorized feature branch.
