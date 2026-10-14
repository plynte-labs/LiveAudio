// SPDX-License-Identifier: MIT

//! Production-grade Process Supervisor running child workers inside a native Windows Job Object.
//!
//! Provides:
//! - Kernel-level containment (0 zombie child processes via Windows Job Object)
//! - Clean startup and ready handshake
//! - Watchdog ping/pong liveness monitoring
//! - Crash detection with bounded exponential backoff (max 3 failures in 300s window)
//! - Cooperative cancellation via `CancellationToken`
//! - Idempotent double-shutdown guarantees

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as SyncMutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, Mutex, Notify};
use tokio_util::sync::CancellationToken;

use crate::event::{EventBus, HealthState, HealthStateMachine};
use crate::job_object::JobObject;
use crate::supervisor::error::SupervisorError;
use crate::supervisor::respawn::{
    RespawnTracker, CHILD_FAILURE_WINDOW_SEC, MAX_CHILD_FAILURES, RESPAWN_BACKOFF_BASE_SEC,
    RESPAWN_BACKOFF_MAX_SEC,
};
use crate::supervisor::traits::{ServiceSupervisor, SupervisorBoxFuture};
use crate::supervisor::types::{ChildStatus, SupervisorHealthReport};

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

async fn handle_job_assignment(
    name: &str,
    child: &mut Child,
    assignment: std::io::Result<()>,
) -> Result<(), SupervisorError> {
    if let Err(error) = assignment {
        let kill_error = child.start_kill().err();
        let wait_error = child.wait().await.err();
        let mut reason = format!("Failed to assign child to Job Object: {error}");
        if let Some(error) = kill_error {
            reason.push_str(&format!("; kill failed: {error}"));
        }
        if let Some(error) = wait_error {
            reason.push_str(&format!("; reap failed: {error}"));
        }
        return Err(SupervisorError::ProcessSpawnFailed {
            name: name.to_string(),
            reason,
        });
    }
    Ok(())
}

/// Tracks the decode the worker reports in its pong and returns true once that same
/// decode has been running longer than `limit`. A new utterance restarts the clock.
fn decode_deadline_exceeded(
    decoding: &mut Option<(String, Instant)>,
    pong: &serde_json::Value,
    limit: Duration,
) -> bool {
    let payload = pong.get("payload");
    let active = payload
        .filter(|p| p.get("state").and_then(|s| s.as_str()) == Some("transcribing"))
        .and_then(|p| p.get("active_utterance_id"))
        .and_then(|id| id.as_str());
    let Some(id) = active else {
        *decoding = None;
        return false;
    };
    match decoding {
        Some((current, started)) if current == id => started.elapsed() > limit,
        _ => {
            *decoding = Some((id.to_string(), Instant::now()));
            false
        }
    }
}

async fn wait_for_child_exit(exited: &AtomicBool, exit_notify: &Notify) {
    while !exited.load(Ordering::SeqCst) {
        // notify_one stores a permit, so an exit between the state check and
        // registering this waiter cannot be lost.
        exit_notify.notified().await;
    }
}

/// Kills a freshly spawned worker when its startup future is dropped before the
/// handshake completes (for example, when stop aborts a respawn). Disarmed once ready.
struct StartupKillGuard {
    kill: Arc<Notify>,
    armed: bool,
}

impl StartupKillGuard {
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for StartupKillGuard {
    fn drop(&mut self) {
        if self.armed {
            // notify_one stores a permit, so the kill is not lost if the waiter has not started.
            self.kill.notify_one();
        }
    }
}

/// Resolves once no worker output has arrived for `stall`; output restarts the window.
async fn wait_for_startup_stall(last_event_ms: &AtomicU64, stall: Duration) {
    loop {
        let silent = Duration::from_millis(
            now_millis().saturating_sub(last_event_ms.load(Ordering::SeqCst)),
        );
        if silent >= stall {
            return;
        }
        tokio::time::sleep(stall - silent).await;
    }
}

#[cfg(test)]
mod assignment_cleanup_tests {
    use super::handle_job_assignment;
    use std::process::Stdio;
    use tokio::process::Command;

