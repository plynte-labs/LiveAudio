# Specification: VAD Onset Grace & Configurable Silero Pre-roll

## 1. Goal

Give a small, configurable amount of "grace" at the start of an utterance so that soft word onsets are not clipped when the Silero VAD transitions from silence to speech, WITHOUT materially increasing end-to-end latency.

Today, on the silence to speech transition, `vad_worker()` prepends a pre-roll buffer of only `maxlen=3` chunks (`liveaudio/core/audio.py:308`), i.e. `3 * 32ms = 96ms` of lead-in audio (`speech_buffer.extend(pre_buffer)` at `liveaudio/core/audio.py:353`). This maxlen is hardcoded and not configurable, and 96ms is too short to reliably capture the very first phoneme of quiet starts. The trailing tail is already preserved (silence chunks are appended to `speech_buffer` at `liveaudio/core/audio.py:376` before the cut), so the weak spot is the LEAD-IN (onset), not the tail.

Secondary goal: the VAD sensitivity threshold `VAD_THRESHOLD = 0.5` is a hardcoded module-level constant (`liveaudio/core/audio.py:22`) consumed at `liveaudio/core/audio.py:348`. A vestigial `vad_threshold` key is already passed (but ignored) in `tests/test_vad_error.py`, implying configurability that does not exist. Expose it as a real, clamped config key so users can tune sensitivity for soft/loud sources.

## 2. Requirements

### REQ-1 (P0): Configurable onset pre-roll via `vad_speech_pad_ms`
- Add config key `vad_speech_pad_ms` (integer milliseconds) to `DEFAULT_CONFIG` (`liveaudio/utils/config.py:59-86`), default `200`.
- `vad_worker()` MUST compute the `pre_buffer` deque maxlen from this key instead of the hardcoded `3`. Formula: `max(1, math.ceil(vad_speech_pad_ms / 1000 * SAMPLE_RATE / CHUNK_SIZE))`. At 200ms => `ceil(0.2 * 16000 / 512) = ceil(6.25) = 7` chunks (~224ms of real lead-in).
- The config read MUST happen in the config-read block alongside `silence_sec` / `max_sec` (`liveaudio/core/audio.py:194-199`), so the maxlen is computed once at thread entry, consistent with `SILENCE_CHUNKS_TO_END` / `MAX_CHUNKS_LIMIT`.
- The pre-roll mechanism (`extend(pre_buffer)` then `pre_buffer.clear()` at `liveaudio/core/audio.py:353-354`; `pre_buffer.append(audio_chunk)` in the silence branch at `liveaudio/core/audio.py:394`) MUST remain otherwise unchanged — only the maxlen source changes.

### REQ-2 (P0): Clamp + auto-migration for `vad_speech_pad_ms`
- Add a `_clamp_number(...)` call in `_normalize_config` (`liveaudio/utils/config.py:101+`) using the confirmed signature `_clamp_number(value, default, min_value, max_value, cast=float)` (`liveaudio/utils/config.py:89-98`).
- Clamp range `0`–`500` ms, `cast=int`, default `200`. A value of `0` is legal and means "no extra pre-roll beyond the single transition chunk" (maxlen floored to `1`).
- Existing `config.json` files MUST auto-fill the missing key on next `load_config()` via the existing fill loop (`liveaudio/utils/config.py:105-108`) — no bespoke migration code.

### REQ-3 (P1): Configurable VAD sensitivity via `vad_threshold`
- Add config key `vad_threshold` (float) to `DEFAULT_CONFIG`, default `0.5` (preserving current behavior).
- `vad_worker()` MUST read `vad_threshold` from config at thread entry (alongside REQ-1) and use a local variable in the comparison at `liveaudio/core/audio.py:348`, replacing the module constant `VAD_THRESHOLD`.
- The comparison MUST remain strictly-greater-than (`speech_prob > threshold`) to preserve the existing edge behavior (at exactly 0.5, the chunk is NOT speech). See `liveaudio/core/audio.py:348`.
- The module constant `VAD_THRESHOLD` (`liveaudio/core/audio.py:22`) MUST remain defined as the default fallback so existing imports in `tests/test_audio.py` and `tests/test_noise_detection.py` keep working.

### REQ-4 (P1): Clamp for `vad_threshold`
- Add a `_clamp_number` call clamping `vad_threshold` to `0.1`–`0.9`, `cast=float`, default `0.5`, then `round(value, 2)` (mirroring the rounding pattern used for `silence_timeout` at `liveaudio/utils/config.py:139`).

