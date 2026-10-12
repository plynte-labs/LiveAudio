// SPDX-License-Identifier: MIT

use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Statistics on ring buffer throughput and overflow drops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RingBufferStats {
    pub total_samples_written: u64,
    pub total_samples_read: u64,
    pub dropped_samples: u64,
    pub dropped_chunks: u64,
}

/// Shared atomic metrics between producer and consumer.
#[derive(Debug, Default)]
pub struct RingBufferMetrics {
    total_samples_written: AtomicU64,
    total_samples_read: AtomicU64,
    dropped_samples: AtomicU64,
    dropped_chunks: AtomicU64,
}

impl RingBufferMetrics {
    pub fn snapshot(&self) -> RingBufferStats {
        RingBufferStats {
            total_samples_written: self.total_samples_written.load(Ordering::Relaxed),
            total_samples_read: self.total_samples_read.load(Ordering::Relaxed),
            dropped_samples: self.dropped_samples.load(Ordering::Relaxed),
            dropped_chunks: self.dropped_chunks.load(Ordering::Relaxed),
        }
    }
}

/// Real-time safe lock-free producer side of the audio ring buffer.
///
/// Guaranteed to never allocate heap memory or acquire blocking locks in CPAL callbacks.
pub struct AudioRingProducer {
    producer: HeapProd<f32>,
    metrics: Arc<RingBufferMetrics>,
    chunk_size: usize,
}

unsafe impl Send for AudioRingProducer {}

impl AudioRingProducer {
    /// Push a slice of audio samples into the ring buffer without allocating or blocking.
    ///
    /// If the buffer does not have enough capacity, the unwritten samples are counted as dropped.
    #[inline]
    pub fn push_samples(&mut self, samples: &[f32]) -> usize {
        let count = self.producer.push_slice(samples);
        self.metrics
            .total_samples_written
            .fetch_add(count as u64, Ordering::Relaxed);

        if count < samples.len() {
            let dropped = samples.len() - count;
            self.metrics
                .dropped_samples
                .fetch_add(dropped as u64, Ordering::Relaxed);

            // Compute integer chunks dropped
            let chunks_dropped = if self.chunk_size > 0 {
                (dropped + self.chunk_size - 1) / self.chunk_size
            } else {
                1
            };
            self.metrics
                .dropped_chunks
                .fetch_add(chunks_dropped as u64, Ordering::Relaxed);
        }

        count
    }

    /// Read instantaneous dropped chunks count.
    #[inline]
    pub fn dropped_chunks(&self) -> u64 {
        self.metrics.dropped_chunks.load(Ordering::Relaxed)
    }

    /// Read instantaneous dropped samples count.
    #[inline]
    pub fn dropped_samples(&self) -> u64 {
        self.metrics.dropped_samples.load(Ordering::Relaxed)
    }

    /// Read metrics snapshot.
    pub fn metrics(&self) -> Arc<RingBufferMetrics> {
        Arc::clone(&self.metrics)
    }
}

/// Consumer side of the audio ring buffer, processed by worker thread.
pub struct AudioRingConsumer {
    consumer: HeapCons<f32>,
    metrics: Arc<RingBufferMetrics>,
}

unsafe impl Send for AudioRingConsumer {}

impl AudioRingConsumer {
    /// Read available samples from the ring buffer into the destination slice.
    #[inline]
    pub fn pop_samples(&mut self, dest: &mut [f32]) -> usize {
        let count = self.consumer.pop_slice(dest);
        self.metrics
            .total_samples_read
            .fetch_add(count as u64, Ordering::Relaxed);
        count
    }

    /// Number of samples currently ready to be read.
    #[inline]
    pub fn available_samples(&self) -> usize {
        self.consumer.occupied_len()
    }

    /// Whether the ring buffer currently contains no audio samples.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.consumer.is_empty()
    }

    /// Read metrics snapshot.
    pub fn metrics(&self) -> Arc<RingBufferMetrics> {
        Arc::clone(&self.metrics)
    }
}

/// Create a new bounded lock-free SPSC audio ring buffer.
///
/// Capacity is specified in samples (e.g. sample_rate * channels * duration_sec).
pub fn create_audio_ring_buffer(
    capacity_samples: usize,
    chunk_size: usize,
) -> (AudioRingProducer, AudioRingConsumer) {
    let rb = HeapRb::<f32>::new(capacity_samples.max(1024));
    let (producer, consumer) = rb.split();
    let metrics = Arc::new(RingBufferMetrics::default());

    (
        AudioRingProducer {
            producer,
            metrics: Arc::clone(&metrics),
            chunk_size: chunk_size.max(1),
        },
        AudioRingConsumer { consumer, metrics },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_push_pop_and_stats() {
        let (mut prod, mut cons) = create_audio_ring_buffer(1024, 512);
        assert_eq!(cons.available_samples(), 0);
        assert!(cons.is_empty());

        let samples = vec![0.5f32; 256];
        let written = prod.push_samples(&samples);
        assert_eq!(written, 256);
        assert_eq!(cons.available_samples(), 256);
        assert!(!cons.is_empty());

        let mut read_buf = vec![0.0f32; 256];
        let read = cons.pop_samples(&mut read_buf);
        assert_eq!(read, 256);
        assert_eq!(read_buf[0], 0.5);
        assert_eq!(cons.available_samples(), 0);
        assert!(cons.is_empty());

        let stats = cons.metrics().snapshot();
        assert_eq!(stats.total_samples_written, 256);
        assert_eq!(stats.total_samples_read, 256);
        assert_eq!(stats.dropped_samples, 0);
        assert_eq!(stats.dropped_chunks, 0);
    }

    #[test]
    fn test_ring_buffer_overflow_recording() {
        let capacity = 1024;
        let chunk_size = 512;
        let (mut prod, mut cons) = create_audio_ring_buffer(capacity, chunk_size);

        // Fill completely
        let data = vec![1.0f32; capacity];
        let written = prod.push_samples(&data);
        assert_eq!(written, capacity);

        // Overflow write: buffer is full
        let extra = vec![2.0f32; 512];
        let extra_written = prod.push_samples(&extra);
        assert_eq!(extra_written, 0); // No space left

        let stats = prod.metrics().snapshot();
        assert_eq!(stats.total_samples_written, capacity as u64);
        assert_eq!(stats.dropped_samples, 512);
        assert_eq!(stats.dropped_chunks, 1);
        assert_eq!(prod.dropped_chunks(), 1);

        // Drain partially and verify push resumes
        let mut drain = vec![0.0f32; 512];
        let drained = cons.pop_samples(&mut drain);
        assert_eq!(drained, 512);

        let resumed = prod.push_samples(&vec![3.0f32; 256]);
        assert_eq!(resumed, 256);
    }
}
