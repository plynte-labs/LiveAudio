// SPDX-License-Identifier: MIT

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::{Client, StatusCode};

/// No total timeout: a large model on a slow link must be allowed to finish. Stalls are caught per read.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const READ_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
struct Sibling {
    rfilename: String,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    lfs: Option<LfsInfo>,
}

#[derive(Debug, Clone, Deserialize)]
struct LfsInfo {
    sha256: String,
}

/// What the Hub reports (at the pinned revision) for a file, used to verify the download.
#[derive(Debug, Clone, Default)]
struct Expected {
    size: Option<u64>,
    sha256: Option<String>,
}

impl From<&Sibling> for Expected {
    fn from(s: &Sibling) -> Self {
        Self { size: s.size, sha256: s.lfs.as_ref().map(|l| l.sha256.to_lowercase()) }
    }
}

/// Immutable commits of the built-in model repos. Custom repos have no pin and track `main`.
fn pinned_revision(repo_id: &str) -> &'static str {
    match repo_id {
        "onnx-community/whisper-tiny" => "ff4177021cc41f7db950912b73ea4fdf7d01d8e7",
        "onnx-community/whisper-base" => "27a07b4b52b3a4c5886134bb4320e93e1a4bbc7c",
        "onnx-community/whisper-small" => "36050c46d777d46dc4b5f43f6d90574fc38f8732",
        "onnx-community/whisper-medium-ONNX" => "d3978248a6b5de6df7ec29ddfbde3993845fa806",
        "onnx-community/whisper-large-v3-turbo" => "360ebcde2559d60bb474678be3c1de9ef347d01a",
        _ => "main",
    }
}

fn resolve_file_url(repo_id: &str, revision: &str, rel_path: &str) -> String {
    format!("https://huggingface.co/{repo_id}/resolve/{revision}/{rel_path}")
}

/// Checks a finished download against the Hub-reported size and (for LFS files) SHA-256.
fn verify_download(path: &Path, expected: &Expected) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    if let Some(size) = expected.size {
        let got = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
        if got != size {
            return Err(format!("size mismatch: expected {size} bytes, got {got}"));
        }
    }
    if let Some(want) = &expected.sha256 {
        let mut hasher = Sha256::new();
        let mut file = File::open(path).map_err(|e| e.to_string())?;
        std::io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
        let got = format!("{:x}", hasher.finalize());
        if &got != want {
            return Err(format!("SHA-256 mismatch: expected {want}, got {got}"));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize)]
