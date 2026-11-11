// SPDX-License-Identifier: MIT

//! End-to-end: drives `liveaudio-cli mcp` over stdio with JSON-RPC lines, against an isolated
//! `LIVEAUDIO_HOME` so the real user config is never read or written.

use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn temp_home(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("liveaudio-mcp-e2e-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs one stdio session with the given request lines and returns the parsed responses.
fn run_session(home: &Path, requests: &[Value]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_liveaudio-cli"))
        .arg("mcp")
        .env("LIVEAUDIO_HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn liveaudio-cli mcp");
    {
        let stdin = child.stdin.as_mut().unwrap();
        for request in requests {
            writeln!(stdin, "{request}").unwrap();
        }
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("wait for mcp server");
    assert!(
        output.status.success(),
        "server exited with {:?}",
        output.status
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).expect("each stdout line is JSON-RPC"))
        .collect()
}

fn response_for(responses: &[Value], id: i64) -> &Value {
    responses
        .iter()
        .find(|r| r["id"] == id)
        .unwrap_or_else(|| panic!("no response for id {id}: {responses:?}"))
}

fn call(id: i64, name: &str, arguments: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
           "params": {"name": name, "arguments": arguments}})
}

#[test]
fn initialize_list_and_call_tools_over_stdio() {
    let home = temp_home("enabled");
    std::fs::write(home.join("config.json"), r#"{"mcp_style_enabled": true, "mcp_system_enabled": true}"#).unwrap();

    let responses = run_session(
        &home,
        &[
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                   "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                              "clientInfo": {"name": "test", "version": "0"}}}),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
            call(3, "get_config", json!({})),
            call(
                4,
                "set_config",
                json!({"settings": {"subtitle_style": "neon"}}),
            ),
            call(
                5,
                "set_config",
                json!({"settings": {"output_dir": "C:/elsewhere"}}),
            ),
            call(
                6,
                "set_config",
                json!({"settings": {"ws_allowed_loopback_ports": [5173]}}),
            ),
            call(7, "get_config", json!({})),
        ],
    );

    assert_eq!(
        responses.len(),
        7,
        "notifications get no response: {responses:?}"
    );
    let init = response_for(&responses, 1);
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "liveaudio");

    let tools = response_for(&responses, 2)["result"]["tools"]
        .as_array()
        .unwrap()
        .clone();
    assert!(tools.iter().any(|t| t["name"] == "set_config"));

    let before = response_for(&responses, 3);
    assert_eq!(before["result"]["isError"], false);

    let accepted = response_for(&responses, 4);
    assert_eq!(accepted["result"]["isError"], false, "{accepted}");
    let accepted_text = accepted["result"]["content"][0]["text"].as_str().unwrap();
    assert!(accepted_text.contains("neon"));

    let rejected = response_for(&responses, 5);
    assert_eq!(rejected["result"]["isError"], true, "{rejected}");

    let rejected_loopback = response_for(&responses, 6);
    assert_eq!(rejected_loopback["result"]["isError"], true, "{rejected_loopback}");
    assert!(rejected_loopback["result"]["content"][0]["text"].as_str().unwrap().contains("cannot be changed via MCP"));

    let after = response_for(&responses, 7)["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    let values: Value = serde_json::from_str(after).unwrap();
    assert_eq!(values["subtitle_style"], "neon");
    assert!(values.get("output_dir").is_none());
    assert!(values.get("ws_allowed_loopback_ports").is_none());

    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn every_tool_is_refused_and_config_untouched_while_disabled() {
    let home = temp_home("disabled");
    std::fs::write(home.join("config.json"), r#"{"mcp_style_enabled": false, "mcp_system_enabled": false}"#).unwrap();
    let before = std::fs::read(home.join("config.json")).unwrap();

    let responses = run_session(
        &home,
        &[
            call(1, "get_config", json!({})),
            call(
                2,
                "set_config",
                json!({"settings": {"subtitle_style": "neon"}}),
            ),
            call(3, "set_config", json!({"settings": {"mcp_style_enabled": true}})),
        ],
    );

    for id in [1, 2, 3] {
        let result = &response_for(&responses, id)["result"];
        assert_eq!(result["isError"], true, "id {id}: {result}");
        assert!(result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("disabled"));
    }
    assert_eq!(std::fs::read(home.join("config.json")).unwrap(), before);

    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn stdio_enforces_style_only_and_system_only_switches() {
    let home = temp_home("style-only-stdio");
    std::fs::write(home.join("config.json"), r#"{"mcp_style_enabled": true, "mcp_system_enabled": false}"#).unwrap();

    let responses = run_session(
        &home,
        &[
            call(1, "get_config", json!({})),
            call(2, "set_config", json!({"settings": {"subtitle_style": "neon"}})),
            call(3, "set_config", json!({"settings": {"ws_port": 9000}})),
        ],
    );

    let cfg_val: Value = serde_json::from_str(response_for(&responses, 1)["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(cfg_val.get("subtitle_style").is_some());
    assert!(cfg_val.get("ws_port").is_none());

    let set_style = response_for(&responses, 2);
    assert_eq!(set_style["result"]["isError"], false);

    let set_sys = response_for(&responses, 3);
    assert_eq!(set_sys["result"]["isError"], true);
    assert_eq!(
        set_sys["result"]["content"][0]["text"].as_str().unwrap(),
        "MCP access to system settings is disabled; enable it in LiveAudio settings (Advanced)"
    );

    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn oversized_and_invalid_utf8_lines_do_not_stop_the_server() {
    let home = temp_home("hostile-input");
    std::fs::write(home.join("config.json"), r#"{"mcp_enabled": true}"#).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_liveaudio-cli"))
        .arg("mcp")
        .env("LIVEAUDIO_HOME", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn liveaudio-cli mcp");
    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(&vec![b'a'; 1024 * 1024 + 16]).unwrap();
        stdin.write_all(b"\n\xff\xfe\n").unwrap();
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": 42, "method": "ping"})
        )
        .unwrap();
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("wait for mcp server");
    assert!(
        output.status.success(),
        "server must survive hostile input: {:?}",
        output.status
    );
    let responses: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 3, "{responses:?}");
    assert_eq!(responses[0]["error"]["code"], -32600);
    assert_eq!(responses[1]["error"]["code"], -32700);
    assert_eq!(responses[2]["id"], 42);

    let _ = std::fs::remove_dir_all(&home);
}