    #[tokio::test]
    async fn assignment_failure_kills_and_reaps_the_spawned_worker() {
        let mut child = Command::new("python")
            .args(["-c", "import time; time.sleep(60)"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn deterministic child");

        let error = handle_job_assignment(
            "test-worker",
            &mut child,
            Err(std::io::Error::other("injected")),
        )
        .await
        .expect_err("failed assignment must reject startup");
        assert!(error.to_string().contains("injected"));
        let reaped = child.try_wait().expect("child status").is_some();
        if !reaped {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
        assert!(reaped, "assignment failure must kill and reap the child");
    }
}

#[cfg(test)]
mod bounded_stop_tests {
    use super::{ProcessSupervisor, WorkerProcessConfig};
    use crate::supervisor::is_parent_alive;
    use crate::supervisor::traits::ServiceSupervisor;
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn cancelling_startup_reaps_worker_while_waiting_for_ready() {
        use std::time::SystemTime;
        use tokio_util::sync::CancellationToken;

        let unique = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let pid_file = std::env::temp_dir().join(format!(
            "liveaudio-start-cancel-{}-{unique}.pid",
            std::process::id()
        ));
        let mut config = WorkerProcessConfig::default();
        config.name = "cancelled-start-worker".to_string();
        config.command = "python".to_string();
        config.args = vec![
            "-u".to_string(),
            "-c".to_string(),
            "import os,sys,time; open(sys.argv[1], 'w').write(str(os.getpid())); time.sleep(60)"
                .to_string(),
            pid_file.to_string_lossy().to_string(),
        ];
        config.ready_timeout = Duration::from_secs(30);
        config.auto_respawn = false;
        let mut supervisor = ProcessSupervisor::new(config, std::process::id(), 0, None)
            .expect("supervisor creation");
        let cancellation = CancellationToken::new();
        let cancel_task = {
            let cancellation = cancellation.clone();
            let pid_file = pid_file.clone();
            tokio::spawn(async move {
                // The worker creates the file before its pid is flushed, so wait for a parseable pid.
                let pid = tokio::time::timeout(Duration::from_secs(5), async {
                    loop {
                        let pid = std::fs::read_to_string(&pid_file)
                            .ok()
                            .and_then(|text| text.trim().parse::<u32>().ok());
                        if let Some(pid) = pid {
                            break pid;
                        }
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                })
                .await
                .expect("worker writes pid before cancellation");
                cancellation.cancel();
                pid
            })
        };

        let result = supervisor.start_internal_cancellable(cancellation).await;
        let pid = cancel_task.await.expect("cancel task");
        assert!(matches!(
            result,
            Err(super::super::error::SupervisorError::Cancelled)
        ));
        assert!(
            !super::super::is_parent_alive(pid),
            "cancel must reap the worker"
        );
        let _ = std::fs::remove_file(pid_file);
    }

    #[tokio::test]
    async fn shutdown_deadline_includes_a_saturated_nonreading_worker_command_queue() {
        let mut config = WorkerProcessConfig::default();
        config.name = "nonreading-worker".to_string();
        config.command = "python".to_string();
        config.args = vec![
            "-u".to_string(),
            "-c".to_string(),
            "import json,time; print(json.dumps({'event':'ready'}), flush=True); time.sleep(3)"
                .to_string(),
        ];
        config.ready_timeout = Duration::from_secs(5);
        config.auto_respawn = false;
        let mut supervisor = ProcessSupervisor::new(config, std::process::id(), 0, None)
            .expect("supervisor creation");
        supervisor
            .start()
            .await
            .expect("mock worker ready handshake");
        let pid = supervisor.child_pid().await.expect("worker pid");

        let cmd_tx = supervisor
            .active_child
            .lock()
            .await
            .as_ref()
            .expect("active worker")
            .cmd_tx
            .clone();
        cmd_tx
            .try_send("x".repeat(32 * 1024 * 1024))
            .expect("queue one payload larger than the worker stdin pipe");
        tokio::time::sleep(Duration::from_millis(100)).await;
        let payload = "queued".to_string();
        let mut filled = 0;
        loop {
            match cmd_tx.try_send(payload.clone()) {
                Ok(()) => filled += 1,
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => break,
                Err(error) => panic!("worker command channel closed unexpectedly: {error}"),
            }
        }
        assert!(
            filled > 0,
            "test must saturate the real worker command queue"
        );
        drop(cmd_tx);

        let started = Instant::now();
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            supervisor.stop_internal(Duration::from_millis(100)),
        )
        .await;
        assert!(
            result.is_ok(),
            "shutdown must not hang while enqueueing shutdown"
        );
        result.unwrap().expect("bounded shutdown should succeed");
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(!is_parent_alive(pid), "worker must be killed and reaped");
    }

    #[tokio::test]
    async fn send_command_gives_up_when_the_worker_stops_reading() {
        let mut config = WorkerProcessConfig::default();
        config.name = "stalled-reader-worker".to_string();
        config.command = "python".to_string();
        config.args = vec![
            "-u".to_string(),
            "-c".to_string(),
            "import json,time; print(json.dumps({'event':'ready'}), flush=True); time.sleep(60)"
                .to_string(),
        ];
        config.ready_timeout = Duration::from_secs(5);
        config.auto_respawn = false;
        let mut supervisor = ProcessSupervisor::new(config, std::process::id(), 0, None)
            .expect("supervisor creation");
        supervisor
            .start()
            .await
            .expect("mock worker ready handshake");

        let cmd_tx = supervisor
            .active_child
            .lock()
            .await
            .as_ref()
            .expect("active worker")
            .cmd_tx
            .clone();
        cmd_tx
            .try_send("x".repeat(32 * 1024 * 1024))
            .expect("queue one payload larger than the worker stdin pipe");
        tokio::time::sleep(Duration::from_millis(100)).await;
        while cmd_tx.try_send("queued".to_string()).is_ok() {}

        let started = Instant::now();
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            supervisor.send_command(&serde_json::json!({"cmd": "ping"})),
        )
        .await
        .expect("send_command must return instead of blocking on a full queue");
        assert!(
            result.is_err(),
            "a full, non-draining queue must be reported"
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        drop(cmd_tx);
        supervisor
            .stop_internal(Duration::from_millis(100))
            .await
            .expect("bounded stop");
    }
}

/// Startup silence longer than this kills the worker. Python `ABSOLUTE_STARTUP_SEC`
/// (liveaudio/core/provisioning.py), not `DEFAULT_STALL_SEC` (150 s): the worker emits one
/// "loading" status and then stays silent while a large model loads, so 150 s would kill slow
/// but healthy CPU loads. There is deliberately no absolute startup ceiling: a worker that keeps
/// reporting progress is still starting.
const STARTUP_STALL_WINDOW: Duration = Duration::from_secs(300);

/// Longest a command waits for room in a worker's stdin queue before the caller gets an error.
const COMMAND_ENQUEUE_TIMEOUT: Duration = Duration::from_secs(2);

/// Configuration defining how a child worker process is spawned and monitored.
#[derive(Debug, Clone)]
pub struct WorkerProcessConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub working_dir: Option<PathBuf>,
    pub envs: HashMap<String, String>,
    /// Startup stall window: the ready handshake fails after this long with no worker output.
    /// Any output line (including download progress) restarts it, so a slow first-run
    /// download is not killed as a crash. Defaults to Python's `DEFAULT_STALL_SEC`.
    pub ready_timeout: Duration,
    pub watchdog_interval: Duration,
    pub watchdog_timeout: Duration,
    pub grace_shutdown_timeout: Duration,
    pub max_failures: usize,
    pub failure_window: Duration,
    pub base_backoff: Duration,
    pub max_backoff: Duration,
    pub auto_respawn: bool,
    pub init_payload: Option<serde_json::Value>,
    /// Longest a single decode may run, as reported by the worker's pong state,
    /// before the worker is treated as failed. `None` disables the deadline.
    pub decode_timeout: Option<Duration>,
}

/// Interpreter paths relative to the exe directory, in trust order.
#[cfg(windows)]
const BUNDLED_PYTHON_RELATIVE: [&str; 3] =
    ["python.exe", "python/python.exe", "runtime/python.exe"];
#[cfg(not(windows))]
const BUNDLED_PYTHON_RELATIVE: [&str; 3] =
    ["bin/python3", "python/bin/python3", "runtime/bin/python3"];

/// Project virtualenv interpreter, also checked in the exe's ancestors for dev layouts.
#[cfg(windows)]
const VENV_PYTHON_RELATIVE: &str = ".venv/Scripts/python.exe";
#[cfg(not(windows))]
const VENV_PYTHON_RELATIVE: &str = ".venv/bin/python";

/// Candidates derived only from the exe location, never from the current directory.
pub(crate) fn bundled_python_candidates(exe_dir: &Path) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = BUNDLED_PYTHON_RELATIVE
        .iter()
        .map(|relative| exe_dir.join(relative))
        .collect();
    for ancestor in ["", "..", "../..", "../../.."] {
        candidates.push(exe_dir.join(ancestor).join(VENV_PYTHON_RELATIVE));
    }
    candidates
}

/// First Python found in an absolute PATH directory. Relative PATH entries resolve against the cwd.
pub(crate) fn python_on_path(path: &std::ffi::OsStr) -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "python.exe"
    } else {
        "python"
    };
    std::env::split_paths(path)
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

impl WorkerProcessConfig {
    /// Discovers the Python executable from the environment, bundled directories, or PATH.
    pub fn find_python_executable() -> PathBuf {
        // 1. Explicit env var override
        if let Ok(py) = std::env::var("LIVEAUDIO_PYTHON") {
            let p = PathBuf::from(py);
            if p.exists() {
                return p;
            }
        }

        // 2. Runtime explicitly provisioned by `setup-runtime`.
        if let Some(managed_python) =
            crate::runtime::discover_managed_python(&crate::runtime::managed_runtime_root())
        {
            return managed_python;
        }

        // 3. Relative to current exe (installed / portable bundle layout or debug bin).
        // The current directory is never searched: it is attacker-writable in practice.
        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(exe_dir) = current_exe.parent() {
                for cand in bundled_python_candidates(exe_dir) {
                    if cand.exists() {
                        if let Ok(canonical) = std::fs::canonicalize(&cand) {
                            return canonical;
                        }
                        return cand;
                    }
                }
            }
        }

