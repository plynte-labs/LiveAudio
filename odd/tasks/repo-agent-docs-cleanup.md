# Repo agent/skills/docs cleanup

Locator: `odd/tasks/repo-agent-docs-cleanup.md` · Engram mirror: `odd/repo-agent-docs-cleanup/tasks`

## Objective
Remove AI vendor/model names from agent and skill identities, update CLAUDE.md and AGENTS.md to the current Rust-only architecture, and prune residual skills and obsolete documentation (user request 2026-11-07).

## Decisions
- Agent/skill names use plain role names. Rename map:
  - `liveaudio-architecture-security-deepseek` -> `liveaudio-architecture-security`
  - `liveaudio-performance-minimax-m27` -> `liveaudio-performance`
  - `liveaudio-product-strategy-chatgpt` -> `liveaudio-product-strategy`
  - `liveaudio-qa-qwen36plus` -> `liveaudio-qa`
  - `liveaudio-research-gemini25pro` -> `liveaudio-research`
  - `liveaudio-ui-ux-security-architect` keeps its name; vendor file `agents/openai.yaml` removed.
- Subagent model policy (user, 2026-11-07): sonnet for complex/implementation, haiku for lightweight. No fable.
- Released CHANGELOG/HISTORIAL entries are history: not rewritten.
- Only tracked or repo-authored files are deleted (recoverable via git). Untracked/ignored local folders are reported, not deleted.
- No commits (AGENTS.md requires approval).

## Tasks
- [x] C1 — CLAUDE.md, AGENTS.md, `.agents/skills/**`: rewrite to current facts, rename skills, remove vendor/model persona names, prune obsolete skills/templates. Route: delegated (haiku A).
- [x] C2 — Docs (`docs/**`, root docs except CLAUDE/AGENTS, `conductor/`, `openspec/`, root residue): delete obsolete, update stale, fix links and skill-name references. Route: delegated (haiku B).

## Checks
- `grep -rniE "minimax|deepseek|qwen|gemini|chatgpt|codex|fable|openai" CLAUDE.md AGENTS.md .agents docs` returns only justified hits.
- No broken relative links to deleted files in remaining markdown.
- `node --test tests-js/*.mjs` and `node desktop/test_frontend.js` still pass (tests may assert on docs).

## Progress / evidence
- C1 (writer A, 2026-11-07): renamed the 5 skill dirs; deleted `agents/openai.yaml` (and empty `agents/`) and unused templates `python.md`, `cpp.md`, `csharp.md`, `dart.md`, `go.md` (conductor-setup lists templates via `ls`, none named); rewrote CLAUDE.md, AGENTS.md, and the 6 role SKILL.md files (plain role names, English labels, Rust/ONNX facts); conductor tool-mapping overrides now name Claude Code tools. Check `grep -rniE "minimax|deepseek|qwen|gemini|chatgpt|openai|codex|fable|copilot|python|faster-whisper|uv " CLAUDE.md AGENTS.md .agents`: only CLAUDE.md:7 (`fable` prohibition), CLAUDE.md:12 and AGENTS.md:64 (Python removal statements). No test reads the owned files; node tests not run by C1. `.claude/settings.local.json` untouched.
- C2 (writer B, 2026-11-07): deleted obsolete tracked docs (EMPAQUETADO_Y_ACTUALIZACION, ENGRAM_LOCAL_COMANDOS, NUEVO_PROYECTO_SDD_ENGRAM, SDD_ENGRAM_WORKFLOW, SDD_SKILLS_USAGE, TEAM_WORKFLOW, `docs/migration/`, `docs/superpowers/`, v1.2.1-retrospective, `arquitectura.md`, `Futuros_pasos.md`, `REQUERIMIENTOS_COPILOTO.md`, `opencode.json`, `tools/`, `openspec/changes/`, `openspec/config.yaml`, `conductor/archive/`, `conductor/code_styleguides/python.md`, 15 conductor track folders). Replaced the stale packaging doc with English `docs/PACKAGING_AND_UPDATES.md`; added `conductor/code_styleguides/rust.md`; rewrote `conductor/tech-stack.md` for Rust; pruned `conductor/tracks.md`. ADR-016: migration links removed and historical notes added to sections 2 and 5. README/README.es: benchmark methodology link removed. CONTRIBUTING: process-boundary rule replaces multiprocessing. SECURITY: VAD is embedded, Whisper weights come from huggingface.co only. Checks: relative-link check over 45 markdown files, 0 broken; no test reads a deleted doc; `node --test tests-js/*.mjs` 135 pass, 0 fail; `node desktop/test_frontend.js` 567 pass, 1 fail, outside C2: "Linux bundle maps the ONNX runtime binary" expects `libonnxruntime.so.1.26.0` while `desktop/src-tauri/tauri.linux.conf.json` (modified outside C2) maps `1.24.4`. Vendor/model-name grep: only justified hits (Spanish "llama" substrings, Claude product names in `docs/MCP.md`, GPT-2 tokenizer name, `CLAUDE.md` file reference, `.atl` machine paths, historical `opencode.json` note). Untracked/ignored root residue reported, not deleted.
- Lead follow-up (inline): `desktop/test_frontend.js:787` expectation 1.26.0 -> 1.24.4 (T5 regression); stale links in `odd/tasks/transcription-buffer-safety.md` and `odd/tasks/subtitle-style-editor.md` point to `git show 64316ed:<path>`; obsolete `tools/` rule removed from `.gitignore`. Checks: `node desktop/test_frontend.js` 568 passed / 0 failed; `node --test tests-js/*.mjs` 135 / 0; vendor grep over CLAUDE.md, AGENTS.md, .agents -> only the CLAUDE.md:7 fable prohibition.
- Local residue deleted (user-approved 2026-11-07, all verified untracked): .venv, dist, build_artifacts, app, legacy, build, .build_cache, __pycache__, .pytest_cache, .pytest-tmp, .ruff_cache, .tmp-ci-collect, .tmp-ci-*.log, temp_service_home, liveaudio-test-home-*. `cargo clean`: removed 133,549 files, 77.1 GiB. Kept: sessions/, root config.json, desktop.ini.
- Previously pending user decisions: delete untracked local residue (`.venv` 4.7G, `dist` 4.5G, `build_artifacts` 704M, `app` 292M, `legacy`, `build`, caches, `.tmp-ci-*` logs); `git rm --cached` of tracked ORT binaries at commit; two core tests still use `python` as a mock worker. Commit: pending (approval).
