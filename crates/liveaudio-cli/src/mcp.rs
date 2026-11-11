// SPDX-License-Identifier: MIT

//! `liveaudio-cli mcp`: Model Context Protocol server over stdio (JSON-RPC 2.0, one message per
//! line). Trust boundary is the filesystem: the server only reads and patches `config.json` in the
//! LiveAudio data home, and every tool refuses unless the user enabled at least one MCP switch in the app.

use liveaudio_core::config::patch::{
    apply_patch, config_schema_for, patchable_values, PatchError,
};
use liveaudio_core::config::{get_data_home, load_config_readonly};
use liveaudio_core::service_lock::service_lock_holder;
use serde_json::{json, Value};
use std::io;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Largest JSON-RPC message accepted on stdin, in bytes. Longer lines are discarded with an error.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

/// Protocol revisions this server speaks, newest first.
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18"];

const JSONRPC_PARSE_ERROR: i64 = -32700;
const JSONRPC_INVALID_REQUEST: i64 = -32600;
const JSONRPC_METHOD_NOT_FOUND: i64 = -32601;
const JSONRPC_INVALID_PARAMS: i64 = -32602;

/// One unit read from the client: a message, an oversized line that was discarded, or end of input.
#[derive(Debug, PartialEq, Eq)]
pub enum StdinLine {
    Message(Vec<u8>),
    TooLong,
    Eof,
}

/// Reads one newline-terminated line of at most `max` bytes (newline excluded). An oversized line
/// is consumed up to its newline and reported as `TooLong`, so the server keeps reading.
pub async fn read_bounded_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    max: usize,
) -> io::Result<StdinLine> {
    let mut line = Vec::new();
    let mut too_long = false;
    loop {
        let (done, consumed) = {
            let available = reader.fill_buf().await?;
            if available.is_empty() {
                // End of input: a trailing unterminated line still counts as a message.
                return Ok(match (too_long, line.is_empty()) {
                    (true, _) => StdinLine::TooLong,
                    (false, true) => StdinLine::Eof,
                    (false, false) => StdinLine::Message(line),
                });
            }
            let newline = available.iter().position(|&b| b == b'\n');
            let content = &available[..newline.unwrap_or(available.len())];
            if !too_long {
                if line.len() + content.len() > max {
                    too_long = true;
                    line.clear();
                } else {
                    line.extend_from_slice(content);
                }
            }
            (
                newline.is_some(),
                newline.map_or(available.len(), |i| i + 1),
            )
        };
        reader.consume(consumed);
        if done {
            return Ok(if too_long {
                StdinLine::TooLong
            } else {
                StdinLine::Message(line)
            });
        }
    }
}

/// Serves MCP over the process stdin/stdout until stdin closes.
pub async fn run_stdio() -> io::Result<()> {
    let mut reader = BufReader::new(tokio::io::stdin());
    let mut stdout = tokio::io::stdout();
    loop {
        let response = match read_bounded_line(&mut reader, MAX_LINE_BYTES).await? {
            StdinLine::Eof => break,
            StdinLine::TooLong => Some(
                error_response(
                    Value::Null,
                    JSONRPC_INVALID_REQUEST,
                    "request exceeds the 1 MiB line limit",
                )
                .to_string(),
            ),
            StdinLine::Message(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => None,
            StdinLine::Message(bytes) => handle_bytes(&bytes),
        };
        if let Some(response) = response {
            stdout.write_all(response.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }
    Ok(())
}

/// Parses one raw stdin line. Invalid UTF-8 is a JSON parse error, not a reason to stop serving.
pub fn handle_bytes(bytes: &[u8]) -> Option<String> {
    match std::str::from_utf8(bytes) {
        Ok(text) => handle_line(text),
        Err(_) => Some(
            error_response(
                Value::Null,
                JSONRPC_PARSE_ERROR,
                "parse error: invalid UTF-8",
            )
            .to_string(),
        ),
    }
}

/// Parses one JSON-RPC line and returns the serialized response, or `None` for notifications.
pub fn handle_line(line: &str) -> Option<String> {
    let response = match serde_json::from_str::<Value>(line) {
        Ok(message) => handle_message(&message)?,
        Err(_) => error_response(Value::Null, JSONRPC_PARSE_ERROR, "parse error"),
    };
    Some(response.to_string())
}

/// Handles one JSON-RPC message. Returns `None` when no response is due (notifications).
pub fn handle_message(message: &Value) -> Option<Value> {
    let Some(object) = message.as_object() else {
        return Some(error_response(
            Value::Null,
            JSONRPC_INVALID_REQUEST,
            "request must be an object",
        ));
    };
    let id = object.get("id").cloned();
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        return id.map(|id| error_response(id, JSONRPC_INVALID_REQUEST, "missing method"));
    };
    // Requests carry an id; notifications do not and never get a response.
    let id = id?;
    let params = object.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => Ok(initialize_result(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tool_definitions()})),
        "tools/call" => call_tool_request(&params),
        _ => Err((
            JSONRPC_METHOD_NOT_FOUND,
            format!("method not found: {method}"),
        )),
    };
    Some(match result {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err((code, message)) => error_response(id, code, &message),
    })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn initialize_result(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let version = requested
        .filter(|v| SUPPORTED_PROTOCOL_VERSIONS.contains(v))
        .unwrap_or(SUPPORTED_PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": {"tools": {}},
        "serverInfo": {"name": "liveaudio", "version": env!("CARGO_PKG_VERSION")},
        "instructions": "Read and change LiveAudio settings. Call get_config_schema first.             Changes apply to the running app; restart_required reports when the ASR service must restart."
    })
}

