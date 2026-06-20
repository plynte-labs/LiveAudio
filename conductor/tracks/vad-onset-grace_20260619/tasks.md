# Tasks: VAD Onset Grace & Configurable Silero Pre-roll

- [x] **T-1 (REQ-1):** Compute `pre_buffer` maxlen from `vad_speech_pad_ms` in `vad_worker()`
  - Given: `liveaudio/core/audio.py:308` hardcodes `pre_buffer = collections.deque(maxlen=3)` and `liveaudio/core/audio.py:194-199` is the config-read block.
  - When: the audio thread starts and reads config (`vad_speech_pad_ms`, default 200) and computes `PRE_BUFFER_CHUNKS = max(1, math.ceil(vad_speech_pad_ms / 1000 * SAMPLE_RATE / CHUNK_SIZE))`.
  - Then: `pre_buffer` is created with that maxlen (7 at the 200ms default); the `extend`/`clear`/`append` logic at `liveaudio/core/audio.py:353-354, 394` is unchanged.
  - Error Path: missing key -> `config.get("vad_speech_pad_ms", 200)` falls back to 200; `0` floors maxlen to 1 (never 0).
  - UI State: N/A (worker-internal).
  - OBS Behavior: soft word onsets are captured (~224ms lead-in) so the first syllable appears in subtitles instead of being clipped.

- [x] **T-2 (REQ-1):** Ensure `import math` present and update the pre-buffer comment
  - Given: `liveaudio/core/audio.py:305-307` comment claims a fixed "3 chunks * 32ms = 96ms".
  - When: T-1 introduces `math.ceil`.
  - Then: `import math` exists at module top; the comment states the lead-in is configurable via `vad_speech_pad_ms`.
  - Error Path: N/A (no logic change).
  - UI State: N/A.
  - OBS Behavior: N/A.

- [x] **T-3 (REQ-2):** Add `vad_speech_pad_ms` to `DEFAULT_CONFIG` and clamp in `_normalize_config`
  - Given: `DEFAULT_CONFIG` (`liveaudio/utils/config.py:59-86`) has no pre-roll key; `_clamp_number(value, default, min_value, max_value, cast=float)` exists at `liveaudio/utils/config.py:89-98`.
  - When: the key `"vad_speech_pad_ms": 200` is appended and a clamp block `_clamp_number(config.get("vad_speech_pad_ms"), 200, 0, 500, int)` is added after `liveaudio/utils/config.py:142-144`.
  - Then: `9999 -> 500`, `-5 -> 0`, non-numeric -> 200; missing key auto-fills via `liveaudio/utils/config.py:105-108`.
  - Error Path: bad type returns default 200 with `updated=True`.
  - UI State: N/A (config layer).
  - OBS Behavior: N/A.

- [x] **T-4 (REQ-3):** Make `vad_threshold` a real config key read by `vad_worker()`
  - Given: `liveaudio/core/audio.py:22` defines `VAD_THRESHOLD = 0.5` and `liveaudio/core/audio.py:348` uses `speech_prob > VAD_THRESHOLD`.
  - When: the config-read block (`liveaudio/core/audio.py:194-199`) adds `vad_threshold = config.get("vad_threshold", VAD_THRESHOLD)` and `liveaudio/core/audio.py:348` becomes `if speech_prob > vad_threshold:`.
  - Then: at `vad_threshold = 0.7`, `speech_prob = 0.6` is not speech and `0.8` is; the module constant `VAD_THRESHOLD` remains defined as the default fallback.
  - Error Path: missing key -> falls back to `VAD_THRESHOLD` (0.5).
  - UI State: N/A (worker-internal).
  - OBS Behavior: lower threshold picks up quieter speech; higher threshold suppresses soft/background speech.

- [x] **T-5 (REQ-3):** Preserve strict greater-than comparison
  - Given: existing boundary semantics at `liveaudio/core/audio.py:348` use strict `>`.
  - When: the operand changes from constant to local variable.
  - Then: the operator stays `>` (a chunk with `speech_prob == vad_threshold` is NOT speech).
  - Error Path: N/A.
  - UI State: N/A.
  - OBS Behavior: identical boundary behavior at the default 0.5 to the prior build.

