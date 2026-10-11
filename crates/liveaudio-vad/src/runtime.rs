use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::error::VadError;

static INITIALIZATION_LOCK: Mutex<()> = Mutex::new(());
static INITIALIZED_LIBRARY: OnceLock<PathBuf> = OnceLock::new();

/// Initialize the process-global ONNX Runtime backend from an explicit app-owned library path.
///
/// Call once during application startup, before any other `ort` API or service side effects.
pub fn initialize_onnx_runtime(path: impl AsRef<Path>) -> Result<(), VadError> {
    let path = path.as_ref();
    if !path.is_absolute() {
        return Err(VadError::RuntimeUnavailable(
            "the bundled CPU library path must be absolute; repair or reinstall LiveAudio."
                .to_string(),
        ));
    }
    let _guard = INITIALIZATION_LOCK.lock().map_err(|_| {
        VadError::RuntimeUnavailable(
            "ONNX Runtime initialization state is unavailable; restart or reinstall LiveAudio."
                .to_string(),
        )
    })?;
    if let Some(initialized) = INITIALIZED_LIBRARY.get() {
        if initialized == path {
            return Ok(());
        }
        return Err(VadError::RuntimeUnavailable(
            "ONNX Runtime was already initialized from another path in this process. Restart LiveAudio."
                .to_string(),
        ));
    }
    if !path.is_file() {
        return Err(VadError::RuntimeUnavailable(
            "the bundled CPU library is missing. Repair or reinstall LiveAudio.".to_string(),
        ));
    }

    let environment = ort::init_from(path).map_err(|error| {
        let _ = error;
        VadError::RuntimeUnavailable(
            "the bundled CPU library could not be loaded. Repair or reinstall LiveAudio."
                .to_string(),
        )
    })?;
    environment.commit();
    INITIALIZED_LIBRARY.set(path.to_path_buf()).map_err(|_| {
        VadError::RuntimeUnavailable(
            "ONNX Runtime initialization state changed unexpectedly. Restart LiveAudio."
                .to_string(),
        )
    })?;
    Ok(())
}

/// Resolve the CPU runtime library underneath an application-owned resource directory.
///
/// The caller must pass the absolute directory returned by the platform's resource resolver.
pub fn cpu_runtime_library_path(resource_dir: impl AsRef<Path>) -> Result<PathBuf, VadError> {
    let resource_dir = resource_dir.as_ref();
    if !resource_dir.is_absolute() {
        return Err(VadError::RuntimeUnavailable(
            "the application resource directory is not absolute. Repair or reinstall LiveAudio."
                .to_string(),
        ));
    }

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    let relative = Path::new("onnxruntime/windows-x64/onnxruntime.dll");
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    let relative = Path::new("onnxruntime/linux-x64/libonnxruntime.so.1.26.0");
    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    )))]
    return Err(VadError::RuntimeUnavailable(
        "this platform has no bundled CPU runtime. Repair or reinstall LiveAudio.".to_string(),
    ));

    #[cfg(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    Ok(resource_dir.join(relative))
}

pub(crate) fn ensure_onnx_runtime_initialized() -> Result<(), VadError> {
    if INITIALIZED_LIBRARY.get().is_some() {
        Ok(())
    } else {
        Err(VadError::RuntimeUnavailable(
            "the application has not initialized its bundled CPU runtime. Repair or reinstall LiveAudio."
                .to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::cpu_runtime_library_path;

    #[test]
    fn resource_path_is_platform_specific_and_requires_absolute_root() {
        let root = std::env::temp_dir().join("liveaudio-resources");
        let resolved = cpu_runtime_library_path(&root).expect("absolute resource root");
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        assert!(resolved.ends_with("onnxruntime/windows-x64/onnxruntime.dll"));
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        assert!(resolved.ends_with("onnxruntime/linux-x64/libonnxruntime.so.1.26.0"));

        assert!(cpu_runtime_library_path("relative/resources").is_err());
    }
}
