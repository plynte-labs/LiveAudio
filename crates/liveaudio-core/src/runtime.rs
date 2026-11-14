// SPDX-License-Identifier: MIT

//! Provisioning, discovery, and readiness checks for the native Rust ASR worker binary.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};

const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

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
        #[cfg(windows)]
        let (cmd, args) = (
            "powershell.exe",
            vec![
                "-NoProfile".to_string(),
                "-NonInteractive".to_string(),
                "-Command".to_string(),
                format!(
                    "[System.IO.File]::WriteAllText('{}', $PID); Start-Sleep -Seconds 60",
                    pid_file.display()
                ),
            ],
        );
        #[cfg(not(windows))]
        let (cmd, args) = (
            "sh",
            vec![
                "-c".to_string(),
                format!("echo $$ > '{}'; sleep 60", pid_file.display()),
            ],
        );

        let mut command = command_for(Path::new(cmd), &args);
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
            "slow probe must time out"
        );
        assert!(
            child.try_wait().expect("reaped probe status").is_some(),
            "timeout must kill and reap the actual probe child"
        );
        let _ = fs::remove_dir_all(root);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("could not prepare managed ASR runtime: {0}")]
    Io(#[from] std::io::Error),
    #[error("ASR dependency preflight exceeded {0} seconds")]
    ProbeTimedOut(u64),
    #[error("Native ASR worker runtime unavailable: {details}")]
    DependenciesUnavailable { details: String },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct RuntimeSetupProgress {
    pub phase: String,
    pub text: String,
    pub percent: Option<f64>,
}

#[cfg(windows)]
pub(crate) const fn windows_no_console_flag() -> u32 {
    0x0800_0000
}

#[cfg(windows)]
pub fn apply_no_console_to_tokio(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command
        .as_std_mut()
        .creation_flags(windows_no_console_flag());
}

#[cfg(windows)]
pub fn apply_no_console_to_std(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(windows_no_console_flag());
}

fn normalize_path(path: &Path) -> PathBuf {
    let canon = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let raw = canon.to_string_lossy();
    let clean = raw.strip_prefix(r"\\?\").unwrap_or(&raw);
    PathBuf::from(clean)
}

/// Discovers the native Rust ASR worker binary (`liveaudio-asr-worker` or `.exe`).
pub fn discover_native_worker() -> Option<PathBuf> {
    let bin_name = if cfg!(windows) {
        "liveaudio-asr-worker.exe"
    } else {
        "liveaudio-asr-worker"
    };

    // 1. Explicit env var override
    if let Ok(worker) = std::env::var("LIVEAUDIO_WORKER_PATH") {
        let p = PathBuf::from(worker);
        if p.is_file() {
            return Some(normalize_path(&p));
        }
    }

    // 2. Relative to std::env::current_exe() parent exe_dir
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            let candidates = [
                exe_dir.join(bin_name),
                exe_dir.join("binaries").join(bin_name),
                exe_dir.join("resources").join(bin_name),
                // Dev workspace build outputs (e.g. current_exe in target/debug/deps/)
                exe_dir.join("..").join(bin_name),
                exe_dir.join("../../release").join(bin_name),
                exe_dir.join("../../debug").join(bin_name),
                exe_dir.join("../../../target/debug").join(bin_name),
                exe_dir.join("../../../target/release").join(bin_name),
            ];
            for cand in &candidates {
                if cand.is_file() {
                    return Some(normalize_path(cand));
                }
            }
        }
    }

    // 3. In debug assertions, dev workspace paths relative to current working dir
    if cfg!(debug_assertions) {
        let cwd_candidates = [
            PathBuf::from(format!("target/debug/{bin_name}")),
            PathBuf::from(format!("target/release/{bin_name}")),
            PathBuf::from(format!("../target/debug/{bin_name}")),
            PathBuf::from(format!("../target/release/{bin_name}")),
            PathBuf::from(format!("../../target/debug/{bin_name}")),
            PathBuf::from(format!("../../target/release/{bin_name}")),
        ];
        for cand in &cwd_candidates {
            if cand.is_file() {
                return Some(normalize_path(cand));
            }
        }
    }

    None
}