        // 4. Absolute PATH directories only. A bare `python` would let Windows CreateProcess
        // consult the current directory before PATH.
        if let Some(found) = std::env::var_os("PATH").and_then(|path| python_on_path(&path)) {
            return found;
        }

        PathBuf::from("python")
    }

    /// Discovers the `asr_worker.py` script from the environment, bundle, or repository.
    pub fn find_asr_worker_script() -> Option<PathBuf> {
        // 1. Explicit env var override
        if let Ok(worker) = std::env::var("LIVEAUDIO_WORKER_PATH") {
            let p = PathBuf::from(worker);
            if p.exists() {
                return std::fs::canonicalize(&p).ok().or(Some(p));
            }
        }

        // The managed project contains the exact package staged for its runtime.
        if let Some(managed_worker) =
            crate::runtime::discover_managed_worker(&crate::runtime::managed_runtime_root())
        {
            return Some(managed_worker);
        }

        Self::find_external_asr_worker_script()
    }

    /// Finds the shipped package source used to refresh the managed runtime.
    pub fn find_asr_worker_source() -> Option<PathBuf> {
        let managed_root = crate::runtime::managed_runtime_root();
        if let Ok(worker) = std::env::var("LIVEAUDIO_WORKER_PATH") {
            let path = PathBuf::from(worker);
            if path.is_file() && !is_managed_runtime_path(&path, &managed_root) {
                return std::fs::canonicalize(&path).ok().or(Some(path));
            }
        }
        Self::find_external_asr_worker_script()
            .filter(|worker| !is_managed_runtime_path(worker, &managed_root))
    }

    fn find_external_asr_worker_script() -> Option<PathBuf> {
        // Relative to current exe (installed bundle or debug build)
        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(exe_dir) = current_exe.parent() {
                let candidates = [
                    exe_dir.join("liveaudio/service/asr_worker.py"),
                    exe_dir.join("resources/liveaudio/service/asr_worker.py"),
                    exe_dir.join("_up_/liveaudio/service/asr_worker.py"),
                    exe_dir.join("../liveaudio/service/asr_worker.py"),
                    exe_dir.join("../../liveaudio/service/asr_worker.py"),
                    exe_dir.join("../../../liveaudio/service/asr_worker.py"),
                ];
                for cand in &candidates {
                    if cand.exists() {
                        return std::fs::canonicalize(cand)
                            .ok()
                            .or_else(|| Some(cand.clone()));
                    }
                }
            }
        }

        // Relative to current working directory: dev builds only (`tauri dev` runs from desktop/).
        // A release build must not execute a worker script planted in the working directory.
        if cfg!(debug_assertions) {
            let cwd_candidates = [
                PathBuf::from("liveaudio/service/asr_worker.py"),
                PathBuf::from("resources/liveaudio/service/asr_worker.py"),
                PathBuf::from("../liveaudio/service/asr_worker.py"),
            ];
            for cand in &cwd_candidates {
                if cand.exists() {
                    return std::fs::canonicalize(cand)
                        .ok()
                        .or_else(|| Some(cand.clone()));
                }
            }
        }

        None
    }

    /// Discovers the root directory containing the `liveaudio` package.
    pub fn find_package_root(worker_script: &Option<PathBuf>) -> Option<PathBuf> {
        if let Some(ref script) = worker_script {
            if let Some(service_dir) = script.parent() {
                if let Some(pkg_dir) = service_dir.parent() {
                    if let Some(root_dir) = pkg_dir.parent() {
                        return std::fs::canonicalize(root_dir)
                            .ok()
                            .or_else(|| Some(root_dir.to_path_buf()));
                    }
                }
            }
        }
        std::env::current_dir().ok()
    }

    /// Discovers and builds a production-ready `WorkerProcessConfig` configured
    /// with UTF-8 flags, HuggingFace model cache directory layout, and path normalization.
    pub fn discover_asr_worker() -> Self {
        let python_exe = Self::find_python_executable();
        let worker_script = Self::find_asr_worker_script();
        let package_root = Self::find_package_root(&worker_script);

        let script_arg = worker_script
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "liveaudio/service/asr_worker.py".to_string());

        let mut envs = HashMap::new();
        envs.insert("PYTHONUNBUFFERED".to_string(), "1".to_string());
        envs.insert("PYTHONUTF8".to_string(), "1".to_string());
        envs.insert("PYTHONIOENCODING".to_string(), "utf-8".to_string());
        envs.insert(
            "HF_HUB_DISABLE_SYMLINKS_WARNING".to_string(),
            "1".to_string(),
        );

        // Model cache directory layout: HF_HOME in %APPDATA%/LiveAudio/models/hf
        let hf_home = crate::config::get_hf_home();
        envs.insert("HF_HOME".to_string(), hf_home.to_string_lossy().to_string());
        let torch_home = crate::config::get_torch_home();
        envs.insert(
            "TORCH_HOME".to_string(),
            torch_home.to_string_lossy().to_string(),
        );

        // Ensure package root is in PYTHONPATH so `import liveaudio` works everywhere
        if let Some(ref root) = package_root {
            envs.insert("PYTHONPATH".to_string(), root.to_string_lossy().to_string());
        }

        Self {
            name: "asr-worker".to_string(),
            command: python_exe.to_string_lossy().to_string(),
            args: vec!["-u".to_string(), script_arg],
            working_dir: package_root,
            envs,
            ready_timeout: STARTUP_STALL_WINDOW,
            watchdog_interval: Duration::from_secs(2),
            watchdog_timeout: Duration::from_secs(5),
            grace_shutdown_timeout: Duration::from_secs(3),
            max_failures: MAX_CHILD_FAILURES,
            failure_window: Duration::from_secs_f64(CHILD_FAILURE_WINDOW_SEC),
            base_backoff: Duration::from_secs_f64(RESPAWN_BACKOFF_BASE_SEC),
            max_backoff: Duration::from_secs_f64(RESPAWN_BACKOFF_MAX_SEC),
            auto_respawn: true,
            init_payload: None,
            decode_timeout: None,
        }
    }
}

