# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| Latest (`main`) | ✅ |
| Older releases | ❌ |

We support security fixes on the latest released version only.

## Scope

Audio and transcripts processed by LiveAudio **never leave your machine**. The application does not collect telemetry, analytics, or user identifiers. The attack surface is limited to:

- **WebSocket server** (local loopback, default port 8765) — broadcasts live subtitles to local OBS browser sources
- **In-app updater checks** — queries GitHub Releases (`latest.json`) over HTTPS every 24 hours to check for new versions (can be disabled in **Settings > Advanced**); update binaries and manifests are cryptographically verified using Minisign Ed25519 signatures before execution
- **Whisper model downloads** — Whisper weights are fetched from `huggingface.co` the first time a model is used. The Silero VAD model is embedded in the binary and is not downloaded.
- **Configuration file** (`config.json`) — stored locally in `%APPDATA%\LiveAudio`, never uploaded

Out of scope: issues that require physical access to the machine, social engineering, or are in third-party dependencies (report those upstream).

## Reporting a Vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Please report them privately via [GitHub Security Advisories](https://github.com/plynte-labs/LiveAudio/security/advisories/new).

Include:
1. A clear description of the vulnerability
2. Steps to reproduce
3. Potential impact
4. Suggested fix (optional)

### What to expect

- **Acknowledgment**: within 48 hours
- **Status update**: within 7 days
- **Fix timeline**: depends on severity — critical issues are prioritized

We follow responsible disclosure. Once a fix is released, we will credit you in the release notes unless you prefer to remain anonymous.

## Security Best Practices for Users

- Keep LiveAudio updated to the latest version
- Do not expose the WebSocket port (8765) to the public internet — use a firewall or bind to `localhost`
- Review `config.json` — it contains your device and model preferences but no secrets
