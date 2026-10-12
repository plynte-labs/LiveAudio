use liveaudio_core::runtime::{
    discover_managed_python, discover_managed_worker, discover_uv_in, managed_python,
    preflight_asr_runtime, provision_runtime_with_uv, provision_runtime_with_uv_cancellable,
    AsrBackend, RuntimeSetupProgress,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn test_root(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "liveaudio-runtime-{label}-{}-{unique}",
        std::process::id()
    ))
}

#[test]
fn backend_must_be_an_explicit_supported_choice() {
    assert_eq!("cpu".parse::<AsrBackend>().unwrap(), AsrBackend::Cpu);
    assert_eq!("cu121".parse::<AsrBackend>().unwrap(), AsrBackend::Cu121);
    assert!("auto".parse::<AsrBackend>().is_err());
    assert!("".parse::<AsrBackend>().is_err());
}

#[test]
fn bundled_uv_is_selected_before_path_uv() {
    let root = test_root("uv-discovery");
    let executable = if cfg!(windows) { "uv.exe" } else { "uv" };
    let bundled = root.join("packaging/vendor").join(executable);
    let path_uv = root.join("path").join(executable);
    fs::create_dir_all(bundled.parent().unwrap()).unwrap();
    fs::create_dir_all(path_uv.parent().unwrap()).unwrap();
    fs::write(&bundled, "fake bundled uv").unwrap();
    fs::write(&path_uv, "fake path uv").unwrap();

    let found = discover_uv_in(&[root.clone()], &[path_uv.clone()]).unwrap();
    assert_eq!(found, bundled);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn provision_materializes_manifests_package_and_invokes_locked_uv_for_backend() {
    let root = test_root("provision");
    let package_source = root.join("source");
    let project_root = root.join("managed");
    let fake_uv = root.join("uv-test-command.cmd");
    fs::create_dir_all(package_source.join("liveaudio/service")).unwrap();
    fs::write(package_source.join("liveaudio/__init__.py"), "# fixture\n").unwrap();
    fs::write(
        package_source.join("liveaudio/service/asr_worker.py"),
        "# worker\n",
    )
    .unwrap();
    create_fake_uv(&fake_uv, &root);

    for backend in [AsrBackend::Cpu, AsrBackend::Cu121] {
        provision_runtime_with_uv(&project_root, &package_source, backend, &fake_uv).unwrap();
        let args = fs::read_to_string(root.join("uv-args.txt")).unwrap();
        let extra = match backend {
            AsrBackend::Cpu => "cpu",
            AsrBackend::Cu121 => "cu121",
        };
        let expected = format!(
            "sync --project {} --locked --no-install-project --no-dev --extra {} --python 3.11",
            project_root.display(),
            extra
        );
        assert_eq!(args.trim(), expected);
    }

    assert!(fs::read_to_string(project_root.join("pyproject.toml"))
        .unwrap()
        .contains("faster-whisper"));
    assert!(fs::read_to_string(project_root.join("uv.lock"))
        .unwrap()
        .contains("version"));
    assert!(project_root
        .join("liveaudio/service/asr_worker.py")
        .is_file());
    assert!(project_root.join(".venv").is_dir());
    let python = managed_python(&project_root);
    fs::create_dir_all(python.parent().unwrap()).unwrap();
    fs::write(&python, "fake interpreter").unwrap();
    assert_eq!(
        discover_managed_python(&project_root).unwrap(),
        fs::canonicalize(&python).unwrap()
    );
    assert_eq!(
        discover_managed_worker(&project_root).unwrap(),
        fs::canonicalize(project_root.join("liveaudio/service/asr_worker.py")).unwrap()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn provisioning_can_refresh_an_existing_managed_worker_tree() {
    let root = test_root("refresh");
    let project_root = root.join("managed");
    let fake_uv = create_fake_uv_path(&root);
    fs::create_dir_all(project_root.join("liveaudio/service")).unwrap();
    fs::write(
        project_root.join("liveaudio/__init__.py"),
        "# prior package\n",
    )
    .unwrap();
    fs::write(
        project_root.join("liveaudio/service/asr_worker.py"),
        "# prior worker\n",
    )
    .unwrap();

    let result = provision_runtime_with_uv(&project_root, &project_root, AsrBackend::Cpu, &fake_uv);
    assert!(
        result.is_ok(),
        "refreshing the managed tree failed: {result:?}"
    );
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn provisioning_refresh_ignores_stale_previous_backup_and_preserves_its_contents() {
    use tokio_util::sync::CancellationToken;

    let root = test_root("stale-previous-backup");
    let managed_root = root.join("managed");
    let stale_backup = managed_root.with_file_name("managed.previous");
    let package_source = root.join("source");
    fs::create_dir_all(package_source.join("liveaudio/service")).unwrap();
    fs::write(package_source.join("liveaudio/__init__.py"), "# fixture\n").unwrap();
    fs::write(
        package_source.join("liveaudio/service/asr_worker.py"),
        "# worker\n",
    )
    .unwrap();
    fs::create_dir_all(managed_root.join("liveaudio/service")).unwrap();
    fs::write(
        managed_root.join("liveaudio/__init__.py"),
        "# prior package\n",
    )
    .unwrap();
    fs::write(
        managed_root.join("liveaudio/service/asr_worker.py"),
        "# prior worker\n",
    )
    .unwrap();
    fs::create_dir_all(&stale_backup).unwrap();
    fs::write(stale_backup.join("sentinel.txt"), "preserve-stale-backup").unwrap();
    let fake_uv = root.join("uv-success.cmd");
    write_fake_uv_for_project(&fake_uv, &root);

    provision_runtime_with_uv_cancellable(
        &managed_root,
        &package_source,
        AsrBackend::Cpu,
        &fake_uv,
        CancellationToken::new(),
        |_| {},
    )
    .await
    .expect("repair should publish despite an unrelated stale backup");

    assert!(managed_root
        .join("liveaudio/service/asr_worker.py")
        .exists());
    assert!(managed_root.join(".venv").is_dir());
    assert_eq!(
        fs::read_to_string(stale_backup.join("sentinel.txt")).unwrap(),
        "preserve-stale-backup"
    );
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn missing_asr_imports_return_actionable_setup_guidance_without_setup() {
    let root = test_root("preflight");
    fs::create_dir_all(&root).unwrap();
    let python = create_fake_python(&root);

    let error = preflight_asr_runtime(&python, Some(&root))
        .await
        .expect_err("missing dependencies must block service startup");
    let message = error.to_string();
    assert!(message.contains("faster_whisper"));
    assert!(message.contains("setup-runtime --backend cpu"));
    assert!(!root.join("uv-called").exists());
    assert!(!root.join("pyproject.toml").exists());
    assert!(!root.join(".venv").exists());
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn cancellable_provisioning_publishes_only_a_complete_runtime() {
    use tokio_util::sync::CancellationToken;

    let root = test_root("cancel-preserves-runtime");
    let package_source = root.join("source");
    let project_root = root.join("managed");
    let fake_uv = root.join("uv-success.cmd");
    fs::create_dir_all(package_source.join("liveaudio/service")).unwrap();
    fs::write(package_source.join("liveaudio/__init__.py"), "# fixture\n").unwrap();
    fs::write(
        package_source.join("liveaudio/service/asr_worker.py"),
        "# worker\n",
    )
    .unwrap();
    fs::create_dir_all(project_root.join("liveaudio/service")).unwrap();
    fs::write(project_root.join("existing-runtime.marker"), "preserve me").unwrap();
    write_fake_uv_for_project(&fake_uv, &root);

    let progress = std::sync::Arc::new(std::sync::Mutex::new(Vec::<RuntimeSetupProgress>::new()));
    let progress_copy = progress.clone();
    provision_runtime_with_uv_cancellable(
        &project_root,
        &package_source,
        AsrBackend::Cpu,
        &fake_uv,
        CancellationToken::new(),
        move |event| progress_copy.lock().unwrap().push(event),
    )
    .await
    .unwrap();

    assert!(project_root.join(".venv").is_dir());
    assert!(project_root.join("pyproject.toml").is_file());
    assert!(project_root
        .join("liveaudio/service/asr_worker.py")
        .is_file());
    assert!(progress
        .lock()
        .unwrap()
        .iter()
        .any(|event| event.phase == "sync"));
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn cancelling_uv_sync_reaps_child_and_preserves_previous_runtime() {
    use tokio_util::sync::CancellationToken;

    let root = test_root("cancel-running-uv");
    let package_source = root.join("source");
    let project_root = root.join("managed");
    let fake_uv = root.join("uv-hang.cmd");
    let started_marker = root.join("uv-started");
    fs::create_dir_all(package_source.join("liveaudio/service")).unwrap();
    fs::write(package_source.join("liveaudio/__init__.py"), "# fixture\n").unwrap();
    fs::write(
        package_source.join("liveaudio/service/asr_worker.py"),
        "# worker\n",
    )
    .unwrap();
    fs::create_dir_all(project_root.join(".venv")).unwrap();
    fs::write(project_root.join("existing-runtime.marker"), "preserve me").unwrap();
    fs::write(
        &fake_uv,
        format!(
            "@echo off\r\necho started > \"{}\"\r\n:wait\r\ngoto wait\r\n",
            started_marker.display()
        ),
    )
    .unwrap();

    let cancellation = CancellationToken::new();
    let cancel_task = cancellation.clone();
    let runtime_root = project_root.clone();
    let source_root = package_source.clone();
    let uv_path = fake_uv.clone();
    let setup = tokio::spawn(async move {
        provision_runtime_with_uv_cancellable(
            &runtime_root,
            &source_root,
            AsrBackend::Cpu,
            &uv_path,
            cancel_task,
            |_| {},
        )
        .await
    });

    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !started_marker.is_file() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("fake uv should start promptly");
    cancellation.cancel();
    let error = tokio::time::timeout(std::time::Duration::from_secs(5), setup)
        .await
        .expect("cancelled setup should reap its child")
        .unwrap()
        .expect_err("cancelled setup must not publish a runtime");

    assert!(error.to_string().to_lowercase().contains("cancel"));
    assert_eq!(
        fs::read_to_string(project_root.join("existing-runtime.marker")).unwrap(),
        "preserve me"
    );
    assert!(project_root.join(".venv").is_dir());

    let retry_uv = root.join("uv-retry.cmd");
    write_fake_uv_for_project(&retry_uv, &root);
    provision_runtime_with_uv_cancellable(
        &project_root,
        &package_source,
        AsrBackend::Cpu,
        &retry_uv,
        CancellationToken::new(),
        |_| {},
    )
    .await
    .expect("a later retry should publish a complete runtime");
    assert!(project_root.join(".venv").is_dir());
    assert!(project_root
        .join("liveaudio/service/asr_worker.py")
        .is_file());
    fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
fn write_fake_uv_for_project(path: &Path, _root: &Path) {
    fs::write(
        path,
        "@echo off\r\nif not exist \"%~3\\.venv\" mkdir \"%~3\\.venv\"\r\nexit /b 0\r\n",
    )
    .unwrap();
}

#[cfg(not(windows))]
fn write_fake_uv_for_project(path: &Path, _root: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, "#!/bin/sh\nmkdir -p \"$3/.venv\"\n").unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(windows)]
fn create_fake_uv(path: &Path, root: &Path) {
    let script = format!(
        "@echo off\r\n>\"{}\" echo %*\r\nmkdir \"{}\" 2>nul\r\nexit /b 0\r\n",
        root.join("uv-args.txt").display(),
        root.join("managed/.venv").display(),
    );
    fs::write(path, script).unwrap();
}

fn create_fake_uv_path(root: &Path) -> PathBuf {
    fs::create_dir_all(root).unwrap();
    let fake_uv = root.join(if cfg!(windows) {
        "uv-test-command.cmd"
    } else {
        "uv-test-command"
    });
    create_fake_uv(&fake_uv, root);
    fake_uv
}

#[cfg(not(windows))]
fn create_fake_uv(path: &Path, root: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" > '{}'\nmkdir -p '{}'\n",
        root.join("uv-args.txt").display(),
        root.join("managed/.venv").display(),
    );
    fs::write(path, script).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(windows)]
fn create_fake_python(root: &Path) -> PathBuf {
    let python = root.join("missing-python.cmd");
    fs::write(
        &python,
        "@echo off\r\necho No module named faster_whisper 1>&2\nexit /b 1\r\n",
    )
    .unwrap();
    python
}

#[cfg(not(windows))]
fn create_fake_python(root: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let python = root.join("missing-python");
    fs::write(
        &python,
        "#!/bin/sh\necho \"No module named faster_whisper\" >&2\nexit 1\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&python).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&python, permissions).unwrap();
    python
}
