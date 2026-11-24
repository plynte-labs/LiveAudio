# Event log i18n and automatic CPU threads

Branch: `feat/i18n-log-auto-threads`, stacked on `feat/ct2-worker-release-cycle` (1d24e9d). Worktree: `E:\LiveAudio-worktrees\ct2-pin`.

## Objective

1. Event-log messages follow the UI language. Today the log mixes hardcoded Spanish lines in the frontend with fixed English text sent by Rust.
2. "CPU threads" gets an "Automatic" switch. While it is on, the slider is disabled and the app picks the thread count.

## Problem (verified)

- `desktop/src/app.js` ~2699-2707 hardcodes Spanish service start and stop lines, and ~2996 hardcodes `Estado: ${runningLabel} | ASR: ${asr_state}`. Neither uses `t()`.
- `desktop/src-tauri/src/commands.rs` ~368, ~1412 and ~2145 send English free text as `text` in worker progress events (`[starting_worker] Starting the ASR worker…`, `[ready] ASR runtime is ready.`). The frontend prints that text as is.
- `crates/liveaudio-core/src/config/mod.rs`: `cpu_threads: usize` defaults to half the cores (`default_cpu_threads`, ~124) and `validate` clamps it to `1..=max` (~526). There is no automatic mode. The UI slider is in the Advanced tab.

## Scope

- T1 i18n event log.
  - Replace the hardcoded frontend strings with `t()` keys in every locale in `desktop/src/i18n.js`.
  - The `asr_state` value (`starting`, `ready`, `listening`, `stopping`, `stopped`) is shown translated through keys.
  - For backend progress events, the frontend translates by the stable phase code (`starting_worker`, `ready`, and the others) and falls back to the backend text for unknown phases or for details such as error messages, model names and percentages.
  - Do not translate error details coming from Rust or the worker; they stay as raw diagnostics.
  - Grep `app.js` for other hardcoded log or status strings (Spanish or English) passed to `appendLogEntry` and convert them too.
- T2 automatic CPU threads.
  - Add a boolean config field, for example `cpu_threads_auto`. The default is `true` for new configs. An existing config without the field loads as auto only when its `cpu_threads` equals `default_cpu_threads()`; otherwise it loads as manual, which keeps a custom choice.
  - When it is on, the effective threads are `default_cpu_threads()` (half the logical cores, clamped), resolved at worker init.
  - UI: a switch "Automatic" next to "CPU threads" (reuse the existing toggle or slider-switch component in the Advanced tab). When it is on, the slider is disabled and shows the automatic value.
  - Keep the existing rule that threads are enabled only when the device is CPU. The switch is disabled together with the slider on GPU.
  - Wire the field through the config patch and schema (`config/patch.rs`), `ADVANCED_KEYS` and `advancedDefaults` in `runtime-settings.js`, the reset, the MCP config if it lists fields, and all i18n locales.

## Constraints

- TDD strict (RED, then GREEN). Runners: `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets`, `node desktop/test_frontend.js`, `node --test tests-js/*.mjs`.
- Generated artifacts are in English. Never commit `gen/schemas`, binaries, `dist` or `*.wav`. No AI attribution. No push.
- About 400 lines per task is advisory only.

## Tasks

- [x] T1 Translate the event log through i18n keys and backend phase codes. Route: delegated writer.
- [x] T2 Add the automatic CPU threads switch (config, worker resolution, UI, i18n). Route: delegated writer.
- [x] T3 Translate every setup/worker phase (downloading, error prefix) with a phase-coverage test, and show the real automatic thread count in the disabled slider. Route: delegated writer.

## Acceptance

- With the UI set to English, no Spanish line appears in the event log during start and stop (and the reverse for Spanish).
- With Automatic on, the slider is disabled and the worker gets half the logical cores. With it off, the slider value is used. Old configs load without errors.
- All four checks pass, and the Clippy warning set matches the base.

## Delivery

`ask-on-risk`. Forecast: about 300 lines.

## Progress / evidence

### T1 (done, see git log)
- Route: delegated writer (single writer). Helpers `setupLogLine` and `statusLogLine` in `desktop/src/runtime-settings.js`; 17 `log_*` keys in es and en; `app.js` uses them.
- Phases mapped by code: `starting`, `starting_worker`, `ready`. Others (`downloading`, `error`, worker phases) keep the backend text.
- RED: `desktop/test_frontend.js` T1 block failed with `runtimeSettings.setupLogLine is not a function`. GREEN: 932 passed, 0 failed.


### T2 (done, see git log)
- Route: delegated writer (single writer). `cpu_threads_auto` in `config/mod.rs` (serde default true, legacy fixup in `load_from_path`, `effective_cpu_threads()`), used by `worker_init_payload`; patch key added (schema derived); UI switch in Advanced; `ADVANCED_KEYS`, `advancedDefaults`, reset label, types, es/en keys, MCP doc, changelog.
- RED: Rust tests `cpu_threads_auto_defaults_on_and_legacy_configs_migrate_by_value`, `effective_cpu_threads_follows_the_auto_switch` and the extended init payload test failed to compile (no field or method); JS asserts failed (`check-cpu-threads-auto` missing, key and labels missing).
- GREEN: `cargo test --workspace --locked` 560 passed, 0 failed; `node desktop/test_frontend.js` 936 passed, 0 failed; `node --test tests-js/*.mjs` 170 passed; clippy warning set identical to base.

### T3 plan
- Phases emitted anywhere: `starting` (frontend), `starting_worker`, `ready`, `error` (commands.rs), `downloading` (worker download progress; text is the file label). Each maps to an i18n key; error is a translated prefix plus the raw detail. A test scans the sources for emitted phases and fails if one has no mapping. Extra asr_state values `fallback_cpu` and `stalled` get status keys.
- Automatic value: new field `default_cpu_threads` on `GpuBackendStatusDto` (get_gpu_backend_status), taken from the core's `default_cpu_threads()`. The slider shows it while Automatic is on. Save decision: the saved manual `cpu_threads` is kept in a variable, so saving with Automatic on never overwrites it with the automatic value.
- RED: JS `TypeError: Cannot read properties of undefined (reading 'replace')` (no `log_phase_error` key); Rust `no field default_cpu_threads on GpuBackendStatusDto`. GREEN: `desktop/test_frontend.js`, `cargo test --workspace --locked`, `node --test tests-js/*.mjs`; clippy warning set equal to base.
