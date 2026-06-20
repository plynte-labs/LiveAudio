# Implementation Plan: VAD Onset Grace & Configurable Silero Pre-roll

## 1. Design Decisions

### Why a millisecond config key instead of a raw chunk count
The user-facing unit is time (ms), not chunks; chunks are an implementation detail of `CHUNK_SIZE = 512` (`liveaudio/core/audio.py:21`). Exposing `vad_speech_pad_ms` keeps the config stable if `CHUNK_SIZE` ever changes, and matches the mental model "how many milliseconds of lead-in to keep". The chunk maxlen is derived: `max(1, math.ceil(vad_speech_pad_ms / 1000 * SAMPLE_RATE / CHUNK_SIZE))`. We use `ceil` (not `round`/`floor`) so the configured pad is a guaranteed lower bound on captured lead-in — never less than the user asked for. The `max(1, ...)` floor guarantees the pre-roll deque is never created with maxlen 0 (which would silently disable lead-in capture even though the silence branch at `liveaudio/core/audio.py:394` would still try to append).

### Why compute maxlen once at thread entry (not per chunk)
`SILENCE_CHUNKS_TO_END` and `MAX_CHUNKS_LIMIT` are already computed once at `vad_worker()` entry (`liveaudio/core/audio.py:198-199`). Computing the pre-roll maxlen and reading `vad_threshold` in the same `liveaudio/core/audio.py:194-199` block keeps a single, consistent "config snapshot at thread start" model. The cost is that runtime changes require an audio-thread restart — which is exactly why REQ-6 adds these keys to `_pending_restart_flags` (`liveaudio/app.py:1298`). Rejected alternative: re-reading config every chunk (adds per-chunk dict lookups on the hot path for no real benefit, since the user changes these via a settings-apply that already restarts the audio thread).

### Why keep the `VAD_THRESHOLD` module constant
`tests/test_audio.py` (TestVadThresholdEnforcement) and `tests/test_noise_detection.py` import `VAD_THRESHOLD` for arithmetic assertions. Keeping the constant as the documented default and using it as the `config.get("vad_threshold", VAD_THRESHOLD)` fallback means: (1) existing tests keep compiling and passing, (2) there is a single source of truth for the default, (3) no behavior change at the default value. Rejected alternative: delete the constant — would break two test modules for no gain.

### Why preserve strict greater-than
`liveaudio/core/audio.py:348` uses `speech_prob > VAD_THRESHOLD` (strict). Switching to a configurable threshold must NOT silently change this to `>=`, or the boundary semantics shift for every existing user at the default 0.5. We replace only the operand (constant -> local var), not the operator.

### Why exclude new keys from presets (default)
The apply path (`liveaudio/app.py:1054-1055`) writes only keys present in the draft; preset application merges preset `values` over the draft. Keys absent from a preset's `values` are preserved from the user's existing config. Excluding `vad_speech_pad_ms`/`vad_threshold` from presets therefore makes them "global user tuning" that survives preset switches — the least surprising behavior. Adding per-preset values is possible (OD-5) but couples a sensitivity knob to latency presets, which the maintainer should opt into explicitly.

### Latency neutrality argument
The pre-roll only prepends chunks that were ALREADY captured into `pre_buffer` during silence (`liveaudio/core/audio.py:394`). At the silence->speech transition (`liveaudio/core/audio.py:351-354`) they are moved to the front of `speech_buffer`. This adds audio to the FRONT of the utterance; it does not delay when the utterance is emitted (emission is driven by `SILENCE_CHUNKS_TO_END` / `MAX_CHUNKS_LIMIT`, unchanged). Hence increasing the pad from 96ms to ~224ms is latency-neutral for time-to-first-subtitle.

## 2. Implementation Steps