### REQ-5 (P1): Settings UI exposure
- Add a `CTkSlider` for `vad_speech_pad_ms` in the "Latency control" section (`liveaudio/app.py:768-783`), following the exact label + slider pattern used by `slider_silence` and `slider_max_dur`.
- Slider `from_`/`to` MUST align with the `_normalize_config` clamp bounds (`0`–`500`).
- Add a `CTkSlider` for `vad_threshold` with `from_=0.1`, `to=0.9`, same pattern.
- New slider values MUST be collected into the draft on apply (`liveaudio/app.py:1054-1055` block), and synced from config on profile load (`liveaudio/app.py:1104-1105` block).
- New i18n labels MUST be added for both sliders (mirroring `silence_detection` / `max_phrase_duration` keys) in both UI languages.

### REQ-6 (P0): Audio restart trigger
- Add `vad_speech_pad_ms` and `vad_threshold` to the `needs_audio_restart` key list in `_pending_restart_flags` (`liveaudio/app.py:1298`), because `vad_worker()` reads them only once at thread entry — changing them at runtime requires an audio-thread restart to take effect.

### REQ-7 (P1): Profile preset policy
- `PROFILE_PRESETS` (`liveaudio/app.py:170-223`) currently define `silence_timeout` and `max_chunk_duration` per preset but not the new keys. The two new keys MUST be excluded from the preset `values` dicts so they remain user-overridable defaults (the apply path at `liveaudio/app.py:1054-1055` only writes keys present in the draft; keys absent from a preset are preserved from existing config). This is the recommended default; see Open Decisions for the alternative.

## 3. Non-Functional Requirements

- NFR-1 (latency): Default behavior MUST NOT materially increase latency. The pre-roll only prepends already-captured silence/lead-in chunks at the moment of transition; it does not delay emission. Increasing pre-roll from 96ms to ~224ms adds audio to the FRONT of the utterance only, not to the time-to-first-subtitle path.
- NFR-2 (architecture): The Silero VAD MUST stay on CPU and the ring-buffer architecture (`RING_BUFFER_MAX_CHUNKS = 500`, `liveaudio/core/audio.py:27`) MUST remain intact.
- NFR-3 (backward compatibility): Existing `config.json` files without the new keys MUST load without error and behave identically to before EXCEPT for the intentional pre-roll increase (96ms -> ~224ms at the 200ms default). `silence_timeout` defaults are NOT changed by this track.
- NFR-4 (no source changes outside scope): Only `liveaudio/core/audio.py`, `liveaudio/utils/config.py`, `liveaudio/app.py`, the i18n source, and `tests/` are touched.

## 4. Acceptance Criteria

- AC-1 (REQ-1): With `vad_speech_pad_ms = 200`, `vad_worker()` constructs `pre_buffer` with maxlen `7` (`math.ceil(0.2*16000/512)`). Verified by a unit test that drives `vad_worker()` (or the extracted maxlen helper) and asserts the deque maxlen equals 7.
- AC-2 (REQ-1): With `vad_speech_pad_ms = 0`, the computed maxlen floors to `1` (never `0`). Verified by unit test on the maxlen formula.
- AC-3 (REQ-2): Loading a config dict that omits `vad_speech_pad_ms` results in the key present with value `200` after `_normalize_config`. Out-of-range values (e.g. `9999`, `-5`, `"abc"`) clamp to `500` / `0` / `200` respectively. Verified by `tests/test_config.py`.
- AC-4 (REQ-3): With `vad_threshold = 0.7`, `vad_worker()` uses `0.7` in the `speech_prob > threshold` comparison; a chunk with `speech_prob = 0.6` is NOT counted as speech while `0.8` is. Verified by unit test driving the comparison path.
- AC-5 (REQ-3): At `speech_prob == threshold` exactly, the chunk is NOT speech (strict `>` preserved). Verified by unit test.
- AC-6 (REQ-4): `vad_threshold` values outside `0.1`–`0.9` clamp into range and round to 2 decimals after `_normalize_config`. Verified by `tests/test_config.py`.
- AC-7 (REQ-5): The settings UI shows two new sliders in the Latency control section with ranges `0`–`500` and `0.1`–`0.9`; their values round-trip through draft collection and profile load. Verified manually + by any existing UI-logic test pattern.
- AC-8 (REQ-6): Changing only `vad_speech_pad_ms` or only `vad_threshold` in the draft causes `_pending_restart_flags` to return `needs_audio_restart = True`. Verified by unit test on `_pending_restart_flags`.
- AC-9 (REQ-7): Applying any preset does NOT clobber a user-set `vad_speech_pad_ms` / `vad_threshold` (keys absent from preset are preserved). Verified by unit test on the profile apply path or by manual check.
- AC-10 (regression): The full existing suite passes. `tests/test_config.py::TestNormalizeConfig::test_adds_missing_keys_with_defaults` (`tests/test_config.py:80-85`) automatically covers the two new keys; `tests/test_audio.py` threshold-constant tests and `tests/test_noise_detection.py` still pass because `VAD_THRESHOLD` remains defined.

