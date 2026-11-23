// SPDX-License-Identifier: MIT

pub mod cpal_source;
pub mod device;
pub mod error;
#[cfg(any(target_os = "linux", test))]
pub(crate) mod linux_pulse;
pub mod mock;
pub mod resampler;
pub mod resilient;
pub mod ring_buffer;
pub mod rnnoise;
pub mod traits;
pub mod types;

pub use cpal_source::CpalAudioSource;
#[cfg(target_os = "linux")]
pub use linux_pulse::configure_linux_parent_death_signal;
pub use device::{
    enumerate_cpal_devices, get_input_device_count, normalize_device_name, with_clean_audio_thread,
};
pub use error::AudioError;
pub use mock::MockAudioSource;
pub use resampler::{
    convert_i16_to_mono_f32, convert_u16_to_mono_f32, mix_multichannel_to_mono, AudioResampler,
};
pub use resilient::ResilientCapture;
pub use ring_buffer::{
    create_audio_ring_buffer, AudioRingConsumer, AudioRingProducer, RingBufferMetrics,
    RingBufferStats,
};
pub use rnnoise::RnnoiseProcessor;
pub use traits::{AudioBoxFuture, AudioChunkReceiver, AudioSource};
pub use types::{
    AudioChunk, AudioDeviceInfo, AudioSourceStatus, AudioStreamConfig, DEFAULT_CHUNK_SIZE,
    DEFAULT_RING_BUFFER_CAPACITY, DEFAULT_SAMPLE_RATE,
};
