// SPDX-License-Identifier: MIT

//! Explicit provisioning and readiness checks for the hybrid Python ASR worker.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::str::FromStr;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};

const MANAGED_RUNTIME_DIR: &str = "asr-runtime";
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
const PYPROJECT_TOML: &str = include_str!("../../../pyproject.toml");
const UV_LOCK: &str = include_str!("../../../uv.lock");
const IMPORT_PROBE: &str = "import faster_whisper, numpy, sounddevice, websockets, torch; import liveaudio.service.asr_worker";

#[cfg(test)]
mod probe_lifecycle_tests {
    use super::*;
    use std::time::SystemTime;

    #[tokio::test]
    async fn timed_out_import_probe_kills_and_reaps_its_child() {
        let unique = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "liveaudio-probe-test-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let pid_file = root.join("probe.pid");
        let script =
            "import os,sys,time; open(sys.argv[1], 'w').write(str(os.getpid())); time.sleep(60)";
        let args = ["-c", script, pid_file.to_str().unwrap()];
        let mut command = command_for(Path::new("python"), &args);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = command.spawn().expect("spawn local slow probe");
        tokio::time::timeout(Duration::from_secs(5), async {
            while !pid_file.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("probe writes its pid before timeout");
        assert!(child.try_wait().expect("probe status").is_none());

        assert!(
            wait_for_probe(&mut child, Duration::from_millis(100))
                .await
                .is_err(),
            "slow import probe must time out"
        );
        assert!(
            child.try_wait().expect("reaped probe status").is_some(),
            "timeout must kill and reap the actual probe child"
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsrBackend {
    Cpu,
    Cu121,
}

impl AsrBackend {
    fn extra(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Cu121 => "cu121",
        }
    }
}

impl FromStr for AsrBackend {
    type Err = RuntimeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "cpu" => Ok(Self::Cpu),
            "cu121" => Ok(Self::Cu121),
            _ => Err(RuntimeError::UnsupportedBackend(value.to_string())),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("unsupported ASR backend '{0}'; choose cpu or cu121 explicitly")]
    UnsupportedBackend(String),
    #[error("uv was not found; install/bundle uv or put uv.exe on PATH")]
    UvNotFound,
    #[error("ASR worker package source was not found; include the liveaudio package with the app")]
    PackageSourceMissing,
    #[error("could not prepare managed ASR runtime: {0}")]
    Io(#[from] std::io::Error),
    #[error("uv sync failed with exit status {status}: {details}")]
    UvFailed { status: String, details: String },
    #[error("ASR dependency preflight exceeded {0} seconds")]
    ProbeTimedOut(u64),
    #[error("ASR runtime is not ready ({details}). Run `liveaudio setup-runtime --backend cpu` or choose `cu121` for NVIDIA CUDA 12.1.")]
    DependenciesUnavailable { details: String },
}

pub fn managed_runtime_root() -> PathBuf {
    crate::config::get_data_home().join(MANAGED_RUNTIME_DIR)
}

pub fn managed_python(root: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        root.join(".venv").join("Scripts").join("python.exe")
    }
    #[cfg(not(windows))]
    {
        root.join(".venv").join("bin").join("python")
    }
}

pub fn discover_managed_python(root: &Path) -> Option<PathBuf> {
    let python = managed_python(root);
    python
        .is_file()
        .then(|| fs::canonicalize(&python).unwrap_or(python))
}

pub fn discover_managed_worker(root: &Path) -> Option<PathBuf> {
    let worker = root.join("liveaudio").join("service").join("asr_worker.py");
    worker
        .is_file()
        .then(|| fs::canonicalize(&worker).unwrap_or(worker))
}

/// Find bundled uv candidates in order, then explicit PATH candidates.
pub fn discover_uv_in(bundle_roots: &[PathBuf], path_candidates: &[PathBuf]) -> Option<PathBuf> {
    let mut bundled = Vec::new();
    for root in bundle_roots {
        let binary = if cfg!(windows) { "uv.exe" } else { "uv" };
        bundled.extend([
            root.join(binary),
            root.join("vendor").join(binary),
            root.join("packaging").join("vendor").join(binary),
            root.join("resources").join("vendor").join(binary),
        ]);
    }
    bundled
        .into_iter()
        .chain(path_candidates.iter().cloned())
        .find(|path| path.is_file())
}

pub fn find_uv_executable() -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            roots.push(parent.to_path_buf());
            roots.push(parent.join(".."));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    let path_candidates = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|directory| directory.join(if cfg!(windows) { "uv.exe" } else { "uv" }))
        .collect::<Vec<_>>();
    discover_uv_in(&roots, &path_candidates)
}

/// Explicit setup operation. It stages the locked project and package, then invokes uv.
pub fn setup_managed_runtime(backend: AsrBackend) -> Result<PathBuf, RuntimeError> {
    let uv = find_uv_executable().ok_or(RuntimeError::UvNotFound)?;
    let worker = crate::supervisor::WorkerProcessConfig::find_asr_worker_source()
        .ok_or(RuntimeError::PackageSourceMissing)?;
    let source_root = crate::supervisor::WorkerProcessConfig::find_package_root(&Some(worker))
        .ok_or(RuntimeError::PackageSourceMissing)?;
    let root = managed_runtime_root();
    provision_runtime_with_uv(&root, &source_root, backend, &uv)?;
    Ok(root)
}

/// Materialize manifests and the Python package in `root`, then run locked uv sync.
pub fn provision_runtime_with_uv(
    root: &Path,
    package_source_root: &Path,
    backend: AsrBackend,
    uv_executable: &Path,
) -> Result<(), RuntimeError> {
    if !package_source_root.join("liveaudio").is_dir() {
        return Err(RuntimeError::PackageSourceMissing);
    }
    fs::create_dir_all(root)?;
    fs::write(root.join("pyproject.toml"), PYPROJECT_TOML)?;
    fs::write(root.join("uv.lock"), UV_LOCK)?;
    copy_package_tree(
        &package_source_root.join("liveaudio"),
        &root.join("liveaudio"),
    )?;

    let args = [
        "sync".to_string(),
        "--project".to_string(),
        root.to_string_lossy().into_owned(),
        "--locked".to_string(),
        "--no-install-project".to_string(),
        "--no-dev".to_string(),
        "--extra".to_string(),
        backend.extra().to_string(),
        "--python".to_string(),
        "3.11".to_string(),
    ];
    let output = sync_command_for(uv_executable, &args)
        .current_dir(root)
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(RuntimeError::UvFailed {
            status: output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "terminated".to_string()),
            details: "uv sync did not complete; review the setup command output".to_string(),
        });
    }
    Ok(())
}

