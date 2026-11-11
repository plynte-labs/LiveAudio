// SPDX-License-Identifier: MIT

//! Blocking Hugging Face download of a CTranslate2 Whisper model directory.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use liveaudio_worker_protocol::ProgressFn;
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::StatusCode;

/// No total timeout: a large model on a slow link must be allowed to finish. Blocking reqwest has no
/// read timeout, so every wait on the network is bounded per step instead (see `bounded`, `body_chunks`).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

use crate::model::{missing_files, preprocessor_repo};

/// `(repo, file)` pairs needed to complete a model directory, given the primary repo's file list.
pub fn select_files(repo_id: &str, siblings: &[String]) -> Result<Vec<(String, String)>, String> {
    let has = |name: &str| siblings.iter().any(|s| s == name);
    for required in ["model.bin", "config.json", "tokenizer.json"] {
        if !has(required) {
            return Err(format!("Repo {repo_id} has no {required}; it is not a CTranslate2 Whisper model"));
        }
    }
    let vocabulary = ["vocabulary.json", "vocabulary.txt"]
        .into_iter()
        .find(|v| has(v))
        .ok_or_else(|| format!("Repo {repo_id} has no vocabulary.json/vocabulary.txt"))?;
    let pre_repo = if has("preprocessor_config.json") {
        repo_id.to_string()
    } else {
        preprocessor_repo(repo_id)
    };
    let own = |f: &str| (repo_id.to_string(), f.to_string());
    Ok(vec![
        own("config.json"),
        own("model.bin"),
        own("tokenizer.json"),
        own(vocabulary),
        (pre_repo, "preprocessor_config.json".to_string()),
    ])
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

/// Runs `step` on a helper thread and waits at most `idle` for its result. A stalled connect, header
/// wait or metadata request is abandoned instead of hanging the worker; the thread is left to finish.
fn bounded<T: Send + 'static>(idle: Duration, step: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(step());
    });
    rx.recv_timeout(idle)
        .map_err(|e| format!("No response within {idle:?}: {e}"))
}

/// Streams the body of `reader` as chunks read on a helper thread. Each chunk must arrive within
/// `idle`; after the first error the caller stops, which abandons the helper read.
fn body_chunks<R: Read + Send + 'static>(
    mut reader: R,
    idle: Duration,
) -> impl Iterator<Item = Result<Vec<u8>, String>> {
    let (tx, rx) = mpsc::sync_channel(4);
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(Ok(buf[..n].to_vec())).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                    break;
                }
            }
        }
    });
    std::iter::from_fn(move || match rx.recv_timeout(idle) {
        Ok(chunk) => Some(chunk),
        Err(mpsc::RecvTimeoutError::Timeout) => Some(Err(format!("No data within {idle:?}"))),
        Err(mpsc::RecvTimeoutError::Disconnected) => None,
    })
}

pub fn part_path(dest: &Path) -> PathBuf {
    let mut os = dest.as_os_str().to_owned();
    os.push(".part");
    PathBuf::from(os)
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent("LiveAudio/1.6.0")
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))
}