struct ModelInfo {
    #[serde(default)]
    siblings: Vec<Sibling>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelPrecision {
    Fp32,
    Fp16,
    Int8,
}

pub fn resolve_target_precision(device: &str, compute_type: Option<&str>) -> ModelPrecision {
    let dev = device.to_lowercase();
    let comp = compute_type.map(|s| s.to_lowercase());
    let comp_ref = comp.as_deref();

    if matches!(comp_ref, Some("int8" | "uint8")) {
        ModelPrecision::Int8
    } else if matches!(comp_ref, Some("float16" | "fp16" | "int8_float16")) {
        ModelPrecision::Fp16
    } else if matches!(comp_ref, Some("float32" | "fp32")) {
        ModelPrecision::Fp32
    } else if matches!(dev.as_str(), "cuda" | "gpu" | "directml" | "dml")
        && (comp_ref.is_none() || matches!(comp_ref, Some("auto" | "default")))
    {
        ModelPrecision::Fp16
    } else if dev == "cpu"
        && !matches!(comp_ref, Some("float32" | "fp32" | "float16" | "fp16"))
    {
        ModelPrecision::Int8
    } else {
        ModelPrecision::Fp32
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedModelPaths {
    pub root_dir: PathBuf,
    pub config_json: PathBuf,
    pub preprocessor_config_json: PathBuf,
    pub tokenizer_json: PathBuf,
    pub encoder_onnx: PathBuf,
    pub decoder_onnx: PathBuf,
    pub decoder_with_past_onnx: Option<PathBuf>,
}

fn path_has_tag(p: &Path, tag: &str) -> bool {
    p.file_name()
        .map(|f| f.to_string_lossy().to_lowercase().contains(tag))
        .unwrap_or(false)
}

impl ResolvedModelPaths {
    pub fn fallback_cpu_paths(&self) -> Option<ResolvedModelPaths> {
        let is_fp16 = path_has_tag(&self.encoder_onnx, "fp16")
            || path_has_tag(&self.decoder_onnx, "fp16");
        if !is_fp16 {
            return None;
        }
        if let Ok(paths) = verify_local_dir_with_precision(&self.root_dir, ModelPrecision::Int8, false) {
            let still_fp16 = path_has_tag(&paths.encoder_onnx, "fp16")
                || path_has_tag(&paths.decoder_onnx, "fp16");
            if !still_fp16 {
                return Some(paths);
            }
        }
        if let Ok(paths) = verify_local_dir_with_precision(&self.root_dir, ModelPrecision::Fp32, false) {
            let still_fp16 = path_has_tag(&paths.encoder_onnx, "fp16")
                || path_has_tag(&paths.decoder_onnx, "fp16");
            if !still_fp16 {
                return Some(paths);
            }
        }
        None
    }
}

pub fn resolve_repo_id(model_name: &str) -> String {
    let trimmed = model_name.trim();
    if trimmed.contains('/') {
        return trimmed.to_string();
    }
    match trimmed.to_lowercase().as_str() {
        "tiny" | "tiny.en" => "onnx-community/whisper-tiny".to_string(),
        "base" | "base.en" => "onnx-community/whisper-base".to_string(),
        "small" | "small.en" => "onnx-community/whisper-small".to_string(),
        "medium" | "medium.en" => "onnx-community/whisper-medium-ONNX".to_string(),
        "large" | "large-v2" | "large-v3" | "turbo" | "large-v3-turbo" => {
            "onnx-community/whisper-large-v3-turbo".to_string()
        }
        other => format!("onnx-community/whisper-{other}"),
    }
}

pub fn resolve_cache_dir(cache_override: Option<&str>, repo_id: &str) -> PathBuf {
    let sanitized = repo_id.replace(['/', '\\'], "--");
    if let Some(dir) = cache_override {
        return PathBuf::from(dir).join(&sanitized);
    }
    if let Ok(env_dir) = std::env::var("LIVEAUDIO_MODEL_CACHE") {
        return PathBuf::from(env_dir).join(&sanitized);
    }
    if let Some(data_local) = dirs::data_local_dir() {
        return data_local
            .join("LiveAudio")
            .join("models")
            .join("onnx")
            .join(&sanitized);
    }
    std::env::temp_dir()
        .join("LiveAudio")
        .join("models")
        .join("onnx")
        .join(&sanitized)
}

pub async fn prepare_model(
    model_name: &str,
    device: &str,
    compute_type: Option<&str>,
    cache_override: Option<&str>,
    local_files_only: bool,
    mut progress_cb: impl FnMut(&str, &str, Option<f64>, Option<u64>, Option<u64>, Option<u64>, Option<f64>),
) -> Result<ResolvedModelPaths, String> {
    let precision = resolve_target_precision(device, compute_type);

    let candidate_dir = PathBuf::from(model_name);
    if candidate_dir.is_dir() {
        return verify_local_dir_with_precision(&candidate_dir, precision, false);
    }

    let repo_id = resolve_repo_id(model_name);
    let cache_dir = resolve_cache_dir(cache_override, &repo_id);

    if local_files_only {
        if cache_dir.is_dir() {
            return verify_local_dir_with_precision(&cache_dir, precision, false);
        }
        return Err(format!(
            "Model {} is not cached locally at {} and local_files_only is true",
            repo_id,
            cache_dir.display()
        ));
    }

    // Check strict precision cache first: ONLY take 0ms fast-path if exact target precision
    // and decoder_with_past are ALREADY cached on disk!
    if cache_dir.is_dir() {
        if let Ok(paths) = verify_local_dir_with_precision(&cache_dir, precision, true) {
            if paths.decoder_with_past_onnx.is_some() {
                return Ok(paths);
            }
        }
    }

    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("Failed to create cache directory {}: {e}", cache_dir.display()))?;

    let client = Client::builder()
        .user_agent("LiveAudio/1.6.0")
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_IDLE_TIMEOUT)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let revision = pinned_revision(&repo_id);
    let siblings_res = fetch_repo_siblings(&client, &repo_id, revision).await;
    let siblings = match siblings_res {
        Ok(s) => s,
        Err(err) => {
            eprintln!(
                "[Worker Download] Failed to fetch model siblings ({err}); checking existing local files from {}",
                cache_dir.display()
            );
            if let Ok(paths) = verify_local_dir_with_precision(&cache_dir, precision, false) {
                tracing::warn!(
                    "Failed to fetch model siblings ({err}); using existing local files from {}",
                    cache_dir.display()
                );
                return Ok(paths);
            }
            return Err(err);
        }
    };

    let mut required_files = vec![
        "config.json".to_string(),
        "preprocessor_config.json".to_string(),
        "tokenizer.json".to_string(),
    ];

    let has_sibling = |name: &str| -> bool {
        siblings.iter().any(|s| s.rfilename == name)
    };

    if has_sibling("generation_config.json") {
        required_files.push("generation_config.json".to_string());
    }

    let is_fp16 = precision == ModelPrecision::Fp16;
    let is_quantized = precision == ModelPrecision::Int8;

    let encoder_file = if is_fp16 && has_sibling("onnx/encoder_model_fp16.onnx") {
        "onnx/encoder_model_fp16.onnx"
    } else if is_quantized && has_sibling("onnx/encoder_model_quantized.onnx") {
        "onnx/encoder_model_quantized.onnx"
    } else {
        "onnx/encoder_model.onnx"
    };
    required_files.push(encoder_file.to_string());

    let enc_data = format!("{encoder_file}_data");
    if has_sibling(&enc_data) {
        required_files.push(enc_data);
    }

    let decoder_file = if is_fp16 && has_sibling("onnx/decoder_model_fp16.onnx") {
        "onnx/decoder_model_fp16.onnx"
    } else if is_quantized && has_sibling("onnx/decoder_model_quantized.onnx") {
        "onnx/decoder_model_quantized.onnx"
    } else {
        "onnx/decoder_model.onnx"
    };
    required_files.push(decoder_file.to_string());

    let dec_data = format!("{decoder_file}_data");
    if has_sibling(&dec_data) {
        required_files.push(dec_data);
    }

    let decoder_with_past_file = if is_fp16 && has_sibling("onnx/decoder_with_past_model_fp16.onnx") {
        "onnx/decoder_with_past_model_fp16.onnx"
    } else if is_quantized && has_sibling("onnx/decoder_with_past_model_quantized.onnx") {
        "onnx/decoder_with_past_model_quantized.onnx"
    } else if !is_quantized && !is_fp16 && has_sibling("onnx/decoder_with_past_model.onnx") {
        "onnx/decoder_with_past_model.onnx"
    } else if has_sibling("onnx/decoder_with_past_model.onnx") {
        "onnx/decoder_with_past_model.onnx"
    } else if has_sibling("onnx/decoder_with_past_model_quantized.onnx") {
        "onnx/decoder_with_past_model_quantized.onnx"
    } else {
        ""
    };

    if !decoder_with_past_file.is_empty() {
        required_files.push(decoder_with_past_file.to_string());
        let past_data = format!("{decoder_with_past_file}_data");
        if has_sibling(&past_data) {
            required_files.push(past_data);
        }
    }

    let total_files = required_files.len();
    for (idx, rel_path) in required_files.iter().enumerate() {
        let dest_path = cache_dir.join(rel_path);
        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory {}: {e}", parent.display()))?;
        }

        let expected = siblings
            .iter()
            .find(|s| &s.rfilename == rel_path)
            .map(Expected::from)
            .unwrap_or_default();
        if let Ok(meta) = std::fs::metadata(&dest_path) {
            // Cached files are accepted only when complete (size matches the pinned revision).
            if meta.len() > 0 && expected.size.is_none_or(|size| size == meta.len()) {
                continue;
            }
        }

        let display_name = format!("({}/{}) {}", idx + 1, total_files, rel_path);
        download_single_file(
            &client,
            &resolve_file_url(&repo_id, revision, rel_path),
            rel_path,
            &expected,
            &dest_path,
            &display_name,
            &mut progress_cb,
        )
        .await?;
    }

