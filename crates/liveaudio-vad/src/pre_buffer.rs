// SPDX-License-Identifier: MIT

use std::collections::VecDeque;

/// Ring buffer preserving the last N discarded silence chunks to recover word onsets (ADR-007).
#[derive(Debug, Clone)]
pub struct PreBuffer {
    capacity_chunks: usize,
    chunks: VecDeque<Vec<f32>>,
}

impl PreBuffer {
    /// Create a new pre-buffer with the given chunk capacity.
    pub fn new(capacity_chunks: usize) -> Self {
        Self {
            capacity_chunks: capacity_chunks.max(1),
            chunks: VecDeque::with_capacity(capacity_chunks.max(1)),
        }
    }

    /// Calculate recommended capacity in chunks based on pad milliseconds, sample rate, and chunk size.
    pub fn chunks_from_pad(pad_ms: u32, sample_rate: u32, chunk_size: usize) -> usize {
        let samples_needed = (pad_ms as f64 / 1000.0) * sample_rate as f64;
        let chunks = (samples_needed / chunk_size as f64).ceil() as usize;
        chunks.max(1)
    }

    /// Push a discarded non-speech chunk into the pre-buffer.
    pub fn push(&mut self, chunk: Vec<f32>) {
        if self.chunks.len() >= self.capacity_chunks {
            self.chunks.pop_front();
        }
        self.chunks.push_back(chunk);
    }

    /// Drain all buffered onset chunks and clear the buffer upon voice onset.
    pub fn drain_onset(&mut self) -> Vec<Vec<f32>> {
        self.chunks.drain(..).collect()
    }

    /// Clear without draining.
    pub fn clear(&mut self) {
        self.chunks.clear();
    }

    /// Current count of stored chunks.
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    /// Whether the pre-buffer contains no chunks.
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }
}
