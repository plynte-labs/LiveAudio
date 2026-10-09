// SPDX-License-Identifier: MIT

use liveaudio_core::asr::AsrTranscriptionResponse;
use liveaudio_core::TranscriptionCue;
use liveaudio_network::protocol::SubtitleWireMessage;
use liveaudio_network::replay::{
    decide_obs_emit, BacklogPolicy, CompletedUtteranceTiming, PendingUtteranceTracker,
    ReplayBuffer, RetryBuffer, WireReplayBuffer, MAX_RETRY_BUFFER, REPLAY_BUFFER_MAX,
};
use liveaudio_network::subtitle_from_worker_result;

fn make_dummy_cue(counter: u64, end_sec: f64) -> TranscriptionCue {
    let dummy_asr = AsrTranscriptionResponse {
        request_id: format!("req-{}", counter),
        text: format!("cue {}", counter),
        language: "es".to_string(),
        duration_sec: 1.0,
        latency_sec: 0.1,
        segments: vec![],
    };
    TranscriptionCue::new(
        counter,
        end_sec - 1.0,
        end_sec,
        format!("cue {}", counter),
        dummy_asr,
    )
}

fn make_dummy_wire(counter: u64, processed_at: f64) -> SubtitleWireMessage {
    SubtitleWireMessage::new(
        format!("msg-{}", counter),
        format!("text {}", counter),
        "default",
        processed_at - 1.0,
        processed_at,
        0.1,
        0.5,
        0.2,
        true,
        1.5,
    )
}

#[test]
fn test_replay_buffer_capacity_and_drop_oldest() {
    let mut buffer = ReplayBuffer::new(REPLAY_BUFFER_MAX);
    assert_eq!(buffer.len(), 0);

    for i in 1..=(REPLAY_BUFFER_MAX + 10) {
        buffer.push(make_dummy_cue(i as u64, i as f64));
    }

    assert_eq!(buffer.len(), REPLAY_BUFFER_MAX);
    assert_eq!(buffer.dropped_count(), 10);
}

#[test]
fn test_wire_replay_buffer_capacity_and_drop_oldest() {
    let mut buffer = WireReplayBuffer::new(REPLAY_BUFFER_MAX);
    assert_eq!(buffer.len(), 0);

    for i in 1..=(REPLAY_BUFFER_MAX + 5) {
        buffer.push(make_dummy_wire(i as u64, i as f64));
    }

    assert_eq!(buffer.len(), REPLAY_BUFFER_MAX);
    assert_eq!(buffer.dropped_count(), 5);

    // Popping front retrieves oldest remaining message
    let first = buffer.pop_front().unwrap();
    assert_eq!(first.id, "msg-6");
    assert_eq!(buffer.len(), REPLAY_BUFFER_MAX - 1);
}

#[test]
fn test_backlog_policy_filtering() {
    let mut buffer = WireReplayBuffer::new(10);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    // 1 very old cue (100s ago)
    buffer.push(make_dummy_wire(1, now - 100.0));
    // 1 moderately old cue (15s ago)
    buffer.push(make_dummy_wire(2, now - 15.0));
    // 1 recent cue (2s ago)
    buffer.push(make_dummy_wire(3, now - 2.0));

    // SendAll returns all 3
    let all = buffer.select_for_client(BacklogPolicy::SendAll, 10.0);
    assert_eq!(all.len(), 3);

    // LiveOnly (max_delay 10s) returns only recent
    let live = buffer.select_for_client(BacklogPolicy::LiveOnly, 10.0);
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].id, "msg-3");

    // Auto (max_delay * 2 = 20s) returns recent + moderate (msg-2 and msg-3)
    let auto = buffer.select_for_client(BacklogPolicy::Auto, 10.0);
    assert_eq!(auto.len(), 2);
    assert_eq!(auto[0].id, "msg-2");
    assert_eq!(auto[1].id, "msg-3");
}

#[test]
fn test_retry_buffer_fifo_and_cap() {
    let mut retry = RetryBuffer::new(MAX_RETRY_BUFFER);
    assert!(retry.is_empty());

    for i in 1..=(MAX_RETRY_BUFFER + 3) {
        retry.push(format!("payload-{}", i));
    }

    assert_eq!(retry.len(), MAX_RETRY_BUFFER);
    assert_eq!(retry.dropped_count(), 3);

    // Oldest remaining should be payload-4
    assert_eq!(retry.pop_front().unwrap(), "payload-4");
    assert_eq!(retry.pop_front().unwrap(), "payload-5");
    assert_eq!(retry.len(), MAX_RETRY_BUFFER - 2);

    retry.clear();
    assert!(retry.is_empty());
}