const TOOL_NAMES: &[&str] = &[
    "get_config",
    "get_config_schema",
    "set_config",
    "get_status",
    "list_audio_devices",
];

fn tool_definitions() -> Value {
    let no_args = json!({"type": "object", "properties": {}, "additionalProperties": false});
    json!([
        {"name": "get_config", "description": "Current values of the agent-readable settings (no paths, no transcripts).", "inputSchema": no_args},
        {"name": "get_config_schema", "description": "Allowed settings with types, enums, ranges and whether a change needs a service restart.", "inputSchema": no_args},
        {"name": "set_config", "description": "Change allowed settings (partial patch). Unknown or restricted keys are rejected.",
         "inputSchema": {"type": "object", "required": ["settings"], "additionalProperties": false,
             "properties": {"settings": {"type": "object", "description": "Map of setting name to new value; see get_config_schema."}}}},
        {"name": "get_status", "description": "Whether the LiveAudio service is running and its WebSocket port. No content.", "inputSchema": no_args},
        {"name": "list_audio_devices", "description": "Audio input devices visible to LiveAudio (ids and names).", "inputSchema": no_args}
    ])
}

fn call_tool_request(params: &Value) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((JSONRPC_INVALID_PARAMS, "missing tool name".to_string()))?;
    if !TOOL_NAMES.contains(&name) {
        return Err((JSONRPC_INVALID_PARAMS, format!("unknown tool: {name}")));
    }
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let (text, is_error) = match run_tool(name, &arguments) {
        Ok(value) => (
            serde_json::to_string_pretty(&value).unwrap_or_default(),
            false,
        ),
        Err(message) => (message, true),
    };
    Ok(json!({"content": [{"type": "text", "text": text}], "isError": is_error}))
}

/// Config as seen right now; every tool refuses unless the user enabled at least one MCP switch in the app.
fn enabled_config() -> Result<liveaudio_core::config::LiveAudioConfig, String> {
    let (cfg, _) = load_config_readonly();
    if !cfg.mcp_style_enabled && !cfg.mcp_system_enabled {
        Err(PatchError::Disabled.to_string())
    } else {
        Ok(cfg)
    }
}

