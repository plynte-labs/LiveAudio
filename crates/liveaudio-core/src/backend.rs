// SPDX-License-Identifier: MIT
//! Host-side ASR worker selection: backend preference, ct2 readiness, one-shot fallback to the
//! ORT worker, and the `doctor` report lines.

use std::path::Path;

use crate::gpu::{
    select_asr_backend, AsrBackendKind, AsrBackendPref, GpuAdapter, GpuVendor,
    MIN_CUDA_DRIVER_VERSION,
};
use crate::nvidia_runtime::NvidiaRuntimeStatus;
use crate::runtime::{preflight_asr_runtime, preflight_ct2_worker, RuntimeError};
use crate::supervisor::WorkerProcessConfig;

/// Maps the `asr_backend` config value (already clamped to a valid value) to a preference.
pub fn pref_from_config(value: &str) -> AsrBackendPref {
    match value {
        "ort" => AsrBackendPref::Ort,
        "ct2_cuda" => AsrBackendPref::Ct2Cuda,
        _ => AsrBackendPref::Auto,
    }
}

/// The CUDA worker cannot run on CPU, so an explicit `device: cpu` always selects the ORT worker.
pub fn effective_backend_pref<'a>(asr_backend: &'a str, device: &str) -> &'a str {
    if device == "cpu" { "ort" } else { asr_backend }
}

pub fn backend_name(kind: AsrBackendKind) -> &'static str {
    match kind {
        AsrBackendKind::Ort => "ort",
        AsrBackendKind::Ct2Cuda => "ct2_cuda",
    }
}

/// The ct2 runtime is ready only when the CUDA DLLs are installed and the worker exe exists.
pub fn ct2_ready(status: &NvidiaRuntimeStatus, worker: Option<&Path>) -> bool {
    status.installed && worker.is_some()
}

/// Config preference plus detected hardware and runtime state to a backend and reason.
pub fn select_backend(
    pref: &str,
    adapters: &[GpuAdapter],
    cuda_driver_version: Option<i32>,
    status: &NvidiaRuntimeStatus,
    worker: Option<&Path>,
) -> (AsrBackendKind, &'static str) {
    select_asr_backend(
        pref_from_config(pref),
        adapters,
        cuda_driver_version,
        ct2_ready(status, worker),
    )
}

/// Where the ct2 worker failed; either way the session continues on the ORT worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ct2Failure {
    Preflight,
    Start,
}

pub fn ct2_fallback(failure: Ct2Failure) -> (AsrBackendKind, &'static str) {
    (
        AsrBackendKind::Ort,
        match failure {
            Ct2Failure::Preflight => "ct2_preflight_failed",
            Ct2Failure::Start => "ct2_start_failed",
        },
    )
}

/// Lines for `doctor`: GPUs, CUDA driver, NVIDIA runtime, ct2 worker and the selected backend.
pub fn doctor_lines(
    adapters: &[GpuAdapter],
    cuda_driver_version: Option<i32>,
    status: &NvidiaRuntimeStatus,
    worker: Option<&Path>,
    selected: (AsrBackendKind, &str),
) -> Vec<String> {
    let gpus = if adapters.is_empty() {
        "none detected".to_string()
    } else {
        adapters
            .iter()
            .map(|a| {
                let vendor = match a.vendor {
                    GpuVendor::Nvidia => "nvidia",
                    GpuVendor::Amd => "amd",
                    GpuVendor::Intel => "intel",
                    GpuVendor::Other(_) => "other",
                };
                format!("{vendor} {} ({} MB)", a.name, a.dedicated_vram_mb)
            })
            .collect::<Vec<_>>()
            .join("; ")
    };
    let driver = cuda_driver_version.map_or("NOT FOUND".to_string(), |v| {
        format!("{v} (minimum {MIN_CUDA_DRIVER_VERSION})")
    });
    let runtime = if status.installed {
        "INSTALLED".to_string()
    } else {
        format!("MISSING ({})", status.missing.join(", "))
    };
    let worker = worker.map_or("NOT FOUND".to_string(), |p| format!("DISCOVERED ({})", p.display()));
    vec![
        format!("GPU Adapters: {gpus}"),
        format!("CUDA Driver: {driver}"),
        format!("NVIDIA Runtime: {runtime}"),
        format!("CT2 CUDA Worker: {worker}"),
        format!("Selected ASR Backend: {} ({})", backend_name(selected.0), selected.1),
    ]
}

