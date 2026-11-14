# MCP Server: Let an AI Agent Read and Change LiveAudio Settings

LiveAudio can expose its settings to an AI assistant through the [Model Context Protocol](https://modelcontextprotocol.io). Claude Desktop, Claude Code, and other MCP clients can then read the current settings, see which ones are allowed, and change them. Changes reach the running desktop app without a restart, except for settings that need the ASR service to restart.

The feature is **off by default**. It is split into two independent opt-in switches with different risk profiles. Only you can turn them on, and only from the desktop app.

## Contents

1. [Enable it](#1-enable-it)
2. [Connect a client](#2-connect-a-client)
3. [Tools](#3-tools)
4. [Settings you can change](#4-settings-you-can-change)
5. [Security model](#5-security-model)
6. [Troubleshooting](#6-troubleshooting)

---

## 1. Enable it

1. Open LiveAudio and go to **Settings → Advanced**.
2. Under the network and agent settings, choose which level of access to allow:
   - **Allow AI agents to control subtitle styles (MCP - Low risk)** (`mcp_style_enabled`): allows reading and modifying only subtitle visual appearance (colors, themes, presets, display mode). It carries low risk because it cannot touch audio devices, models, transcription, or network parameters.
   - **Allow AI agents to control system settings (MCP - Medium risk)** (`mcp_system_enabled`): allows changing operational settings such as model size, compute device, VAD sensitivity, prompts, blacklist, and WebSocket port. It carries medium risk because incorrect settings can trigger ASR service restarts or degrade performance.
3. Click **Save**.

Both switches are off by default. If neither switch is enabled, all MCP tools refuse requests. Agents cannot enable either switch themselves: `mcp_style_enabled` and `mcp_system_enabled` are not settable keys, and the server refuses any attempt to change them.

**Migrating from earlier versions:** If you previously enabled MCP under the single `mcp_enabled` switch, LiveAudio automatically migrates your configuration to activate only `mcp_style_enabled` (low risk). The medium-risk `mcp_system_enabled` switch remains off (`false`). If you previously used MCP for full system and model control, you must explicitly re-enable **Allow AI agents to control system settings** in Settings → Advanced and click Save.

## 2. Connect a client

LiveAudio ships the server as the `liveaudio-cli` binary (`liveaudio-cli.exe` on Windows). It is a stdio server: the client starts it and talks to it over stdin and stdout. It opens no network port and needs no token.

Replace `<path-to>` with the absolute path to `liveaudio-cli` on your machine. You can also build it from a checkout with `cargo build -p liveaudio-cli --release`.

**Claude Desktop** (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "liveaudio": {
      "command": "<path-to>\\liveaudio-cli.exe",
      "args": ["mcp"]
    }
  }
}
```

**Claude Code:**

```bash
claude mcp add liveaudio -- "<path-to>/liveaudio-cli" mcp
```

The server reads and writes the same `config.json` as the desktop app, in the same data home. Run the client as the same user as the app. To use a different data home, set `LIVEAUDIO_HOME` in the server's environment.

## 3. Tools

| Tool | Arguments | What it does |
|---|---|---|
| `get_config` | none | Current values of the settings authorized by your active MCP switches. |
| `get_config_schema` | none | Allowed settings for your active switches, including types, allowed values, ranges, and whether changes need a restart. Call this before `set_config`. |
| `set_config` | `settings`: object of setting name to new value | Changes the listed settings. Returns the stored values and `restart_required`. Atomic: if any key is unauthorized, nothing is written. |
| `get_status` | none | Whether the LiveAudio service is running and its WebSocket port. Requires at least one active MCP switch. No transcript or audio content. |
| `list_audio_devices` | none | Input devices LiveAudio can use (id, name, default flag). Requires at least one active MCP switch. |

When both MCP switches are off, every tool returns an error with the message `MCP access is disabled; enable it in LiveAudio settings (Advanced)`.

When a specific switch is off, attempts to change its keys fail with `MCP access to <group> settings is disabled; enable it in LiveAudio settings (Advanced)` (where `<group>` is `style` or `system`).

`set_config` is a partial update. Settings you leave out keep their values. If a patch contains mixed keys and any of them belongs to a disabled switch or violates the schema, the entire patch is rejected and nothing is written to disk.

A value that does not match the schema is rejected, and nothing is written. That includes a number the setting's type cannot hold: `ws_port: 70000`, a negative or fractional `vad_speech_pad_ms`, or a negative `cpu_threads`. A number of the right type but outside the allowed range is clamped to the nearest allowed value, and the response shows the stored value: `silence_timeout: 9.0` is stored as `2.0`.

Text settings accept at most 4096 characters; a longer value is rejected and nothing is written. Each request line is limited to 1 MiB. A longer line gets an `invalid request` error, and a line that is not valid UTF-8 gets a `parse error`. The server keeps running in both cases.

`restart_required: true` means at least one changed setting only takes effect when the ASR service restarts. Restart it from the app. Other changes are picked up by the running app within about a second.

## 4. Settings you can change

Run `get_config_schema` for the authoritative list matching your active switches.

### Style Settings (Group A, Low Risk — enabled by `mcp_style_enabled`)
- `subtitle_style`: Built-in subtitle preset style (`default`, `karaoke`, `neon`, `minimal`, `bold`, `rgb`, `typewriter`).
- `subtitle_display_mode`: Display mode (`single`, `ribbon`, `adaptive`).
- `subtitle_theme`: Object of CSS tokens for the OBS subtitle overlay (for example `"--sub-color": "#ffcc00"`). Invalid token values are dropped and numeric sizes are clamped.
- `subtitle_presets`: Up to 20 named style presets (names up to 40 characters).

Changes to style settings take effect in the running OBS overlay within about a second and do not require restarting the ASR service.

### System Settings (Group B, Medium Risk — enabled by `mcp_system_enabled`)
- Transcription: `device`, `asr_backend`, `cpu_threads`, `model_size`, `asr_language`, `transcription_purpose`, `asr_decode_timeout_sec`, `whisper_context_prompt_es`, `whisper_context_prompt_en`, `blacklist`.
- Audio segmentation and VAD: `silence_timeout`, `max_chunk_duration`, `vad_speech_pad_ms`, `vad_threshold`, `continuous_session`, `rnnoise_enabled`, `acoustic_events_enabled`, `live_partial_subtitles_enabled`, `prewarm`.
- Subtitles and output: `subtitle_ribbon_max_lines`, `subtitle_backlog_policy`, `subtitle_max_live_delay_sec`, `subtitle_catchup_interval_sec`, `save_transcript_enabled`, `save_vtt_enabled`.
- Network and app: `ws_port`, `obs_enabled`, `language`, `settings_navigation_mode`, `diagnostics_enabled`, `diagnostics_level`.

### Settings that CANNOT be changed through MCP
- `mcp_style_enabled` and `mcp_system_enabled`, the opt-in switches themselves.
- `ws_allowed_loopback_ports`: The loopback port allowlist controls which local applications can receive live transcription data from the WebSocket server. To prevent AI agents from granting themselves or untrusted local processes access to live transcripts, this allowlist is managed exclusively through the desktop UI / Tauri IPC and can never be changed via MCP.
- Paths: `output_dir` and `diagnostics_export_dir`.
- `audio_device`, `selected_profile_id`, `profile_mode`, which the desktop app manages.
- `last_update_check`, and any key the schema does not know, which is rejected rather than ignored.

## 5. Security model

- **Two-tier opt-in, on your side.** The server distinguishes low-risk style adjustments from medium-risk system reconfiguration. Both switches are off by default and can only be enabled in the desktop app.
- **Atomic updates.** A patch that mixes valid settings with disallowed or disabled-switch settings is rejected in its entirety without modifying `config.json`.
- **Local only.** No network listener and no token. The only channel is the stdio pipe between the client you started and the server.
- **Transcript privacy.** No tool returns transcripts, audio, or subtitle text. The loopback port allowlist (`ws_allowed_loopback_ports`) is strictly UI-only and cannot be altered by MCP.
- **Form saves keep agent edits.** Saving the settings form changes only the fields you edited in it. Other fields keep the value on disk, including values an agent set.
- **An explicit style save wins.** The subtitle style is saved by the Style tab and keeps the rest of the file as it is on disk. If you save a style in the app within about a second of an agent changing the theme or presets, your save replaces what the agent wrote.
- **Narrow surface.** Reads and schemas return only the allowlisted settings for active switches. Writes accept only allowlisted keys, check types/enums, and normalize values before saving.
- **Safe writes.** Writes take the same config lock as the desktop app, validate, and then replace `config.json` atomically. If the file cannot be read, the write fails and the file is left alone.
- **No clobbering.** The desktop app reloads external edits and keeps them when you save the settings form, so a form that is still open cannot revert a change an agent made.

This is a consent gate for agents, not a sandbox. Any program running as your user can already edit `config.json` directly. Enable MCP only with clients you trust, and turn it off when you do not need it.

## 6. Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `MCP access is disabled` | Both MCP switches are disabled. Enable either or both in Settings → Advanced and click Save. |
| `MCP access to style settings is disabled` | The agent attempted to change or read style settings, but `mcp_style_enabled` is off. Enable **Allow AI agents to control subtitle styles** in Settings → Advanced. |
| `MCP access to system settings is disabled` | The agent attempted to change system settings, but `mcp_system_enabled` is off. Enable **Allow AI agents to control system settings** in Settings → Advanced. |
| `config is locked by another process; retry` | The desktop app or another writer holds the config lock. Retry after a moment. |
| `unknown config key` or `cannot be changed via MCP` | The key is not on the allowlist (for example paths, `ws_allowed_loopback_ports`, or MCP switches). See section 4. |
| `invalid value for '<key>'` | The value has the wrong type or is not one of the allowed options. Check `get_config_schema`. |
| `config could not be read` | `config.json` is not valid JSON. Fix or remove the file; the server does not overwrite it. |
| Changes do not show up in the app | Check `restart_required`. If it is `false`, confirm the app is using the same data home as the server. |
