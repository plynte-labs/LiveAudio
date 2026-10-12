// SPDX-License-Identifier: MIT

pub mod emitter;
pub mod error;
pub mod gate;
pub mod process;
pub mod respawn;
pub mod traits;
pub mod types;
pub mod watchdog;

pub use emitter::HealthEmitter;
pub use error::SupervisorError;
pub use gate::FirstClientGate;
pub use process::{ProcessSupervisor, WorkerProcessConfig};
pub use respawn::{
    RespawnTracker, CHILD_FAILURE_WINDOW_SEC, MAX_CHILD_FAILURES, RESPAWN_BACKOFF_BASE_SEC,
    RESPAWN_BACKOFF_MAX_SEC,
};
pub use traits::{ServiceSupervisor, SupervisorBoxFuture};
pub use types::{ChildStatus, SupervisorHealthReport};
pub use watchdog::is_parent_alive;
