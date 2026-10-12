// SPDX-License-Identifier: MIT

use crate::protocol::SubtitleWireMessage;
use liveaudio_core::TranscriptionCue;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const REPLAY_BUFFER_MAX: usize = 256;
pub const MAX_RETRY_BUFFER: usize = 10;
pub const HIGH_WATER_MARK_BYTES: usize = 65536; // 64 KB

/// Policy determining how backlogged subtitles are dispatched to connected OBS clients (ADR-009).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BacklogPolicy {
    /// Emit fresh cues, pace small backlog, discard cues that are excessively delayed.
    #[default]
    Auto,
    /// Strictly emit cues within `max_live_delay_sec` of real-time.
    LiveOnly,
    /// Deliver all recorded cues up to the buffer ceiling.
    SendAll,
}

impl BacklogPolicy {
    pub fn from_config(value: &str) -> Self {
        match value {
            "live_only" => Self::LiveOnly,
            "send_all" => Self::SendAll,
            _ => Self::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObsEmitDecision {
    pub emit: bool,
    pub is_replay: bool,
    pub catchup_interval_sec: f64,
}

/// Mirrors the Python `_obs_emit_decision` policy at the Rust worker boundary.
pub fn decide_obs_emit(
    policy: BacklogPolicy,
    max_live_delay_sec: f64,
    catchup_interval_sec: f64,
    queue_delay_sec: f64,
) -> ObsEmitDecision {
    match policy {
        BacklogPolicy::SendAll => ObsEmitDecision {
            emit: true,
            is_replay: queue_delay_sec > 1.0,
            catchup_interval_sec: 0.0,
        },
        BacklogPolicy::LiveOnly => ObsEmitDecision {
            emit: queue_delay_sec <= max_live_delay_sec,
            is_replay: false,
            catchup_interval_sec: 0.0,
        },
        BacklogPolicy::Auto if queue_delay_sec > max_live_delay_sec => ObsEmitDecision {
            emit: false,
            is_replay: false,
            catchup_interval_sec: 0.0,
        },
        BacklogPolicy::Auto => {
            let is_replay = queue_delay_sec > 1.0;
            ObsEmitDecision {
                emit: true,
                is_replay,
                catchup_interval_sec: if is_replay { catchup_interval_sec } else { 0.0 },
            }
        }
    }
}

const PENDING_UTTERANCES_MAX: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompletedUtteranceTiming {
    pub created_at_sec: f64,
    pub processed_at_sec: f64,
    pub queue_delay_sec: f64,
}

/// Build an OBS wire cue from a worker result without treating unknown or inference-heavy
/// timing as timely. `latency` remains the worker's inference duration; queue delay is metadata.
pub fn subtitle_from_worker_result(
    id: impl Into<String>,
    text: impl Into<String>,
    style: impl Into<String>,
    inference_sec: f64,
    timing: Option<CompletedUtteranceTiming>,
    policy: BacklogPolicy,
    max_live_delay_sec: f64,
    catchup_interval_sec: f64,
) -> Option<SubtitleWireMessage> {
    let (timing, decision) = match timing {
        Some(timing) => {
            let total_age_sec = (timing.processed_at_sec - timing.created_at_sec).max(0.0);
            let decision = decide_obs_emit(
                policy,
                max_live_delay_sec,
                catchup_interval_sec,
                total_age_sec,
            );
            (timing, decision)
        }
        None if policy == BacklogPolicy::SendAll => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);
            (
                CompletedUtteranceTiming {
                    created_at_sec: now,
                    processed_at_sec: now,
                    queue_delay_sec: 0.0,
                },
                ObsEmitDecision {
                    emit: true,
                    is_replay: false,
                    catchup_interval_sec: 0.0,
                },
            )
        }
        None => return None,
    };
    if !decision.emit {
        return None;
    }
    Some(SubtitleWireMessage::new(
        id,
        text,
        style,
        timing.created_at_sec,
        timing.processed_at_sec,
        timing.queue_delay_sec,
        timing.queue_delay_sec + inference_sec.max(0.0),
        inference_sec.max(0.0),
        decision.is_replay,
        decision.catchup_interval_sec,
    ))
}

#[derive(Debug, Clone, Copy)]
struct PendingUtterance {
    created_at_sec: f64,
    dispatched_at: Instant,
}

/// Bounded Rust-side correlation between submitted utterance IDs and worker results.
pub struct PendingUtteranceTracker {
    capacity: usize,
    pending: HashMap<String, PendingUtterance>,
    order: VecDeque<String>,
}

impl Default for PendingUtteranceTracker {
    fn default() -> Self {
        Self::new(PENDING_UTTERANCES_MAX)
    }
}

impl PendingUtteranceTracker {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            pending: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn track(&mut self, utterance_id: impl Into<String>, created_at_sec: f64) {
        let utterance_id = utterance_id.into();
        self.pending.remove(&utterance_id);
        self.order.retain(|id| id != &utterance_id);
        while self.pending.len() >= self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.pending.remove(&oldest);
            } else {
                break;
            }
        }
        self.order.push_back(utterance_id.clone());
        self.pending.insert(
            utterance_id,
            PendingUtterance {
                created_at_sec,
                dispatched_at: Instant::now(),
            },
        );
    }

    pub fn remove(&mut self, utterance_id: &str) {
        self.pending.remove(utterance_id);
        self.order.retain(|id| id != utterance_id);
    }

    pub fn complete(
        &mut self,
        utterance_id: &str,
        inference_sec: f64,
    ) -> Option<CompletedUtteranceTiming> {
        let pending = self.pending.remove(utterance_id)?;
        self.order.retain(|id| id != utterance_id);
        let elapsed_sec = pending.dispatched_at.elapsed().as_secs_f64();
        let processed_at_sec = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_secs_f64();
        Some(CompletedUtteranceTiming {
            created_at_sec: pending.created_at_sec,
            processed_at_sec,
            queue_delay_sec: (elapsed_sec - inference_sec.max(0.0)).max(0.0),
        })
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

/// Bounded FIFO queue of TranscriptionCue implementing drop-oldest to protect OBS overlays from OOM during freezes.
#[derive(Debug, Clone)]
pub struct ReplayBuffer {
    max_capacity: usize,
    buffer: VecDeque<TranscriptionCue>,
    dropped_count: usize,
}

impl Default for ReplayBuffer {
    fn default() -> Self {
        Self::new(REPLAY_BUFFER_MAX)
    }
}

impl ReplayBuffer {
    pub fn new(max_capacity: usize) -> Self {
        Self {
            max_capacity: max_capacity.max(1),
            buffer: VecDeque::with_capacity(max_capacity.max(1)),
            dropped_count: 0,
        }
    }

    /// Push a cue. If capacity is exceeded, drops the oldest cue to preserve the live edge.
    pub fn push(&mut self, cue: TranscriptionCue) {
        if self.buffer.len() >= self.max_capacity {
            self.buffer.pop_front();
            self.dropped_count += 1;
        }
        self.buffer.push_back(cue);
    }

    /// Retrieve all buffered cues matching the specified backlog policy and max live delay.
    pub fn select_for_client(
        &self,
        policy: BacklogPolicy,
        max_live_delay_sec: f32,
    ) -> Vec<TranscriptionCue> {
        let now_sec = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);

        match policy {
            BacklogPolicy::SendAll => self.buffer.iter().cloned().collect(),
            BacklogPolicy::LiveOnly => self
                .buffer
                .iter()
                .filter(|c| (now_sec - c.end_timestamp_sec) <= max_live_delay_sec as f64)
                .cloned()
                .collect(),
            BacklogPolicy::Auto => self
                .buffer
                .iter()
                .filter(|c| (now_sec - c.end_timestamp_sec) <= (max_live_delay_sec * 2.0) as f64)
                .cloned()
                .collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn dropped_count(&self) -> usize {
        self.dropped_count
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

/// Bounded FIFO replay queue for serialized wire messages (OBS catch-up & late-joiners).
#[derive(Debug, Clone)]
pub struct WireReplayBuffer {
    max_capacity: usize,
    buffer: VecDeque<SubtitleWireMessage>,
    dropped_count: usize,
}

impl Default for WireReplayBuffer {
    fn default() -> Self {
        Self::new(REPLAY_BUFFER_MAX)
    }
}

impl WireReplayBuffer {
    pub fn new(max_capacity: usize) -> Self {
        Self {
            max_capacity: max_capacity.max(1),
            buffer: VecDeque::with_capacity(max_capacity.max(1)),
            dropped_count: 0,
        }
    }

    /// Push a wire message. Drops oldest if capacity is exceeded.
    pub fn push(&mut self, msg: SubtitleWireMessage) {
        if self.buffer.len() >= self.max_capacity {
            self.buffer.pop_front();
            self.dropped_count += 1;
        }
        self.buffer.push_back(msg);
    }

    /// Pop the next catch-up replay message.
    pub fn pop_front(&mut self) -> Option<SubtitleWireMessage> {
        self.buffer.pop_front()
    }

    /// Retrieve all buffered messages suitable for a late-joining client under `policy`.
    pub fn select_for_client(
        &self,
        policy: BacklogPolicy,
        max_live_delay_sec: f32,
    ) -> Vec<SubtitleWireMessage> {
        let now_sec = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);

        match policy {
            BacklogPolicy::SendAll => self.buffer.iter().cloned().collect(),
            BacklogPolicy::LiveOnly => self
                .buffer
                .iter()
                .filter(|m| (now_sec - m.processed_at) <= max_live_delay_sec as f64)
                .cloned()
                .collect(),
            BacklogPolicy::Auto => self
                .buffer
                .iter()
                .filter(|m| (now_sec - m.processed_at) <= (max_live_delay_sec * 2.0) as f64)
                .cloned()
                .collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn dropped_count(&self) -> usize {
        self.dropped_count
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

/// Bounded buffer for messages delayed due to transport backpressure.
#[derive(Debug, Clone)]
pub struct RetryBuffer {
    max_capacity: usize,
    buffer: VecDeque<String>,
    dropped_count: usize,
}

impl Default for RetryBuffer {
    fn default() -> Self {
        Self::new(MAX_RETRY_BUFFER)
    }
}

impl RetryBuffer {
    pub fn new(max_capacity: usize) -> Self {
        Self {
            max_capacity: max_capacity.max(1),
            buffer: VecDeque::with_capacity(max_capacity.max(1)),
            dropped_count: 0,
        }
    }

    /// Push a message into the retry buffer. Drops oldest if full.
    pub fn push(&mut self, payload: String) {
        if self.buffer.len() >= self.max_capacity {
            self.buffer.pop_front();
            self.dropped_count += 1;
        }
        self.buffer.push_back(payload);
    }

    /// Pop the next pending retry message.
    pub fn pop_front(&mut self) -> Option<String> {
        self.buffer.pop_front()
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn dropped_count(&self) -> usize {
        self.dropped_count
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}
