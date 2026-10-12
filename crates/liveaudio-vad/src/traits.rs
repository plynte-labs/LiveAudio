// SPDX-License-Identifier: MIT

use crate::error::VadError;
use crate::types::{VadConfig, VadFrameDecision};

/// Abstract Voice Activity Detection (VAD) Engine.
///
/// Implemented by ONNX/Silero VAD or mock VAD engines.
/// Must be safe to transfer and share across threads (`Send + Sync + 'static`).
pub trait VadEngine: Send + Sync + 'static {
    /// Evaluate an incoming 16kHz audio chunk (default: 512 samples = 32ms) and produce a speech decision.
    fn evaluate_chunk(&mut self, chunk: &[f32]) -> Result<VadFrameDecision, VadError>;

    /// Reset internal recurrent neural network states, onset counters, and hysteresis buffers.
    fn reset(&mut self);

    /// Borrow the active configuration parameters.
    fn config(&self) -> &VadConfig;

    /// Update configuration thresholds and timeouts dynamically without reloading the model.
    fn update_config(&mut self, config: VadConfig) -> Result<(), VadError>;
}
