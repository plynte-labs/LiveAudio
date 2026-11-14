// SPDX-License-Identifier: MIT

// Own test binary (own process): the unit tests in silero.rs initialize the process-global
// ONNX Runtime, so this check is only deterministic when nothing else in the process did.

use liveaudio_vad::{SileroVad, VadConfig, VadError};

#[test]
fn silero_creation_without_explicit_runtime_returns_an_error_not_a_panic() {
    let result = SileroVad::new(VadConfig::default());
    assert!(
        matches!(result, Err(VadError::RuntimeUnavailable(_))),
        "VAD construction without app-owned runtime initialization must fail actionably"
    );
}