fn is_managed_runtime_path(path: &PathBuf, managed_root: &PathBuf) -> bool {
    let normalized_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
    let normalized_root =
        std::fs::canonicalize(managed_root).unwrap_or_else(|_| managed_root.clone());
    normalized_path.starts_with(normalized_root)
}

impl Default for WorkerProcessConfig {
    fn default() -> Self {
        Self {
            name: "asr-worker".to_string(),
            command: "python".to_string(),
            args: vec![
                "-u".to_string(),
                "liveaudio/service/asr_worker.py".to_string(),
            ],
            working_dir: None,
            envs: HashMap::new(),
            ready_timeout: STARTUP_STALL_WINDOW,
            watchdog_interval: Duration::from_secs(2),
            watchdog_timeout: Duration::from_secs(5),
            grace_shutdown_timeout: Duration::from_secs(3),
            max_failures: MAX_CHILD_FAILURES,
            failure_window: Duration::from_secs_f64(CHILD_FAILURE_WINDOW_SEC),
            base_backoff: Duration::from_secs_f64(RESPAWN_BACKOFF_BASE_SEC),
            max_backoff: Duration::from_secs_f64(RESPAWN_BACKOFF_MAX_SEC),
            auto_respawn: true,
            init_payload: None,
            decode_timeout: None,
        }
    }
}

/// Represents an actively running child worker process.
struct ActiveChild {
    pid: u32,
    cmd_tx: mpsc::Sender<String>,
    kill_notify: Arc<Notify>,
    exit_notify: Arc<Notify>,
    exited: Arc<AtomicBool>,
    is_ready: Arc<AtomicBool>,
    _last_pong_ms: Arc<AtomicU64>,
    watchdog_handle: Option<tokio::task::JoinHandle<()>>,
}

/// Process Supervisor managing child worker processes inside a native Windows Job Object.
pub struct ProcessSupervisor {
    config: WorkerProcessConfig,
    job_object: Arc<JobObject>,
    respawn_tracker: Arc<Mutex<RespawnTracker>>,
    cancellation_token: CancellationToken,
    service_pid: u32,
    parent_pid: u32,
    event_bus: Arc<EventBus>,
    state_machine: Arc<Mutex<HealthStateMachine>>,
    active_child: Arc<Mutex<Option<ActiveChild>>>,
    restart_count: Arc<AtomicUsize>,
    is_shutting_down: Arc<AtomicBool>,
    shutdown_complete: Arc<AtomicBool>,
    worker_attempt: Arc<AtomicU64>,
    start_time: Instant,
    supervisor_loop_handle: SyncMutex<Option<tokio::task::JoinHandle<()>>>,
    worker_event_tx: tokio::sync::broadcast::Sender<serde_json::Value>,
}

impl ProcessSupervisor {
    /// Creates a new `ProcessSupervisor`.
    pub fn new(
        config: WorkerProcessConfig,
        service_pid: u32,
        parent_pid: u32,
        health_file: Option<PathBuf>,
    ) -> std::io::Result<Self> {
        let job_object = Arc::new(JobObject::create()?);
        let event_bus = Arc::new(EventBus::default());
        // This is a bounded event stream: consumers use progress as latest state,
        // not a lossless history of every download sample.
        let (worker_event_tx, _) = tokio::sync::broadcast::channel(1024);
        let state_machine = Arc::new(Mutex::new(HealthStateMachine::new(
            service_pid,
            parent_pid,
            health_file,
            event_bus.clone(),
        )));
        let respawn_tracker = Arc::new(Mutex::new(RespawnTracker::new(
            config.max_failures,
            config.failure_window,
            config.base_backoff,
            config.max_backoff,
        )));

        Ok(Self {
            config,
            job_object,
            respawn_tracker,
            cancellation_token: CancellationToken::new(),
            service_pid,
            parent_pid,
            event_bus,
            state_machine,
            active_child: Arc::new(Mutex::new(None)),
            restart_count: Arc::new(AtomicUsize::new(0)),
            is_shutting_down: Arc::new(AtomicBool::new(false)),
            shutdown_complete: Arc::new(AtomicBool::new(false)),
            worker_attempt: Arc::new(AtomicU64::new(0)),
            start_time: Instant::now(),
            supervisor_loop_handle: SyncMutex::new(None),
            worker_event_tx,
        })
    }

    fn loop_slot(&self) -> std::sync::MutexGuard<'_, Option<tokio::task::JoinHandle<()>>> {
        self.supervisor_loop_handle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Subscribe to raw events emitted by the child worker over stdout (e.g. transcription_result, status).
    pub fn subscribe_worker_events(&self) -> tokio::sync::broadcast::Receiver<serde_json::Value> {
        self.worker_event_tx.subscribe()
    }

    /// Returns the generation assigned to the most recently spawned worker process.
    pub fn current_worker_attempt(&self) -> u64 {
        self.worker_attempt.load(Ordering::SeqCst)
    }