fn run_tool(name: &str, arguments: &Value) -> Result<Value, String> {
    let cfg = enabled_config()?;
    match name {
        "get_config" => {
            Ok(Value::Object(patchable_values(&cfg)))
        }
        "get_config_schema" => Ok(config_schema_for(cfg.mcp_style_enabled, cfg.mcp_system_enabled)),
        "get_status" => {
            let holder = service_lock_holder(&get_data_home())
                .map_err(|e| format!("service status unavailable: {e}"))?;
            Ok(json!({
                "service_running": holder.is_some(),
                "ws_port": holder.and_then(|h| h.ws_port),
            }))
        }
        "list_audio_devices" => {
            let devices = liveaudio_audio::enumerate_cpal_devices().map_err(|e| e.to_string())?;
            Ok(Value::Array(
                devices
                    .iter()
                    .map(|d| json!({"id": d.id, "name": d.name, "is_default": d.is_default}))
                    .collect(),
            ))
        }
        "set_config" => {
            let settings = arguments
                .get("settings")
                .and_then(Value::as_object)
                .ok_or_else(|| "settings must be an object of allowed keys".to_string())?;
            let outcome = apply_patch(settings).map_err(|e| e.to_string())?;
            Ok(json!({"applied": outcome.applied, "restart_required": outcome.restart_required}))
        }
        other => Err(format!("unknown tool: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;
    use std::sync::Mutex;

    /// Serializes tests that mutate the process-wide `LIVEAUDIO_HOME` variable.
    static HOME_LOCK: Mutex<()> = Mutex::new(());

    fn with_home_switches<T>(
        name: &str,
        style: bool,
        system: bool,
        f: impl FnOnce(&std::path::Path) -> T,
    ) -> T {
        let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir =
            std::env::temp_dir().join(format!("liveaudio-mcp-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = liveaudio_core::config::LiveAudioConfig {
            mcp_style_enabled: style,
            mcp_system_enabled: system,
            ..Default::default()
        };
        liveaudio_core::config::save_to_path(&cfg, &dir.join("config.json")).unwrap();
        std::env::set_var("LIVEAUDIO_HOME", &dir);
        let out = f(&dir);
        std::env::remove_var("LIVEAUDIO_HOME");
        let _ = std::fs::remove_dir_all(&dir);
        out
    }

    fn with_home<T>(name: &str, enabled: bool, f: impl FnOnce(&std::path::Path) -> T) -> T {
        with_home_switches(name, enabled, enabled, f)
    }

    fn call(name: &str, arguments: Value) -> Value {
        handle_message(&json!({
            "jsonrpc": "2.0", "id": 7, "method": "tools/call",
            "params": {"name": name, "arguments": arguments}
        }))
        .expect("tools/call always gets a response")
    }

    fn tool_text(response: &Value) -> (bool, String) {
        let result = &response["result"];
        (
            result["isError"].as_bool().unwrap_or(false),
            result["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        )
    }

    #[tokio::test]
    async fn bounded_reader_splits_lines_and_discards_oversized_ones() {
        let input: Vec<u8> = [
            b"ab\n".as_slice(),
            &[b'a'; 20],
            b"\n",
            b"ok\n",
            b"tail",
        ]
        .concat();
        let mut reader = BufReader::new(&input[..]);
        assert_eq!(
            read_bounded_line(&mut reader, 10).await.unwrap(),
            StdinLine::Message(b"ab".to_vec())
        );
        assert_eq!(
            read_bounded_line(&mut reader, 10).await.unwrap(),
            StdinLine::TooLong
        );
        assert_eq!(
            read_bounded_line(&mut reader, 10).await.unwrap(),
            StdinLine::Message(b"ok".to_vec())
        );
        assert_eq!(
            read_bounded_line(&mut reader, 10).await.unwrap(),
            StdinLine::Message(b"tail".to_vec())
        );
        assert_eq!(
            read_bounded_line(&mut reader, 10).await.unwrap(),
            StdinLine::Eof
        );
    }

    #[test]
    fn invalid_utf8_gets_a_parse_error_and_the_server_keeps_going() {
        let response: Value =
            serde_json::from_str(&handle_bytes(&[0xff, 0xfe, b'{']).unwrap()).unwrap();
        assert_eq!(response["error"]["code"], JSONRPC_PARSE_ERROR);
        assert!(response["id"].is_null());
        let ping: Value = serde_json::from_str(
            &handle_bytes(br#"{"jsonrpc":"2.0","id":9,"method":"ping"}"#).unwrap(),
        )
        .unwrap();
        assert_eq!(ping["id"], 9);
    }

    #[test]
    fn initialize_negotiates_a_supported_protocol_version() {
        let response = handle_message(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                       "clientInfo": {"name": "t", "version": "1"}}
        }))
        .unwrap();
        assert_eq!(response["id"], 1);
        assert_eq!(response["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(response["result"]["serverInfo"]["name"], "liveaudio");
        assert!(response["result"]["capabilities"]["tools"].is_object());

        let fallback = handle_message(&json!({
            "jsonrpc": "2.0", "id": 2, "method": "initialize",
            "params": {"protocolVersion": "1999-01-01"}
        }))
        .unwrap();
        assert_eq!(
            fallback["result"]["protocolVersion"],
            SUPPORTED_PROTOCOL_VERSIONS[0]
        );
    }

    #[test]
    fn notifications_get_no_response_and_ping_gets_empty_result() {
        assert!(
            handle_message(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
                .is_none()
        );
        let pong = handle_message(&json!({"jsonrpc": "2.0", "id": 3, "method": "ping"})).unwrap();
        assert_eq!(pong["result"], json!({}));
    }

    #[test]
    fn tools_list_exposes_the_agreed_tools_with_input_schemas() {
        let response =
            handle_message(&json!({"jsonrpc": "2.0", "id": 4, "method": "tools/list"})).unwrap();
        let tools = response["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        for expected in [
            "get_config",
            "get_config_schema",
            "set_config",
            "get_status",
            "list_audio_devices",
        ] {
            assert!(names.contains(&expected), "missing tool {expected}");
        }
        assert!(tools.iter().all(|t| t["inputSchema"]["type"] == "object"));
    }

    #[test]
    fn protocol_errors_use_json_rpc_codes() {
        let unknown =
            handle_message(&json!({"jsonrpc": "2.0", "id": 5, "method": "resources/list"}))
                .unwrap();
        assert_eq!(unknown["error"]["code"], JSONRPC_METHOD_NOT_FOUND);
        let bad_line: Value = serde_json::from_str(&handle_line("{not json").unwrap()).unwrap();
        assert_eq!(bad_line["error"]["code"], JSONRPC_PARSE_ERROR);
        assert!(bad_line["id"].is_null());
        let unknown_tool = call("delete_everything", json!({}));
        assert_eq!(unknown_tool["error"]["code"], JSONRPC_INVALID_PARAMS);
    }

    #[test]
    fn every_tool_refuses_while_mcp_is_disabled() {
        with_home("gate", false, |dir| {
            let before = std::fs::read(dir.join("config.json")).unwrap();
            for (name, args) in [
                ("get_config", json!({})),
                ("get_config_schema", json!({})),
                ("get_status", json!({})),
                ("list_audio_devices", json!({})),
                (
                    "set_config",
                    json!({"settings": {"subtitle_style": "neon"}}),
                ),
            ] {
                let (is_error, text) = tool_text(&call(name, args));
                assert!(is_error, "{name} must refuse");
                assert!(text.contains("disabled"), "{name}: {text}");
            }
            assert_eq!(std::fs::read(dir.join("config.json")).unwrap(), before);
        });
    }

    #[test]
    fn get_config_returns_only_allowlisted_fields() {
        with_home("read", true, |_| {
            let (is_error, text) = tool_text(&call("get_config", json!({})));
            assert!(!is_error);
            let values: Map<String, Value> = serde_json::from_str(&text).unwrap();
            assert!(values.contains_key("subtitle_style"));
            assert!(!values.contains_key("output_dir"));
            assert!(!values.contains_key("mcp_enabled"));
            assert!(!values.contains_key("mcp_style_enabled"));
            assert!(!values.contains_key("mcp_system_enabled"));
            assert!(!values.contains_key("ws_allowed_loopback_ports"));
        });
    }

    #[test]
    fn set_config_applies_allowed_keys_and_rejects_the_rest() {
        with_home("write", true, |dir| {
            let (is_error, text) = tool_text(&call(
                "set_config",
                json!({"settings": {"subtitle_style": "neon"}}),
            ));
            assert!(!is_error, "{text}");
            assert!(text.contains("neon"));
            let saved = liveaudio_core::config::load_from_path(&dir.join("config.json")).unwrap();
            assert_eq!(saved.subtitle_style, "neon");
            assert!(saved.mcp_style_enabled && saved.mcp_system_enabled);

            for settings in [
                json!({"output_dir": "C:/elsewhere"}),
                json!({"mcp_enabled": true}),
                json!({"mcp_style_enabled": true}),
                json!({"mcp_system_enabled": true}),
                json!({"ws_allowed_loopback_ports": [5173]}),
                json!({"nope": 1}),
            ] {
                let (is_error, _) = tool_text(&call("set_config", json!({ "settings": settings })));
                assert!(is_error, "{settings} must be rejected");
            }
            let (is_error, _) =
                tool_text(&call("set_config", json!({"settings": "not-an-object"})));
            assert!(is_error);
            let after = liveaudio_core::config::load_from_path(&dir.join("config.json")).unwrap();
            assert_eq!(after.output_dir, saved.output_dir);
            assert!(
                after.mcp_style_enabled && after.mcp_system_enabled,
                "set_config must not flip the opt-in flags"
            );
        });
    }

    #[test]
    fn style_only_opt_in_allows_style_and_rejects_system() {
        with_home_switches("style-only", true, false, |dir| {
            // get_config returns only style keys
            let (is_error, text) = tool_text(&call("get_config", json!({})));
            assert!(!is_error);
            let values: Map<String, Value> = serde_json::from_str(&text).unwrap();
            assert!(values.contains_key("subtitle_style"));
            assert!(values.contains_key("subtitle_theme"));
            assert!(!values.contains_key("ws_port"));
            assert!(!values.contains_key("device"));

            // get_config_schema returns only style keys
            let (is_error, schema_text) = tool_text(&call("get_config_schema", json!({})));
            assert!(!is_error);
            let schema: Value = serde_json::from_str(&schema_text).unwrap();
            assert!(schema.get("subtitle_style").is_some());
            assert!(schema.get("ws_port").is_none());

            // get_status and list_audio_devices work with style-only switch
            let (is_error, _) = tool_text(&call("get_status", json!({})));
            assert!(!is_error);
            let (is_error, _) = tool_text(&call("list_audio_devices", json!({})));
            assert!(!is_error);

            // set_config allows style change
            let (is_error, text) =
                tool_text(&call("set_config", json!({"settings": {"subtitle_style": "neon"}})));
            assert!(!is_error, "{text}");
            assert!(text.contains("neon"));

            // set_config rejects system change with clear error
            let (is_error, text) =
                tool_text(&call("set_config", json!({"settings": {"ws_port": 9000}})));
            assert!(is_error);
            assert_eq!(
                text,
                "MCP access to system settings is disabled; enable it in LiveAudio settings (Advanced)"
            );

            // mixed set_config is atomic: rejected without writing
            let before = std::fs::read(dir.join("config.json")).unwrap();
            let (is_error, text) = tool_text(&call(
                "set_config",
                json!({"settings": {"subtitle_style": "minimal", "ws_port": 9000}}),
            ));
            assert!(is_error);
            assert_eq!(
                text,
                "MCP access to system settings is disabled; enable it in LiveAudio settings (Advanced)"
            );
            assert_eq!(std::fs::read(dir.join("config.json")).unwrap(), before);
        });
    }

    #[test]
    fn system_only_opt_in_allows_system_and_rejects_style() {
        with_home_switches("system-only", false, true, |_| {
            // get_config returns only system keys
            let (is_error, text) = tool_text(&call("get_config", json!({})));
            assert!(!is_error);
            let values: Map<String, Value> = serde_json::from_str(&text).unwrap();
            assert!(values.contains_key("ws_port"));
            assert!(!values.contains_key("subtitle_style"));

            // get_config_schema returns only system keys
            let (is_error, schema_text) = tool_text(&call("get_config_schema", json!({})));
            assert!(!is_error);
            let schema: Value = serde_json::from_str(&schema_text).unwrap();
            assert!(schema.get("ws_port").is_some());
            assert!(schema.get("subtitle_style").is_none());

            // set_config allows system change
            let (is_error, text) =
                tool_text(&call("set_config", json!({"settings": {"ws_port": 9000}})));
            assert!(!is_error, "{text}");

            // set_config rejects style change with clear error
            let (is_error, text) =
                tool_text(&call("set_config", json!({"settings": {"subtitle_style": "neon"}})));
            assert!(is_error);
            assert_eq!(
                text,
                "MCP access to style settings is disabled; enable it in LiveAudio settings (Advanced)"
            );
        });
    }

    #[test]
    fn get_status_reports_service_state_without_content() {
        with_home("status", true, |_| {
            let (is_error, text) = tool_text(&call("get_status", json!({})));
            assert!(!is_error);
            let status: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(status["service_running"], false);
            assert!(status.get("transcript").is_none());
        });
    }
}