- [x] **T-6 (REQ-4):** Add `vad_threshold` to `DEFAULT_CONFIG` and clamp/round in `_normalize_config`
  - Given: no `vad_threshold` key in `DEFAULT_CONFIG`.
  - When: `"vad_threshold": 0.5` is appended and `_clamp_number(config.get("vad_threshold"), 0.5, 0.1, 0.9, float)` then `round(value, 2)` is added.
  - Then: `1.5 -> 0.9`, `0.0 -> 0.1`, non-numeric -> 0.5; result rounded to 2 decimals.
  - Error Path: bad type returns default 0.5 with `updated=True`.
  - UI State: N/A (config layer).
  - OBS Behavior: N/A.

- [x] **T-7 (REQ-5):** Add pre-roll and threshold sliders to the Latency control UI
  - Given: existing sliders at `liveaudio/app.py:768-783` (`slider_silence`, `slider_max_dur`).
  - When: two new `CTkSlider`s are added — `slider_vad_pad` (`from_=0, to=500`) and `slider_vad_threshold` (`from_=0.1, to=0.9`) — with labels and the same `pack` pattern, `.set(...)` from `config_data`, `command=self.on_setting_change`.
  - Then: both sliders render in the Latency control section with ranges matching the `_normalize_config` clamps.
  - Error Path: if `config_data` lacks a key, normalize has already filled it (T-3/T-6), so `.set` always has a value.
  - UI State: user sees two new labeled sliders under "Latency control"; moving them marks settings dirty via `on_setting_change`.
  - OBS Behavior: N/A until applied (then via T-9 restart).

- [x] **T-8 (REQ-5):** Wire draft collection and profile-load sync for the new sliders
  - Given: draft collection at `liveaudio/app.py:1054-1055` and slider sync at `liveaudio/app.py:1104-1105`.
  - When: `draft["vad_speech_pad_ms"] = int(round(slider_vad_pad.get()))`, `draft["vad_threshold"] = round(slider_vad_threshold.get(), 2)` are added; and `slider_vad_pad.set(config["vad_speech_pad_ms"])`, `slider_vad_threshold.set(config["vad_threshold"])` are added on load.
  - Then: slider values round-trip config -> UI -> draft -> config.
  - Error Path: N/A (keys guaranteed by normalize).
  - UI State: switching profile updates both sliders to the loaded config values.
  - OBS Behavior: N/A.

- [x] **T-9 (REQ-6):** Add new keys to the audio-restart trigger list
  - Given: `_pending_restart_flags` (`liveaudio/app.py:1298`) lists `["audio_device", "silence_timeout", "max_chunk_duration"]` for `needs_audio_restart`.
  - When: `"vad_speech_pad_ms"` and `"vad_threshold"` are appended to that list.
  - Then: changing either key in the draft returns `needs_audio_restart = True`, so the audio thread restarts and re-reads the values (they are read once at thread entry).
  - Error Path: if not added, the slider would appear inert until next launch — this task prevents that.
  - UI State: applying a changed value shows the audio-restart status feedback.
  - OBS Behavior: new pre-roll/threshold take effect on the next utterance after restart.

- [x] **T-10 (REQ-5):** Add i18n labels for both new sliders in all UI languages
  - Given: existing keys `silence_detection` / `max_phrase_duration` used at `liveaudio/app.py:772, 779`.
  - When: new label keys (and optional help text) are added to the i18n source for every supported UI language and wired via `t(...)` in T-7.
  - Then: both sliders show localized labels; no missing-key fallback appears.
  - Error Path: a missing language entry would surface the raw key — add to every language to avoid this.
  - UI State: localized slider labels render correctly in each UI language.
  - OBS Behavior: N/A.

