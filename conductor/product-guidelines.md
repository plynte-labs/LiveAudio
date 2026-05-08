# Product Guidelines

## UX Principles

- Prefer safe defaults over exposing every internal knob first.
- Show clear warnings before changes that can interrupt live subtitles.
- Separate pending UI changes from active runtime configuration.
- Keep streamer workflows fast: launch, pick profile/device, start, monitor OBS.
- Error states must be recoverable and explain what the user can do next.

## Privacy And Security

- Treat local audio, transcripts, and subtitle sessions as sensitive data.
- Avoid sending audio or transcript content to external services unless explicitly added and documented.
- Keep WebSocket and OBS browser-source behavior local and narrow by default.
- Validate filesystem paths, config values, and device/model choices before persisting changes.

## Documentation

- Update `README.md`, `docs/GETTING_STARTED.md`, and `HISTORIAL_CAMBIOS.md` for user-facing changes.
- Use requirement docs under `docs/requirements/` for non-trivial features.
- Record specialist-agent comments with labels such as `[Agent Arquitecto Deepseek V4 PRO]`.

## Collaboration

- Product Owner: user.
- Principal coordinator: assistant.
- Specialized review owners: architecture/security, QA/product, performance/resilience, research/traceability.