fn ort_dylib_rel() -> &'static Path {
    if cfg!(windows) {
        Path::new("onnxruntime/windows-x64/onnxruntime.dll")
    } else {
        Path::new("onnxruntime/linux-x64/libonnxruntime.so.1.24.4")
    }
}

/// Probes install-relative locations for the ONNX Runtime library.
/// `../lib/LiveAudio` is the Tauri 2 Linux (AppImage) resource dir.
fn find_ort_dylib_near(exe_dir: &Path, rel: &Path) -> Option<PathBuf> {
    [
        exe_dir.join(rel),
        exe_dir.join("resources").join(rel),
        exe_dir.join("../lib/LiveAudio").join(rel),
        exe_dir.join("_up_/packaging").join(rel),
        exe_dir.join("../packaging").join(rel),
        exe_dir.join("../../packaging").join(rel),
        exe_dir.join("../../../packaging").join(rel),
        exe_dir.join("../../../../packaging").join(rel),
    ]
    .into_iter()
    .find(|cand| cand.is_file())
    .map(|cand| fs::canonicalize(&cand).unwrap_or(cand))
}

/// Discovers ONNX Runtime dynamic library for native ASR execution.
pub fn discover_ort_dylib() -> Option<PathBuf> {
    if let Ok(path_str) = std::env::var("LIVEAUDIO_ORT_DYLIB_PATH") {
        let p = PathBuf::from(path_str);
        if p.is_file() {
            return fs::canonicalize(&p).ok().or(Some(p));
        }
    }

    let rel = ort_dylib_rel();

    if let Some(found) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().and_then(|dir| find_ort_dylib_near(dir, rel)))
    {
        return Some(found);
    }

    if cfg!(debug_assertions) {
        let cwd_candidates = [
            PathBuf::from("packaging").join(rel),
            PathBuf::from("../packaging").join(rel),
            PathBuf::from("../../packaging").join(rel),
        ];
        for cand in &cwd_candidates {
            if cand.is_file() {
                return fs::canonicalize(cand).ok().or_else(|| Some(cand.clone()));
            }
        }
    }

    None
}

/// Preflight check for the native ASR worker binary via `--self-test`.
pub async fn preflight_asr_runtime(
    worker_executable: &Path,
    ort_dylib: Option<&Path>,
) -> Result<(), RuntimeError> {
    preflight_inner(worker_executable, ort_dylib, None).await
}

async fn preflight_inner(
    worker_executable: &Path,
    ort_dylib: Option<&Path>,
    path_env: Option<std::ffi::OsString>,
) -> Result<(), RuntimeError> {
    if !worker_executable.is_file() {
        return Err(RuntimeError::DependenciesUnavailable {
            details: format!(
                "ASR worker binary not found at {}",
                worker_executable.display()
            ),
        });
    }

    let args = ["--self-test"];
    let mut command = command_for(worker_executable, &args);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let effective_ort = ort_dylib
        .filter(|p| p.is_file())
        .map(Path::to_path_buf)
        .or_else(discover_ort_dylib);

    if let Some(ort_path) = effective_ort {
        command.env("LIVEAUDIO_ORT_DYLIB_PATH", ort_path);
    }
    if let Some(path) = path_env {
        command.env("PATH", path);
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return Err(RuntimeError::DependenciesUnavailable {
                details: format!(
                    "could not run the ASR worker binary ({})",
                    error.kind()
                ),
            });
        }
    };

    let mut stderr = child
        .stderr
        .take()
        .expect("stderr is piped for diagnostics");
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
    let details = self_test_failure_details(&String::from_utf8_lossy(&stderr), status.code());
    Err(RuntimeError::DependenciesUnavailable { details })
}

