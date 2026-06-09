# Implementation Plan: ASR Language Separation with Dual Context Prompts

## Design Decisions

### Decoupled Language Model
The ASR voice language (`asr_language`) is stored as a separate config key from the UI language (`language`). The engine reads `asr_language` from `shared_config` on every transcription cycle — same mechanism as the existing hot-swap config pattern. This means zero restart is required when the user changes the spoken language.

### Dual Prompt Strategy
Each language gets its own context prompt stored as `whisper_context_prompt_es` and `whisper_context_prompt_en`. The UI swaps between them atomically via `_on_asr_language_change`, which saves the current textbox content to the outgoing language key before loading the incoming language's prompt. `_read_ui_config` preserves both keys from `draft_config` to prevent data loss during apply.

### Migration Without Breakage
`_normalize_config` detects the old `whisper_context_prompt` key at load time. If it contains content, it's copied to `whisper_context_prompt_es` (default language for existing users). The deprecated key is then deleted. New configs never receive the old key since it's not in `DEFAULT_CONFIG`.

## Implementation Steps

### Step 1: Config Layer (`utils/config.py`)
- Added `asr_language`, `whisper_context_prompt_es`, `whisper_context_prompt_en` to `DEFAULT_CONFIG`
- Removed deprecated `whisper_context_prompt` from `DEFAULT_CONFIG`
- Added migration logic in `_normalize_config`
- Added validation: `asr_language` ∈ {"es", "en"}, prompts must be strings

### Step 2: i18n Labels (`utils/i18n.py`)
- Added `spoken_language_label` in both Spanish and English sections
- Added `whisper_context_help_es` and `whisper_context_help_en` with contextual examples

### Step 3: UI Layer (`main.py`)
- Inserted ASR language dropdown (`opt_asr_lang`) in Performance tab between model selector and context textbox
- Implemented `_on_asr_language_change`: save outgoing prompt → switch language → load incoming prompt → update help text → notify engine
- Updated `_load_ui_from_config`: set `var_asr_lang`, load correct prompt by `asr_language`, update help label
- Updated `_read_ui_config`: preserve both prompts from `draft_config`, overwrite active language prompt from textbox

### Step 4: Engine Layer (`core/engine.py`)
- Added `language="es"` parameter to `_transcribe_with_timeout`
- Replaced hardcoded `"language": "es"` with dynamic `language` param
- In `asr_consumer` loop: read `asr_language` and `whisper_context_prompt_{lang}` from `shared_config` each cycle
- Pass both to `_transcribe_with_timeout`

### Step 5: Tests (`tests/test_asr_language.py`)
- 47 stress tests across 10 test classes
- Covers config migration (5), validation (10), engine injection (7), UI swap logic (9), config read/load (12), method existence (4)
- Initial version had 51.2% false positives — audited and rewritten to exercise real production code

### Step 6: Manual Verification
- Test migration of existing prompt
- Test ASR language dropdown visibility
- Test prompt swap on ES→EN→ES
- Test live inference language switch without restart