/// Check imports without starting a model, choosing a GPU, or invoking a package manager.
pub async fn preflight_asr_runtime(
    python: &Path,
    package_root: Option<&Path>,
) -> Result<(), RuntimeError> {
    let args = ["-c", IMPORT_PROBE];
    let mut command = command_for(python, &args);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(root) = package_root {
        command.env("PYTHONPATH", root);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return Err(RuntimeError::DependenciesUnavailable {
                details: format!(
                    "could not run the selected Python interpreter ({})",
                    error.kind()
                ),
            })
        }
    };
    let mut stderr = child
        .stderr
        .take()
        .expect("stderr is piped for import diagnostics");
    let stderr_task = tokio::spawn(async move {
        let mut limited = (&mut stderr).take(4096);
        let mut bytes = Vec::new();
        let _ = limited.read_to_end(&mut bytes).await;
        bytes
    });
    let status = wait_for_probe(&mut child, PROBE_TIMEOUT).await;
    if status.is_err() {
        let _ = stderr_task.await;
        return status.map(|_| ());
    }
    let status = status.expect("checked probe status");
    let stderr = stderr_task.await.unwrap_or_default();
    if status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&stderr);
    let details = missing_module_name(&stderr)
        .unwrap_or_else(|| "required Python imports failed".to_string());
    Err(RuntimeError::DependenciesUnavailable { details })
}

async fn wait_for_probe(
    child: &mut Child,
    timeout: Duration,
) -> Result<std::process::ExitStatus, RuntimeError> {
    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => Ok(status),
        Ok(Err(error)) => Err(RuntimeError::Io(error)),
        Err(_) => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            Err(RuntimeError::ProbeTimedOut(timeout.as_secs().max(1)))
        }
    }
}

fn missing_module_name(stderr: &str) -> Option<String> {
    let marker = "No module named ";
    let tail = stderr.split_once(marker)?.1;
    let module = tail
        .trim_start_matches(['\'', '"'])
        .split(['\'', '"', '\r', '\n'])
        .next()?
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '.')
        .collect::<String>();
    (!module.is_empty()).then_some(module)
}

fn copy_package_tree(source: &Path, target: &Path) -> Result<(), std::io::Error> {
    if let (Ok(source), Ok(target)) = (fs::canonicalize(source), fs::canonicalize(target)) {
        if source == target {
            return Ok(());
        }
    }
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            if entry.file_name() == "__pycache__" {
                continue;
            }
            copy_package_tree(&source_path, &target_path)?;
        } else if metadata.is_file() {
            fs::copy(source_path, target_path)?;
        }
    }
    Ok(())
}

fn command_for(executable: &Path, args: &[impl AsRef<std::ffi::OsStr>]) -> Command {
    #[cfg(windows)]
    if executable
        .extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("cmd"))
    {
        let mut command = Command::new("cmd.exe");
        command.arg("/D").arg("/C").arg(executable).args(args);
        return command;
    }
    let mut command = Command::new(executable);
    command.args(args);
    command
}

fn sync_command_for(
    executable: &Path,
    args: &[impl AsRef<std::ffi::OsStr>],
) -> std::process::Command {
    #[cfg(windows)]
    if executable
        .extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("cmd"))
    {
        let mut command = std::process::Command::new("cmd.exe");
        command.arg("/D").arg("/C").arg(executable).args(args);
        return command;
    }
    let mut command = std::process::Command::new(executable);
    command.args(args);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_module_message_is_reduced_to_module_name() {
        assert_eq!(
            missing_module_name("ModuleNotFoundError: No module named 'faster_whisper'"),
            Some("faster_whisper".to_string())
        );
        assert_eq!(
            missing_module_name("generic import failure at C:\\private\\path"),
            None
        );
    }
}