/// A worker config together with the backend it runs and why it was chosen.
#[derive(Debug, Clone)]
pub struct SelectedWorker {
    pub config: WorkerProcessConfig,
    pub kind: AsrBackendKind,
    pub reason: String,
}

/// Detects hardware, picks the backend and preflights its worker. A failing ct2 preflight falls
/// back to the ORT worker; only the ORT preflight error is returned.
pub async fn prepare_worker(pref: &str) -> Result<SelectedWorker, RuntimeError> {
    let worker = crate::runtime::discover_ct2_worker();
    let (mut kind, mut reason) = select_backend(
        pref,
        &crate::gpu::detect_gpu_adapters(),
        crate::gpu::cuda_driver_version(),
        &crate::nvidia_runtime::nvidia_runtime_status(&crate::nvidia_runtime::runtime_dir()),
        worker.as_deref(),
    );
    if kind == AsrBackendKind::Ct2Cuda {
        let config = WorkerProcessConfig::discover_asr_worker_for(kind);
        match preflight_ct2_worker(Path::new(&config.command)).await {
            Ok(()) => return Ok(SelectedWorker { config, kind, reason: reason.into() }),
            Err(_) => (kind, reason) = ct2_fallback(Ct2Failure::Preflight),
        }
    }
    let config = WorkerProcessConfig::discover_asr_worker_for(AsrBackendKind::Ort);
    let ort_dylib = config.envs.get("LIVEAUDIO_ORT_DYLIB_PATH").map(std::path::PathBuf::from);
    preflight_asr_runtime(Path::new(&config.command), ort_dylib.as_deref()).await?;
    Ok(SelectedWorker { config, kind, reason: reason.into() })
}

