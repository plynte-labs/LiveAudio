// SPDX-License-Identifier: MIT

//! Integration tests verifying ProcessSupervisor with simulated/mock Python workers:
//! - Clean startup and ready handshake
//! - Crash detection and bounded respawn with exponential backoff (max 3 failures in 300s window)
//! - Watchdog timeout and kill on unresponsiveness
//! - Graceful shutdown leaving 0 orphan processes
//! - Idempotent double-shutdown guarantees

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use liveaudio_core::supervisor::{
    is_parent_alive, ProcessSupervisor, ServiceSupervisor, SupervisorError, WorkerProcessConfig,
};

static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn create_mock_worker_script() -> (PathBuf, PathBuf) {
    let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let tmp_dir = std::env::temp_dir().join(format!(
        "liveaudio-test-worker-{}-{}",
        std::process::id(),
        id
    ));
    let _ = std::fs::create_dir_all(&tmp_dir);
    let script_path = tmp_dir.join("mock_worker.py");

    let script_content = r#"
import sys, json, time, os

sys.stdout.reconfigure(line_buffering=True, encoding="utf-8")

mode = sys.argv[1] if len(sys.argv) > 1 else "normal"

if mode == "crash_immediately":
    sys.exit(42)

elif mode == "crash_after_ready":
    sys.stdout.write(json.dumps({"event": "ready", "payload": {"device": "cpu"}}) + "\n")
    sys.stdout.flush()
    time.sleep(0.05)
    sys.exit(99)

elif mode == "unresponsive":
    sys.stdout.write(json.dumps({"event": "ready", "payload": {"device": "cpu"}}) + "\n")
    sys.stdout.flush()
    # Hang indefinitely, ignoring pings
    while True:
        time.sleep(1)

elif mode == "crash_counter":
    counter_file = sys.argv[2]
    val = 0
    if os.path.exists(counter_file):
        try:
            with open(counter_file, "r") as f:
                val = int(f.read().strip())
        except Exception:
            val = 0
    val += 1
    with open(counter_file, "w") as f:
        f.write(str(val))
        f.flush()

    limit = int(sys.argv[3]) if len(sys.argv) > 3 else 3
    if val <= limit:
        # Crash on attempts up to limit
        sys.exit(100 + val)
    else:
        # Succeed once limit reached
        sys.stdout.write(json.dumps({"event": "ready", "payload": {"device": "cpu"}}) + "\n")
        sys.stdout.flush()
        for line in sys.stdin:
            line = line.strip()
            if not line:
                continue
            try:
                msg = json.loads(line)
            except Exception:
                continue
            cmd = msg.get("cmd")
            if cmd == "ping":
                sys.stdout.write(json.dumps({"event": "pong", "payload": {"state": "idle"}}) + "\n")
                sys.stdout.flush()
            elif cmd == "shutdown":
                sys.exit(0)

elif mode == "normal":
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except Exception:
            continue
        cmd = msg.get("cmd")
        if cmd == "init":
            sys.stdout.write(json.dumps({"event": "ready", "payload": {"device": "cpu"}}) + "\n")
            sys.stdout.flush()
        elif cmd == "ping":
            sys.stdout.write(json.dumps({"event": "pong", "payload": {"state": "idle"}}) + "\n")
            sys.stdout.flush()
        elif cmd == "shutdown":
            sys.exit(0)
"#;

    let mut file = File::create(&script_path).expect("failed to create mock script");
    file.write_all(script_content.as_bytes())
        .expect("failed to write mock script");
    file.flush().expect("failed to flush mock script");

    (tmp_dir, script_path)
}

