# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| Latest (`main`) | ✅ |
| Older releases | ❌ |

We support security fixes on the latest released version only.

## Scope

LiveAudio runs **100% locally**. It does not transmit audio, transcriptions, or any personal data to external servers. The attack surface is limited to:

- **WebSocket server** (local, default port 8765) — accessible only within the local network by default
- **Whisper model downloads** — fetched from Hugging Face/official sources on first run
- **Configuration file** (`config.json`) — stored locally, not uploaded

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
