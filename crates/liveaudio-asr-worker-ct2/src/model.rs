// SPDX-License-Identifier: MIT

//! Model mapping, cache layout, required-file check and load planning (no GPU, no network).

use std::path::{Path, PathBuf};

use liveaudio_worker_protocol::protocol::InitPayload;
use liveaudio_worker_protocol::LoadError;

/// Files every CTranslate2 Whisper model directory must contain (plus one `vocabulary.*`).
pub const REQUIRED_FILES: [&str; 4] =
    ["model.bin", "config.json", "tokenizer.json", "preprocessor_config.json"];

/// Maps a model name to a CTranslate2-format Hugging Face repo. Names containing `/` pass through.
pub fn resolve_repo_id(model_name: &str) -> Result<String, String> {
    let name = model_name.trim();
    if name.contains('/') {
        return Ok(name.to_string());
    }
    let lower = name.to_lowercase();
    match lower.as_str() {
        "tiny" | "tiny.en" | "base" | "base.en" | "small" | "small.en" | "medium" | "medium.en"
        | "large-v1" | "large-v2" | "large-v3" => Ok(format!("Systran/faster-whisper-{lower}")),
        "large" => Ok("Systran/faster-whisper-large-v3".to_string()),
        // Verified on the HF API 2026-11-07: MIT, ships preprocessor_config.json.
        "turbo" | "large-v3-turbo" => Ok("deepdml/faster-whisper-large-v3-turbo-ct2".to_string()),
        _ => Err(format!(
            "Unknown model '{name}': use tiny, base, small, medium, large-v3, turbo or an owner/repo CTranslate2 repo"
        )),
    }
}

/// Repo that provides `preprocessor_config.json`. The small Systran repos omit it, so it comes
/// from the matching `openai/whisper-<size>` repo; every other repo is used as-is.
pub fn preprocessor_repo(repo_id: &str) -> String {
    match repo_id.strip_prefix("Systran/faster-whisper-") {
        Some(size) if !size.starts_with("large") => format!("openai/whisper-{size}"),
        _ => repo_id.to_string(),
    }
}

/// `<override | LIVEAUDIO_MODEL_CACHE | %LOCALAPPDATA%\LiveAudio\models\ct2>\<repo with / as -->`.
pub fn resolve_cache_dir(
    cache_override: Option<&str>,
    env_cache: Option<&str>,
    data_local: Option<&Path>,
    repo_id: &str,
) -> PathBuf {
    let sanitized = repo_id.replace(['/', '\\'], "--");
    if let Some(dir) = cache_override {
        return PathBuf::from(dir).join(sanitized);
    }
    if let Some(dir) = env_cache {
        return PathBuf::from(dir).join(sanitized);
    }
    data_local
        .map(Path::to_path_buf)
        .unwrap_or_else(std::env::temp_dir)
        .join("LiveAudio")
        .join("models")
        .join("ct2")
        .join(sanitized)
}

fn has_content(p: &Path) -> bool {
    std::fs::metadata(p).map(|m| m.is_file() && m.len() > 0).unwrap_or(false)
}

/// Names of required files absent (or empty) in `dir`; `vocabulary.*` stands for json or txt.
pub fn missing_files(dir: &Path) -> Vec<String> {
    let mut missing: Vec<String> = REQUIRED_FILES
        .iter()
        .filter(|f| !has_content(&dir.join(f)))
        .map(|f| f.to_string())
        .collect();
    if !has_content(&dir.join("vocabulary.json")) && !has_content(&dir.join("vocabulary.txt")) {
        missing.push("vocabulary.*".to_string());
    }
    missing
}

/// CUDA runtime DLLs the engine needs. cuda-dynamic-loading defers them, so a missing one would
/// otherwise crash the first decode instead of failing `init`.
pub const CUDA_RUNTIME_DLLS: [&str; 6] = [
    "cublas64_12.dll",
    "cublasLt64_12.dll",
    "cudnn64_9.dll",
    "cudnn_ops64_9.dll",
    "cudnn_cnn64_9.dll",
    "cudnn_graph64_9.dll",
];

/// DLLs from [`CUDA_RUNTIME_DLLS`] not found in any of `dirs` (exe directory plus `PATH`).
pub fn missing_cuda_dlls(dirs: &[PathBuf]) -> Vec<&'static str> {
    CUDA_RUNTIME_DLLS
        .into_iter()
        .filter(|dll| !dirs.iter().any(|d| d.join(dll).is_file()))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadPlan {
    /// CTranslate2 compute type string: float16, float32, int8, int8_float16, ...
    pub compute_type: String,
    pub beam_size: usize,
}

fn fail(code: &'static str, message: String) -> LoadError {
    LoadError { code, message, recoverable: false }
}

/// Validates the init request for the CUDA-only engine. Anything but CUDA is
/// `DEVICE_UNAVAILABLE` (non-recoverable) so the host falls back to the ONNX Runtime worker.
pub fn plan_load(init: &InitPayload) -> Result<LoadPlan, LoadError> {
    match init.device.trim().to_lowercase().as_str() {
        "cuda" | "gpu" | "auto" => {}
        other => {
            return Err(fail(
                "DEVICE_UNAVAILABLE",
                format!("The CTranslate2 worker only supports CUDA (requested device '{other}')"),
            ))
        }
    }
    let compute_type = match init.compute_type.as_deref().map(|c| c.trim().to_lowercase()) {
        None => "float16".to_string(),
        Some(c) => match c.as_str() {
            "" | "auto" | "default" | "float16" | "fp16" => "float16".to_string(),
            "float32" | "fp32" => "float32".to_string(),
            "int8" | "int8_float16" | "int8_float32" | "bfloat16" => c,
            other => {
                return Err(fail(
                    "ENGINE_LOAD_FAILED",
                    format!("Unsupported compute_type '{other}' for the CTranslate2 worker"),
                ))
            }
        },
    };
    Ok(LoadPlan { compute_type, beam_size: init.beam_size.unwrap_or(1).max(1) })
}

