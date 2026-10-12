// SPDX-License-Identifier: MIT

use serde::{Deserialize, Serialize};

/// Audio sample format and stream configuration constants.
pub const DEFAULT_SAMPLE_RATE: u32 = 16000;
pub const DEFAULT_CHUNK_SIZE: usize = 512; // 32ms at 16kHz
pub const DEFAULT_RING_BUFFER_CAPACITY: usize = 500; // ~16 seconds at 32ms/chunk

/// Discrete chunk of standardized audio samples (mono, 16kHz f32).
#[derive(Debug, Clone, PartialEq)]
pub struct AudioChunk {
    /// 32-bit floating point PCM audio samples normalized to [-1.0, 1.0].
    pub samples: Vec<f32>,
    /// Sample rate in Hz (expected: 16000).
    pub sample_rate: u32,
    /// Channel count (expected: 1 for mono).
    pub channels: u16,
    /// Monotonic timestamp in nanoseconds when this chunk was captured.
    pub timestamp_ns: u64,
    /// Monotonically increasing sequence number for jitter/drop detection.
    pub sequence_id: u64,
}

impl AudioChunk {
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16, sequence_id: u64) -> Self {
        let timestamp_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);

        Self {
            samples,
            sample_rate,
            channels,
            timestamp_ns,
            sequence_id,
        }
    }

    /// Duration of audio in this chunk in seconds.
    pub fn duration_sec(&self) -> f32 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.samples.len() as f32 / self.sample_rate as f32
        }
    }
}

/// Metadata describing an audio input capture device.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub channels: u16,
    pub supported_sample_rates: Vec<u32>,
    #[serde(default)]
    pub is_loopback: bool,
}

impl AudioDeviceInfo {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        is_default: bool,
        channels: u16,
        supported_sample_rates: Vec<u32>,
        is_loopback: bool,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            is_default,
            channels,
            supported_sample_rates,
            is_loopback,
        }
    }
}

/// Audio capture stream configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioStreamConfig {
    pub device_id: Option<String>,
    pub sample_rate: u32,
    pub channels: u16,
    pub chunk_size: usize,
    pub ring_buffer_capacity: usize,
}

impl Default for AudioStreamConfig {
    fn default() -> Self {
        Self {
            device_id: None,
            sample_rate: DEFAULT_SAMPLE_RATE,
            channels: 1,
            chunk_size: DEFAULT_CHUNK_SIZE,
            ring_buffer_capacity: DEFAULT_RING_BUFFER_CAPACITY,
        }
    }
}

/// State of the audio capture source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioSourceStatus {
    Stopped,
    Initializing,
    Standby,
    Capturing,
    Reconnecting,
    Failed,
}
