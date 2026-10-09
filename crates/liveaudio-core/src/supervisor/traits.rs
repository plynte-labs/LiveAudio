// SPDX-License-Identifier: MIT

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::supervisor::error::SupervisorError;
use crate::supervisor::types::SupervisorHealthReport;

pub type SupervisorBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Abstract supervisor managing child processes, health emission, and watchdogs.
pub trait ServiceSupervisor: Send + Sync + 'static {
    /// Launch supervised child processes and start watchdog monitoring loops.
    fn start(&mut self) -> SupervisorBoxFuture<'_, Result<(), SupervisorError>>;

    /// Perform a health check poll across all supervised subsystems.
    fn poll_health(&mut self) -> SupervisorHealthReport;

    /// Cleanly terminate all child processes with a bounded grace period before hard termination.
    fn stop(&mut self, timeout: Duration) -> SupervisorBoxFuture<'_, Result<(), SupervisorError>>;

    /// Restart a failed child process using the supervisor's backoff and respawn policy.
    fn restart_child(
        &mut self,
        child_name: &str,
    ) -> SupervisorBoxFuture<'_, Result<(), SupervisorError>>;
}