    verify_local_dir_with_precision(&cache_dir, precision, false)
}

/// Byte offset at which a download response body starts. A 200 carries the whole file; a 206 must
/// state where its range starts. Any other response cannot be appended safely.
fn body_start(status: StatusCode, content_range: Option<&str>) -> Option<u64> {
    match status {
        StatusCode::OK => Some(0),
        StatusCode::PARTIAL_CONTENT => content_range?.strip_prefix("bytes ")?.split('-').next()?.parse().ok(),
        _ => None,
    }
}

/// Partial-download path: the full file name plus `.part`, so sibling files never share one.
fn part_path(dest: &Path) -> PathBuf {
    let mut os = dest.as_os_str().to_owned();
    os.push(".part");
    PathBuf::from(os)
}

async fn fetch_repo_siblings(client: &Client, repo_id: &str, revision: &str) -> Result<Vec<Sibling>, String> {
    let url = format!("https://huggingface.co/api/models/{repo_id}/revision/{revision}?blobs=true");
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("API request failed for {repo_id}: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("HuggingFace API returned status {}", response.status()));
    }

    let info: ModelInfo = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse model API response: {e}"))?;

    Ok(info.siblings)
}

/// GET `url`, asking only for the bytes from `offset` on when a partial file is being resumed.
async fn request_file(client: &Client, url: &str, offset: u64) -> Result<reqwest::Response, String> {
    let mut request = client.get(url);
    if offset > 0 {
        request = request.header(RANGE, format!("bytes={offset}-"));
    }
    request
        .send()
        .await
        .map_err(|e| format!("Failed to start download from {url}: {e}"))
}

