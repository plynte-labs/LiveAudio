// SPDX-License-Identifier: MIT

use liveaudio_core::supervisor::{
    is_parent_alive, ProcessSupervisor, ServiceSupervisor, WorkerProcessConfig,
};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static TEST_COUNTER: AtomicUsize = AtomicUsize::new(100);

fn create_mock_worker_script() -> (PathBuf, PathBuf) {
    let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let tmp_dir = std::env::temp_dir().join(format!(
        "liveaudio-bench-worker-{}-{}",
        std::process::id(),
        id
    ));
    let _ = std::fs::create_dir_all(&tmp_dir);
    let script_path = tmp_dir.join("mock_worker.py");

    let script_content = r#"
import sys, json

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

    let mut file = File::create(&script_path).expect("failed to create script");
    file.write_all(script_content.as_bytes())
        .expect("write script");
    file.flush().expect("flush script");

    (tmp_dir, script_path)
}

#[tokio::test]
async fn bench_job_object_shutdown_latency() {
    let (tmp_dir, script_path) = create_mock_worker_script();

    let runs = 5;
    let mut teardown_times_ms = Vec::new();

    for i in 0..runs {
        let mut config = WorkerProcessConfig::default();
        config.name = format!("bench-worker-{}", i);
        config.command = "python".to_string();
        config.args = vec!["-u".to_string(), script_path.to_str().unwrap().to_string()];
        config.ready_timeout = Duration::from_secs(5);
        config.watchdog_interval = Duration::from_millis(500);
        config.watchdog_timeout = Duration::from_millis(1500);

        let mut supervisor = ProcessSupervisor::new(config, std::process::id(), 0, None)
            .expect("supervisor creation");

        supervisor.start().await.expect("start worker");
        let pid = supervisor.child_pid().await.expect("pid");
        assert!(is_parent_alive(pid), "worker must be alive");

        // Measure shutdown teardown
        let t0 = Instant::now();
        supervisor
            .stop(Duration::from_millis(1500))
            .await
            .expect("stop supervisor");
        let elapsed = t0.elapsed();
        let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
        teardown_times_ms.push(elapsed_ms);

        // Verify zero orphans
        assert!(!supervisor.is_running().await);
        assert!(
            !is_parent_alive(pid),
            "Process must be terminated (0 orphans)"
        );
    }

    let _ = std::fs::remove_dir_all(&tmp_dir);

    teardown_times_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean_ms = teardown_times_ms.iter().sum::<f64>() / (runs as f64);
    let min_ms = teardown_times_ms[0];
    let max_ms = teardown_times_ms[runs - 1];
    let p50_ms = teardown_times_ms[runs / 2];

    println!("\n=== RUST CORE SUPERVISOR & JOB OBJECT SHUTDOWN BENCHMARK ===");
    println!("Runs: {}", runs);
    println!("Mean Teardown Latency: {:.2} ms", mean_ms);
    println!("P50 Teardown Latency:  {:.2} ms", p50_ms);
    println!("Min Teardown Latency:  {:.2} ms", min_ms);
    println!("Max Teardown Latency:  {:.2} ms", max_ms);
    println!("Orphan Processes:      0 (100% terminated via Windows Job Object limit)");
    println!("Target SLA:            < 150 ms");
    println!("SLA Pass:              {}", mean_ms < 150.0);
    println!("============================================================\n");

    assert!(
        mean_ms < 150.0,
        "Teardown must be under 150ms, got {:.2}ms",
        mean_ms
    );
}