#[test]
fn test_obs_backlog_policy_matches_python_cutoffs() {
    let auto_live = decide_obs_emit(BacklogPolicy::Auto, 10.0, 1.5, 1.0);
    assert!(auto_live.emit);
    assert!(!auto_live.is_replay);
    assert_eq!(auto_live.catchup_interval_sec, 0.0);

    let auto_catchup = decide_obs_emit(BacklogPolicy::Auto, 10.0, 1.5, 1.01);
    assert!(auto_catchup.emit && auto_catchup.is_replay);
    assert_eq!(auto_catchup.catchup_interval_sec, 1.5);

    assert!(decide_obs_emit(BacklogPolicy::Auto, 10.0, 1.5, 10.0).emit);
    assert!(!decide_obs_emit(BacklogPolicy::Auto, 10.0, 1.5, 10.001).emit);
    assert!(decide_obs_emit(BacklogPolicy::LiveOnly, 10.0, 1.5, 10.0).emit);
    assert!(!decide_obs_emit(BacklogPolicy::LiveOnly, 10.0, 1.5, 10.001).emit);
    let send_all = decide_obs_emit(BacklogPolicy::SendAll, 10.0, 1.5, 900.0);
    assert!(send_all.emit && send_all.is_replay);
    assert_eq!(send_all.catchup_interval_sec, 0.0);
}

#[test]
fn test_obs_backlog_policy_uses_total_subtitle_age_not_queue_delay() {
    let timing = liveaudio_network::CompletedUtteranceTiming {
        created_at_sec: 10.0,
        processed_at_sec: 15.0,
        queue_delay_sec: 0.4,
    };
    let message = subtitle_from_worker_result(
        "stale",
        "text",
        "default",
        4.6,
        Some(timing),
        BacklogPolicy::Auto,
        2.0,
        1.0,
    );
    assert!(
        message.is_none(),
        "stale subtitle must be dropped using its total age"
    );
}

#[test]
fn test_worker_result_wire_latency_is_inference_duration() {
    let inference_sec = 2.4;
    let message = subtitle_from_worker_result(
        "utterance",
        "text",
        "default",
        inference_sec,
        Some(CompletedUtteranceTiming {
            created_at_sec: 10.0,
            processed_at_sec: 15.0,
            queue_delay_sec: 0.4,
        }),
        BacklogPolicy::SendAll,
        2.0,
        1.0,
    )
    .unwrap();
    assert_eq!(
        message.latency, inference_sec,
        "wire latency must use worker inference_sec, not audio duration"
    );
}

#[test]
fn test_unknown_timing_drops_automatic_and_live_only_but_preserves_send_all() {
    assert!(subtitle_from_worker_result(
        "unknown",
        "text",
        "default",
        0.5,
        None,
        BacklogPolicy::Auto,
        10.0,
        1.0
    )
    .is_none());
    assert!(subtitle_from_worker_result(
        "unknown",
        "text",
        "default",
        0.5,
        None,
        BacklogPolicy::LiveOnly,
        10.0,
        1.0
    )
    .is_none());
    let send_all = subtitle_from_worker_result(
        "unknown",
        "text",
        "default",
        0.5,
        None,
        BacklogPolicy::SendAll,
        10.0,
        1.0,
    )
    .unwrap();
    assert_eq!(send_all.text, "text");
}

#[test]
fn test_pending_utterance_timing_is_correlated_and_bounded() {
    let mut tracker = PendingUtteranceTracker::new(2);
    tracker.track("utt-1", 100.0);
    tracker.track("utt-2", 200.0);
    tracker.track("utt-3", 300.0);

    assert_eq!(tracker.len(), 2);
    assert!(
        tracker.complete("utt-1", 0.0).is_none(),
        "oldest dispatch is evicted at capacity"
    );
    std::thread::sleep(std::time::Duration::from_millis(20));
    let completed = tracker
        .complete("utt-2", 0.005)
        .expect("correlate by utterance id");
    assert_eq!(completed.created_at_sec, 200.0);
    assert!(
        completed.queue_delay_sec >= 0.01,
        "subtract inference time from elapsed dispatch delay"
    );
    assert!(completed.processed_at_sec >= completed.created_at_sec);
    assert_eq!(tracker.len(), 1);
}