    /// Attach an external event bus.
    pub fn with_event_bus(mut self, event_bus: Arc<EventBus>) -> Self {
        self.event_bus = event_bus;
        self
    }

    /// Access the internal event bus.
    pub fn event_bus(&self) -> Arc<EventBus> {
        self.event_bus.clone()
    }

    /// Access the cancellation token.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation_token.clone()
    }

    /// Current child PID if alive.
    pub async fn child_pid(&self) -> Option<u32> {
        let guard = self.active_child.lock().await;
        guard.as_ref().map(|c| c.pid)
    }

    /// Check if child worker is active.
    pub async fn is_running(&self) -> bool {
        let guard = self.active_child.lock().await;
        guard.is_some()
    }

    /// Check if shutdown has completed.
    pub fn is_shutdown(&self) -> bool {
        self.shutdown_complete.load(Ordering::SeqCst)
    }

    /// Number of restarts recorded.
    pub fn restart_count(&self) -> usize {
        self.restart_count.load(Ordering::SeqCst)
    }

    /// Send a JSON command line to the active worker.
    pub async fn send_command(&self, cmd: &serde_json::Value) -> Result<(), SupervisorError> {
        let cmd_tx = {
            let guard = self.active_child.lock().await;
            guard
                .as_ref()
                .map(|child| child.cmd_tx.clone())
                .ok_or_else(|| {
                    SupervisorError::InternalError("No active child worker".to_string())
                })?
        };
        let line = serde_json::to_string(cmd)
            .map_err(|e| SupervisorError::InternalError(e.to_string()))?;
        match tokio::time::timeout(COMMAND_ENQUEUE_TIMEOUT, cmd_tx.send(line)).await {
            Ok(sent) => sent.map_err(|_| {
                SupervisorError::InternalError("Child command pipe disconnected".to_string())
            }),
            Err(_) => Err(SupervisorError::InternalError(
                "Child command queue stayed full".to_string(),
            )),
        }
    }

    /// Internal function to spawn a single worker process and complete ready handshake.
    async fn spawn_and_handshake(
        config: &WorkerProcessConfig,
        job_object: &Arc<JobObject>,
        event_bus: &Arc<EventBus>,
        state_machine: &Arc<Mutex<HealthStateMachine>>,
        service_pid: u32,
        parent_pid: u32,
        worker_event_tx: &tokio::sync::broadcast::Sender<serde_json::Value>,
        attempt_id: u64,
        cancellation: Option<&CancellationToken>,
    ) -> Result<ActiveChild, SupervisorError> {
        let _ = worker_event_tx.send(serde_json::json!({
            "event": "worker_attempt_started",
            "attempt_id": attempt_id,
        }));
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);
        cmd.envs(&config.envs);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        // Covers a startup dropped before the wait task owns the child.
        cmd.kill_on_drop(true);
        #[cfg(windows)]
        crate::runtime::apply_no_console_to_tokio(&mut cmd);

        if let Some(ref dir) = config.working_dir {
            cmd.current_dir(dir);
        }

        let mut child: Child = cmd
            .spawn()
            .map_err(|e| SupervisorError::ProcessSpawnFailed {
                name: config.name.clone(),
                reason: e.to_string(),
            })?;

        let pid = child
            .id()
            .ok_or_else(|| SupervisorError::ProcessSpawnFailed {
                name: config.name.clone(),
                reason: "Child process exited immediately after spawn".to_string(),
            })?;

        let kill_notify = Arc::new(Notify::new());
        let mut startup_guard = StartupKillGuard {
            kill: kill_notify.clone(),
            armed: true,
        };

        // Critical: assign PID to native Windows Job Object immediately!
        handle_job_assignment(&config.name, &mut child, job_object.assign_process_id(pid)).await?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| SupervisorError::ProcessSpawnFailed {
                name: config.name.clone(),
                reason: "Failed to open child stdin".to_string(),
            })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| SupervisorError::ProcessSpawnFailed {
                name: config.name.clone(),
                reason: "Failed to open child stdout".to_string(),
            })?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| SupervisorError::ProcessSpawnFailed {
                name: config.name.clone(),
                reason: "Failed to open child stderr".to_string(),
            })?;

        let (cmd_tx, mut cmd_rx) = mpsc::channel::<String>(128);
        let exit_notify = Arc::new(Notify::new());
        let exited = Arc::new(AtomicBool::new(false));
        let is_ready = Arc::new(AtomicBool::new(false));
        let last_pong_ms = Arc::new(AtomicU64::new(now_millis()));
        let last_event_ms = Arc::new(AtomicU64::new(now_millis()));

        // Stdin writer task
        tokio::spawn(async move {
            while let Some(line) = cmd_rx.recv().await {
                if stdin.write_all(line.as_bytes()).await.is_err() {
                    break;
                }
                if stdin.write_all(b"\n").await.is_err() {
                    break;
                }
                if stdin.flush().await.is_err() {
                    break;
                }
            }
        });

        // Stderr logger task
        let child_name_stderr = config.name.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                tracing::warn!("[{} stderr] {}", child_name_stderr, line);
            }
        });

        // Stdout reader task
        let is_ready_clone = is_ready.clone();
        let last_pong_clone = last_pong_ms.clone();
        let last_event_clone = last_event_ms.clone();
        let ready_notify = Arc::new(Notify::new());
        let ready_notify_clone = ready_notify.clone();
        let event_bus_stdout = event_bus.clone();
        let child_name_stdout = config.name.clone();
        let worker_event_tx_stdout = worker_event_tx.clone();
        let decode_timeout = config.decode_timeout;
        let decode_kill = kill_notify.clone();

        tokio::spawn(async move {
            // The worker answers pings from its reader thread while decoding, so the
            // ping watchdog cannot see a hung decode; the pong state can.
            let mut decoding: Option<(String, Instant)> = None;
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                last_event_clone.store(now_millis(), Ordering::SeqCst);
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                    let _ = worker_event_tx_stdout.send(val.clone());
                    if let Some(event_type) = val.get("event").and_then(|v| v.as_str()) {
                        match event_type {
                            "ready" => {
                                is_ready_clone.store(true, Ordering::SeqCst);
                                last_pong_clone.store(now_millis(), Ordering::SeqCst);
                                ready_notify_clone.notify_one();
                                if let Some(map) = val.as_object() {
                                    event_bus_stdout.emit_ready(
                                        service_pid,
                                        parent_pid,
                                        map.clone(),
                                    );
                                }
                            }
                            "pong" => {
                                last_pong_clone.store(now_millis(), Ordering::SeqCst);
                                if let Some(limit) = decode_timeout {
                                    if decode_deadline_exceeded(&mut decoding, &val, limit) {
                                        tracing::error!(
                                            "ASR decode exceeded its {:?} deadline; treating worker PID {} as failed",
                                            limit,
                                            pid
                                        );
                                        decode_kill.notify_one();
                                    }
                                }
                            }
                            "status" => {
                                if let Some(map) = val.as_object() {
                                    event_bus_stdout.emit_status(
                                        service_pid,
                                        parent_pid,
                                        map.clone(),
                                    );
                                }
                            }
                            "error" => {
                                if let Some(map) = val.as_object() {
                                    event_bus_stdout.emit_error(
                                        service_pid,
                                        parent_pid,
                                        map.clone(),
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                } else {
                    tracing::debug!("[{} stdout] {}", child_name_stdout, line);
                }
            }
        });

        // Process wait & kill runner task
        let kill_notify_wait = kill_notify.clone();
        let exit_notify_wait = exit_notify.clone();
        let exited_wait = exited.clone();
        tokio::spawn(async move {
            tokio::select! {
                status = child.wait() => {
                    tracing::info!("Child process PID {} exited with {:?}", pid, status);
                }
                _ = kill_notify_wait.notified() => {
                    tracing::warn!("Force killing child process PID {}", pid);
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                }
            }
            exited_wait.store(true, Ordering::SeqCst);
            exit_notify_wait.notify_one();
        });

        // Send init command with configured or default payload
        let init_payload = config.init_payload.clone().unwrap_or_else(|| {
            serde_json::json!({
                "model_name": "small",
                "device": "auto",
                "language": "es",
                "auto_cpu_fallback": true
            })
        });
        let init_cmd = serde_json::json!({
            "version": 1,
            "cmd": "init",
            "attempt_id": attempt_id,
            "timestamp_ms": now_millis(),
            "payload": init_payload,
        });
        let _ = cmd_tx.send(init_cmd.to_string()).await;

        // Await the ready handshake. The stall window restarts on every worker output line,
        // so a slow first-run download is not killed; silence for the whole window fails it.
        let stall_window = config.ready_timeout;
        let handshake = async {
            if is_ready.load(Ordering::SeqCst) {
                return Ok(());
            }
            tokio::select! {
                _ = ready_notify.notified() => Ok(()),
                _ = exit_notify.notified() => Err(SupervisorError::ProcessSpawnFailed {
                    name: config.name.clone(),
                    reason: "Child exited prematurely before sending ready event".to_string(),
                }),
                _ = async { if let Some(token) = cancellation { token.cancelled().await } } => Err(SupervisorError::Cancelled),
                _ = wait_for_startup_stall(&last_event_ms, stall_window) => Err(SupervisorError::ProcessSpawnFailed {
                    name: config.name.clone(),
                    reason: format!(
                        "No startup progress for {:?} while waiting for ready handshake",
                        stall_window
                    ),
                }),
            }
        };
        match handshake.await {
            Ok(()) => {
                startup_guard.disarm();
                let mut sm = state_machine.lock().await;
                sm.transition_to(HealthState::Ready, Some("Worker ready"));
            }
            Err(e) => {
                // Kill before waiting: the wait task only reaps once it is told to kill.
                kill_notify.notify_one();
                if matches!(&e, SupervisorError::Cancelled) {
                    wait_for_child_exit(&exited, &exit_notify).await;
                }
                return Err(e);
            }
        }

        // Start watchdog task
        let watchdog_kill = kill_notify.clone();
        let watchdog_cmd_tx = cmd_tx.clone();
        let watchdog_last_pong = last_pong_ms.clone();
        let watchdog_interval = config.watchdog_interval;
        let watchdog_timeout = config.watchdog_timeout;
        let is_ready_watchdog = is_ready.clone();

        let watchdog_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(watchdog_interval);
            // Skip first tick
            interval.tick().await;

            loop {
                interval.tick().await;
                if !is_ready_watchdog.load(Ordering::SeqCst) {
                    continue;
                }

                let elapsed_ms =
                    now_millis().saturating_sub(watchdog_last_pong.load(Ordering::SeqCst));
                if elapsed_ms > watchdog_timeout.as_millis() as u64 {
                    tracing::error!(
                        "Watchdog timeout: child PID {} unresponsive for {}ms (limit {:?})",
                        pid,
                        elapsed_ms,
                        watchdog_timeout
                    );
                    watchdog_kill.notify_waiters();
                    break;
                }

                let ping = serde_json::json!({
                    "version": 1,
                    "cmd": "ping",
                    "timestamp_ms": now_millis(),
                });
                if watchdog_cmd_tx.send(ping.to_string()).await.is_err() {
                    break;
                }
            }
        });

        Ok(ActiveChild {
            pid,
            cmd_tx,
            kill_notify,
            exit_notify,
            exited,
            is_ready,
            _last_pong_ms: last_pong_ms,
            watchdog_handle: Some(watchdog_handle),
        })
    }

    /// Spawns and supervises the child process, handling auto-respawn and backoff.
    pub async fn start_internal(&mut self) -> Result<(), SupervisorError> {
        self.start_internal_cancellable(CancellationToken::new())
            .await
    }

    /// Starts the worker with a caller-owned cancellation token for the ready handshake.
    pub async fn start_internal_cancellable(
        &mut self,
        cancellation: CancellationToken,
    ) -> Result<(), SupervisorError> {
        if self.active_child.lock().await.is_some() {
            return Ok(());
        }

        self.is_shutting_down.store(false, Ordering::SeqCst);
        self.shutdown_complete.store(false, Ordering::SeqCst);

        // Initial launch
        let child = Self::spawn_and_handshake(
            &self.config,
            &self.job_object,
            &self.event_bus,
            &self.state_machine,
            self.service_pid,
            self.parent_pid,
            &self.worker_event_tx,
            self.worker_attempt.fetch_add(1, Ordering::SeqCst) + 1,
            Some(&cancellation),
        )
        .await?;

        let exit_notify = child.exit_notify.clone();
        *self.active_child.lock().await = Some(child);

        // If auto-respawn is configured, start the supervisor background monitor
        if self.config.auto_respawn {
            let config = self.config.clone();
            let job_object = self.job_object.clone();
            let event_bus = self.event_bus.clone();
            let state_machine = self.state_machine.clone();
            let respawn_tracker = self.respawn_tracker.clone();
            let active_child = self.active_child.clone();
            let restart_count = self.restart_count.clone();
            let is_shutting_down = self.is_shutting_down.clone();
            let cancel_token = self.cancellation_token.clone();
            let service_pid = self.service_pid;
            let parent_pid = self.parent_pid;
            let worker_event_tx = self.worker_event_tx.clone();
            let worker_attempt = self.worker_attempt.clone();

            let handle = tokio::spawn(async move {
                let mut current_exit_notify = exit_notify;

                'supervise: loop {
                    tokio::select! {
                        _ = cancel_token.cancelled() => {
                            break;
                        }
                        _ = current_exit_notify.notified() => {
                            if is_shutting_down.load(Ordering::SeqCst) {
                                break;
                            }

                            // Unexpected crash or watchdog termination
                            tracing::warn!("Supervised worker exited unexpectedly. Evaluating respawn policy...");
                            // The dead worker must not stay registered as the active child.
                            *active_child.lock().await = None;

                            // Each failed respawn handshake counts as another failure, so the
                            // loop retries with backoff until a worker is ready or the policy
                            // ceiling escalates to Failed.
                            loop {
                                let backoff_res = {
                                    let mut tracker = respawn_tracker.lock().await;
                                    tracker.record_failure()
                                };

                                let backoff = match backoff_res {
                                    Ok(d) => d,
                                    Err(err) => {
                                        tracing::error!("Respawn policy threshold exceeded: {}", err);
                                        let mut sm = state_machine.lock().await;
                                        sm.transition_to(HealthState::Failed, Some("Max crash limit reached"));
                                        break 'supervise;
                                    }
                                };

                                {
                                    let mut sm = state_machine.lock().await;
                                    sm.record_failure();
                                    sm.transition_to(
                                        HealthState::Degraded,
                                        Some(&format!("Worker crashed, backoff {:?}", backoff)),
                                    );
                                }

                                tracing::info!("Backing off for {:?} before respawning worker...", backoff);
                                tokio::select! {
                                    _ = cancel_token.cancelled() => break 'supervise,
                                    _ = tokio::time::sleep(backoff) => {}
                                }

                                if is_shutting_down.load(Ordering::SeqCst) {
                                    break 'supervise;
                                }

                                // Respawn
                                match Self::spawn_and_handshake(
                                    &config,
                                    &job_object,
                                    &event_bus,
                                    &state_machine,
                                    service_pid,
                                    parent_pid,
                                    &worker_event_tx,
                                    worker_attempt.fetch_add(1, Ordering::SeqCst) + 1,
                                    Some(&cancel_token),
                                ).await {
                                    Ok(new_child) => {
                                        current_exit_notify = new_child.exit_notify.clone();
                                        restart_count.fetch_add(1, Ordering::SeqCst);
                                        *active_child.lock().await = Some(new_child);
                                        // Stop may have cancelled after its take: reap what we stored.
                                        if cancel_token.is_cancelled() {
                                            let taken = active_child.lock().await.take();
                                            if let Some(child) = taken {
                                                child.kill_notify.notify_one();
                                                wait_for_child_exit(&child.exited, &child.exit_notify).await;
                                            }
                                            break 'supervise;
                                        }
                                        tracing::info!("Worker respawned successfully");
                                        break;
                                    }
                                    Err(e) => {
                                        if cancel_token.is_cancelled() {
                                            break 'supervise;
                                        }
                                        tracing::error!("Failed to respawn worker: {}", e);
                                    }
                                }
                            }
                        }
                    }
                }
            });

            *self.loop_slot() = Some(handle);
        }

        Ok(())
    }

    /// Cleanly terminate the child process with bounded grace period. Idempotent.
    pub async fn stop_internal(&self, timeout: Duration) -> Result<(), SupervisorError> {
        // Idempotent check
        if self.shutdown_complete.load(Ordering::SeqCst) {
            return Ok(());
        }

        self.is_shutting_down.store(true, Ordering::SeqCst);
        self.cancellation_token.cancel();

        // Cancellation reaches a respawn handshake, which reaps its worker before the loop
        // returns. Join the loop so the worker is gone when stop returns; abort only if the
        // join overruns the deadline (the dropped startup guard still kills its worker).
        let loop_handle = self.loop_slot().take();
        if let Some(mut handle) = loop_handle {
            if tokio::time::timeout(timeout, &mut handle).await.is_err() {
                handle.abort();
            }
        }

        let child_opt = self.active_child.lock().await.take();
        if let Some(mut child) = child_opt {
            if let Some(wd) = child.watchdog_handle.take() {
                wd.abort();
            }

            // The graceful deadline covers both queue admission and worker exit.
            let shutdown_cmd = serde_json::json!({
                "version": 1,
                "cmd": "shutdown",
                "payload": {
                    "grace_timeout_ms": timeout.as_millis() as u64
                }
            });
            let deadline = tokio::time::Instant::now() + timeout;
            let graceful = tokio::time::timeout_at(deadline, async {
                let _ = child.cmd_tx.send(shutdown_cmd.to_string()).await;
                wait_for_child_exit(&child.exited, &child.exit_notify).await;
            })
            .await;
            drop(child.cmd_tx);
            if graceful.is_err() {
                tracing::warn!("Child did not stop within grace period; issuing force kill");
                child.kill_notify.notify_one();
                tokio::time::timeout(
                    Duration::from_millis(500),
                    wait_for_child_exit(&child.exited, &child.exit_notify),
                )
                .await
                .map_err(|_| SupervisorError::ShutdownTimeout)?;
            }
        }

        self.shutdown_complete.store(true, Ordering::SeqCst);
        let mut sm = self.state_machine.lock().await;
        sm.transition_to(HealthState::Stopped, Some("Supervisor stopped"));

        Ok(())
    }

    /// Runs until the cancellation token is cancelled, then executes clean shutdown.
    pub async fn run_until_cancelled(
        &mut self,
        token: CancellationToken,
    ) -> Result<(), SupervisorError> {
        self.start_internal().await?;
        tokio::select! {
            _ = token.cancelled() => {}
            _ = self.cancellation_token.cancelled() => {}
        }
        self.stop_internal(self.config.grace_shutdown_timeout).await
    }
}