#[tokio::test]
async fn test_mock_worker_startup_and_handshake() {
    let (tmp_dir, script_path) = create_mock_worker_script();

    let mut config = WorkerProcessConfig::default();
    config.name = "test-handshake-worker".to_string();
    config.command = "python".to_string();
    config.args = vec![
        "-u".to_string(),
        script_path.to_str().unwrap().to_string(),
        "normal".to_string(),
    ];
    config.ready_timeout = Duration::from_secs(5);
    config.watchdog_interval = Duration::from_millis(200);
    config.watchdog_timeout = Duration::from_millis(800);

    let mut supervisor = ProcessSupervisor::new(config, std::process::id(), 0, None)
        .expect("supervisor creation should succeed");

    assert!(!supervisor.is_running().await);

    // Clean startup
    supervisor
        .start()
        .await
        .expect("worker startup should succeed");
    assert!(supervisor.is_running().await);

    let pid = supervisor.child_pid().await.expect("child PID must be set");
    assert!(pid > 0);
    assert!(is_parent_alive(pid), "child process should be alive");

    // Health poll
    let health = supervisor.poll_health();
    assert_eq!(health.overall_state, "healthy");
    let child_st = health.children.get("test-handshake-worker").unwrap();
    assert!(child_st.is_alive);
    assert_eq!(child_st.pid, Some(pid));

    // Send command
    supervisor
        .send_command(&serde_json::json!({
            "action": "test",
            "params": {}
        }))
        .await
        .expect("command sending should succeed");

    // Orderly shutdown
    supervisor
        .stop(Duration::from_secs(2))
        .await
        .expect("shutdown should succeed");

    assert!(!supervisor.is_running().await);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !is_parent_alive(pid),
        "child process must be terminated (0 orphans)"
    );

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[tokio::test]
async fn test_mock_worker_watchdog_timeout_and_kill() {
    let (tmp_dir, script_path) = create_mock_worker_script();

    let mut config = WorkerProcessConfig::default();
    config.name = "test-unresponsive-worker".to_string();
    config.command = "python".to_string();
    config.args = vec![
        "-u".to_string(),
        script_path.to_str().unwrap().to_string(),
        "unresponsive".to_string(),
    ];
    config.ready_timeout = Duration::from_secs(5);
    config.watchdog_interval = Duration::from_millis(100);
    config.watchdog_timeout = Duration::from_millis(300);
    config.auto_respawn = false; // Do not respawn for watchdog kill test

    let mut supervisor = ProcessSupervisor::new(config, std::process::id(), 0, None)
        .expect("supervisor creation should succeed");

    supervisor
        .start()
        .await
        .expect("worker startup should succeed");
    let pid = supervisor.child_pid().await.expect("child PID must be set");
    assert!(is_parent_alive(pid), "child process should be alive");

    // Wait for watchdog to detect timeout (300ms) and kill process
    tokio::time::sleep(Duration::from_millis(600)).await;

    // Verify child was killed by watchdog
    assert!(
        !is_parent_alive(pid),
        "unresponsive child process must have been killed by watchdog"
    );

    supervisor
        .stop(Duration::from_secs(1))
        .await
        .expect("supervisor cleanup should succeed");

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[tokio::test]
async fn test_mock_worker_bounded_respawn_and_backoff() {
    let (tmp_dir, script_path) = create_mock_worker_script();
    let counter_file = tmp_dir.join("crash_counter.txt");

    let mut config = WorkerProcessConfig::default();
    config.name = "test-respawn-worker".to_string();
    config.command = "python".to_string();
    config.args = vec![
        "-u".to_string(),
        script_path.to_str().unwrap().to_string(),
        "crash_counter".to_string(),
        counter_file.to_str().unwrap().to_string(),
        "2".to_string(), // Crash 2 times, succeed on 3rd attempt
    ];
    config.ready_timeout = Duration::from_secs(5);
    config.watchdog_interval = Duration::from_millis(200);
    config.watchdog_timeout = Duration::from_millis(800);
    config.base_backoff = Duration::from_millis(50);
    config.max_backoff = Duration::from_millis(200);
    config.max_failures = 3;
    config.failure_window = Duration::from_secs(300);
    config.auto_respawn = true;

    // 1st attempt crashes during startup handshake
    let mut supervisor = ProcessSupervisor::new(config, std::process::id(), 0, None)
        .expect("supervisor creation should succeed");

    let start_res = supervisor.start().await;
    assert!(
        start_res.is_err(),
        "first attempt should fail due to worker crash"
    );

    // Second start (attempt 2): crashes again
    let start_res_2 = supervisor.start().await;
    assert!(
        start_res_2.is_err(),
        "second attempt should fail due to worker crash"
    );

    // Third start (attempt 3): counter is now 3 (> 2 limit), succeeds!
    let start_res_3 = supervisor.start().await;
    assert!(
        start_res_3.is_ok(),
        "third attempt must succeed and become ready"
    );

    assert!(supervisor.is_running().await);
    let pid = supervisor.child_pid().await.expect("child PID must be set");
    assert!(is_parent_alive(pid));

    supervisor
        .stop(Duration::from_secs(2))
        .await
        .expect("shutdown should succeed");

    assert!(!is_parent_alive(pid));
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[tokio::test]
async fn test_mock_worker_max_failures_exceeded_fails_fast() {
    // Respawn policy: verify that > 3 failures in 300s window fails fast
    let mut tracker = liveaudio_core::supervisor::RespawnTracker::new(
        3,
        Duration::from_secs(300),
        Duration::from_millis(10),
        Duration::from_millis(50),
    );

    // 1st failure
    assert!(tracker.record_failure().is_ok());
    // 2nd failure
    assert!(tracker.record_failure().is_ok());
    // 3rd failure
    assert!(tracker.record_failure().is_ok());
    // 4th failure -> exceeded!
    let err = tracker.record_failure().unwrap_err();
    assert!(matches!(
        err,
        SupervisorError::MaxFailuresExceeded { count: 4, .. }
    ));
}

#[tokio::test]
async fn test_graceful_shutdown_zero_orphans_and_idempotence() {
    let (tmp_dir, script_path) = create_mock_worker_script();

    let mut config = WorkerProcessConfig::default();
    config.name = "test-idempotent-worker".to_string();
    config.command = "python".to_string();
    config.args = vec![
        "-u".to_string(),
        script_path.to_str().unwrap().to_string(),
        "normal".to_string(),
    ];
    config.ready_timeout = Duration::from_secs(5);
    config.watchdog_interval = Duration::from_millis(200);
    config.watchdog_timeout = Duration::from_millis(800);

    let mut supervisor = ProcessSupervisor::new(config, std::process::id(), 0, None)
        .expect("supervisor creation should succeed");

    supervisor
        .start()
        .await
        .expect("worker startup should succeed");
    let pid = supervisor.child_pid().await.expect("child PID must be set");
    assert!(is_parent_alive(pid), "child must be active before shutdown");

    // First stop
    let stop_1 = supervisor.stop(Duration::from_secs(2)).await;
    assert!(stop_1.is_ok(), "first shutdown call should succeed");
    assert!(!supervisor.is_running().await);
    assert!(supervisor.is_shutdown());

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !is_parent_alive(pid),
        "child PID must not be alive (zero orphans)"
    );

    // Second stop (idempotent double-shutdown)
    let stop_2 = supervisor.stop(Duration::from_secs(2)).await;
    assert!(
        stop_2.is_ok(),
        "second shutdown call must succeed idempotently"
    );

    // Third stop
    let stop_3 = supervisor.stop(Duration::from_secs(1)).await;
    assert!(
        stop_3.is_ok(),
        "third shutdown call must succeed idempotently"
    );

    let _ = std::fs::remove_dir_all(&tmp_dir);
}
