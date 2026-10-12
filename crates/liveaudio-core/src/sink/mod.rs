// SPDX-License-Identifier: MIT

pub mod composite;
pub mod error;
pub mod jsonl;
pub mod traits;
pub mod types;
pub mod webvtt;

pub use composite::CompositeSink;
pub use error::SinkError;
pub use jsonl::JsonlSink;
pub use traits::TranscriptionSink;
pub use types::{SinkStats, TranscriptionCue};
pub use webvtt::{format_vtt_timestamp, WebVttSink};