impl ServiceSupervisor for ProcessSupervisor {
    fn start(&mut self) -> SupervisorBoxFuture<'_, Result<(), SupervisorError>> {
        Box::pin(self.start_internal())
    }

    fn poll_health(&mut self) -> SupervisorHealthReport {
        let failure_count = {
            if let Ok(mut tracker) = self.respawn_tracker.try_lock() {
                tracker.failure_count()
            } else {
                0
            }
        };

        let (pid, is_alive) = {
            if let Ok(guard) = self.active_child.try_lock() {
                if let Some(ref c) = *guard {
                    (Some(c.pid), c.is_ready.load(Ordering::SeqCst))
                } else {
                    (None, false)
                }
            } else {
                (None, false)
            }
        };

        let mut children = HashMap::new();
        children.insert(
            self.config.name.clone(),
            ChildStatus {
                name: self.config.name.clone(),
                pid,
                is_alive,
                restart_count: self.restart_count.load(Ordering::SeqCst),
                last_heartbeat_timestamp: now_millis() / 1000,
            },
        );

        let overall_state = if self.shutdown_complete.load(Ordering::SeqCst) {
            "stopped".to_string()
        } else if is_alive {
            "healthy".to_string()
        } else {
            "degraded".to_string()
        };

        SupervisorHealthReport {
            service_pid: self.service_pid,
            parent_pid: self.parent_pid,
            overall_state,
            children,
            failure_count,
            uptime_sec: self.start_time.elapsed().as_secs_f64(),
        }
    }

    fn stop(&mut self, timeout: Duration) -> SupervisorBoxFuture<'_, Result<(), SupervisorError>> {
        Box::pin(self.stop_internal(timeout))
    }

    fn restart_child(
        &mut self,
        _child_name: &str,
    ) -> SupervisorBoxFuture<'_, Result<(), SupervisorError>> {
        Box::pin(async move {
            let child_opt = self.active_child.lock().await.take();
            if let Some(child) = child_opt {
                child.kill_notify.notify_waiters();
                let _ =
                    tokio::time::timeout(Duration::from_millis(500), child.exit_notify.notified())
                        .await;
            }

            let new_child = Self::spawn_and_handshake(
                &self.config,
                &self.job_object,
                &self.event_bus,
                &self.state_machine,
                self.service_pid,
                self.parent_pid,
                &self.worker_event_tx,
                self.worker_attempt.fetch_add(1, Ordering::SeqCst) + 1,
                None,
            )
            .await?;

            self.restart_count.fetch_add(1, Ordering::SeqCst);
            *self.active_child.lock().await = Some(new_child);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn windows_worker_processes_use_create_no_window() {
        assert_eq!(crate::runtime::windows_no_console_flag(), 0x0800_0000);
    }

    #[test]
    fn test_worker_process_config_discovery() {
        let config = WorkerProcessConfig::discover_asr_worker();
        assert_eq!(config.name, "asr-worker");
        assert!(!config.command.is_empty());
        assert!(!config.args.is_empty());
        assert!(config.envs.contains_key("PYTHONUNBUFFERED"));
        assert_eq!(config.envs.get("PYTHONUNBUFFERED"), Some(&"1".to_string()));
        assert_eq!(config.envs.get("PYTHONUTF8"), Some(&"1".to_string()));
        assert!(config.envs.contains_key("HF_HOME"));
        assert!(config.envs.contains_key("TORCH_HOME"));
    }

    #[test]
    fn test_worker_process_config_env_override() {
        let temp_dir = std::env::temp_dir();
        let fake_py = temp_dir.join("fake_python_exe.exe");
        let fake_worker = temp_dir.join("fake_asr_worker.py");
        let _ = std::fs::write(&fake_py, b"fake");
        let _ = std::fs::write(&fake_worker, b"fake");

        std::env::set_var("LIVEAUDIO_PYTHON", &fake_py);
        std::env::set_var("LIVEAUDIO_WORKER_PATH", &fake_worker);

        let discovered_py = WorkerProcessConfig::find_python_executable();
        let discovered_worker = WorkerProcessConfig::find_asr_worker_script();

        assert_eq!(discovered_py, fake_py);
        assert!(discovered_worker.is_some());

        std::env::remove_var("LIVEAUDIO_PYTHON");
        std::env::remove_var("LIVEAUDIO_WORKER_PATH");

        let _ = std::fs::remove_file(&fake_py);
        let _ = std::fs::remove_file(&fake_worker);
    }
}

#[cfg(test)]
mod python_discovery_order_tests {
    use super::{bundled_python_candidates, python_on_path};
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};

    #[test]
    fn bundled_candidates_stay_under_the_exe_directory_and_its_venv_ancestors() {
        let exe_dir = if cfg!(windows) {
            PathBuf::from(r"C:\Program Files\LiveAudio")
        } else {
            PathBuf::from("/opt/liveaudio")
        };
        let candidates = bundled_python_candidates(&exe_dir);

        assert!(
            candidates.iter().all(|path| path.is_absolute()),
            "cwd-relative candidates must not be trusted: {candidates:?}"
        );
        let venv = if cfg!(windows) {
            Path::new(".venv/Scripts/python.exe")
        } else {
            Path::new(".venv/bin/python")
        };
        assert!(candidates.contains(&exe_dir.join(venv)));
        assert!(candidates.contains(&exe_dir.join("..").join(venv)));
        let bundled = if cfg!(windows) {
            exe_dir.join("python.exe")
        } else {
            exe_dir.join("bin/python3")
        };
        assert_eq!(candidates.first(), Some(&bundled));
    }

    #[test]
    fn path_lookup_returns_the_python_in_an_absolute_directory() {
        let root = std::env::temp_dir().join(format!("liveaudio-py-path-{}", std::process::id()));
        let absolute_dir = root.join("bin");
        std::fs::create_dir_all(&absolute_dir).unwrap();
        let name = if cfg!(windows) {
            "python.exe"
        } else {
            "python"
        };
        std::fs::write(absolute_dir.join(name), "real").unwrap();

        let path = std::env::join_paths([absolute_dir.clone()]).unwrap();
        let found = python_on_path(&path);
        std::fs::remove_dir_all(&root).ok();

        assert_eq!(found, Some(absolute_dir.join(name)));
        assert_eq!(python_on_path(OsStr::new("")), None);
    }
}
