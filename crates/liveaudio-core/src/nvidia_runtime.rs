// SPDX-License-Identifier: MIT
//! On-demand NVIDIA CUDA 12 runtime (cuBLAS + cuDNN) installer for the CTranslate2 worker.
//!
//! Nothing here runs unless the caller invokes `install_nvidia_runtime` after an explicit user
//! action. Archives are pinned by URL, SHA-256 and size; only the listed DLLs and the NVIDIA
//! license text are extracted into the runtime directory.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::download::{download_verified, extract_zip_members, DownloadError};

/// A plain pinned file (license text).
pub struct PinnedFile<'a> {
    pub url: &'a str,
    pub sha256: &'a str,
    pub size: u64,
    /// File name written into the runtime directory.
    pub name: &'a str,
}

/// A pinned zip archive from which only `members` are extracted.
pub struct Archive<'a> {
    pub url: &'a str,
    pub sha256: &'a str,
    pub size: u64,
    pub members: &'a [&'a str],
    pub license: PinnedFile<'a>,
}

pub struct Manifest<'a> {
    pub archives: &'a [Archive<'a>],
    /// The ct2 CUDA worker, downloaded straight to `dir/<name>` (no zip). `None` = nothing to fetch.
    pub worker: Option<PinnedFile<'a>>,
}

/// `packaging/ct2-worker.pin.json`: the worker attached to the GitHub release of `version`.
#[derive(Debug, serde::Deserialize)]
pub struct WorkerPin {
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

impl WorkerPin {
    /// An all-zero sha256 is the placeholder committed until a release uploads the worker.
    pub fn is_published(&self) -> bool {
        self.sha256.bytes().any(|b| b != b'0')
    }
}

pub fn parse_pin(json: &str) -> Result<WorkerPin, serde_json::Error> {
    serde_json::from_str(json)
}

fn worker_pin() -> &'static WorkerPin {
    static PIN: std::sync::OnceLock<WorkerPin> = std::sync::OnceLock::new();
    PIN.get_or_init(|| parse_pin(include_str!("../../../packaging/ct2-worker.pin.json")).expect("valid ct2-worker.pin.json"))
}

/// CUDA 12.8.2 redist (libcublas 12.8.5.5) and cuDNN 9.14.0.64 (cuda12 variant, the version
/// validated in H3). Source manifests: `cuda/redist/redistrib_12.8.2.json`,
/// `cudnn/redist/redistrib_9.14.0.json`.
pub const MANIFEST: Manifest<'static> = Manifest {
    archives: &[
        Archive {
            url: "https://developer.download.nvidia.com/compute/cuda/redist/libcublas/windows-x86_64/libcublas-windows-x86_64-12.8.5.5-archive.zip",
            sha256: "0a2beedd7c1203cb9de5e5ab11943d27e41ee5d18dc3810b21bcd75be7e57a05",
            size: 563_633_310,
            members: &["cublas64_12.dll", "cublasLt64_12.dll"],
            license: PinnedFile {
                url: "https://developer.download.nvidia.com/compute/cuda/redist/libcublas/LICENSE.txt",
                sha256: "e2c71babfd18a8e69542dd7e9ca018f9caa438094001a58e6bc4d8c999bf0d07",
                size: 63_021,
                name: "LICENSE-cublas.txt",
            },
        },
        Archive {
            url: "https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/windows-x86_64/cudnn-windows-x86_64-9.14.0.64_cuda12-archive.zip",
            sha256: "27db1cf50f830a1335de991cf1edde43afb4fc65c365dbf013245f1364ec2b12",
            size: 624_633_536,
            members: &[
                "cudnn64_9.dll",
                "cudnn_ops64_9.dll",
                "cudnn_cnn64_9.dll",
                "cudnn_adv64_9.dll",
                "cudnn_graph64_9.dll",
                "cudnn_heuristic64_9.dll",
                "cudnn_engines_precompiled64_9.dll",
                "cudnn_engines_runtime_compiled64_9.dll",
            ],
            license: PinnedFile {
                url: "https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/LICENSE.txt",
                sha256: "4fce5a3b1ecaf4d7d202478e969e4563c7220db2718a76067e286c0202e1195f",
                size: 29_662,
                name: "LICENSE-cudnn.txt",
            },
        },
    ],
    worker: None,
};

