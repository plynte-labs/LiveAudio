# WebSocket Protocol and OBS Integration

LiveAudio runs a local WebSocket server that streams live subtitles to connected clients. Its main consumer is the bundled OBS Browser Source overlay (`liveaudio/assets/subtitulos_obs.html`). This document is the protocol reference for that overlay and for any third-party client.

> **Scope.** This document describes the Rust server in `crates/liveaudio-network`, which the Tauri desktop app and the `liveaudio-cli` crate use. The repository also contains a Python implementation (`liveaudio/core/network.py`) used by the legacy Python app and service mode. It uses the same `hello` shape, but it was not checked against this document and behaves differently in places. For example, it sends WebSocket keepalive pings every 10 seconds, and the Rust server does not.

## Contents

1. [Overview](#1-overview)
2. [Connecting and discovering the port](#2-connecting-and-discovering-the-port)
3. [Versioning and compatibility](#3-versioning-and-compatibility)
4. [Server-to-client messages](#4-server-to-client-messages)
5. [Subtitle delivery, backlog policy and replay](#5-subtitle-delivery-backlog-policy-and-replay)
6. [Client-to-server traffic](#6-client-to-server-traffic)
7. [Security](#7-security)
8. [Client examples](#8-client-examples)
9. [OBS Studio setup](#9-obs-studio-setup)
10. [Overlay styles](#10-overlay-styles)
11. [Customizing the overlay](#11-customizing-the-overlay)
12. [Troubleshooting](#12-troubleshooting)

---

## 1. Overview

| Property | Value |
|---|---|
| Transport | WebSocket, plain `ws://` (no TLS) |
| Address | `127.0.0.1` only (IPv4 loopback) |
| Default port | `8765` (config key `ws_port`) |
| Frames | Text frames, one JSON object per frame, UTF-8 |
| Direction | Server to client. The server does not act on client messages (see [section 6](#6-client-to-server-traffic)). |
| Fan-out | Every connected client receives the same live stream. A client that connects late also receives replayed subtitles (see [section 5.3](#53-late-join-replay)). |
| Authentication | None |

```
ASR worker -> LiveAudio pipeline (desktop app or liveaudio-cli)
                        |
                        v
        WebSocket server  ws://127.0.0.1:<port>
            |                               |
            v                               v
   OBS Browser Source              your application
```

---

## 2. Connecting and discovering the port

The port LiveAudio actually serves on can differ from the configured one.

1. **Know the base port.** It is `ws_port` in LiveAudio's configuration (default `8765`). Valid values are 1 to 65535. Invalid values fall back to `8765`.
2. **Know the fallback range.** If the base port is in use, the server tries the next ports in order. The range is `base` through `base + 9` (10 candidates, `WS_PORT_FALLBACK_RANGE = 10`). Only an "address in use" error triggers the fallback. Any other bind error stops startup.
3. **Find the live server.** Connect to each candidate in order, and accept a connection only if its first frame is a `hello` with `app` equal to `"liveaudio"`. The bundled overlay waits up to 2 seconds for that `hello` and moves to the next candidate if it does not arrive.
4. **Use `hello.port`.** It is the effective port the server is listening on, and it matches the port you connected to. Store it for later connections.

If your application launches `liveaudio-cli` itself, it can also read the effective port from the process's stdout. See [section 4.5](#45-defined-but-not-sent-over-the-websocket).

When the server starts, `liveaudio-cli` prints `WebSocket server listening on ws://127.0.0.1:<port>` to stderr.

---

## 3. Versioning and compatibility

The `proto` field of the `hello` message is the protocol version. It is an integer, and the current value is `1` (`WS_PROTO_VERSION` in `crates/liveaudio-network/src/protocol.rs`).

**Compatibility policy (stated in this document):**

- **Non-breaking changes** keep `proto` unchanged:
  - adding fields to existing objects;
  - adding new `type` values;
  - adding new messages;
  - adding new subtitle `style` names.
- **Breaking changes** increment `proto`:
  - removing or renaming a field;
  - changing a field's JSON type or units;
  - changing the meaning of an existing field;
  - changing how a message kind is identified;
  - changing the `hello` handshake.
- Clients **must ignore** unknown fields and unknown `type` values.
- Clients **should treat** an unrecognized `style` as `default`. The bundled overlay does this.
- Clients **should accept only** the `proto` values they were written for. The bundled overlay accepts any integer greater than or equal to `1`.

---

## 4. Server-to-client messages

Messages are identified as follows. Use the `type` field when it is present. A subtitle has no `type` field.

| Object shape | Message | Section |
|---|---|---|
| `"type": "hello"` | Handshake, always the first frame | [4.1](#41-hello) |
| no `type`, has `text` | Subtitle | [4.2](#42-subtitle) |
| `"type": "theme"` | Theme token update | [4.3](#43-theme) |
| `"type": "speech_activity_start"` or `"speech_activity_end"` | Speech activity event (optional) | [4.4](#44-speech-activity-events) |

### 4.1 `hello`

The server sends this frame first on every accepted connection, before any other frame. It is not stored for replay.

| Field | JSON type | Meaning |
|---|---|---|
| `type` | string | Always `"hello"` |
| `app` | string | Always `"liveaudio"` |
| `proto` | integer | Protocol version. Currently `1`. |
| `port` | integer | Effective port the server is listening on (1 to 65535). |

```json
{"type": "hello", "app": "liveaudio", "proto": 1, "port": 8766}
```

### 4.2 Subtitle

A subtitle is one recognized utterance. It has no `type` field. All time values are in **seconds** (floating point). `created_at` and `processed_at` are Unix timestamps in seconds, not milliseconds.

| Field | JSON type | Units | Meaning |
|---|---|---|---|
| `id` | string | none | Utterance identifier. Current builds emit `utt_<n>`, where `n` starts at 1 for each pipeline run. Treat it as opaque. Do not parse it. |
| `text` | string | none | Display text after LiveAudio's text filtering and formatting. |
| `style` | string | none | Configured subtitle style. One of `default`, `karaoke`, `neon`, `minimal`, `bold`, `rgb`, `typewriter`. |
| `created_at` | number | seconds since Unix epoch | Wall-clock time when the utterance was dispatched to the transcription worker. |
| `processed_at` | number | seconds since Unix epoch | Wall-clock time when the transcription result was received. |
| `queue_delay` | number | seconds | Time between dispatch and result that was not inference (queueing and transport). Never negative. |
| `total_delay` | number | seconds | `queue_delay` plus `latency`. |
| `latency` | number | seconds | Inference time reported by the worker. `0` if the worker did not report it. |
| `is_replay` | boolean | none | `true` for replayed or delayed subtitles. See [section 5.4](#54-interpreting-is_replay). |
| `catchup_interval_sec` | number | seconds | Pacing interval used for replayed subtitles. `0` for subtitles sent immediately. |

Live subtitle:

```json
{
  "id": "utt_42",
  "text": "Hola a todos, bienvenidos al stream",
  "style": "default",
  "created_at": 1760000000.0,
  "processed_at": 1760000000.7,
  "queue_delay": 0.1,
  "total_delay": 0.7,
  "latency": 0.6,
  "is_replay": false,
  "catchup_interval_sec": 0.0
}
```

Subtitle replayed to a late-joining client, or delayed by more than one second:

```json
{
  "id": "utt_41",
  "text": "Esta frase llego tarde",
  "style": "default",
  "created_at": 1760000010.0,
  "processed_at": 1760000012.0,
  "queue_delay": 1.6,
  "total_delay": 2.0,
  "latency": 0.4,
  "is_replay": true,
  "catchup_interval_sec": 1.5
}
```

### 4.3 `theme`

A theme message updates CSS custom properties on the overlay.

| Field | JSON type | Meaning |
|---|---|---|
| `type` | string | Always `"theme"` |
| `tokens` | object (string to string) | CSS custom property names mapped to values |

```json
{"type": "theme", "tokens": {"--sub-bg": "rgba(0, 0, 0, 0.7)", "--sub-font-size": "42px"}}
```

The bundled overlay applies only these 12 token names. Other names are ignored.

| Token | Accepted values |
|---|---|
| `--sub-bg` | color |
| `--sub-color` | color |
| `--sub-font-size` | size, numeric part from 12 to 120 |
| `--sub-font-weight` | weight |
| `--sub-radius` | size, numeric part from 0 to 50 |
| `--sub-shadow` | shadow |
| `--sub-border` | border |
| `--sub-padding` | padding |
| `--sub-animation-duration` | duration, numeric part from 0.1 to 2.0 |
| `--sub-font-family` | font |
| `--sub-text-transform` | transform |
| `--sub-letter-spacing` | spacing |

For size and duration tokens, the overlay parses the leading number and checks it against the range. Other values are applied as written.

**Status:** the bundled overlay handles this message, and the server exposes `broadcast_theme`. No shipped binary in this repository calls it at the time of writing, so theme frames are not sent in normal operation.

### 4.4 Speech activity events

These events report when speech starts and ends. They are off by default and are enabled by the config key `acoustic_events_enabled` (default `false`). They are live only: a client does not receive them for events that happened before it connected, and they never contain text.

| Field | JSON type | Meaning |
|---|---|---|
| `type` | string | `"speech_activity_start"` or `"speech_activity_end"` |
| `event_version` | integer | Always `1` |
| `run_id` | string | Opaque 32-character hex identifier for one service run |
| `attempt_id` | integer | Capture attempt number within the run. Starts at `1`. |
| `utterance_id` | string | Opaque 32-character hex identifier for one speech segment. Not the same namespace as the subtitle `id`. |
| `seq` | integer | Event sequence number within the run |
| `monotonic_ms` | integer | Milliseconds since the start of the current attempt |
| `reason` | string | `speech_activity_end` only. One of `silence`, `duration_limit`, `service_stopped`, `capture_error`, `attempt_reset`. |

```json
{"type": "speech_activity_start", "event_version": 1, "run_id": "0185f3a2c4d5e6f718293a4b5c6d7e8f", "attempt_id": 1, "utterance_id": "9f8e7d6c5b4a39281716151413121110", "seq": 1, "monotonic_ms": 0}
{"type": "speech_activity_end", "event_version": 1, "run_id": "0185f3a2c4d5e6f718293a4b5c6d7e8f", "attempt_id": 1, "utterance_id": "9f8e7d6c5b4a39281716151413121110", "seq": 2, "monotonic_ms": 1830, "reason": "silence"}
```

Speech activity events bypass the per-client retry queue. Under backpressure they are dropped, not delayed.

### 4.5 Defined but not sent over the WebSocket

- **`status`**: the `StatusWireEvent` type exists in `protocol.rs`, but no code path sends it on the WebSocket.
- **`ws_port`**: when `liveaudio-cli` starts with `obs_enabled` set to `true`, it writes a `ws_port` service event as a JSON line to its stdout. With `obs_enabled` set to `false`, no listener is bound and no event is written. It is not a WebSocket frame. The event uses the service envelope from `liveaudio-ipc`, and its payload contains `base_port`, `effective_port`, `base` and `port`:

  ```json
  {"schema": "liveaudio.service.event", "version": 1, "service_pid": 1234, "parent_pid": 5678, "type": "ws_port", "base_port": 8765, "effective_port": 8766, "base": 8765, "port": 8766}
  ```

  Use `port` (the effective port) from this event if you run the CLI as a child process.

---

## 5. Subtitle delivery, backlog policy and replay

### 5.1 Backlog policy

The policy is set by `subtitle_backlog_policy`. It is applied when a transcription result arrives. The subtitle's **age** is `processed_at - created_at`.

| Policy | Live delivery | `is_replay` | `catchup_interval_sec` |
|---|---|---|---|
| `auto` (default) | Sent if age is at most `subtitle_max_live_delay_sec`. Otherwise dropped. | `true` if age is greater than 1.0 s | `subtitle_catchup_interval_sec` if `is_replay` is `true`, otherwise `0` |
| `live_only` | Sent only if age is at most `subtitle_max_live_delay_sec`. Otherwise dropped. | Always `false` | `0` |
| `send_all` | Always sent | `true` if age is greater than 1.0 s | `0` |

If a result has no timing information (its utterance was not tracked), `send_all` sends it as a live subtitle with `is_replay` set to `false`. The other policies drop it.

Config keys and defaults:

| Key | Default | Range |
|---|---|---|
| `subtitle_backlog_policy` | `"auto"` | `auto`, `live_only`, `send_all` |
| `subtitle_max_live_delay_sec` | `10.0` | 1.0 to 120.0 |
| `subtitle_catchup_interval_sec` | `1.5` | 0.0 to 10.0 |

### 5.2 Pacing

A subtitle with `catchup_interval_sec` greater than `0` is a paced replay. The server sends it after the previous paced message, waiting that message's `catchup_interval_sec` first. Subtitles with `catchup_interval_sec` equal to `0` go out immediately.

### 5.3 Late-join replay

The server keeps the last 256 subtitles it actually sent (`REPLAY_BUFFER_MAX`). The oldest entries are dropped when the buffer is full. When a client connects:

1. The server sends `hello`.
2. It selects buffered subtitles by the policy in effect:
   - `auto`: subtitles whose `processed_at` is within the last `2 x subtitle_max_live_delay_sec` seconds (20 seconds with defaults);
   - `live_only`: within the last `subtitle_max_live_delay_sec` seconds;
   - `send_all`: everything in the buffer.
3. It sends the selected subtitles in order, with `is_replay` set to `true` on every one, whatever its original value.
4. Each replayed subtitle is spaced by the `catchup_interval_sec` of the one before it. The first is sent immediately.
5. Live subtitles produced during the burst are queued for the client and sent after the burst.

### 5.4 Interpreting `is_replay`

`is_replay: true` means one of two things, and the flag does not say which:

- the subtitle is being re-sent to a client that just connected; or
- the subtitle is live but was more than 1 second old when it was processed (policies `auto` and `send_all`).

A client that wants only fresh subtitles can skip every message with `is_replay: true`. That also skips delayed live subtitles. A client that wants all text can ignore the flag.

The bundled overlay uses the flag only to choose its stacked render mode. It never drops a subtitle because of the flag. The example clients in section 8 do skip flagged messages on purpose, so they also skip delayed live subtitles.

### 5.5 Delivery guarantees and slow clients

- There are no acknowledgements, sequence numbers, or resume tokens. A client that disconnects misses everything until it reconnects. On reconnection it gets the replay described above.
- Each client has an outgoing queue of 64 frames and a retry queue of 10 frames. Frames that do not fit in the outgoing queue wait in the retry queue. When the retry queue is full, its oldest frame is dropped and counted. The client is not disconnected. Queued frames are sent during the next broadcast, not when the client catches up. If no subtitle follows a burst, the last queued frames wait until one does.
- Replay buffer overflow is counted in `replay_drops`. Retry overflow is counted in `retry_drops`. Both counters appear in the network health report (`NetworkHealthReport`), not on the wire.
- The server does not de-duplicate. In a narrow timing window while a client is being admitted, a subtitle can arrive twice: once in the replay burst and once live. If your client must display each subtitle exactly once, de-duplicate by `id`.

---

## 6. Client-to-server traffic

The protocol is server-to-client only. The server does not interpret client messages:

- **Text and binary frames** from a client are read and discarded. They have no effect. The server reads them only after the late-join replay burst has been sent.
- **Ping** frames are answered with Pong. Because frames are read only after the replay burst, a Ping sent during a long burst is answered when the burst finishes.
- **Close** frames close the connection.

The server does not send its own WebSocket pings, and it has no idle timeout. A client that disappears without a Close frame (for example, a laptop that went to sleep) stays in the client count until TCP reports the connection dead. Such a client does not block broadcasts, and its queues stay bounded. A client that uses protocol-level keepalive should either turn it off or allow for delayed replies during replay. When the server shuts down, it sends a Close frame with code `1000` and reason `"Server shutdown"`.

---

## 7. Security

- **Loopback only.** The listener is bound to `127.0.0.1`. Connections from non-loopback addresses are dropped before the handshake.
- **Origin filter.** The handshake checks the `Origin` header as follows:
  - **No `Origin` header** (native applications, scripts, most non-browser clients): accepted.
  - **More than one `Origin` header**: rejected with HTTP `403`.
  - **Accepted origins:**
    - `http://absolute` (the OBS Browser Source);
    - `http://tauri.localhost` and `https://tauri.localhost` (exact match, the desktop app's webview);
    - any `http` or `https` origin whose host is `localhost`, `127.0.0.1` or `::1`, on any port.
  - **Anything else**: rejected with HTTP `403`, body `Origen no permitido: <origin>`.
- **No authentication.** Any process on the same machine can connect and read subtitles. The Origin filter restricts browser pages only. A page served from any loopback origin, on any port, is allowed and can read the stream. A page opened from `file://` usually sends `Origin: null`, which is rejected.
- **Request path.** The server does not check the request path. Use `/` by convention.
- **Handshake timeout.** A client must finish the WebSocket handshake within 5 seconds.

---

## 8. Client examples

Browser clients must be served from `http://localhost` or `http://127.0.0.1`. A page opened from `file://` is rejected (see [section 7](#7-security)). The examples use the base port. Use the discovery procedure in [section 2](#2-connecting-and-discovering-the-port) to find the effective port.

### 8.1 JavaScript (browser)

```html
<script>
  const PORT = 8765; // base port; see "Connecting and discovering the port"
  const ws = new WebSocket(`ws://127.0.0.1:${PORT}`);
  let helloSeen = false;

  ws.onmessage = (event) => {
    const msg = JSON.parse(event.data);

    if (!helloSeen) {
      if (msg.type !== "hello" || msg.app !== "liveaudio") {
        ws.close();
        return;
      }
      helloSeen = true;
      console.log(`Connected: proto ${msg.proto}, port ${msg.port}`);
      return;
    }

    if ("type" in msg) return;  // theme, speech_activity_*, or a type added later
    if (msg.is_replay) return;  // late-join replay or delayed subtitle
    console.log(msg.text);
  };
</script>
```

### 8.2 Python (`websockets` library)

```python
import asyncio
import json

import websockets  # pip install websockets

URI = "ws://127.0.0.1:8765"  # base port; see "Connecting and discovering the port"


async def main() -> None:
    # ping_interval=None: the server answers pings only after its replay burst.
    async with websockets.connect(URI, ping_interval=None) as ws:
        hello = json.loads(await ws.recv())
        if hello.get("type") != "hello" or hello.get("app") != "liveaudio":
            raise SystemExit("Not a LiveAudio WebSocket server")
        print(f"Connected: proto {hello['proto']}, port {hello['port']}")

        async for frame in ws:
            msg = json.loads(frame)
            if "type" in msg:  # theme, speech_activity_*, or a type added later
                continue
            if msg.get("is_replay"):  # late-join replay or delayed subtitle
                continue
            print(msg["text"])


if __name__ == "__main__":
    asyncio.run(main())
```

---

## 9. OBS Studio setup

### Step 1: Add a Browser Source

1. In OBS, click **+** in the **Sources** panel.
2. Select **Browser**.
3. Give it a name, for example `LiveAudio Subtitles`.

### Step 2: Configure the source

| Property | Recommended value |
|---|---|
| **URL / Local file** | Enable **Local file** and select `liveaudio/assets/subtitulos_obs.html` in the project folder. |
| **Width** | `1920` |
| **Height** | `200` |
| **FPS** | `30` or `60` |
| **Shutdown source when not visible** | Off (keeps the WebSocket connected) |
| **Refresh browser when scene becomes active** | Off (avoids unnecessary reconnects) |

### Step 3: Position the source

Place it at the bottom center of the scene and adjust its size as needed.

### Step 4: Start LiveAudio

1. Start LiveAudio (the installed launcher, or `uv run liveaudio` in development) and press **Start system**.
2. Speak. Subtitles appear in OBS automatically.

### Using a custom port

Set `ws_port` in the LiveAudio configuration, restart LiveAudio, and add `?port=<port>` to the source's file path:

```
file:///path/to/liveaudio/assets/subtitulos_obs.html?port=9876
```

Without `?port=`, the overlay starts at `8765`. In both cases it scans `port` through `port + 9`.

### Transcript-only mode (no OBS)

In the Python app, you can save transcripts without sending subtitles to OBS:

1. Open the **Subtitles** tab.
2. Turn off **Send subtitles to OBS** (config key `obs_enabled`).
3. Transcripts are still saved to disk (JSONL and VTT).

> **Note (Rust desktop app and `liveaudio-cli`):** When `obs_enabled` is `false`, the server is not bound. Transcription and the transcript keep working. The desktop app reports the WebSocket port as `0` and shows `WS: off`, and the overlay URL command returns an error. The desktop app has no UI switch for this key, so set it in the configuration file.
>
> The Python engine also checks `obs_enabled` before it emits to the WebSocket (`liveaudio/core/engine.py`).

---

## 10. Overlay styles

The `style` field holds one of these values. The overlay falls back to `default` for any other value.

| Value | Look |
|---|---|
| `default` | White text on a semi-transparent black background, with a text shadow and a smooth enter/exit animation |
| `karaoke` | Words appear one after another with a `popIn` animation. Bright yellow with a black outline. |
| `neon` | Retro-futuristic style. Bright cyan border, uppercase text, wide letter spacing, dark background with glow |
| `minimal` | Clean, no background, subtle fade |
| `bold` | High contrast, heavy weight, fast animation |
| `rgb` | Each word gets a different rainbow color |
| `typewriter` | Words appear one by one, with a blinking cursor and a monospace font |

These visual descriptions were carried over from the previous version of this document and were not re-verified against the CSS.

---

## 11. Customizing the overlay

Edit `liveaudio/assets/subtitulos_obs.html` directly.

- **Font:** change `font-family` in the `body` rule of the `<style>` block.
- **On-screen time:** change the `SUB_LIFETIME_MS` constant (default `5000`, in milliseconds).
- **Colors and sizes at runtime:** send a `theme` message (see [section 4.3](#43-theme)). Nothing in the HTML needs to change for this.
- **Port:** use `?port=` in the source URL (see [section 9](#using-a-custom-port)).

---

## 12. Troubleshooting

| Symptom | Likely cause | What to do |
|---|---|---|
| Overlay shows nothing and never receives `hello` | LiveAudio is not running, or the base port in the URL does not match | Start LiveAudio. Check the startup log for `WebSocket server listening on ws://127.0.0.1:<port>` and add `?port=<port>` to the URL if the port differs. |
| Connection to the base port fails | The base port is in use | The server uses the next free port in `base` to `base + 9`. Use the effective port from the log or from `hello.port`. |
| The WebSocket handshake fails, and the server log or the network panel shows a 403 | The page's `Origin` is not allowed | Serve the page from `http://localhost` or `http://127.0.0.1`. Do not open it from `file://`. |
| Subtitles arrive late or in bursts | Backlog policy or pacing | Check `subtitle_backlog_policy`, `subtitle_max_live_delay_sec` and `subtitle_catchup_interval_sec`. See [section 5](#5-subtitle-delivery-backlog-policy-and-replay). |
| Subtitles are missing on a client | The client was too slow, or it was disconnected | Process messages without blocking. The server counts drops in `retry_drops` and `replay_drops`. |

---

*Last updated against the `liveaudio-network` crate (`protocol.rs`, `server.rs`, `replay.rs`, `security.rs`, `health.rs`) and the bundled overlay `liveaudio/assets/subtitulos_obs.html`.*
