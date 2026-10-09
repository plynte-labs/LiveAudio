// SPDX-License-Identifier: MIT

pub mod asr;
pub mod config;
pub mod event;
pub mod job_object;
pub mod pipeline;
pub mod runtime;
pub mod sink;
pub mod supervisor;

pub use asr::{
    AsrBackend, AsrBoxFuture, AsrError, AsrHealthStatus, AsrModelConfig, AsrTranscriptionRequest,
    AsrTranscriptionResponse, TranscriptionSegment,
};
pub use config::{
    audio_queue_capacity, get_data_home, get_hf_home, get_models_home, get_torch_home, load_config,
    load_config_readonly, load_from_path, save_config, save_to_path, ConfigReadonlyInfo,
    LiveAudioConfig,
};
pub use event::{EventBus, HealthState, HealthStateMachine};
pub use job_object::JobObject;
pub use pipeline::{PipelineConfig, PipelineLifecycleManager};
pub use runtime::{
    discover_managed_python, discover_managed_worker, find_uv_executable, managed_python,
    managed_runtime_root, preflight_asr_runtime, provision_runtime_with_uv, setup_managed_runtime,
    RuntimeError,
};
pub use sink::traits::BoxFuture;
pub use sink::{
    format_vtt_timestamp, CompositeSink, JsonlSink, SinkError, SinkStats, TranscriptionCue,
    TranscriptionSink, WebVttSink,
};
pub use supervisor::{
    is_parent_alive, ChildStatus, FirstClientGate, HealthEmitter, ProcessSupervisor,
    RespawnTracker, ServiceSupervisor, SupervisorBoxFuture, SupervisorError,
    SupervisorHealthReport, WorkerProcessConfig, CHILD_FAILURE_WINDOW_SEC, MAX_CHILD_FAILURES,
    RESPAWN_BACKOFF_BASE_SEC, RESPAWN_BACKOFF_MAX_SEC,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_vtt_timestamp_formatting() {
        assert_eq!(format_vtt_timestamp(0.0), "00:00:00.000");
        assert_eq!(format_vtt_timestamp(61.25), "00:01:01.250");
        assert_eq!(format_vtt_timestamp(3665.123), "01:01:05.123");
    }

    #[test]
    fn test_respawn_policy_backoff_and_limit() {
        let mut tracker = RespawnTracker::new(
            3,
            Duration::from_secs(300),
            Duration::from_secs(1),
            Duration::from_secs(15),
        );

        // 1st failure: base backoff 1.0s
        let d1 = tracker
            .record_failure()
            .expect("1st failure should be allowed");
        assert_eq!(d1, Duration::from_secs(1));

        // 2nd failure: 2.0s
        let d2 = tracker
            .record_failure()
            .expect("2nd failure should be allowed");
        assert_eq!(d2, Duration::from_secs(2));

        // 3rd failure: 4.0s
        let d3 = tracker
            .record_failure()
            .expect("3rd failure should be allowed");
        assert_eq!(d3, Duration::from_secs(4));

        // 4th failure: exceeds max 3 in 5 min -> fails fast
        let d4 = tracker.record_failure();
        assert!(matches!(
            d4,
            Err(SupervisorError::MaxFailuresExceeded { .. })
        ));
    }

    #[test]
    fn test_first_client_gate() {
        let gate = FirstClientGate::new();
        assert!(gate.fire());
        assert!(!gate.fire());
        assert!(gate.is_fired());
    }

    #[test]
    fn test_cancellation_hierarchy() {
        let lifecycle = PipelineLifecycleManager::new();
        let child = lifecycle.child_token();
        assert!(!child.is_cancelled());
        lifecycle.cancel();
        assert!(child.is_cancelled());
    }
}