### Step 1 — Config defaults and clamps (`liveaudio/utils/config.py`)
- In `DEFAULT_CONFIG` (`liveaudio/utils/config.py:59-86`), append `"vad_speech_pad_ms": 200,` and `"vad_threshold": 0.5,` at the end of the dict (do not reorder existing keys — see Track Coordination).
- In `_normalize_config` (`liveaudio/utils/config.py:101+`), after the `max_chunk_duration` clamp block (`liveaudio/utils/config.py:142-144`), add:
  - `vad_speech_pad_ms, changed = _clamp_number(config.get("vad_speech_pad_ms"), DEFAULT_CONFIG["vad_speech_pad_ms"], 0, 500, int)` then `config["vad_speech_pad_ms"] = vad_speech_pad_ms; updated = updated or changed`.
  - `vad_threshold, changed = _clamp_number(config.get("vad_threshold"), DEFAULT_CONFIG["vad_threshold"], 0.1, 0.9, float)` then `config["vad_threshold"] = round(vad_threshold, 2); updated = updated or changed`.
- The existing missing-key fill loop (`liveaudio/utils/config.py:105-108`) handles migration automatically.

### Step 2 — VAD worker consumes config (`liveaudio/core/audio.py`)
- Ensure `import math` is present at the top of the module (add if missing).
- In the config-read block (`liveaudio/core/audio.py:194-199`), add:
  - `vad_threshold = config.get("vad_threshold", VAD_THRESHOLD)`
  - `vad_speech_pad_ms = config.get("vad_speech_pad_ms", 200)`
  - `PRE_BUFFER_CHUNKS = max(1, math.ceil(vad_speech_pad_ms / 1000 * SAMPLE_RATE / CHUNK_SIZE))`
- Change `pre_buffer = collections.deque(maxlen=3)` (`liveaudio/core/audio.py:308`) to `pre_buffer = collections.deque(maxlen=PRE_BUFFER_CHUNKS)`. Update the adjacent comment (`liveaudio/core/audio.py:305-307`) to reflect that the lead-in is now configurable.
- Change the comparison at `liveaudio/core/audio.py:348` from `if speech_prob > VAD_THRESHOLD:` to `if speech_prob > vad_threshold:` (keep strict `>`).
- Leave `liveaudio/core/audio.py:353-354` (`extend`/`clear`) and `liveaudio/core/audio.py:394` (`append`) unchanged.

### Step 3 — Settings UI sliders (`liveaudio/app.py`)
- In the Latency control section (`liveaudio/app.py:768-783`), after `slider_max_dur`, add:
  - `self.lbl_vad_pad` + `self.slider_vad_pad = ctk.CTkSlider(tab_audio, from_=0, to=500, command=self.on_setting_change)`, `.set(self.config_data["vad_speech_pad_ms"])`.
  - `self.lbl_vad_threshold` + `self.slider_vad_threshold = ctk.CTkSlider(tab_audio, from_=0.1, to=0.9, command=self.on_setting_change)`, `.set(self.config_data["vad_threshold"])`.
  - Use the same `pack` layout as the existing two sliders.
- In `_read_ui_config` (`liveaudio/app.py:1054-1055` block), add `draft["vad_speech_pad_ms"] = int(round(self.slider_vad_pad.get()))` and `draft["vad_threshold"] = round(self.slider_vad_threshold.get(), 2)`.
- In `_load_ui_from_config` (`liveaudio/app.py:1104-1105` block), add `self.slider_vad_pad.set(config["vad_speech_pad_ms"])` and `self.slider_vad_threshold.set(config["vad_threshold"])`.

### Step 4 — Restart trigger (`liveaudio/app.py`)
- In `_pending_restart_flags` (`liveaudio/app.py:1298`), extend the `needs_audio_restart` key list to `["audio_device", "silence_timeout", "max_chunk_duration", "vad_speech_pad_ms", "vad_threshold"]`.

### Step 5 — i18n labels (i18n source)
- Add label keys for the two new sliders (e.g. `vad_speech_pad`, `vad_threshold_label`, plus optional help text) mirroring `silence_detection` / `max_phrase_duration`, for every supported UI language. Wire the labels via `t(...)` in Step 3, matching the existing `lbl_silence` / `lbl_max_dur` usage (`liveaudio/app.py:772, 779`).

