// SPDX-License-Identifier: MIT

pub mod error;
pub mod mock;
pub mod pre_buffer;
pub mod silero;
pub mod traits;
pub mod types;

pub use error::VadError;
pub use mock::MockVadEngine;
pub use pre_buffer::PreBuffer;
pub use silero::{SileroVad, BUNDLED_SILERO_VAD_V6};
pub use traits::VadEngine;
pub use types::{SpeechEndReason, VadConfig, VadFrameDecision, VadTransition};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pre_buffer_pad_conversion_and_drain() {
        // 96ms @ 16kHz, 512 chunk size: 96/1000 * 16000 = 1536 samples / 512 = 3 chunks
        let chunks = PreBuffer::chunks_from_pad(96, 16000, 512);
        assert_eq!(chunks, 3);

        let mut pre_buf = PreBuffer::new(3);
        assert!(pre_buf.is_empty());

        pre_buf.push(vec![0.1; 512]);
        pre_buf.push(vec![0.2; 512]);
        pre_buf.push(vec![0.3; 512]);
        assert_eq!(pre_buf.len(), 3);

        // Fourth push discards oldest (0.1)
        pre_buf.push(vec![0.4; 512]);
        assert_eq!(pre_buf.len(), 3);

        let drained = pre_buf.drain_onset();
        assert_eq!(drained.len(), 3);
        assert_eq!(drained[0][0], 0.2);
        assert_eq!(drained[1][0], 0.3);
        assert_eq!(drained[2][0], 0.4);
        assert!(pre_buf.is_empty());
    }
}
