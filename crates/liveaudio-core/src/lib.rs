// SPDX-License-Identifier: MIT

pub mod acoustic_events;
pub mod asr;
pub mod backend;
pub mod config;
pub mod download;
pub mod event;
pub mod gpu;
pub mod job_object;
pub mod nvidia_runtime;
pub mod pipeline;
pub mod runtime;
pub mod service_lock;
pub mod sink;
pub mod supervisor;
pub mod system_fonts;
pub mod usage_stats;

pub use asr::{
    AsrBackend, AsrBoxFuture, AsrError, AsrHealthStatus, AsrModelConfig, AsrTranscriptionRequest,
    AsrTranscriptionResponse, TranscriptionSegment,
};
pub use config::{
    audio_queue_capacity, get_data_home, get_hf_home, get_models_home, load_config,
    load_config_readonly, load_from_path, save_config, save_to_path, ConfigReadonlyInfo,
    LiveAudioConfig,
};
pub use download::{download_verified, extract_zip_members, DownloadError};
pub use gpu::{
    cuda_driver_version, detect_gpu_adapters, select_asr_backend, AsrBackendKind, AsrBackendPref,
    GpuAdapter, GpuVendor, MIN_CUDA_DRIVER_VERSION,
};
pub use nvidia_runtime::{
    install_nvidia_runtime, license_urls, nvidia_runtime_status, runtime_dir as nvidia_runtime_dir,
    NvidiaRuntimeStatus,
};
pub use event::{EventBus, HealthState, HealthStateMachine};
pub use job_object::JobObject;
pub use pipeline::{PipelineConfig, PipelineLifecycleManager};
pub use runtime::{preflight_asr_runtime, suppress_loader_error_dialogs, RuntimeError};
pub use service_lock::{service_lock_holder, ServiceLock, ServiceLockHolder, SERVICE_LOCK_NAME};
pub use sink::traits::BoxFuture;
pub use sink::{
    format_srt_timestamp, format_vtt_timestamp, CompositeSink, CueFileSink, JsonlSink, SinkError,
    SinkStats, TranscriptionCue, TranscriptionSink,
};
pub use supervisor::{
    is_parent_alive, ChildStatus, FirstClientGate, HealthEmitter, ProcessSupervisor,
    RespawnTracker, ServiceSupervisor, SupervisorBoxFuture, SupervisorError,
    SupervisorHealthReport, WorkerProcessConfig, CHILD_FAILURE_WINDOW_SEC, MAX_CHILD_FAILURES,
    RESPAWN_BACKOFF_BASE_SEC, RESPAWN_BACKOFF_MAX_SEC,
};

/// Serializes unit tests that read or mutate process environment variables (`LIVEAUDIO_HOME`,
/// `LIVEAUDIO_WORKER_PATH`, ...). The harness runs tests on parallel threads that share one
/// environment, so an unguarded read can observe another test's override.
#[cfg(test)]
pub(crate) mod test_env {
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// A panicking test must not turn the lock into a cascade of failures, so poison is ignored.
    pub(crate) fn lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }
}

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

        // 3rd failure: reaches max 3 in 5 min -> fails fast
        let d3 = tracker.record_failure();
        assert!(matches!(
            d3,
            Err(SupervisorError::MaxFailuresExceeded { count: 3, .. })
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