/// `MANIFEST` plus the pinned worker, when the pin file names a published one.
fn current_manifest() -> Manifest<'static> {
    let pin = worker_pin();
    let worker = pin.is_published().then(|| PinnedFile {
        url: &pin.url,
        sha256: &pin.sha256,
        size: pin.size,
        name: crate::runtime::CT2_WORKER_EXE,
    });
    Manifest { archives: MANIFEST.archives, worker }
}

/// NVIDIA license pages shown to the user before the download.
pub fn license_urls() -> Vec<String> {
    MANIFEST.archives.iter().map(|a| a.license.url.to_string()).collect()
}

const STAMP: &str = "installed.json";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NvidiaRuntimeStatus {
    pub installed: bool,
    pub missing: Vec<String>,
}

/// `%LOCALAPPDATA%\LiveAudio\runtime\cuda12` (large binaries stay out of roaming data);
/// `LIVEAUDIO_HOME` overrides the base like the rest of the app.
pub fn runtime_dir() -> PathBuf {
    let base = match std::env::var("LIVEAUDIO_HOME").ok().filter(|h| !h.trim().is_empty()) {
        Some(home) => PathBuf::from(home),
        None => std::env::var("LOCALAPPDATA")
            .map(|p| PathBuf::from(p).join("LiveAudio"))
            .unwrap_or_else(|_| crate::config::get_data_home()),
    };
    base.join("runtime").join("cuda12")
}

fn expected_stamp<'a>(m: &Manifest<'a>) -> Vec<&'a str> {
    m.archives.iter().map(|a| a.sha256).chain(m.worker.as_ref().map(|w| w.sha256)).collect()
}

pub fn runtime_status(dir: &Path, m: &Manifest) -> NvidiaRuntimeStatus {
    let mut missing: Vec<String> = m
        .archives
        .iter()
        .flat_map(|a| a.members.iter().copied().chain(std::iter::once(a.license.name)))
        .chain(m.worker.as_ref().map(|w| w.name))
        .filter(|n| !dir.join(n).is_file())
        .map(str::to_string)
        .collect();
    let stamp_ok = std::fs::read_to_string(dir.join(STAMP))
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .is_some_and(|got| got == expected_stamp(m));
    if !stamp_ok {
        missing.push(STAMP.to_string());
    }
    NvidiaRuntimeStatus { installed: missing.is_empty(), missing }
}

pub fn nvidia_runtime_status(dir: &Path) -> NvidiaRuntimeStatus {
    nvidia_runtime_status_with(dir, &current_manifest(), worker_pin().is_published())
}

/// Unpublished worker (placeholder pin): never installed, with the reason spelled out.
fn nvidia_runtime_status_with(dir: &Path, m: &Manifest, worker_published: bool) -> NvidiaRuntimeStatus {
    let mut s = runtime_status(dir, m);
    if !worker_published {
        s.missing.push(format!("{} (not published for this release)", crate::runtime::CT2_WORKER_EXE));
        s.installed = false;
    }
    s
}

fn file_sha256(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    std::io::copy(&mut std::fs::File::open(path).ok()?, &mut h).ok()?;
    Some(format!("{:x}", h.finalize()))
}

