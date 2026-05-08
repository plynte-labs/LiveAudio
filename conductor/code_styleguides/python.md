# Python Style Guide

## Project Rules

- Match the existing code style in the file being edited.
- Prefer small, direct changes over broad rewrites.
- Use clear names and avoid unnecessary comments.
- Add comments only when the code is not self-explanatory.
- Avoid mutable default arguments.
- Prefer explicit `is None` checks for nullable values.
- Keep imports grouped by standard library, third-party, and project modules.

## LiveAudio-Specific Rules

- Treat multiprocessing boundaries and shared config updates carefully.
- Validate config values before saving them.
- Avoid blocking UI operations in the main thread.
- Consider OBS/browser-source burst behavior for subtitle delivery changes.
- Prefer recoverable user-facing errors over silent fallback when live streaming can be affected.
