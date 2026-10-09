// SPDX-License-Identifier: MIT

use serde::{Deserialize, Serialize};

/// Versioned commands sent from Client / UI to LiveAudio Core daemon.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action", content = "params")]
pub enum DaemonCommand {
    StartPipeline {
        device_id: Option<String>,
        model_name: Option<String>,
    },
    StopPipeline,
    SwitchModel {
        model_name: String,
        device: Option<String>,
        compute_type: Option<String>,
    },
    GetStatus,
    GetDevices,
    SetBacklogPolicy {
        policy: String,
    },
    SetThemeTokens {
        tokens: std::collections::HashMap<String, String>,
    },
}

/// Versioned responses returned from Core daemon to Client / UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status")]
pub enum DaemonResponse {
    Ok {
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<serde_json::Value>,
    },
    Error {
        code: String,
        message: String,
    },
}