// SPDX-License-Identifier: MIT

#[cfg(test)]
mod tests {
    use super::*;
    use liveaudio_worker_protocol::protocol::InitPayload;
    use std::path::{Path, PathBuf};

    fn init(device: &str, compute: Option<&str>, beam: Option<usize>) -> InitPayload {
        InitPayload {
            device: device.into(),
            compute_type: compute.map(String::from),
            beam_size: beam,
            ..InitPayload::default()
        }
    }

    #[test]
    fn maps_sizes_to_systran_repos() {
        for size in ["tiny", "base", "small", "medium"] {
            assert_eq!(resolve_repo_id(size).unwrap(), format!("Systran/faster-whisper-{size}"));
        }
        assert_eq!(resolve_repo_id(" Small ").unwrap(), "Systran/faster-whisper-small");
        assert_eq!(resolve_repo_id("small.en").unwrap(), "Systran/faster-whisper-small.en");
        assert_eq!(resolve_repo_id("large-v3").unwrap(), "Systran/faster-whisper-large-v3");
        assert_eq!(resolve_repo_id("turbo").unwrap(), "deepdml/faster-whisper-large-v3-turbo-ct2");
        assert_eq!(resolve_repo_id("me/custom-ct2").unwrap(), "me/custom-ct2");
        assert!(resolve_repo_id("nonsense").is_err());
    }

    #[test]
    fn preprocessor_comes_from_openai_for_systran_sizes() {
        assert_eq!(preprocessor_repo("Systran/faster-whisper-small"), "openai/whisper-small");
        assert_eq!(preprocessor_repo("Systran/faster-whisper-medium.en"), "openai/whisper-medium.en");
        // Repos that ship their own preprocessor_config.json are used as-is.
        assert_eq!(preprocessor_repo("Systran/faster-whisper-large-v3"), "Systran/faster-whisper-large-v3");
        assert_eq!(
            preprocessor_repo("deepdml/faster-whisper-large-v3-turbo-ct2"),
            "deepdml/faster-whisper-large-v3-turbo-ct2"
        );
    }

    #[test]
    fn cache_dir_precedence_and_layout() {
        let local = Path::new("C:/Users/x/AppData/Local");
        let repo = "Systran/faster-whisper-small";
        assert_eq!(
            resolve_cache_dir(Some("D:/m"), Some("E:/env"), Some(local), repo),
            PathBuf::from("D:/m").join("Systran--faster-whisper-small")
        );
        assert_eq!(
            resolve_cache_dir(None, Some("E:/env"), Some(local), repo),
            PathBuf::from("E:/env").join("Systran--faster-whisper-small")
        );
        assert_eq!(
            resolve_cache_dir(None, None, Some(local), repo),
            local.join("LiveAudio").join("models").join("ct2").join("Systran--faster-whisper-small")
        );
    }

    #[test]
    fn required_file_check_needs_all_files_and_a_vocabulary() {
        let dir = std::env::temp_dir().join(format!("la-ct2-req-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let missing = missing_files(&dir);
        assert!(missing.contains(&"model.bin".to_string()));
        assert!(missing.contains(&"vocabulary.*".to_string()));
        for f in ["model.bin", "config.json", "tokenizer.json", "preprocessor_config.json", "vocabulary.txt"] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        assert!(missing_files(&dir).is_empty());
        std::fs::write(dir.join("model.bin"), b"").unwrap(); // empty counts as missing
        assert_eq!(missing_files(&dir), vec!["model.bin".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cpu_device_is_unavailable_and_not_recoverable() {
        for dev in ["cpu", "CPU", "directml"] {
            let err = plan_load(&init(dev, None, None)).err().unwrap();
            assert_eq!(err.code, "DEVICE_UNAVAILABLE");
            assert!(!err.recoverable);
        }
    }

    #[test]
    fn cuda_defaults_to_float16_and_beam_one() {
        for dev in ["cuda", "auto", "gpu"] {
            let plan = plan_load(&init(dev, None, None)).ok().unwrap();
            assert_eq!(plan.compute_type, "float16");
            assert_eq!(plan.beam_size, 1);
        }
        assert_eq!(plan_load(&init("cuda", Some("default"), None)).ok().unwrap().compute_type, "float16");
    }

    #[test]
    fn honors_explicit_compute_type_and_beam() {
        let plan = plan_load(&init("cuda", Some("int8_float16"), Some(5))).ok().unwrap();
        assert_eq!(plan.compute_type, "int8_float16");
        assert_eq!(plan.beam_size, 5);
        assert_eq!(plan_load(&init("cuda", Some("fp32"), Some(0))).ok().unwrap().beam_size, 1);
        assert_eq!(plan_load(&init("cuda", Some("fp32"), None)).ok().unwrap().compute_type, "float32");
        assert!(plan_load(&init("cuda", Some("bogus"), None)).is_err());
    }

    #[test]
    fn reports_missing_cuda_runtime_dlls() {
        let dir = std::env::temp_dir().join(format!("la-ct2-dll-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(missing_cuda_dlls(&[dir.clone()]).len(), CUDA_RUNTIME_DLLS.len());
        for name in CUDA_RUNTIME_DLLS {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        assert!(missing_cuda_dlls(&[PathBuf::from("Z:/nonexistent"), dir.clone()]).is_empty());
        std::fs::remove_file(dir.join("cudnn64_9.dll")).unwrap();
        assert_eq!(missing_cuda_dlls(&[dir.clone()]), vec!["cudnn64_9.dll"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
