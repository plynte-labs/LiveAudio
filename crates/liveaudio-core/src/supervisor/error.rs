// SPDX-License-Identifier: MIT

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SupervisorError {
    #[error(
        "Child failure threshold exceeded ({count} crashes in {window_sec:.1}s), failing fast"
    )]
    MaxFailuresExceeded { count: usize, window_sec: f64 },

    #[error("Parent process PID {parent_pid} has exited")]
    ParentDied { parent_pid: u32 },

    #[error("Failed to spawn supervised child process '{name}': {reason}")]
    ProcessSpawnFailed { name: String, reason: String },

    #[error("Timed out waiting for child processes to shutdown")]
    ShutdownTimeout,

    #[error("WebSocket port {0} conflict and fallback candidates exhausted")]
    PortConflict(u16),

    #[error("Child process '{name}' timed out during watchdog probe")]
    WatchdogTimeout { name: String },

    #[error("Supervisor internal error: {0}")]
    InternalError(String),
}