### Step 6 — Tests (see Test Strategy)
- Add/extend tests for the maxlen formula, threshold comparison, config clamps/migration, and restart-flag detection.

## 3. Risks & Mitigations

- RISK: Increasing default pre-roll bleeds prior noise/breath into the utterance front, hurting ASR. MITIGATION: keep default conservative (200ms => ~224ms); the pad is bounded by what `pre_buffer` captured during silence; allow users to lower via the slider; clamp floor 0.
- RISK: `maxlen=0` if formula not floored. MITIGATION: `max(1, ...)` floor (AC-2) plus unit test.
- RISK: Operator drift from `>` to `>=` when refactoring the threshold line, changing boundary behavior for all users. MITIGATION: explicit AC-5 test asserting strict `>`.
- RISK: Forgetting the restart trigger leaves the slider apparently inert until next app launch. MITIGATION: REQ-6 + AC-8 unit test on `_pending_restart_flags`.
- RISK: Reordering `DEFAULT_CONFIG` keys causes merge conflicts with sibling config tracks. MITIGATION: append-only, documented in Track Coordination.
- RISK: i18n key collision or missing a language. MITIGATION: add identical key set to every language; follow existing key naming.
- RISK: `tests/test_vad_error.py` already passes an (ignored) `vad_threshold`; making it real changes that test's effective behavior. MITIGATION: review those two tests (`tests/test_vad_error.py` ~lines 16-78) and assert the now-active behavior intentionally, or set the key to 0.5 to preserve prior behavior.

## 4. Test Strategy

### Unit tests to add/extend
- `tests/test_config.py` — extend `TestNormalizeConfig`:
  - new key present with default after normalize (auto-covered by `test_adds_missing_keys_with_defaults`, `tests/test_config.py:80-85`; add explicit value assertions).
  - `vad_speech_pad_ms` clamps `9999 -> 500`, `-5 -> 0`, `"abc" -> 200` (AC-3).
  - `vad_threshold` clamps `1.5 -> 0.9`, `0.0 -> 0.1`, non-numeric -> `0.5`, rounds to 2dp (AC-6).
- `tests/test_audio.py` — add a small helper or test for the maxlen formula: assert `200 -> 7`, `0 -> 1`, `100 -> 4` (AC-1, AC-2). Prefer extracting the formula into a tiny pure helper in `liveaudio/core/audio.py` so it is unit-testable without spinning up `vad_worker()` (the existing tests do not exercise `vad_worker()`).
- `tests/test_audio.py` / `tests/test_noise_detection.py` — confirm `VAD_THRESHOLD` constant still importable and equal to 0.5 (regression, AC-10).
- threshold comparison test (AC-4, AC-5): assert `0.6 > 0.7` is False, `0.8 > 0.7` is True, `0.7 > 0.7` is False — via the helper/local-var path used by `vad_worker()`.
- `tests/test_vad_error.py` — review the two tests passing `vad_threshold` (~lines 16-78); make the now-active behavior intentional.
- `_pending_restart_flags` test (AC-8): construct `config_data` and a `draft` differing only in `vad_speech_pad_ms` (and separately `vad_threshold`); assert `needs_audio_restart` is True.
- `tests/helpers.py` `make_shared_config()` (~lines 41-61): add the two new keys so any test exercising `vad_worker()` has them; tests relying on defaults remain safe.

### Manual verification (OBS / live stream)
- Start a session, speak a soft-onset word (e.g. a quiet "hello"/"hola") at default 200ms; confirm in OBS subtitles that the first syllable is no longer clipped vs. the prior 96ms build.
- Open settings, move the pre-roll slider to 0, apply; confirm the audio thread restarts (status feedback) and that very soft onsets clip again — proving the knob is live.
- Move `vad_threshold` to 0.3 with a quiet mic; confirm more sensitive pickup; move to 0.8; confirm it ignores soft/background speech. Confirm at default 0.5 behavior is unchanged from the prior build.
- Switch between presets (fast/balanced/quality/stable_streaming) and confirm the pre-roll/threshold sliders are NOT reset (AC-9).
