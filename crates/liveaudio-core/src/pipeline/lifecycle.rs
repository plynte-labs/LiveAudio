// SPDX-License-Identifier: MIT

use tokio_util::sync::CancellationToken;

/// Orchestrates hierarchical cancellation and graceful shutdown for all pipeline tasks.
#[derive(Debug, Clone)]
pub struct PipelineLifecycleManager {
    root_token: CancellationToken,
}

impl Default for PipelineLifecycleManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineLifecycleManager {
    pub fn new() -> Self {
        Self {
            root_token: CancellationToken::new(),
        }
    }

    /// Primary cancellation token for the pipeline root.
    pub fn token(&self) -> CancellationToken {
        self.root_token.clone()
    }

    /// Create a child cancellation token for a specific subsystem (audio, vad, asr, sink, ws).
    pub fn child_token(&self) -> CancellationToken {
        self.root_token.child_token()
    }

    /// Trigger cancellation across all child tasks and wait loops.
    pub fn cancel(&self) {
        self.root_token.cancel();
    }

    /// Check if cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.root_token.is_cancelled()
    }

    /// Asynchronously wait until cancellation is signalled.
    pub async fn cancelled(&self) {
        self.root_token.cancelled().await;
    }
}
