// SPDX-License-Identifier: MIT

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::error::AudioError;
use crate::types::{AudioChunk, AudioDeviceInfo, AudioSourceStatus, AudioStreamConfig};

pub type AudioBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Asynchronous receiver for audio chunks arriving from the real-time capture layer.
pub struct AudioChunkReceiver {
    receiver: mpsc::Receiver<AudioChunk>,
    capture_error: Arc<AtomicBool>,
}

impl AudioChunkReceiver {
    pub fn new(receiver: mpsc::Receiver<AudioChunk>) -> Self {
        Self::with_capture_error(receiver, Arc::new(AtomicBool::new(false)))
    }

    /// Attach the real-time source's fatal-capture signal to the receiver.
    pub fn with_capture_error(
        receiver: mpsc::Receiver<AudioChunk>,
        capture_error: Arc<AtomicBool>,
    ) -> Self {
        Self {
            receiver,
            capture_error,
        }
    }

    /// Whether stream termination was caused by a fatal capture callback.
    pub fn capture_failed(&self) -> bool {
        self.capture_error.load(Ordering::SeqCst)
    }

    /// Asynchronously await the next audio chunk from the capture stream.
    pub async fn recv(&mut self) -> Option<AudioChunk> {
        self.receiver.recv().await
    }

    /// Attempt to retrieve the next chunk without waiting.
    pub fn try_recv(&mut self) -> Result<AudioChunk, mpsc::error::TryRecvError> {
        self.receiver.try_recv()
    }
}

/// Abstract hardware/virtual audio input source.
///
/// Thread safety: All implementers must satisfy `Send + Sync + 'static`.
/// Methods returning `AudioBoxFuture` are dyn-compatible.
pub trait AudioSource: Send + Sync + 'static {
    /// Enumerate all available audio input capture devices.
    fn enumerate_devices(&self) -> AudioBoxFuture<'_, Result<Vec<AudioDeviceInfo>, AudioError>>;

    /// Initialize the capture hardware and start streaming chunks to the returned receiver.
    fn start<'a>(
        &'a mut self,
        config: &'a AudioStreamConfig,
    ) -> AudioBoxFuture<'a, Result<AudioChunkReceiver, AudioError>>;

    /// Cease audio capture, release hardware handles, and drain the buffer.
    fn stop(&mut self) -> AudioBoxFuture<'_, Result<(), AudioError>>;

    /// Current operational status of the capture source.
    fn status(&self) -> AudioSourceStatus;
}