/// Windows loader failure (STATUS_DLL_NOT_FOUND) when a dependent DLL such as VCRUNTIME140.dll is absent.
const STATUS_DLL_NOT_FOUND: i32 = 0xC000_0135u32 as i32;

fn self_test_failure_details(stderr: &str, code: Option<i32>) -> String {
    if !stderr.trim().is_empty() {
        stderr.trim().to_string()
    } else if code == Some(STATUS_DLL_NOT_FOUND) {
        "Microsoft Visual C++ runtime DLL missing; reinstall LiveAudio".to_string()
    } else {
        format!("ASR worker self-test exited with status {}", code.map_or("unknown".into(), |c| c.to_string()))
    }
}

/// Makes missing-DLL loader failures in this process and its children exit immediately instead of
/// blocking on a modal system dialog. Call once at startup.
#[cfg(windows)]
pub fn suppress_loader_error_dialogs() {
    use windows_sys::Win32::System::Diagnostics::Debug::{SetErrorMode, SEM_FAILCRITICALERRORS};
    // SAFETY: SetErrorMode only changes process-wide error-mode flags.
    unsafe {
        let prev = SetErrorMode(0);
        SetErrorMode(prev | SEM_FAILCRITICALERRORS);
    }
}

#[cfg(not(windows))]
pub fn suppress_loader_error_dialogs() {}

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

pub const CT2_WORKER_EXE: &str = "liveaudio-asr-worker-cuda.exe";

/// Env override first, then the NVIDIA runtime dir (the worker sits next to the CUDA DLLs).
fn find_ct2_worker(env_override: Option<&str>, runtime_dir: &Path) -> Option<PathBuf> {
    env_override
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .or_else(|| Some(runtime_dir.join(CT2_WORKER_EXE)).filter(|p| p.is_file()))
}

/// Discovers the ct2 CUDA worker: env `LIVEAUDIO_CT2_WORKER_PATH`, the NVIDIA runtime dir, then
/// (debug builds only) dev build outputs.
pub fn discover_ct2_worker() -> Option<PathBuf> {
    let env = std::env::var("LIVEAUDIO_CT2_WORKER_PATH").ok();
    let found = find_ct2_worker(env.as_deref(), &crate::nvidia_runtime::runtime_dir());
    if found.is_some() || !cfg!(debug_assertions) {
        return found.map(|p| normalize_path(&p));
    }
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    ["", "..", "../../release", "../../debug"]
        .iter()
        .map(|rel| exe_dir.join(rel).join(CT2_WORKER_EXE))
        .chain(["target/release", "target/debug"].map(|d| Path::new(d).join(CT2_WORKER_EXE)))
        .find(|p| p.is_file())
        .map(|p| normalize_path(&p))
}

/// `dir` prepended to a PATH value, so the ct2 worker finds the CUDA DLLs.
pub fn prepend_path(dir: &Path, existing: Option<std::ffi::OsString>) -> std::ffi::OsString {
    let rest = existing.iter().flat_map(std::env::split_paths);
    std::env::join_paths(std::iter::once(dir.to_path_buf()).chain(rest)).unwrap_or_else(|_| dir.into())
}

/// Preflight of the ct2 worker (`--self-test`) with the NVIDIA runtime dir on its PATH.
pub async fn preflight_ct2_worker(worker: &Path) -> Result<(), RuntimeError> {
    let path = prepend_path(&crate::nvidia_runtime::runtime_dir(), std::env::var_os("PATH"));
    preflight_inner(worker, None, Some(path)).await
}