/// Downloads, verifies and extracts every archive in `m` into `dir`. `progress(done, total)`
/// aggregates bytes across all downloads. The stamp is written last.
pub async fn install_runtime(
    dir: &Path,
    m: &Manifest<'_>,
    mut progress: impl FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<(), DownloadError> {
    tokio::fs::create_dir_all(dir).await?;
    let old: Vec<String> = tokio::fs::read_to_string(dir.join(STAMP))
        .await
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    // The previous stamp stays until the new one is written, so a failed install never invalidates it.
    // An archive is skipped when the old stamp lists it or its license file matches the pin, and all members exist.
    let have = |a: &Archive| {
        let licensed = old.iter().any(|o| o == a.sha256) || file_sha256(&dir.join(a.license.name)).is_some_and(|h| h == a.license.sha256);
        licensed && a.members.iter().copied().chain([a.license.name]).all(|n| dir.join(n).is_file())
    };
    let downloads = dir.join("downloads");
    let total: u64 = m.archives.iter().map(|a| a.size + a.license.size).sum::<u64>() + m.worker.as_ref().map_or(0, |w| w.size);
    let mut base = 0u64;
    // The small pinned worker goes first so a missing or mismatched release asset fails before ~1.2 GB of archives.
    if let Some(w) = &m.worker {
        let path = dir.join(w.name);
        let matches = tokio::task::spawn_blocking(move || file_sha256(&path)).await.ok().flatten().is_some_and(|h| h == w.sha256);
        if !matches {
            download_verified(w.url, &dir.join(w.name), w.sha256, Some(w.size), |d, _| progress(d, total), cancel).await?;
        }
        base += w.size;
        progress(base, total);
    }
    for a in m.archives {
        let l = &a.license;
        if have(a) {
            base += l.size + a.size;
            progress(base, total);
            continue;
        }
        // A license left by an earlier install must not mark this archive complete. The new one is
        // staged and renamed into place only after every member is extracted.
        let _ = tokio::fs::remove_file(dir.join(l.name)).await;
        let license_tmp = downloads.join(format!("{}.license", &a.sha256[..16]));
        download_verified(l.url, &license_tmp, l.sha256, Some(l.size), |d, _| progress(base + d, total), cancel).await?;
        base += l.size;
        let zip = downloads.join(format!("{}.zip", &a.sha256[..16]));
        download_verified(a.url, &zip, a.sha256, Some(a.size), |d, _| progress(base + d, total), cancel).await?;
        base += a.size;
        let (zip2, dir2) = (zip.clone(), dir.to_path_buf());
        let members: Vec<String> = a.members.iter().map(|s| s.to_string()).collect();
        tokio::task::spawn_blocking(move || {
            let wanted: Vec<&str> = members.iter().map(String::as_str).collect();
            extract_zip_members(&zip2, &dir2, &wanted).map(|_| ())
        })
        .await
        .map_err(|e| DownloadError::Http(e.to_string()))??;
        let _ = tokio::fs::remove_file(&zip).await;
        tokio::fs::rename(&license_tmp, dir.join(l.name)).await?;
    }
    let _ = tokio::fs::remove_dir(&downloads).await;
    progress(total, total);
    let stamp = serde_json::to_string(&expected_stamp(m)).map_err(|e| DownloadError::Zip(e.to_string()))?;
    tokio::fs::write(dir.join(STAMP), stamp).await?;
    Ok(())
}

pub async fn install_nvidia_runtime(
    dir: &Path,
    progress: impl FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<(), DownloadError> {
    install_runtime(dir, &current_manifest(), progress, cancel).await
}

#[cfg(test)]
mod tests {
    use super::*;
    const REDIST: &str = "https://developer.download.nvidia.com/compute";
    use sha2::{Digest, Sha256};
    use std::io::Write;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn sha(b: &[u8]) -> String {
        format!("{:x}", Sha256::digest(b))
    }

    fn tempdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("liveaudio-nv-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn manifest_is_sane() {
        let mut names = std::collections::HashSet::new();
        for a in MANIFEST.archives {
            assert!(a.url.starts_with(REDIST) && a.url.ends_with(".zip"));
            for (sha, size) in [(a.sha256, a.size), (a.license.sha256, a.license.size)] {
                assert_eq!(sha.len(), 64);
                assert!(sha.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
                assert!(size > 0);
            }
            assert!(!a.members.is_empty());
            for n in a.members.iter().chain([&a.license.name]) {
                assert!(names.insert(*n), "duplicate {n}");
            }
        }
        assert_eq!(license_urls().len(), 2);
    }

    /// Serves `path -> body` on 200 for known paths, 404 otherwise.
    async fn serve(routes: Vec<(&'static str, Vec<u8>)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut s, _) = listener.accept().await.unwrap();
                let routes = routes.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = s.read(&mut buf).await.unwrap();
                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
                    let path = req.split_whitespace().nth(1).unwrap_or("").to_string();
                    let body = routes.iter().find(|(p, _)| *p == path).map(|(_, b)| b.clone());
                    let head = match &body {
                        Some(b) => format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", b.len()),
                        None => "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".to_string(),
                    };
                    let _ = s.write_all(head.as_bytes()).await;
                    if let Some(b) = body {
                        let _ = s.write_all(&b).await;
                    }
                });
            }
        });
        format!("http://{addr}")
    }

    fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (n, d) in files {
            w.start_file(*n, zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(d).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    struct Fixture {
        base: String,
        zip: Vec<u8>,
        lic: Vec<u8>,
    }

    async fn fixture() -> Fixture {
        let zip = zip_of(&[("p/bin/a64.dll", b"AAA"), ("p/bin/b64.dll", b"BBBB"), ("p/bin/skip.dll", b"S")]);
        let lic = b"license text".to_vec();
        let base = serve(vec![("/a.zip", zip.clone()), ("/lic.txt", lic.clone())]).await;
        Fixture { base, zip, lic }
    }

    /// Leaks one small manifest per call; tests are short-lived.
    fn manifest_for(f: &Fixture, sha_zip: &str, sha_lic: &str) -> Manifest<'static> {
        manifest_with(f, sha_zip, sha_lic, &["a64.dll", "b64.dll"])
    }

    fn manifest_with(f: &Fixture, sha_zip: &str, sha_lic: &str, members: &'static [&'static str]) -> Manifest<'static> {
        let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
        let archives: &'static [Archive<'static>] = Box::leak(Box::new([Archive {
            url: leak(format!("{}/a.zip", f.base)),
            sha256: leak(sha_zip.to_string()),
            size: f.zip.len() as u64,
            members,
            license: PinnedFile {
                url: leak(format!("{}/lic.txt", f.base)),
                sha256: leak(sha_lic.to_string()),
                size: f.lic.len() as u64,
                name: "LICENSE-test.txt",
            },
        }]));
        Manifest { archives, worker: None }
    }

    #[tokio::test]
    async fn install_extracts_members_license_and_stamp() {
        let f = fixture().await;
        let m = manifest_for(&f, &sha(&f.zip), &sha(&f.lic));
        let dir = tempdir("install");
        let mut last = (0, 0);
        install_runtime(&dir, &m, |d, t| last = (d, t), &AtomicBool::new(false)).await.unwrap();
        assert_eq!(std::fs::read(dir.join("a64.dll")).unwrap(), b"AAA");
        assert_eq!(std::fs::read(dir.join("b64.dll")).unwrap(), b"BBBB");
        assert!(!dir.join("skip.dll").exists());
        assert!(!dir.join("downloads").exists());
        assert_eq!(last.0, last.1);
        assert_eq!(last.1, (f.zip.len() + f.lic.len()) as u64);
        assert!(runtime_status(&dir, &m).installed);
    }

    #[tokio::test]
    async fn install_cancelled_leaves_no_stamp() {
        let f = fixture().await;
        let m = manifest_for(&f, &sha(&f.zip), &sha(&f.lic));
        let dir = tempdir("cancel");
        let err = install_runtime(&dir, &m, |_, _| {}, &AtomicBool::new(true)).await.unwrap_err();
        assert!(matches!(err, DownloadError::Cancelled));
        assert!(!runtime_status(&dir, &m).installed);
    }

    #[tokio::test]
    async fn install_rejects_bad_hash() {
        let f = fixture().await;
        let m = manifest_for(&f, &sha(b"wrong"), &sha(&f.lic));
        let dir = tempdir("badhash");
        let err = install_runtime(&dir, &m, |_, _| {}, &AtomicBool::new(false)).await.unwrap_err();
        assert!(matches!(err, DownloadError::HashMismatch { .. }));
        assert!(!dir.join("a64.dll").exists());
    }

    #[tokio::test]
    async fn status_detects_missing_stale_and_complete() {
        let f = Fixture { base: String::new(), zip: vec![0; 3], lic: vec![0; 3] };
        let m = manifest_for(&f, &"a".repeat(64), &"b".repeat(64));
        let dir = tempdir("status");
        let s = runtime_status(&dir, &m);
        assert!(!s.installed);
        assert_eq!(s.missing, vec!["a64.dll", "b64.dll", "LICENSE-test.txt", "installed.json"]);
        for n in ["a64.dll", "b64.dll", "LICENSE-test.txt"] {
            std::fs::write(dir.join(n), b"x").unwrap();
        }
        std::fs::write(dir.join("installed.json"), r#"["old"]"#).unwrap();
        assert_eq!(runtime_status(&dir, &m).missing, vec!["installed.json"]);
        std::fs::write(dir.join("installed.json"), format!(r#"["{}"]"#, "a".repeat(64))).unwrap();
        assert!(runtime_status(&dir, &m).installed);
    }

    fn with_worker(mut m: Manifest<'static>, f: &Fixture, body: &[u8]) -> Manifest<'static> {
        let url: &'static str = Box::leak(format!("{}/w.exe", f.base).into_boxed_str());
        let sha256: &'static str = Box::leak(sha(body).into_boxed_str());
        m.worker = Some(PinnedFile { url, sha256, size: body.len() as u64, name: "worker.exe" });
        m
    }

    #[tokio::test]
    async fn install_downloads_worker_and_stamps_it() {
        let mut f = fixture().await;
        let body = b"MZ-worker".to_vec();
        f.base = serve(vec![("/a.zip", f.zip.clone()), ("/lic.txt", f.lic.clone()), ("/w.exe", body.clone())]).await;
        let m = with_worker(manifest_for(&f, &sha(&f.zip), &sha(&f.lic)), &f, &body);
        let dir = tempdir("worker");
        let mut last = (0, 0);
        install_runtime(&dir, &m, |d, t| last = (d, t), &AtomicBool::new(false)).await.unwrap();
        assert_eq!(std::fs::read(dir.join("worker.exe")).unwrap(), body);
        assert_eq!(last.0, last.1);
        assert!(runtime_status(&dir, &m).installed);
        std::fs::remove_file(dir.join("worker.exe")).unwrap();
        assert_eq!(runtime_status(&dir, &m).missing, vec!["worker.exe"]);
    }

    #[tokio::test]
    async fn new_worker_reinstalls_only_what_changed() {
        let mut f = fixture().await;
        let body = b"MZ-worker".to_vec();
        f.base = serve(vec![("/a.zip", f.zip.clone()), ("/lic.txt", f.lic.clone()), ("/w.exe", body.clone())]).await;
        let dir = tempdir("delta");
        let first = manifest_for(&f, &sha(&f.zip), &sha(&f.lic));
        install_runtime(&dir, &first, |_, _| {}, &AtomicBool::new(false)).await.unwrap();
        // Same archive hashes, but the archive/license URLs are now unreachable: only the worker may download.
        let dead = Fixture { base: "http://127.0.0.1:1".into(), zip: f.zip.clone(), lic: f.lic.clone() };
        let second = with_worker(manifest_for(&dead, &sha(&f.zip), &sha(&f.lic)), &f, &body);
        assert!(!runtime_status(&dir, &second).installed, "stamp differs once a worker is pinned");
        install_runtime(&dir, &second, |_, _| {}, &AtomicBool::new(false)).await.unwrap();
        assert!(runtime_status(&dir, &second).installed);
    }

    #[tokio::test]
    async fn missing_worker_asset_fails_before_large_archives() {
        let f = fixture().await; // archives reachable, /w.exe is 404
        let m = with_worker(manifest_for(&f, &sha(&f.zip), &sha(&f.lic)), &f, b"MZ-worker");
        let dir = tempdir("workerfirst");
        install_runtime(&dir, &m, |_, _| {}, &AtomicBool::new(false)).await.unwrap_err();
        assert!(!dir.join("a64.dll").exists(), "archives must not be fetched before the worker is verified");
    }

    #[test]
    fn placeholder_pin_means_worker_not_published() {
        let p = parse_pin(include_str!("../../../packaging/ct2-worker.pin.json")).unwrap();
        let ph = parse_pin(&format!(r#"{{"version":"1.0.0","url":"u","sha256":"{}","size":0}}"#, "0".repeat(64))).unwrap();
        assert!(!ph.is_published());
        let real = parse_pin(&format!(r#"{{"version":"1.0.0","url":"u","sha256":"{}","size":5}}"#, "ab".repeat(32))).unwrap();
        assert!(real.is_published());
        assert!(parse_pin("{}").is_err());
        let _ = p;
    }

    #[test]
    fn tracked_pin_is_placeholder_or_valid_release_pin() {
        let p = parse_pin(include_str!("../../../packaging/ct2-worker.pin.json")).unwrap();
        assert_eq!(p.version, env!("CARGO_PKG_VERSION"));
        if p.is_published() {
            assert_eq!(p.sha256.len(), 64);
            assert!(p.sha256.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
            assert!(p.size > 0);
            assert_eq!(
                p.url,
                format!("https://github.com/plynte-labs/LiveAudio/releases/download/v{}/liveaudio-asr-worker-cuda.exe", p.version)
            );
        }
    }

    #[test]
    fn status_reports_unpublished_worker_with_reason() {
        let dir = tempdir("unpub");
        let s = nvidia_runtime_status_with(&dir, &MANIFEST, false);
        assert!(!s.installed);
        assert!(s.missing.iter().any(|m| m.contains("liveaudio-asr-worker-cuda.exe") && m.contains("not published")), "{:?}", s.missing);
    }

    /// Files already on disk and matching their pins, but no stamp, and every URL answers 404 or is unreachable.
    async fn seed_valid_files(dir: &Path, f: &Fixture) {
        std::fs::write(dir.join("a64.dll"), b"AAA").unwrap();
        std::fs::write(dir.join("b64.dll"), b"BBBB").unwrap();
        std::fs::write(dir.join("LICENSE-test.txt"), &f.lic).unwrap();
        std::fs::write(dir.join("worker.exe"), b"MZ-worker").unwrap();
    }

    #[tokio::test]
    async fn valid_files_without_stamp_install_offline() {
        let f = fixture().await;
        let dead = Fixture { base: serve(vec![]).await, zip: f.zip.clone(), lic: f.lic.clone() };
        let m = with_worker(manifest_for(&dead, &sha(&f.zip), &sha(&f.lic)), &dead, b"MZ-worker");
        let dir = tempdir("offline");
        seed_valid_files(&dir, &f).await;
        assert!(!runtime_status(&dir, &m).installed);
        install_runtime(&dir, &m, |_, _| {}, &AtomicBool::new(false)).await.unwrap();
        assert!(runtime_status(&dir, &m).installed);
    }

    #[tokio::test]
    async fn failed_install_keeps_previous_valid_stamp() {
        let f = fixture().await;
        let dead = Fixture { base: serve(vec![]).await, zip: f.zip.clone(), lic: f.lic.clone() };
        let m = manifest_for(&dead, &sha(&f.zip), &sha(&f.lic));
        let dir = tempdir("keepstamp");
        std::fs::write(dir.join("installed.json"), format!(r#"["{}"]"#, sha(&f.zip))).unwrap();
        // Worker file is corrupt, so it must download; the URL is 404.
        std::fs::write(dir.join("worker.exe"), b"corrupt").unwrap();
        let m = with_worker(m, &dead, b"MZ-worker");
        let err = install_runtime(&dir, &m, |_, _| {}, &AtomicBool::new(false)).await.unwrap_err();
        assert!(matches!(err, DownloadError::Status(404)), "{err:?}");
        assert_eq!(std::fs::read_to_string(dir.join("installed.json")).unwrap(), format!(r#"["{}"]"#, sha(&f.zip)));
    }

    #[tokio::test]
    async fn failed_extraction_removes_stale_license_so_retry_reextracts() {
        let f = fixture().await;
        let dir = tempdir("nolicense");
        std::fs::write(dir.join("LICENSE-test.txt"), &f.lic).unwrap();
        // "missing.dll" is not in the zip: extraction fails after "a64.dll" was written.
        let broken = manifest_with(&f, &sha(&f.zip), &sha(&f.lic), &["a64.dll", "missing.dll"]);
        let err = install_runtime(&dir, &broken, |_, _| {}, &AtomicBool::new(false)).await.unwrap_err();
        assert!(matches!(err, DownloadError::MemberNotFound(_)), "{err:?}");
        assert!(!dir.join("LICENSE-test.txt").exists(), "a failed extraction must not leave the license");
        let m = manifest_for(&f, &sha(&f.zip), &sha(&f.lic));
        install_runtime(&dir, &m, |_, _| {}, &AtomicBool::new(false)).await.unwrap();
        assert_eq!(std::fs::read(dir.join("b64.dll")).unwrap(), b"BBBB");
        assert!(runtime_status(&dir, &m).installed);
    }
}