async fn download_single_file(
    client: &Client,
    url: &str,
    rel_path: &str,
    expected: &Expected,
    dest_path: &Path,
    display_name: &str,
    progress_cb: &mut impl FnMut(&str, &str, Option<f64>, Option<u64>, Option<u64>, Option<u64>, Option<f64>),
) -> Result<(), String> {
    let part = part_path(dest_path);

    progress_cb("downloading", display_name, Some(0.0), Some(0), None, None, None);

    // A leftover .part is resumed with a Range request; the server then answers 206 (append) or 200 (restart).
    let mut offset = std::fs::metadata(&part).map_or(0, |m| m.len());
    let mut response = request_file(client, &url, offset).await?;
    if offset > 0 && response.status() == StatusCode::RANGE_NOT_SATISFIABLE {
        // The partial file is not a usable prefix (it may already hold the whole file): start over.
        std::fs::remove_file(&part)
            .map_err(|e| format!("Failed to discard {}: {e}", part.display()))?;
        offset = 0;
        response = request_file(client, &url, 0).await?;
    }
    if !response.status().is_success() {
        return Err(format!(
            "Download failed with status {} for {url}",
            response.status()
        ));
    }

    let content_range = response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|v| v.to_str().ok());
    let start = body_start(response.status(), content_range)
        .filter(|start| *start == 0 || *start == offset)
        .ok_or_else(|| format!("Unexpected range response for {url}"))?;
    let total_bytes = response.content_length().map(|len| start + len);
    let mut downloaded: u64 = start;
    let mut last_progress_time = Instant::now();
    let mut last_progress_bytes: u64 = start;

    let mut file = if start > 0 {
        OpenOptions::new().append(true).open(&part)
    } else {
        File::create(&part)
    }
    .map_err(|e| format!("Failed to create temporary file {}: {e}", part.display()))?;

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Chunk download error for {url}: {e}"))?
    {
        file.write_all(&chunk)
            .map_err(|e| format!("Write failed for {}: {e}", part.display()))?;

        downloaded += chunk.len() as u64;

        let elapsed = last_progress_time.elapsed();
        if elapsed.as_millis() >= 200 {
            let bytes_since = downloaded.saturating_sub(last_progress_bytes);
            let speed_bps = ((bytes_since as f64 * 8.0) / elapsed.as_secs_f64()) as u64;

            let percent = total_bytes.map(|tot| ((downloaded as f64) / (tot as f64)) * 100.0);
            let eta_sec = total_bytes.and_then(|tot| {
                if speed_bps > 0 {
                    let remaining_bits = tot.saturating_sub(downloaded) * 8;
                    Some(remaining_bits as f64 / speed_bps as f64)
                } else {
                    None
                }
            });

            progress_cb(
                "downloading",
                display_name,
                percent,
                Some(downloaded),
                total_bytes,
                Some(speed_bps),
                eta_sec,
            );

            last_progress_time = Instant::now();
            last_progress_bytes = downloaded;
        }
    }

    file.flush()
        .map_err(|e| format!("Flush failed for {}: {e}", part.display()))?;
    drop(file);

    // Keep the .part on a short download so the next attempt resumes it.
    if let Some(total) = total_bytes {
        if downloaded != total {
            return Err(format!(
                "Incomplete download of {rel_path}: got {downloaded} of {total} bytes"
            ));
        }
    }

    if let Err(reason) = verify_download(&part, expected) {
        let _ = std::fs::remove_file(&part);
        return Err(format!("Integrity check failed for {rel_path}: {reason}"));
    }

    std::fs::rename(&part, dest_path)
        .map_err(|e| format!("Failed to commit downloaded file to {}: {e}", dest_path.display()))?;

    progress_cb(
        "downloading",
        display_name,
        Some(100.0),
        Some(downloaded),
        Some(downloaded),
        None,
        Some(0.0),
    );

    Ok(())
}

