// SPDX-License-Identifier: MIT

use liveaudio_vad::{SpeechEndReason, VadTransition};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

static NEXT_OPAQUE_ID: AtomicU64 = AtomicU64::new(1);

fn new_opaque_id() -> String {
    let sequence = NEXT_OPAQUE_ID.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or_default();
    format!("{:016x}{:016x}", nanos ^ sequence.rotate_left(17), sequence)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AcousticSpeechEndReason {
    #[serde(rename = "silence")]
    Silence,
    #[serde(rename = "duration_limit")]
    DurationLimit,
    #[serde(rename = "service_stopped")]
    ServiceStopped,
    #[serde(rename = "capture_error")]
    CaptureError,
    #[serde(rename = "attempt_reset")]
    AttemptReset,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AcousticSpeechEvent {
    #[serde(rename = "speech_activity_start")]
    SpeechActivityStart {
        event_version: u8,
        run_id: String,
        attempt_id: u64,
        utterance_id: String,
        seq: u64,
        monotonic_ms: u64,
    },
    #[serde(rename = "speech_activity_end")]
    SpeechActivityEnd {
        event_version: u8,
        run_id: String,
        attempt_id: u64,
        utterance_id: String,
        seq: u64,
        monotonic_ms: u64,
        reason: AcousticSpeechEndReason,
    },
}

impl AcousticSpeechEvent {
    pub fn run_id(&self) -> &str {
        match self {
            Self::SpeechActivityStart { run_id, .. } | Self::SpeechActivityEnd { run_id, .. } => {
                run_id
            }
        }
    }

    pub fn attempt_id(&self) -> u64 {
        match self {
            Self::SpeechActivityStart { attempt_id, .. }
            | Self::SpeechActivityEnd { attempt_id, .. } => *attempt_id,
        }
    }

    pub fn utterance_id(&self) -> &str {
        match self {
            Self::SpeechActivityStart { utterance_id, .. }
            | Self::SpeechActivityEnd { utterance_id, .. } => utterance_id,
        }
    }

    pub fn seq(&self) -> u64 {
        match self {
            Self::SpeechActivityStart { seq, .. } | Self::SpeechActivityEnd { seq, .. } => *seq,
        }
    }

    pub fn end_reason(&self) -> Option<AcousticSpeechEndReason> {
        match self {
            Self::SpeechActivityEnd { reason, .. } => Some(*reason),
            Self::SpeechActivityStart { .. } => None,
        }
    }
}

/// Bounded, in-memory correlation state for one service run.
pub struct AcousticEventLifecycle {
    run_id: String,
    attempt_id: u64,
    attempt_started: Instant,
    seq: u64,
    utterance_counter: u64,
    active_utterance: Option<String>,
}

impl AcousticEventLifecycle {
    pub fn new() -> Self {
        Self {
            run_id: new_opaque_id(),
            attempt_id: 1,
            attempt_started: Instant::now(),
            seq: 0,
            utterance_counter: 0,
            active_utterance: None,
        }
    }

    pub fn observe_vad_transition(
        &mut self,
        transition: VadTransition,
        at: Instant,
    ) -> Option<AcousticSpeechEvent> {
        match transition {
            VadTransition::SpeechStart if self.active_utterance.is_none() => {
                self.utterance_counter = self.utterance_counter.saturating_add(1);
                let utterance_id = new_opaque_id();
                self.active_utterance = Some(utterance_id.clone());
                Some(self.start_event(utterance_id, at))
            }
            VadTransition::SpeechEnd { reason, .. } => self.close_attempt(
                match reason {
                    SpeechEndReason::SilenceTimeout => AcousticSpeechEndReason::Silence,
                    SpeechEndReason::MaxDurationReached => AcousticSpeechEndReason::DurationLimit,
                },
                at,
            ),
            VadTransition::None | VadTransition::SpeechStart => None,
        }
    }

    pub fn close_attempt(
        &mut self,
        reason: AcousticSpeechEndReason,
        at: Instant,
    ) -> Option<AcousticSpeechEvent> {
        let utterance_id = self.active_utterance.take()?;
        self.seq = self.seq.saturating_add(1);
        Some(AcousticSpeechEvent::SpeechActivityEnd {
            event_version: 1,
            run_id: self.run_id.clone(),
            attempt_id: self.attempt_id,
            utterance_id,
            seq: self.seq,
            monotonic_ms: self.elapsed_ms(at),
            reason,
        })
    }

    /// Begin a new attempt after capture/VAD has actually restarted.
    pub fn start_attempt(&mut self, at: Instant) -> u64 {
        self.attempt_id = self.attempt_id.saturating_add(1);
        self.attempt_started = at;
        // Callers close an active utterance with `AttemptReset` before advancing.
        // Clearing here prevents stale state from crossing an attempt boundary.
        self.active_utterance = None;
        self.attempt_id
    }

    fn start_event(&mut self, utterance_id: String, at: Instant) -> AcousticSpeechEvent {
        self.seq = self.seq.saturating_add(1);
        AcousticSpeechEvent::SpeechActivityStart {
            event_version: 1,
            run_id: self.run_id.clone(),
            attempt_id: self.attempt_id,
            utterance_id,
            seq: self.seq,
            monotonic_ms: self.elapsed_ms(at),
        }
    }

    fn elapsed_ms(&self, at: Instant) -> u64 {
        at.checked_duration_since(self.attempt_started)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liveaudio_vad::{SpeechEndReason, VadTransition};
    use serde_json::json;
    use std::time::{Duration, Instant};

    #[test]
    fn start_and_end_use_correlated_versioned_wire_fields() {
        let mut lifecycle = AcousticEventLifecycle::new();
        let at = Instant::now();
        let start = lifecycle.observe_vad_transition(VadTransition::SpeechStart, at);
        assert_eq!(
            lifecycle.observe_vad_transition(VadTransition::SpeechStart, at),
            None
        );
        let end = lifecycle.observe_vad_transition(
            VadTransition::SpeechEnd {
                duration_ms: 8,
                reason: SpeechEndReason::SilenceTimeout,
            },
            at + Duration::from_millis(8),
        );

        let start = serde_json::to_value(start.unwrap()).unwrap();
        let end = serde_json::to_value(end.unwrap()).unwrap();
        assert_eq!(start["type"], "speech_activity_start");
        assert_eq!(start["event_version"], 1);
        assert_eq!(start["attempt_id"], 1);
        assert_eq!(start["monotonic_ms"], 0);
        assert_eq!(end["type"], "speech_activity_end");
        assert_eq!(end["reason"], "silence");
        assert_eq!(start["run_id"], end["run_id"]);
        assert_eq!(start["utterance_id"], end["utterance_id"]);
        assert!(end["seq"].as_u64().unwrap() > start["seq"].as_u64().unwrap());
        assert!(end["monotonic_ms"].as_u64().unwrap() >= 7);
        assert!(start.get("text").is_none());
    }

    #[test]
    fn separate_runs_get_distinct_opaque_run_ids() {
        let at = Instant::now();
        let first = AcousticEventLifecycle::new()
            .observe_vad_transition(VadTransition::SpeechStart, at)
            .unwrap();
        let second = AcousticEventLifecycle::new()
            .observe_vad_transition(VadTransition::SpeechStart, at)
            .unwrap();
        assert_ne!(first.run_id(), second.run_id());
        assert_ne!(first.utterance_id(), second.utterance_id());
    }

    #[test]
    fn attempt_close_emits_one_matching_end_and_advances_only_when_started() {
        let at = Instant::now();
        let mut lifecycle = AcousticEventLifecycle::new();
        let start = lifecycle
            .observe_vad_transition(VadTransition::SpeechStart, at)
            .unwrap();

        let end = lifecycle
            .close_attempt(
                AcousticSpeechEndReason::CaptureError,
                at + Duration::from_millis(2),
            )
            .unwrap();
        assert_eq!(end.utterance_id(), start.utterance_id());
        assert_eq!(
            end.end_reason(),
            Some(AcousticSpeechEndReason::CaptureError)
        );
        assert_eq!(
            lifecycle.close_attempt(AcousticSpeechEndReason::ServiceStopped, at),
            None
        );

        let reset_start = lifecycle
            .observe_vad_transition(VadTransition::SpeechStart, at + Duration::from_secs(1))
            .unwrap();
        let reset_end = lifecycle
            .close_attempt(
                AcousticSpeechEndReason::AttemptReset,
                at + Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(reset_end.utterance_id(), reset_start.utterance_id());
        assert_eq!(
            reset_end.end_reason(),
            Some(AcousticSpeechEndReason::AttemptReset)
        );

        assert_eq!(lifecycle.start_attempt(at + Duration::from_secs(1)), 2);
        let next = lifecycle
            .observe_vad_transition(VadTransition::SpeechStart, at + Duration::from_secs(1))
            .unwrap();
        assert_eq!(next.attempt_id(), 2);
        assert!(next.seq() > end.seq());
        assert_ne!(next.utterance_id(), start.utterance_id());
        assert_ne!(next.run_id(), "text-or-identity");
    }

    #[test]
    fn maps_all_vad_end_reasons_and_ignores_unmatched_transitions() {
        let mut lifecycle = AcousticEventLifecycle::new();
        assert_eq!(
            lifecycle.observe_vad_transition(VadTransition::None, Instant::now()),
            None
        );
        assert_eq!(
            lifecycle.observe_vad_transition(
                VadTransition::SpeechEnd {
                    duration_ms: 1,
                    reason: SpeechEndReason::SilenceTimeout,
                },
                Instant::now()
            ),
            None
        );

        for (vad_reason, expected) in [
            (
                SpeechEndReason::SilenceTimeout,
                AcousticSpeechEndReason::Silence,
            ),
            (
                SpeechEndReason::MaxDurationReached,
                AcousticSpeechEndReason::DurationLimit,
            ),
        ] {
            lifecycle
                .observe_vad_transition(VadTransition::SpeechStart, Instant::now())
                .unwrap();
            let event = lifecycle
                .observe_vad_transition(
                    VadTransition::SpeechEnd {
                        duration_ms: 1,
                        reason: vad_reason,
                    },
                    Instant::now(),
                )
                .unwrap();
            assert_eq!(event.end_reason(), Some(expected));
        }

        for (reason, expected) in [
            (AcousticSpeechEndReason::Silence, "silence"),
            (AcousticSpeechEndReason::DurationLimit, "duration_limit"),
            (AcousticSpeechEndReason::ServiceStopped, "service_stopped"),
            (AcousticSpeechEndReason::CaptureError, "capture_error"),
            (AcousticSpeechEndReason::AttemptReset, "attempt_reset"),
        ] {
            assert_eq!(serde_json::to_value(reason).unwrap(), json!(expected));
        }
    }
}
