# Track i18n-completeness_20260905 — Specification

> Status: **exploratory proposal, PO approval pending — NOT approved, NOT implemented.**
> Source: read-only exploratory audit (Front 3). Every requirement below is PROPOSED.

## Outcome

The ES/EN UI has no silent no-ops or untranslated leftovers on the audited paths: the
stable-streaming preset applies in both languages, every user-visible string resolves
through `t()`, and dynamic status events use keys — not reverse-matched literals.

## Quick path

1. Read the Findings table — the `stable_streaming` silent no-op is the blocking row.
2. Confirm the key mismatch still holds on the implementation base commit.
3. PO approves/rejects per-ticket Acceptance Criteria before any implementation track.

## Overview

Coverage looks healthy at the aggregate level — 138 keys per locale, zero missing
literal `t()` calls, zero placeholder mismatches — but the `stable_streaming` preset
selector is a silent no-op in BOTH languages: the locale files define
`profile_preset_stable_label` / `profile_preset_stable_desc`
(verified: ES `liveaudio/utils/i18n.py:166-167`, EN `:343-344`), while the dispatch in
`on_profile_select` iterates pid `stable_streaming` and builds the nonexistent key
`profile_preset_stable_streaming_label` (verified: `liveaudio/app.py:1342-1343` inside
`on_profile_select` at `:1331-1353`), so `profile_id` stays `None` and the method
returns without applying. The matched-refresh path (`refresh_profile_status`,
`app.py:1305-1329`, verified) exposes the same missing keys. An AST-extracted
reproduction of the real `on_profile_select` with mocks confirmed: Fast applies,
`stable_streaming` applies in NEITHER ES nor EN — no GUI/models/files needed.
Additional gaps: model descriptions, folder dialog, validation errors, and the
anti-hallucination label stay in Spanish; diagnostics button/dialog stay in English;
dynamic status events (download percent, transcribing, OBS client counts) bypass
translation via exact-Spanish-string reverse-match. The language selector lives only
in welcome; its callback ignores `save_config=False`. UI auto-detect is independent
of the ASR default and hidden under Performance until explicitly changed. Existing
coverage (not executed in the audit): `tests/test_asr_language.py`.

## Functional Requirements (PROPOSED — not approved)

- **REQ-1 (PROPOSED):** Selecting the stable-streaming preset MUST apply its values
  in ES and EN (fix the key mismatch in EITHER the locale keys OR the dispatch pid —
  PO picks the direction; either way both locales + dispatch + refresh agree).
- **REQ-2 (PROPOSED):** `refresh_profile_status` MUST resolve every preset label/desc
  key it can display; no matched-refresh path may reference a nonexistent key
  (`app.py:1305-1329`).
- **REQ-3 (PROPOSED):** Model descriptions, folder dialog, validation errors, and the
  anti-hallucination label MUST resolve through `t()` in both locales (no
  Spanish-only remainders).
- **REQ-4 (PROPOSED):** Diagnostics button/dialog copy MUST resolve through `t()` in
  both locales (no English-only remainders).
- **REQ-5 (PROPOSED):** Dynamic status events (download percent, transcribing state,
  OBS client counts) MUST use parameterized locale keys — the exact-Spanish-string
  reverse-match bypass MUST be replaced on the audited paths
  (`app.py:1693-1718`, `engine.py:367,539`, `network.py:227,243`, `i18n.py:387-394`).
- **REQ-6 (PROPOSED):** The language-selector callback MUST honor `save_config=False`
  (no write when the caller asked for none); the welcome-only placement and the
  Performance-hidden UI-auto-detect-vs-ASR-default relationship MUST be a PO-signed
  UX decision (copy/placement), not a silent behavior.
- **REQ-7 (PROPOSED):** A locale-key consistency probe (every constructed
  `profile_preset_<pid>_label/desc` key exists in both locales; no reverse-matched
  literals on the audited status paths) MUST run in CI so this class cannot regress.

## Non-Functional Requirements

- No new locale may ship with fewer keys than the reference locale; the probe enforces parity.
- Placeholder order/count per key must match across locales (already zero-mismatch — keep it).
- Reproduction pattern stays AST-extracted + mocked (no GUI) for selector/dispatch logic.
- Privacy: sanitized technical summaries only.

## Acceptance Criteria

### T1 — stable_streaming applies in ES and EN (REQ-1, REQ-2, REQ-7)

- Given: UI language ES, then EN; the preset dropdown showing the stable option
- When: the user selects stable-streaming in each language
- Then: the preset values apply (same resulting config in both languages); refresh shows the matching label + desc with no missing-key fallback
- Error Path: any still-missing key renders a visible `missing:<key>` marker in dev/diagnostics, never a silent no-op
- UI State: dropdown label + description update immediately in the active language
- OBS Behavior: unchanged (preset switch follows existing restart semantics)