pub fn verify_local_dir_with_precision(
    dir: &Path,
    precision: ModelPrecision,
    strict_precision: bool,
) -> Result<ResolvedModelPaths, String> {
    let config_json = dir.join("config.json");
    if !config_json.is_file() {
        return Err(format!("Missing config.json in {}", dir.display()));
    }

    let preprocessor_config_json = dir.join("preprocessor_config.json");
    if !preprocessor_config_json.is_file() {
        return Err(format!("Missing preprocessor_config.json in {}", dir.display()));
    }

    let tokenizer_json = dir.join("tokenizer.json");
    if !tokenizer_json.is_file() {
        return Err(format!("Missing tokenizer.json in {}", dir.display()));
    }

    let file_has_content = |p: &Path| -> bool {
        p.is_file() && std::fs::metadata(p).map(|m| m.len() > 0).unwrap_or(false)
    };

    let find_first_file = |candidates: &[&str]| -> Option<PathBuf> {
        for &rel in candidates {
            let p = dir.join(rel);
            if file_has_content(&p) {
                return Some(p);
            }
        }
        None
    };

    let fp16_enc_candidates = ["onnx/encoder_model_fp16.onnx", "encoder_model_fp16.onnx"];
    let int8_enc_candidates = [
        "onnx/encoder_model_quantized.onnx",
        "encoder_model_quantized.onnx",
        "onnx/encoder_model_int8.onnx",
        "encoder_model_int8.onnx",
    ];
    let fp32_enc_candidates = ["onnx/encoder_model.onnx", "encoder_model.onnx"];

    let encoder_onnx = match precision {
        ModelPrecision::Fp16 => {
            if let Some(p) = find_first_file(&fp16_enc_candidates) {
                p
            } else if strict_precision {
                return Err(format!("Missing FP16 encoder ONNX model in {}", dir.display()));
            } else if let Some(p) = find_first_file(&fp32_enc_candidates) {
                p
            } else if let Some(p) = find_first_file(&int8_enc_candidates) {
                p
            } else {
                return Err(format!("Missing encoder ONNX model in {}", dir.display()));
            }
        }
        ModelPrecision::Int8 => {
            if let Some(p) = find_first_file(&int8_enc_candidates) {
                p
            } else if strict_precision {
                return Err(format!("Missing quantized/int8 encoder ONNX model in {}", dir.display()));
            } else if let Some(p) = find_first_file(&fp32_enc_candidates) {
                p
            } else if let Some(p) = find_first_file(&fp16_enc_candidates) {
                p
            } else {
                return Err(format!("Missing encoder ONNX model in {}", dir.display()));
            }
        }
        ModelPrecision::Fp32 => {
            if let Some(p) = find_first_file(&fp32_enc_candidates) {
                p
            } else if strict_precision {
                return Err(format!("Missing standard FP32 encoder ONNX model in {}", dir.display()));
            } else if let Some(p) = find_first_file(&fp16_enc_candidates) {
                p
            } else if let Some(p) = find_first_file(&int8_enc_candidates) {
                p
            } else {
                return Err(format!("Missing encoder ONNX model in {}", dir.display()));
            }
        }
    };

    let fp16_dec_candidates = ["onnx/decoder_model_fp16.onnx", "decoder_model_fp16.onnx"];
    let int8_dec_candidates = [
        "onnx/decoder_model_quantized.onnx",
        "decoder_model_quantized.onnx",
        "onnx/decoder_model_int8.onnx",
        "decoder_model_int8.onnx",
    ];
    let fp32_dec_candidates = ["onnx/decoder_model.onnx", "decoder_model.onnx"];

    let decoder_onnx = match precision {
        ModelPrecision::Fp16 => {
            if let Some(p) = find_first_file(&fp16_dec_candidates) {
                p
            } else if strict_precision {
                return Err(format!("Missing FP16 decoder ONNX model in {}", dir.display()));
            } else if let Some(p) = find_first_file(&fp32_dec_candidates) {
                p
            } else if let Some(p) = find_first_file(&int8_dec_candidates) {
                p
            } else {
                return Err(format!("Missing decoder ONNX model in {}", dir.display()));
            }
        }
        ModelPrecision::Int8 => {
            if let Some(p) = find_first_file(&int8_dec_candidates) {
                p
            } else if strict_precision {
                return Err(format!("Missing quantized/int8 decoder ONNX model in {}", dir.display()));
            } else if let Some(p) = find_first_file(&fp32_dec_candidates) {
                p
            } else if let Some(p) = find_first_file(&fp16_dec_candidates) {
                p
            } else {
                return Err(format!("Missing decoder ONNX model in {}", dir.display()));
            }
        }
        ModelPrecision::Fp32 => {
            if let Some(p) = find_first_file(&fp32_dec_candidates) {
                p
            } else if strict_precision {
                return Err(format!("Missing standard FP32 decoder ONNX model in {}", dir.display()));
            } else if let Some(p) = find_first_file(&fp16_dec_candidates) {
                p
            } else if let Some(p) = find_first_file(&int8_dec_candidates) {
                p
            } else {
                return Err(format!("Missing decoder ONNX model in {}", dir.display()));
            }
        }
    };

    let fp16_past_candidates = [
        "onnx/decoder_with_past_model_fp16.onnx",
        "decoder_with_past_model_fp16.onnx",
    ];
    let int8_past_candidates = [
        "onnx/decoder_with_past_model_quantized.onnx",
        "decoder_with_past_model_quantized.onnx",
        "onnx/decoder_with_past_model_int8.onnx",
        "decoder_with_past_model_int8.onnx",
    ];
    let fp32_past_candidates = [
        "onnx/decoder_with_past_model.onnx",
        "decoder_with_past_model.onnx",
    ];

    let decoder_is_fp16 = path_has_tag(&decoder_onnx, "fp16");
    let decoder_is_quantized = path_has_tag(&decoder_onnx, "quantized") || path_has_tag(&decoder_onnx, "int8");

    let decoder_with_past_onnx = if strict_precision {
        match precision {
            ModelPrecision::Fp16 => {
                let p = find_first_file(&fp16_past_candidates);
                if p.is_none() {
                    return Err(format!("Missing FP16 decoder_with_past ONNX model in {}", dir.display()));
                }
                p
            }
            ModelPrecision::Int8 => {
                let p = find_first_file(&int8_past_candidates);
                if p.is_none() {
                    return Err(format!("Missing quantized decoder_with_past ONNX model in {}", dir.display()));
                }
                p
            }
            ModelPrecision::Fp32 => {
                let p = find_first_file(&fp32_past_candidates);
                if p.is_none() {
                    return Err(format!("Missing FP32 decoder_with_past ONNX model in {}", dir.display()));
                }
                p
            }
        }
    } else if decoder_is_fp16 {
        find_first_file(&fp16_past_candidates)
            .or_else(|| find_first_file(&fp32_past_candidates))
            .or_else(|| find_first_file(&int8_past_candidates))
    } else if decoder_is_quantized {
        find_first_file(&int8_past_candidates)
            .or_else(|| find_first_file(&fp32_past_candidates))
            .or_else(|| find_first_file(&fp16_past_candidates))
    } else {
        find_first_file(&fp32_past_candidates)
            .or_else(|| find_first_file(&int8_past_candidates))
            .or_else(|| find_first_file(&fp16_past_candidates))
    };

    Ok(ResolvedModelPaths {
        root_dir: dir.to_path_buf(),
        config_json,
        preprocessor_config_json,
        tokenizer_json,
        encoder_onnx,
        decoder_onnx,
        decoder_with_past_onnx,
    })
}

