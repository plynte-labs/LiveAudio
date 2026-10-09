// SPDX-License-Identifier: MIT

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::asr::AsrModelConfig;
use liveaudio_audio::AudioStreamConfig;
use liveaudio_vad::VadConfig;

/// Master configuration driving the unified LiveAudio processing pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PipelineConfig {
    pub audio: AudioStreamConfig,
    pub vad: VadConfig,
    pub asr: AsrModelConfig,
    pub ws_port: u16,
    pub session_dir: PathBuf,
    pub write_jsonl: bool,
    pub write_webvtt: bool,
    pub max_live_delay_sec: f32,
    pub parent_pid: Option<u32>,
    pub health_file: Option<PathBuf>,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            audio: AudioStreamConfig::default(),
            vad: VadConfig::default(),
            asr: AsrModelConfig::default(),
            ws_port: 8765,
            session_dir: PathBuf::from("sessions"),
            write_jsonl: true,
            write_webvtt: true,
            max_live_delay_sec: 4.0,
            parent_pid: None,
            health_file: None,
        }
    }
}
