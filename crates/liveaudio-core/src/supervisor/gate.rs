// SPDX-License-Identifier: MIT

use std::sync::atomic::{AtomicBool, Ordering};

/// One-shot gate: triggers the lazy start of audio/ASR pipelines upon first WebSocket client connection.
#[derive(Debug, Default)]
pub struct FirstClientGate {
    fired: AtomicBool,
}

impl FirstClientGate {
    pub fn new() -> Self {
        Self {
            fired: AtomicBool::new(false),
        }
    }

    /// Returns `true` exactly once on the first call, and `false` on all subsequent calls.
    pub fn fire(&self) -> bool {
        self.fired
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    /// Returns whether the gate has already fired.
    pub fn is_fired(&self) -> bool {
        self.fired.load(Ordering::SeqCst)
    }

    /// Reset gate state (e.g. across full pipeline restarts).
    pub fn reset(&self) {
        self.fired.store(false, Ordering::SeqCst);
    }
}