- [x] **T-11 (REQ-7):** Keep new keys out of `PROFILE_PRESETS` (default policy)
  - Given: `PROFILE_PRESETS` (`liveaudio/app.py:170-223`) define `silence_timeout`/`max_chunk_duration` but not the new keys; apply path (`liveaudio/app.py:1054-1055`) writes only draft keys.
  - When: presets are left WITHOUT `vad_speech_pad_ms`/`vad_threshold` in their `values` dicts (per OD-5 default).
  - Then: applying any preset preserves the user's existing pre-roll/threshold values.
  - Error Path: N/A.
  - UI State: switching presets does not reset the two new sliders.
  - OBS Behavior: N/A.

- [x] **T-12 (REQ-2, REQ-4):** Config tests for clamps and migration
  - Given: `tests/test_config.py` with `TestNormalizeConfig` (incl. `test_adds_missing_keys_with_defaults`, `tests/test_config.py:80-85`).
  - When: tests assert default values present after normalize and clamp behavior for both keys (pad: 9999/-5/"abc"; threshold: 1.5/0.0/non-numeric + 2dp rounding).
  - Then: all new assertions pass and the existing missing-keys test now also covers the two new keys.
  - Error Path: a failing clamp assertion blocks the change.
  - UI State: N/A.
  - OBS Behavior: N/A.

- [x] **T-13 (REQ-1, REQ-3):** VAD-layer tests for maxlen formula and threshold comparison
  - Given: `tests/test_audio.py` and `tests/test_noise_detection.py` currently only import `VAD_THRESHOLD` for arithmetic; `vad_worker()` is not exercised.
  - When: tests assert the maxlen formula (`200 -> 7`, `0 -> 1`, `100 -> 4`), the strict `>` threshold comparison (`0.6 > 0.7` False, `0.8 > 0.7` True, `0.7 > 0.7` False), and that `VAD_THRESHOLD` is still importable and equals 0.5.
  - Then: AC-1, AC-2, AC-4, AC-5, AC-10 are covered. Prefer extracting the maxlen formula into a small pure helper to keep it unit-testable.
  - Error Path: a failing formula/comparison assertion blocks the change.
  - UI State: N/A.
  - OBS Behavior: N/A.

- [x] **T-14 (REQ-6):** Test `_pending_restart_flags` returns audio restart for new keys
  - Given: `_pending_restart_flags` (`liveaudio/app.py:1298`).
  - When: a test builds `config_data` and a `draft` differing only in `vad_speech_pad_ms` (and separately only in `vad_threshold`).
  - Then: `needs_audio_restart` is True in both cases; `needs_asr_restart` is False.
  - Error Path: a False result indicates the key was not added to the list (blocks the change).
  - UI State: N/A.
  - OBS Behavior: N/A.

- [x] **T-15 (REQ-3):** Reconcile vestigial `vad_threshold` in `tests/test_vad_error.py`
  - Given: `tests/test_vad_error.py` (~lines 16-78) passes `"vad_threshold": 0.5` in config dicts that `audio_producer` previously ignored.
  - When: `vad_threshold` becomes a real, consumed key (T-4).
  - Then: the two tests are reviewed so the now-active behavior is intentional (keep 0.5 to preserve prior behavior, or assert the configurable behavior explicitly).
  - Error Path: leaving them unreviewed risks an accidental behavior change going untested.
  - UI State: N/A.
  - OBS Behavior: N/A.

- [x] **T-16 (REQ-1, REQ-3):** Update `make_shared_config()` test helper
  - Given: `tests/helpers.py` `make_shared_config()` (~lines 41-61) lacks the two new keys.
  - When: `vad_speech_pad_ms` and `vad_threshold` are added to the shared fixture (or documented as relying on `config.get` defaults).
  - Then: tests exercising `vad_worker()` via the helper get explicit, deterministic values.
  - Error Path: tests relying on defaults remain safe (the worker reads via `config.get` with defaults).
  - UI State: N/A.
  - OBS Behavior: N/A.