/// ORT worker config carrying over the session settings of a failed ct2 config.
pub fn ort_fallback_config(failed: &WorkerProcessConfig) -> WorkerProcessConfig {
    WorkerProcessConfig {
        init_payload: failed.init_payload.clone(),
        decode_timeout: failed.decode_timeout,
        ..WorkerProcessConfig::discover_asr_worker_for(AsrBackendKind::Ort)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn nvidia() -> Vec<GpuAdapter> {
        vec![GpuAdapter { vendor: GpuVendor::Nvidia, name: "RTX".into(), dedicated_vram_mb: 12115 }]
    }
    fn status(installed: bool) -> NvidiaRuntimeStatus {
        NvidiaRuntimeStatus {
            installed,
            missing: if installed { vec![] } else { vec!["cudnn64_9.dll".into()] },
        }
    }

    #[test]
    fn config_values_map_to_prefs_and_unknown_means_auto() {
        assert_eq!(pref_from_config("auto"), AsrBackendPref::Auto);
        assert_eq!(pref_from_config("ort"), AsrBackendPref::Ort);
        assert_eq!(pref_from_config("ct2_cuda"), AsrBackendPref::Ct2Cuda);
        assert_eq!(pref_from_config("bogus"), AsrBackendPref::Auto);
        assert_eq!(backend_name(AsrBackendKind::Ort), "ort");
        assert_eq!(backend_name(AsrBackendKind::Ct2Cuda), "ct2_cuda");
    }

    #[test]
    fn cpu_device_forces_the_ort_worker() {
        assert_eq!(effective_backend_pref("auto", "cpu"), "ort");
        assert_eq!(effective_backend_pref("ct2_cuda", "cpu"), "ort");
        assert_eq!(effective_backend_pref("auto", "cuda"), "auto");
        assert_eq!(effective_backend_pref("ct2_cuda", "cuda"), "ct2_cuda");
    }

    #[test]
    fn ct2_needs_both_the_runtime_and_the_worker() {
        let w = PathBuf::from("w.exe");
        assert!(ct2_ready(&status(true), Some(&w)));
        assert!(!ct2_ready(&status(true), None));
        assert!(!ct2_ready(&status(false), Some(&w)));
    }

    #[test]
    fn selection_composes_pref_hardware_and_readiness() {
        let w = PathBuf::from("w.exe");
        let pick = |p, s: &NvidiaRuntimeStatus, w: Option<&Path>| {
            select_backend(p, &nvidia(), Some(13040), s, w)
        };
        assert_eq!(pick("auto", &status(true), Some(&w)), (AsrBackendKind::Ct2Cuda, "nvidia_cuda_ready"));
        assert_eq!(pick("ct2_cuda", &status(true), Some(&w)).0, AsrBackendKind::Ct2Cuda);
        assert_eq!(pick("ort", &status(true), Some(&w)), (AsrBackendKind::Ort, "user_selected_ort"));
        assert_eq!(pick("auto", &status(true), None), (AsrBackendKind::Ort, "nvidia_runtime_not_installed"));
        assert_eq!(pick("auto", &status(false), Some(&w)).1, "nvidia_runtime_not_installed");
        assert_eq!(select_backend("auto", &[], None, &status(true), Some(&w)).1, "no_nvidia_gpu");
    }

    #[test]
    fn ct2_failures_fall_back_to_ort_with_a_stage_reason() {
        assert_eq!(ct2_fallback(Ct2Failure::Preflight), (AsrBackendKind::Ort, "ct2_preflight_failed"));
        assert_eq!(ct2_fallback(Ct2Failure::Start), (AsrBackendKind::Ort, "ct2_start_failed"));
    }

    #[test]
    fn doctor_lines_report_hardware_runtime_worker_and_selection() {
        let w = PathBuf::from("C:/rt/liveaudio-asr-worker-cuda.exe");
        let lines = doctor_lines(&nvidia(), Some(13040), &status(false), None, (AsrBackendKind::Ort, "nvidia_runtime_not_installed"));
        let text = lines.join("
");
        assert!(text.contains("GPU Adapters: nvidia RTX (12115 MB)"), "{text}");
        assert!(text.contains("CUDA Driver: 13040"), "{text}");
        assert!(text.contains("NVIDIA Runtime: MISSING (cudnn64_9.dll)"), "{text}");
        assert!(text.contains("CT2 CUDA Worker: NOT FOUND"), "{text}");
        assert!(text.contains("Selected ASR Backend: ort (nvidia_runtime_not_installed)"), "{text}");

        let ok = doctor_lines(&[], None, &status(true), Some(&w), (AsrBackendKind::Ort, "no_nvidia_gpu")).join("
");
        assert!(ok.contains("GPU Adapters: none detected"), "{ok}");
        assert!(ok.contains("CUDA Driver: NOT FOUND"), "{ok}");
        assert!(ok.contains("NVIDIA Runtime: INSTALLED"), "{ok}");
        assert!(ok.contains("CT2 CUDA Worker: DISCOVERED (C:/rt/liveaudio-asr-worker-cuda.exe)"), "{ok}");
    }

    #[test]
    fn ort_fallback_keeps_session_settings() {
        let _env = crate::test_env::lock();
        let failed = WorkerProcessConfig {
            command: "cuda-worker.exe".into(),
            init_payload: Some(serde_json::json!({"model": "small"})),
            decode_timeout: Some(std::time::Duration::from_secs(7)),
            ..WorkerProcessConfig::default()
        };
        let ort = ort_fallback_config(&failed);
        assert_ne!(ort.command, "cuda-worker.exe");
        assert_eq!(ort.init_payload, failed.init_payload);
        assert_eq!(ort.decode_timeout, failed.decode_timeout);
        assert!(!ort.envs.contains_key("PATH"));
    }

    #[test]
    fn ct2_config_swaps_exe_and_prepends_the_runtime_dir_to_path() {
        // Reads LIVEAUDIO_HOME/LOCALAPPDATA twice (discovery, then the expected dir): hold the lock
        // across both so a concurrent config patch test cannot change the runtime dir in between.
        let _env = crate::test_env::lock();
        let ort = WorkerProcessConfig::discover_asr_worker_for(AsrBackendKind::Ort);
        assert!(!ort.envs.contains_key("PATH"));
        let ct2 = WorkerProcessConfig::discover_asr_worker_for(AsrBackendKind::Ct2Cuda);
        assert!(ct2.command.ends_with(crate::runtime::CT2_WORKER_EXE));
        let rt = crate::nvidia_runtime::runtime_dir();
        let first = std::env::split_paths(&ct2.envs["PATH"]).next().unwrap();
        assert_eq!(first, rt);
    }

    #[tokio::test]
    async fn explicit_ort_pref_never_touches_ct2() {
        // The ORT preflight may fail on machines without a built worker; the kind/reason are what matter.
        if let Ok(sel) = prepare_worker("ort").await {
            assert_eq!(sel.kind, AsrBackendKind::Ort);
            assert_eq!(sel.reason, "user_selected_ort");
        }
    }
}