pub fn verify_local_dir(dir: &Path, prefer_quantized: bool) -> Result<ResolvedModelPaths, String> {
    let precision = if prefer_quantized {
        ModelPrecision::Int8
    } else {
        ModelPrecision::Fp32
    };
    verify_local_dir_with_precision(dir, precision, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_repos_are_pinned_to_a_commit_and_custom_repos_track_main() {
        for name in ["tiny", "base", "small", "medium", "turbo"] {
            let rev = pinned_revision(&resolve_repo_id(name));
            assert_eq!(rev.len(), 40, "{name}");
            assert!(rev.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), "{name}");
        }
        assert_eq!(pinned_revision("custom/my-whisper"), "main");
        assert_eq!(
            resolve_file_url("o/r", "abc", "onnx/x.onnx"),
            "https://huggingface.co/o/r/resolve/abc/onnx/x.onnx"
        );
    }

    #[test]
    fn download_integrity_checks_size_and_lfs_sha256() {
        use sha2::{Digest, Sha256};
        let path = std::env::temp_dir().join(format!("liveaudio-dl-verify-{}", std::process::id()));
        std::fs::write(&path, b"hello").unwrap();
        let good = format!("{:x}", Sha256::digest(b"hello"));
        let expect = |size, sha: Option<&str>| Expected { size: Some(size), sha256: sha.map(str::to_string) };

        assert!(verify_download(&path, &expect(5, Some(&good))).is_ok());
        assert!(verify_download(&path, &expect(5, None)).is_ok());
        assert!(verify_download(&path, &Expected::default()).is_ok());
        assert!(verify_download(&path, &expect(6, None)).unwrap_err().contains("size"));
        assert!(verify_download(&path, &expect(5, Some(&"0".repeat(64)))).unwrap_err().contains("SHA-256"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn resolve_known_repo_aliases() {
        assert_eq!(resolve_repo_id("tiny"), "onnx-community/whisper-tiny");
        assert_eq!(resolve_repo_id("base"), "onnx-community/whisper-base");
        assert_eq!(resolve_repo_id("small"), "onnx-community/whisper-small");
        assert_eq!(resolve_repo_id("medium"), "onnx-community/whisper-medium-ONNX");
        assert_eq!(resolve_repo_id("medium.en"), "onnx-community/whisper-medium-ONNX");
        assert_eq!(resolve_repo_id("turbo"), "onnx-community/whisper-large-v3-turbo");
        assert_eq!(
            resolve_repo_id("custom/my-whisper"),
            "custom/my-whisper"
        );
    }

    #[test]
    fn cache_dir_sanitizes_slashes() {
        let dir = resolve_cache_dir(Some("E:/custom_cache"), "onnx-community/whisper-base");
        assert!(dir.to_string_lossy().contains("onnx-community--whisper-base"));
    }

    #[test]
    fn resolve_target_precision_combinations() {
        assert_eq!(resolve_target_precision("cuda", None), ModelPrecision::Fp16);
        assert_eq!(resolve_target_precision("cuda", Some("auto")), ModelPrecision::Fp16);
        assert_eq!(resolve_target_precision("cuda", Some("default")), ModelPrecision::Fp16);
        assert_eq!(resolve_target_precision("gpu", None), ModelPrecision::Fp16);
        assert_eq!(resolve_target_precision("directml", None), ModelPrecision::Fp16);
        assert_eq!(resolve_target_precision("directml", Some("default")), ModelPrecision::Fp16);
        assert_eq!(resolve_target_precision("dml", Some("auto")), ModelPrecision::Fp16);
        assert_eq!(resolve_target_precision("cuda", Some("fp16")), ModelPrecision::Fp16);
        assert_eq!(resolve_target_precision("cuda", Some("float16")), ModelPrecision::Fp16);
        assert_eq!(resolve_target_precision("cuda", Some("int8_float16")), ModelPrecision::Fp16);
        assert_eq!(resolve_target_precision("cuda", Some("float32")), ModelPrecision::Fp32);
        assert_eq!(resolve_target_precision("cuda", Some("fp32")), ModelPrecision::Fp32);
        assert_eq!(resolve_target_precision("cuda", Some("int8")), ModelPrecision::Int8);
        assert_eq!(resolve_target_precision("cuda", Some("uint8")), ModelPrecision::Int8);

        assert_eq!(resolve_target_precision("cpu", None), ModelPrecision::Int8);
        assert_eq!(resolve_target_precision("cpu", Some("auto")), ModelPrecision::Int8);
        assert_eq!(resolve_target_precision("cpu", Some("int8")), ModelPrecision::Int8);
        assert_eq!(resolve_target_precision("cpu", Some("uint8")), ModelPrecision::Int8);
        assert_eq!(resolve_target_precision("cpu", Some("float32")), ModelPrecision::Fp32);
        assert_eq!(resolve_target_precision("cpu", Some("fp32")), ModelPrecision::Fp32);
        assert_eq!(resolve_target_precision("cpu", Some("float16")), ModelPrecision::Fp16);
        assert_eq!(resolve_target_precision("cpu", Some("fp16")), ModelPrecision::Fp16);
    }

    #[test]
    fn part_path_appends_to_the_full_file_name() {
        // `encoder_model.onnx` and `encoder_model.onnx_data` must never share one partial file.
        assert_eq!(
            part_path(Path::new("cache/onnx/encoder_model.onnx")),
            PathBuf::from("cache/onnx/encoder_model.onnx.part")
        );
        assert_ne!(
            part_path(Path::new("cache/onnx/encoder_model.onnx")),
            part_path(Path::new("cache/onnx/encoder_model.onnx_data"))
        );
    }

    #[test]
    fn body_start_of_a_full_response_is_zero() {
        assert_eq!(body_start(StatusCode::OK, None), Some(0));
    }

    #[test]
    fn body_start_of_a_partial_response_is_its_range_start() {
        assert_eq!(
            body_start(StatusCode::PARTIAL_CONTENT, Some("bytes 100-999/1000")),
            Some(100)
        );
    }

    #[test]
    fn body_start_rejects_responses_it_cannot_place() {
        assert_eq!(body_start(StatusCode::PARTIAL_CONTENT, None), None);
        assert_eq!(body_start(StatusCode::PARTIAL_CONTENT, Some("items 1-2/3")), None);
        assert_eq!(body_start(StatusCode::RANGE_NOT_SATISFIABLE, Some("bytes */1000")), None);
    }

    #[test]
    fn verify_local_dir_handles_past_model_presence() {
        let tmp = std::env::temp_dir().join(format!("test_model_dir_{}", std::process::id()));
        let onnx_dir = tmp.join("onnx");
        std::fs::create_dir_all(&onnx_dir).unwrap();

        std::fs::write(tmp.join("config.json"), b"{}").unwrap();
        std::fs::write(tmp.join("preprocessor_config.json"), b"{}").unwrap();
        std::fs::write(tmp.join("tokenizer.json"), b"{}").unwrap();
        std::fs::write(onnx_dir.join("encoder_model.onnx"), b"fake_enc").unwrap();
        std::fs::write(onnx_dir.join("decoder_model.onnx"), b"fake_dec").unwrap();

        let resolved_without_past = verify_local_dir(&tmp, false).expect("valid dir without past");
        assert!(resolved_without_past.decoder_with_past_onnx.is_none());

        std::fs::write(onnx_dir.join("decoder_with_past_model.onnx"), b"fake_past").unwrap();
        let resolved_with_past = verify_local_dir(&tmp, false).expect("valid dir with past");
        assert!(resolved_with_past.decoder_with_past_onnx.is_some());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn verify_local_dir_with_precision_fp16() {
        let tmp = std::env::temp_dir().join(format!("test_fp16_model_dir_{}", std::process::id()));
        let onnx_dir = tmp.join("onnx");
        std::fs::create_dir_all(&onnx_dir).unwrap();

        std::fs::write(tmp.join("config.json"), b"{}").unwrap();
        std::fs::write(tmp.join("preprocessor_config.json"), b"{}").unwrap();
        std::fs::write(tmp.join("tokenizer.json"), b"{}").unwrap();
        std::fs::write(onnx_dir.join("encoder_model_fp16.onnx"), b"fake_enc_fp16").unwrap();
        std::fs::write(onnx_dir.join("decoder_model_fp16.onnx"), b"fake_dec_fp16").unwrap();
        std::fs::write(onnx_dir.join("decoder_with_past_model_fp16.onnx"), b"fake_past_fp16").unwrap();

        let strict_ok = verify_local_dir_with_precision(&tmp, ModelPrecision::Fp16, true).expect("strict fp16 ok");
        assert!(strict_ok.encoder_onnx.to_string_lossy().contains("fp16"));
        assert!(strict_ok.decoder_onnx.to_string_lossy().contains("fp16"));
        assert!(strict_ok.decoder_with_past_onnx.is_some());

        assert!(verify_local_dir_with_precision(&tmp, ModelPrecision::Fp32, true).is_err());
        let fallback_paths = verify_local_dir_with_precision(&tmp, ModelPrecision::Fp32, false).expect("fallback ok");
        assert!(fallback_paths.encoder_onnx.to_string_lossy().contains("fp16"));

        // Test fallback_cpu_paths when only fp16 is present (returns None)
        assert!(strict_ok.fallback_cpu_paths().is_none());

        // Now add quantized files to directory
        std::fs::write(onnx_dir.join("encoder_model_quantized.onnx"), b"fake_enc_int8").unwrap();
        std::fs::write(onnx_dir.join("decoder_model_quantized.onnx"), b"fake_dec_int8").unwrap();
        std::fs::write(onnx_dir.join("decoder_with_past_model_quantized.onnx"), b"fake_past_int8").unwrap();

        let cpu_paths = strict_ok.fallback_cpu_paths().expect("cpu fallback found");
        assert!(cpu_paths.encoder_onnx.to_string_lossy().contains("quantized"));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
