// SPDX-License-Identifier: MIT

pub mod error;
pub mod traits;
pub mod types;

pub use error::AsrError;
pub use traits::{AsrBackend, AsrBoxFuture};
pub use types::{
    AsrHealthStatus, AsrModelConfig, AsrTranscriptionRequest, AsrTranscriptionResponse,
    TranscriptionSegment,
};
