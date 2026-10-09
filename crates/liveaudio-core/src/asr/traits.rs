// SPDX-License-Identifier: MIT

use std::future::Future;
use std::pin::Pin;

use crate::asr::error::AsrError;
use crate::asr::types::{
    AsrHealthStatus, AsrModelConfig, AsrTranscriptionRequest, AsrTranscriptionResponse,
};

pub type AsrBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Abstract Automatic Speech Recognition (ASR) backend.
///
/// Implemented by local Faster-Whisper sidecars, ONNX models, or remote endpoints.
/// Must be thread-safe (`Send + Sync + 'static`).
pub trait AsrBackend: Send + Sync + 'static {
    /// Initialize model weights and spawn backend worker processes/threads.
    fn start(&mut self) -> AsrBoxFuture<'_, Result<(), AsrError>>;

    /// Cleanly terminate the backend worker and release GPU VRAM.
    fn stop(&mut self) -> AsrBoxFuture<'_, Result<(), AsrError>>;

    /// Query instantaneous health and worker responsiveness.
    fn health_check(&self) -> AsrHealthStatus;

    /// Submit a speech audio utterance for asynchronous transcription.
    fn transcribe(
        &self,
        request: AsrTranscriptionRequest,
    ) -> AsrBoxFuture<'_, Result<AsrTranscriptionResponse, AsrError>>;

    /// Hot-swap active model weights/parameters without terminating the supervisor.
    fn switch_model(&mut self, config: AsrModelConfig) -> AsrBoxFuture<'_, Result<(), AsrError>>;
}
