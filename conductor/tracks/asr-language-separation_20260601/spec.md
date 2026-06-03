# Specification: ASR Language Separation with Dual Context Prompts

## Goal
Decouple the ASR voice language from the UI display language so users can view the app in one language while speaking in another, without relogging or restarting AI engine processes. Implement automatic transparent management of two independent context prompts to eliminate phonetic-conflict hallucinations.

## Requirements

### REQ-1: Config Migration
- New keys: `asr_language` (es/en), `whisper_context_prompt_es`, `whisper_context_prompt_en`
- Old `whisper_context_prompt` content migrates automatically to `whisper_context_prompt_es`
- Deprecated key removed from active config after migration

### REQ-2: ASR Language UI Selector
- Dropdown in Performance tab between model size selector and context textbox
- Values: Español / English
- Dynamic help text updates based on selected language
- Language change does NOT require engine restart

### REQ-3: Dual Context Prompts
- Two independent prompt fields stored in config
- Switching ASR language saves current prompt to the outgoing language key
- Textbox loads the saved prompt for the incoming language
- Rapid ES→EN→ES switching preserves both prompts

### REQ-4: Engine Hot Injection
- `asr_consumer` reads `asr_language` and corresponding `whisper_context_prompt_{lang}` from `shared_config` each cycle
- `_transcribe_with_timeout` accepts dynamic `language` parameter (no hardcoded "es")
- Language change takes effect immediately without process restart

### REQ-5: i18n Labels
- `spoken_language_label` in both Spanish and English UI
- `whisper_context_help_es` and `whisper_context_help_en` with language-specific examples

## Acceptance Criteria
- **AC-1:** Existing `whisper_context_prompt` content migrates to `whisper_context_prompt_es` on app launch
- **AC-2:** ASR language dropdown visible in Performance tab with Español/English options
- **AC-3:** Typing "Claves en español" with Español selected, switching to English (textbox clears), typing "English hints", switching back to Español shows "Claves en español" intact
- **AC-4:** Starting system, speaking in Spanish transcribes in Spanish. Switching dropdown to English (without stopping) transcribes immediately in English without engine freeze or restart