## 5. Out of Scope

- Any app/stream warm-up grace period before VAD engages (interpretation (b) of "tiempo de gracia de inicio"). This track implements per-utterance onset pre-roll only. See Open Decisions.
- Changing the `silence_timeout` default or the trailing-silence cut logic (`liveaudio/core/audio.py:373-390`). The tail is already preserved; a `silence_timeout` nudge is explicitly deferred (NFR-1).
- Adaptive / probability-weighted pre-roll, dynamic threshold, or hysteresis (separate enter/exit thresholds).
- Adding the new keys to `PROFILE_PRESETS` values (unless the maintainer overrides Open Decision OD-2).
- Removing the vestigial `vad_threshold` key from `tests/test_vad_error.py` config dicts (handled as a test-coverage clarification, not a behavior change).

## 6. Open Decisions

- OD-1 — Meaning of "tiempo de gracia de inicio". DEFAULT: interpretation (a), per-utterance onset pre-roll, implemented by this track. Rationale: the only grace mechanism in the codebase is `pre_buffer` within a single silence->speech transition (`liveaudio/core/audio.py:305-308, 351-354, 394`); there is no app/stream warm-up grace anywhere. Interpretation (b) (a startup grace before VAD engages) is noted but NOT implemented; if the maintainer wants (b), it is a separate track.
- OD-2 — Default `vad_speech_pad_ms` value. RECOMMENDED: `200` ms (=> 7 chunks, ~224ms real lead-in). Rationale: roughly doubles the current 96ms onset window to catch soft starts while keeping the added front-padding small and latency-neutral (NFR-1). Conservative alternatives: `150` ms (5 chunks) or `250` ms (8 chunks).
- OD-3 — Expose `vad_threshold` at all. RECOMMENDED: yes (REQ-3/REQ-4), because a vestigial key already exists in tests and users with quiet mics benefit from lowering it. If the maintainer prefers minimal surface area, `vad_threshold` can be deferred and only REQ-1/REQ-2/REQ-5(pad)/REQ-6 shipped.
- OD-4 — `vad_threshold` clamp range. RECOMMENDED: `0.1`–`0.9` (avoids degenerate `0.0`/`1.0` that would make the VAD always-on or always-off). Default `0.5` preserves current behavior exactly.
- OD-5 — Preset integration. RECOMMENDED: exclude both keys from `PROFILE_PRESETS.values` (REQ-7) so switching presets never overrides the user's onset/threshold tuning. Alternative: add explicit per-preset values (e.g. lower pad for `fast`, higher for `quality`) — only if the maintainer wants presets to drive these knobs.

## 7. Track Coordination

- Shared file `liveaudio/utils/config.py` (`DEFAULT_CONFIG`, `_normalize_config`): touched by sibling config-adding tracks (e.g. `asr-language-separation_20260601`, `subtitle-style-system_20260509`). Add new keys at the end of `DEFAULT_CONFIG` and append new clamp blocks in `_normalize_config`; do NOT reorder existing keys to avoid merge conflicts.
- Shared file `liveaudio/app.py` (settings UI, draft collection, `_pending_restart_flags`, `PROFILE_PRESETS`): heavily touched by UI/profile tracks. Sequence this track AFTER any in-flight UI restructure of the Latency control section to avoid slider-layout conflicts; otherwise no hard ordering dependency.
- Shared file `liveaudio/core/audio.py`: primary file for this track; no current sibling track edits `vad_worker()`. If a concurrent ASR/audio track lands, re-confirm line numbers around `liveaudio/core/audio.py:194-199, 308, 348, 353-354, 394` before applying.
- The i18n source file: new label keys must not collide with existing keys; follow the same key-naming convention as `silence_detection` / `max_phrase_duration` and add the same set of keys to every supported UI language.
