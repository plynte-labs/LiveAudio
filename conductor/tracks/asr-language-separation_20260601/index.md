# Track: ASR Language Separation with Dual Context Prompts

**ID:** `asr-language-separation_20260601`
**Status:** 🔲 pending manual verification
**Branch:** `master` (implementation commit also remains reachable from historical branch `feature/empaquetado-portatil`)
**Repository:** `https://github.com/plynte-labs/LiveAudio`
**PR:** historical pre-migration PR reference removed; implementation is already present on `master`
**Commit:** `5680b08`

## Summary
Decouple ASR voice language (`asr_language`) from UI display language (`language`). Add per-language context prompts (`whisper_context_prompt_es`/`_en`) with transparent migration. Users can set UI to English while speaking Spanish (or vice versa) without restarting the engine.

> Repository migration note: this track was created before the canonical repository moved to `plynte-labs/LiveAudio`. Do not use old personal-fork PR links as current review or release status.

## Files Changed
- `core/engine.py` — dynamic `language` param + per-cycle prompt read from `shared_config`
- `main.py` — ASR language dropdown in Performance tab, `_on_asr_language_change`, updated `_read_ui_config`/`_load_ui_from_config`
- `utils/config.py` — new keys (`asr_language`, `whisper_context_prompt_es`, `whisper_context_prompt_en`), migration, validation
- `utils/i18n.py` — `spoken_language_label`, `whisper_context_help_es`/`_en` labels (×2 UI languages)
- `config.json.example` — updated example keys
- `tests/test_asr_language.py` — **47 stress tests** (new file)

## Tests
- **47 new tests**: migration (5), validation (10), engine injection (7), UI swap logic (9), config read/load (12), method existence (4)
- **263/263** total suite passing
- 0% false positive rate after audit (was 51.2%)

## Pending Verification
- [ ] Manual test 1: Migration of old `whisper_context_prompt` → `whisper_context_prompt_es`
- [ ] Manual test 2: ASR language dropdown visible in Performance tab
- [ ] Manual test 3: Prompt swap preserves both language prompts on rapid ES→EN→ES
- [ ] Manual test 4: Live inference switches language without engine restart

## Artifacts
- [spec.md](./spec.md) — Requirements and acceptance criteria
- [plan.md](./plan.md) — Design decisions and implementation steps
