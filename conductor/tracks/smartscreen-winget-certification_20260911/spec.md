# Specification: Microsoft SmartScreen Whitelisting & Winget Certification

## Summary
Establish verified application reputation with Microsoft Defender SmartScreen and publish the official Windows Package Manager (Winget) manifest for LiveAudio v1.2.6.

## Context
LiveAudio releases are distributed as a single-file bootstrapper installer (`LiveAudio-Setup-1.2.6.exe`) compiled via PyInstaller in GitHub Actions. Because the binary is not yet signed with an Extended Validation (EV) code-signing certificate, Windows Defender SmartScreen presents an "Unrecognized app / Unknown Publisher" modal on first download.

To eliminate friction for streamers and end-users:
1. Submit the official release binary to Microsoft Security Intelligence (WDSI) for reputation and false-positive whitelisting.
2. Publish `PlynteLabs.LiveAudio` to `microsoft/winget-pkgs` to provide automated CLI installation and verified repository distribution.

---

## Target Artifacts & Hashes (v1.2.6)
- **Installer Binary**: `LiveAudio-Setup-1.2.6.exe`
- **Official Release**: `https://github.com/plynte-labs/LiveAudio/releases/tag/v1.2.6`
- **Download URL**: `https://github.com/plynte-labs/LiveAudio/releases/download/v1.2.6/LiveAudio-Setup-1.2.6.exe`
- **SHA-256 Checksum**: `578e8d68592d646c26bb0c32f480a954c7bd93d0223018bfd3b4a83b8c8520be`

---

## Submission Guide: Microsoft Security Intelligence (WDSI)
- **Portal URL**: `https://www.microsoft.com/en-us/wdsi/filesubmission`

### Field Values
| Field | Value |
|---|---|
| **Product** | `Microsoft Defender Antivirus` (or `Microsoft Defender SmartScreen`) |
| **Company Name** | `Plynte Labs` |
| **Support case number?** | `No` |
| **Remove from database?** | `No — remove the file automatically after a period of inactivity` |
| **What do you believe this file is?** | `Incorrectly detected as malware/malicious` |
| **Detection name** | `Unrecognized app / SmartScreen Unknown Publisher warning` |

### Additional Information Template
```text
LiveAudio is an open-source, local-first real-time speech-to-text (ASR) desktop application for content creators, streamers, and local AI workflows (repository: https://github.com/plynte-labs/LiveAudio).

The installer "LiveAudio-Setup-1.2.6.exe" is an automated bootstrapper compiled with PyInstaller on GitHub Actions CI. It operates entirely offline-first on the local machine (using local Whisper and Silero VAD inference) and binds its WebSocket subtitle server strictly to loopback (127.0.0.1).

This binary is clean, contains zero telemetry or malware, and its SHA256 checksum is published in our official GitHub release:
SHA256: 578e8d68592d646c26bb0c32f480a954c7bd93d0223018bfd3b4a83b8c8520be

We kindly request verification and reputation clearance in Microsoft Defender SmartScreen so that our users do not receive the unrecognized application warning.
```

---

## Winget Distribution Plan
- **Target Repository**: `microsoft/winget-pkgs`
- **Package Identifier**: `PlynteLabs.LiveAudio`
- **Reference**: Follow similar manifest architecture used in `microsoft/winget-pkgs/pull/433525`.
- **Install Type**: Portable / PyInstaller bootstrapper.
