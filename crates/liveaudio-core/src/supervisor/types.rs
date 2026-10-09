// SPDX-License-Identifier: MIT

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Health status of a supervised child process or internal worker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChildStatus {
    pub name: String,
    pub pid: Option<u32>,
    pub is_alive: bool,
    pub restart_count: usize,
    pub last_heartbeat_timestamp: u64,
}

/// Comprehensive health and telemetry report produced by the supervisor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SupervisorHealthReport {
    pub service_pid: u32,
    pub parent_pid: u32,
    pub overall_state: String,
    pub children: HashMap<String, ChildStatus>,
    pub failure_count: usize,
    pub uptime_sec: f64,
}