### T2 — No Spanish-only remainders (REQ-3)

- Given: UI language EN
- When: visiting model descriptions, folder dialog, validation errors, anti-hallucination label
- Then: every string on those surfaces renders in English; zero Spanish literals
- Error Path: validation failures still show the English message + preserve the blocking behavior
- UI State: dialogs and labels fully English; layout holds for longer EN strings
- OBS Behavior: unchanged

### T3 — No English-only remainders + dynamic events keyed (REQ-4, REQ-5)

- Given: UI language ES
- When: opening diagnostics UI and observing download-percent / transcribing / OBS-client-count events
- Then: diagnostics copy renders in Spanish; dynamic events render via parameterized keys with correct numbers in both languages
- Error Path: a missing dynamic key falls back to the keyed English string + `missing:<key>` diagnostics marker, never a crash or a frozen label
- UI State: progress and counts update live in the active language
- OBS Behavior: OBS client counts shown in UI match actual connections (no display-only divergence)

### T4 — Selector callback honesty + UX decisions signed (REQ-6)

- Given: language selector invoked with `save_config=False`
- When: the user changes language
- Then: no config file write occurs; the welcome-only placement and the hidden UI-auto-detect-vs-ASR-default relationship ship PO-signed copy
- Error Path: a write attempt under `save_config=False` is a blocking defect, not a warning
- UI State: language applies to the session immediately per current semantics; persistence only when requested
- OBS Behavior: unchanged

## Out of Scope (explicitly NOT proven by the audit — do not claim)

- Full third-language support or new locales.
- Restructuring settings tabs or moving the language selector (placement is a PO UX decision).
- Changing ASR default-language semantics (only the UI-vs-ASR independence needs a signed decision).
- Rewriting the i18n framework (`i18n.py` architecture stays).

## Findings

| File:line | Evidence | Severity | Blocking? |
|---|---|---|---|
| `liveaudio/app.py:1342-1343` (method `:1331-1353`) | Dispatch iterates `["fast","balanced","quality","stable_streaming"]` and builds `t(f"profile_preset_{pid}_label")` → nonexistent `profile_preset_stable_streaming_label`. Verified verbatim on `develop @ e004ae3`. | High | Yes (for T1) |
| `liveaudio/utils/i18n.py:166-167` (ES), `:343-344` (EN) | Locales define `profile_preset_stable_label/desc` — the key the dispatch never builds. Verified verbatim. | High | Yes (for T1) |
| `liveaudio/app.py:1305-1329` | `refresh_profile_status` builds `t(f"profile_preset_{profile_id}_label/desc")` — same missing-key exposure on the refresh path. Verified verbatim. | Medium | No (T1 covers) |
| `liveaudio/app.py:665-684,835-847,920,1093,1311-1312,1342-1348,1502-1508,1693-1718` | Additional gap sites cited by audit (model descs, folder dialog, validation errors, anti-hallucination label, diagnostics UI, dynamic status). Auditor-verified; re-verify exact lines before implementation. | Medium | No (T2/T3 cover) |
| `liveaudio/utils/i18n.py:387-394` | Reverse-match helper region cited by audit (exact-Spanish-string bypass). Auditor-verified; re-verify before implementation. | Medium | No (T3 covers) |
| `liveaudio/core/engine.py:367,539`; `liveaudio/core/network.py:227,243` | Dynamic status emission sites bypassing translation. Auditor-verified; re-verify before implementation. | Medium | No (T3 covers) |
| Selector callback + auto-detect | Welcome-only selector; callback ignores `save_config=False`; UI auto-detect independent of ASR default, hidden under Performance. Auditor-verified observation. | Low | No (T4 covers) |
| Aggregate health | 138 keys/locale, zero missing literal `t()` calls, zero placeholder mismatches. Good baseline — keep. | Info | No |

> Drift note: `app.py:1331-1353` (dispatch at `:1342-1343`), `app.py:1305-1329`,
> `i18n.py:166-167` and `:343-344` verified byte-for-byte on `develop @ e004ae3`
> during track writing. All other `app.py` / `engine.py` / `network.py` / `i18n.py`
> line numbers are carried from the verified audit context — re-confirm before editing
> and record drift here. (Minor span note: the audit cited the dispatch as
> `:1342-1348`; the current method spans `:1331-1353` with the pid loop at
> `:1342-1343` — same defect, wider verified span.)

## Traceability

- Engram observation **#6431** + session **#6433** (Front 3: ES/EN localization audit).
- Existing coverage (not executed in audit): `tests/test_asr_language.py`.
- Cross-ref: `firstuse-startup-progress_20260905` (progress/error strings must use the
  keyed strategy defined here, REQ-5).