fn fetch_siblings(client: &reqwest::blocking::Client, repo_id: &str) -> Result<Vec<String>, String> {
    let url = format!("https://huggingface.co/api/models/{repo_id}");
    let response = client.get(&url).send().map_err(|e| format!("API request failed for {repo_id}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("HuggingFace API returned status {} for {repo_id}", response.status()));
    }
    let body = response.text().map_err(|e| format!("Failed to read model API response: {e}"))?;
    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Failed to parse model API response: {e}"))?;
    Ok(json["siblings"]
        .as_array()
        .map(|a| a.iter().filter_map(|s| s["rfilename"].as_str().map(String::from)).collect())
        .unwrap_or_default())
}

/// Ensures `cache_dir` holds a complete model, downloading what is missing (`.part` + rename).
pub fn prepare_model(
    repo_id: &str,
    cache_dir: &Path,
    local_files_only: bool,
    progress: &mut ProgressFn,
) -> Result<(), String> {
    if missing_files(cache_dir).is_empty() {
        return Ok(());
    }
    if local_files_only {
        return Err(format!(
            "Model {repo_id} is incomplete at {} (missing: {}) and local_files_only is true",
            cache_dir.display(),
            missing_files(cache_dir).join(", ")
        ));
    }
    std::fs::create_dir_all(cache_dir)
        .map_err(|e| format!("Failed to create cache directory {}: {e}", cache_dir.display()))?;

    let client = client()?;
    let api_client = client.clone();
    let api_repo = repo_id.to_string();
    let siblings = bounded(IDLE_TIMEOUT, move || fetch_siblings(&api_client, &api_repo))??;
    let files = select_files(repo_id, &siblings)?;
    let total = files.len();
    for (idx, (repo, file)) in files.iter().enumerate() {
        let dest = cache_dir.join(file);
        if std::fs::metadata(&dest).map(|m| m.is_file() && m.len() > 0).unwrap_or(false) {
            continue;
        }
        let label = format!("({}/{}) {}", idx + 1, total, file);
        download_file(&client, repo, file, &dest, &label, progress)?;
    }
    let still = missing_files(cache_dir);
    if still.is_empty() {
        Ok(())
    } else {
        Err(format!("Model {repo_id} is still incomplete after download (missing: {})", still.join(", ")))
    }
}

/// GET `url`, asking only for the bytes from `offset` on when a partial file is being resumed.
fn send_get(client: &reqwest::blocking::Client, url: &str, offset: u64) -> Result<reqwest::blocking::Response, String> {
    let mut request = client.get(url);
    if offset > 0 {
        request = request.header(RANGE, format!("bytes={offset}-"));
    }
    bounded(IDLE_TIMEOUT, move || request.send())?
        .map_err(|e| format!("Failed to start download from {url}: {e}"))
}

fn download_file(
    client: &reqwest::blocking::Client,
    repo: &str,
    file: &str,
    dest: &Path,
    label: &str,
    progress: &mut ProgressFn,
) -> Result<(), String> {
    let url = format!("https://huggingface.co/{repo}/resolve/main/{file}");
    let part = part_path(dest);
    progress("downloading", label, Some(0.0), Some(0), None, None, None);

    // A leftover .part is resumed with a Range request; the server then answers 206 (append) or 200 (restart).
    let mut offset = std::fs::metadata(&part).map_or(0, |m| m.len());
    let mut response = send_get(client, &url, offset)?;
    if offset > 0 && response.status() == StatusCode::RANGE_NOT_SATISFIABLE {
        // The partial file is not a usable prefix (it may already hold the whole file): start over.
        std::fs::remove_file(&part).map_err(|e| format!("Failed to discard {}: {e}", part.display()))?;
        offset = 0;
        response = send_get(client, &url, 0)?;
    }
    if !response.status().is_success() {
        return Err(format!("Download failed with status {} for {url}", response.status()));
    }
    let content_range = response.headers().get(CONTENT_RANGE).and_then(|v| v.to_str().ok());
    let start = body_start(response.status(), content_range)
        .filter(|start| *start == 0 || *start == offset)
        .ok_or_else(|| format!("Unexpected range response for {url}"))?;
    let total = response.content_length().map(|len| start + len);

    let mut out = if start > 0 {
        OpenOptions::new().append(true).open(&part)
    } else {
        File::create(&part)
    }
    .map_err(|e| format!("Failed to create {}: {e}", part.display()))?;
    let mut downloaded = start;
    let (mut last_bytes, mut last) = (start, Instant::now());
    for chunk in body_chunks(response, IDLE_TIMEOUT) {
        let chunk = chunk.map_err(|e| format!("Chunk download error for {url}: {e}"))?;
        out.write_all(&chunk).map_err(|e| format!("Write failed for {}: {e}", part.display()))?;
        downloaded += chunk.len() as u64;
        let elapsed = last.elapsed();
        if elapsed.as_millis() >= 200 {
            // Same units as the ORT worker: speed in bits per second.
            let speed = (((downloaded - last_bytes) as f64 * 8.0) / elapsed.as_secs_f64()) as u64;
            let percent = total.map(|t| downloaded as f64 / t as f64 * 100.0);
            let eta = total.and_then(|t| (speed > 0).then(|| t.saturating_sub(downloaded) as f64 * 8.0 / speed as f64));
            progress("downloading", label, percent, Some(downloaded), total, Some(speed), eta);
            last = Instant::now();
            last_bytes = downloaded;
        }
    }
    out.flush().map_err(|e| format!("Flush failed for {}: {e}", part.display()))?;
    drop(out);
    // Keep the .part on a short download so the next attempt resumes it.
    if let Some(t) = total {
        if downloaded != t {
            return Err(format!("Incomplete download of {file}: got {downloaded} of {t} bytes"));
        }
    }
    std::fs::rename(&part, dest).map_err(|e| format!("Failed to commit {}: {e}", dest.display()))?;
    progress("downloading", label, Some(100.0), Some(downloaded), Some(downloaded), None, Some(0.0));
    Ok(())
}

// SPDX-License-Identifier: MIT

#[cfg(test)]
mod tests {
    use super::*;

    fn sib(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn small_repo_takes_preprocessor_from_openai_and_txt_vocabulary() {
        let files = select_files(
            "Systran/faster-whisper-small",
            &sib(&["config.json", "model.bin", "tokenizer.json", "vocabulary.txt", "README.md"]),
        )
        .unwrap();
        let got: Vec<(&str, &str)> = files.iter().map(|(r, f)| (r.as_str(), f.as_str())).collect();
        assert_eq!(
            got,
            vec![
                ("Systran/faster-whisper-small", "config.json"),
                ("Systran/faster-whisper-small", "model.bin"),
                ("Systran/faster-whisper-small", "tokenizer.json"),
                ("Systran/faster-whisper-small", "vocabulary.txt"),
                ("openai/whisper-small", "preprocessor_config.json"),
            ]
        );
    }

    #[test]
    fn large_repo_uses_own_preprocessor_and_json_vocabulary() {
        let files = select_files(
            "Systran/faster-whisper-large-v3",
            &sib(&["config.json", "model.bin", "preprocessor_config.json", "tokenizer.json", "vocabulary.json"]),
        )
        .unwrap();
        assert!(files.iter().all(|(r, _)| r == "Systran/faster-whisper-large-v3"));
        assert!(files.iter().any(|(_, f)| f == "vocabulary.json"));
    }

    #[test]
    fn repo_without_model_bin_is_rejected() {
        let err = select_files("a/b", &sib(&["config.json", "tokenizer.json", "vocabulary.txt"])).unwrap_err();
        assert!(err.contains("model.bin"));
    }

    #[test]
    fn body_start_of_a_full_response_is_zero() {
        assert_eq!(body_start(reqwest::StatusCode::OK, None), Some(0));
    }

    #[test]
    fn body_start_of_a_partial_response_is_its_range_start() {
        assert_eq!(
            body_start(reqwest::StatusCode::PARTIAL_CONTENT, Some("bytes 100-999/1000")),
            Some(100)
        );
    }

    #[test]
    fn body_start_rejects_responses_it_cannot_place() {
        assert_eq!(body_start(reqwest::StatusCode::PARTIAL_CONTENT, None), None);
        assert_eq!(
            body_start(reqwest::StatusCode::RANGE_NOT_SATISFIABLE, Some("bytes */1000")),
            None
        );
    }

    #[test]
    fn bounded_returns_the_step_result_in_time() {
        assert_eq!(bounded(Duration::from_secs(5), || 7), Ok(7));
    }

    #[test]
    fn bounded_gives_up_on_a_step_that_outlives_the_idle_limit() {
        let started = Instant::now();
        let result = bounded(Duration::from_millis(50), || {
            std::thread::sleep(Duration::from_secs(5))
        });
        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn body_chunks_yields_the_whole_body_then_ends() {
        let body: Vec<u8> = body_chunks(&b"hello world"[..], Duration::from_secs(5))
            .map(|chunk| chunk.expect("chunk"))
            .flatten()
            .collect();
        assert_eq!(body, b"hello world");
    }

    /// Sends one chunk, then blocks until the test releases it.
    struct Stalled {
        sent: bool,
        gate: mpsc::Receiver<()>,
    }

    impl Read for Stalled {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if !self.sent {
                self.sent = true;
                buf[..3].copy_from_slice(b"abc");
                return Ok(3);
            }
            let _ = self.gate.recv();
            Ok(0)
        }
    }

    #[test]
    fn body_chunks_fails_when_the_body_stalls() {
        let (release, gate) = mpsc::channel::<()>();
        let mut chunks = body_chunks(Stalled { sent: false, gate }, Duration::from_millis(100));
        assert_eq!(chunks.next(), Some(Ok(b"abc".to_vec())));
        assert!(matches!(chunks.next(), Some(Err(_))));
        drop(release);
    }

    #[test]
    fn part_path_appends_suffix() {
        let p = part_path(std::path::Path::new("C:/m/model.bin"));
        assert_eq!(p, std::path::PathBuf::from("C:/m/model.bin.part"));
    }
}