fn command_for(executable: &Path, args: &[impl AsRef<std::ffi::OsStr>]) -> Command {
    #[cfg(windows)]
    if executable
        .extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("cmd"))
    {
        let mut command = Command::new("cmd.exe");
        command.arg("/D").arg("/C").arg(executable).args(args);
        apply_no_console_to_tokio(&mut command);
        return command;
    }
    let mut command = Command::new(executable);
    command.args(args);
    #[cfg(windows)]
    apply_no_console_to_tokio(&mut command);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn windows_runtime_commands_use_create_no_window() {
        assert_eq!(windows_no_console_flag(), 0x0800_0000);
    }

    #[test]
    fn dll_not_found_exit_maps_to_vc_runtime_message() {
        let msg = self_test_failure_details("", Some(0xC000_0135u32 as i32));
        assert!(msg.contains("Visual C++ runtime DLL missing"), "{msg}");
        assert_eq!(self_test_failure_details("boom", Some(0xC000_0135u32 as i32)), "boom");
        assert!(self_test_failure_details("", Some(1)).contains("status"));
    }

    #[cfg(windows)]
    #[test]
    fn loader_error_dialogs_are_suppressed() {
        suppress_loader_error_dialogs();
        // SetErrorMode returns the previous mode; restore it to keep the test side-effect free.
        use windows_sys::Win32::System::Diagnostics::Debug::{SetErrorMode, SEM_FAILCRITICALERRORS};
        let prev = unsafe { SetErrorMode(0) };
        unsafe { SetErrorMode(prev) };
        assert!(prev & SEM_FAILCRITICALERRORS != 0, "SEM_FAILCRITICALERRORS must be set");
    }

    #[test]
    fn ct2_worker_is_found_by_env_override_then_runtime_dir() {
        let root = std::env::temp_dir().join(format!("liveaudio-ct2-find-{}", std::process::id()));
        let rt = root.join("cuda12");
        fs::create_dir_all(&rt).unwrap();
        assert_eq!(find_ct2_worker(None, &rt), None);

        let in_rt = rt.join(CT2_WORKER_EXE);
        fs::write(&in_rt, b"x").unwrap();
        assert_eq!(find_ct2_worker(None, &rt), Some(in_rt.clone()));
        assert_eq!(find_ct2_worker(Some("missing.exe"), &rt), Some(in_rt.clone()), "bad override falls through");

        let custom = root.join("custom.exe");
        fs::write(&custom, b"x").unwrap();
        assert_eq!(find_ct2_worker(Some(custom.to_str().unwrap()), &rt), Some(custom));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ort_dylib_is_found_in_tauri_linux_resource_dir() {
        let root = std::env::temp_dir().join(format!("liveaudio-ort-res-{}", std::process::id()));
        let exe_dir = root.join("usr").join("bin");
        fs::create_dir_all(&exe_dir).unwrap();
        let rel = ort_dylib_rel();
        assert_eq!(find_ort_dylib_near(&exe_dir, rel), None);

        let lib = root.join("usr/lib/LiveAudio").join(rel);
        fs::create_dir_all(lib.parent().unwrap()).unwrap();
        fs::write(&lib, b"x").unwrap();
        let found = find_ort_dylib_near(&exe_dir, rel).expect("resource_dir probe");
        assert_eq!(found.file_name(), lib.file_name());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prepend_path_puts_the_runtime_dir_first() {
        let joined = prepend_path(Path::new("C:/rt"), Some("C:/a;C:/b".into()));
        let parts: Vec<_> = std::env::split_paths(&joined).collect();
        assert_eq!(parts[0], PathBuf::from("C:/rt"));
        assert!(parts.len() >= 2);
        assert_eq!(prepend_path(Path::new("C:/rt"), None), std::ffi::OsString::from("C:/rt"));
    }

    #[tokio::test]
    async fn ct2_preflight_of_a_missing_worker_fails() {
        let r = preflight_ct2_worker(Path::new("nonexistent_ct2_worker.exe")).await;
        assert!(matches!(r, Err(RuntimeError::DependenciesUnavailable { .. })));
    }

    #[tokio::test]
    async fn test_preflight_missing_worker_fails() {
        let nonexistent = Path::new("nonexistent_binary_file_never_exists.xyz");
        let result = preflight_asr_runtime(nonexistent, None).await;
        assert!(matches!(
            result,
            Err(RuntimeError::DependenciesUnavailable { .. })
        ));
    }
}
