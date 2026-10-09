// SPDX-License-Identifier: MIT

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::supervisor::error::SupervisorError;

pub const MAX_CHILD_FAILURES: usize = 3;
pub const CHILD_FAILURE_WINDOW_SEC: f64 = 300.0;
pub const RESPAWN_BACKOFF_BASE_SEC: f64 = 1.0;
pub const RESPAWN_BACKOFF_MAX_SEC: f64 = 15.0;

/// Policy tracker recording failures and calculating exponential backoff.
#[derive(Debug, Clone)]
pub struct RespawnTracker {
    max_failures: usize,
    window: Duration,
    base_backoff: Duration,
    max_backoff: Duration,
    failure_timestamps: VecDeque<Instant>,
}

impl Default for RespawnTracker {
    fn default() -> Self {
        Self::new(
            MAX_CHILD_FAILURES,
            Duration::from_secs_f64(CHILD_FAILURE_WINDOW_SEC),
            Duration::from_secs_f64(RESPAWN_BACKOFF_BASE_SEC),
            Duration::from_secs_f64(RESPAWN_BACKOFF_MAX_SEC),
        )
    }
}

impl RespawnTracker {
    pub fn new(
        max_failures: usize,
        window: Duration,
        base_backoff: Duration,
        max_backoff: Duration,
    ) -> Self {
        Self {
            max_failures,
            window,
            base_backoff,
            max_backoff,
            failure_timestamps: VecDeque::new(),
        }
    }

    /// Prune failures older than the sliding window.
    pub fn prune(&mut self) {
        let now = Instant::now();
        while let Some(&front) = self.failure_timestamps.front() {
            if now.duration_since(front) > self.window {
                self.failure_timestamps.pop_front();
            } else {
                break;
            }
        }
    }

    /// Record a new crash/failure. Returns the required backoff duration, or errors if threshold exceeded.
    pub fn record_failure(&mut self) -> Result<Duration, SupervisorError> {
        self.prune();
        self.failure_timestamps.push_back(Instant::now());

        let count = self.failure_timestamps.len();
        if count > self.max_failures {
            return Err(SupervisorError::MaxFailuresExceeded {
                count,
                window_sec: self.window.as_secs_f64(),
            });
        }

        let exponent = (count as u32).saturating_sub(1);
        let multiplier = 2u64.pow(exponent.min(6));
        let computed = self.base_backoff.mul_f64(multiplier as f64);
        let backoff = computed.min(self.max_backoff);

        Ok(backoff)
    }

    /// Current count of failures in the active window.
    pub fn failure_count(&mut self) -> usize {
        self.prune();
        self.failure_timestamps.len()
    }

    /// Reset all failure history.
    pub fn reset(&mut self) {
        self.failure_timestamps.clear();
    }
}
