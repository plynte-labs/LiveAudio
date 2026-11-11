// SPDX-License-Identifier: MIT
//! Verified, resumable file download and selective zip extraction.

use std::path::{Path, PathBuf};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("http: {0}")]
    Http(String),
    #[error("unexpected HTTP status {0}")]
    Status(u16),
    #[error("sha256 mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("size mismatch: expected {expected}, got {actual}")]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("download cancelled")]
    Cancelled,
    #[error("zip: {0}")]
    Zip(String),
    #[error("member not found in archive: {0}")]
    MemberNotFound(String),
}

/// Downloads `url` to `dest` through `<dest>.part`, resuming with an HTTP Range request when a
/// partial file exists. The full file is SHA-256 verified before the atomic rename; a mismatch
/// deletes the partial file. `progress(downloaded, total)` reports `total == 0` when unknown.
pub async fn download_verified(
    url: &str,
    dest: &Path,
    expected_sha256: &str,
    expected_size: Option<u64>,
    mut progress: impl FnMut(u64, u64),
    cancel: &AtomicBool,
) -> Result<(), DownloadError> {
    use tokio::io::AsyncWriteExt;
    let mut part_name = dest.as_os_str().to_owned();
    part_name.push(".part");
    let part = PathBuf::from(part_name);
    if let Some(dir) = dest.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let mut have = tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);
    let client = reqwest::Client::new();
    let send = |from: u64| {
        let mut req = client.get(url);
        if from > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={from}-"));
        }
        async move { req.send().await.map_err(|e| DownloadError::Http(e.to_string())) }
    };
    let mut resp = send(have).await?;
    if resp.status().as_u16() == 416 {
        // Stale or oversized partial file: discard it and restart.
        have = 0;
        resp = send(0).await?;
    }
    let status = resp.status().as_u16();
    if !matches!(status, 200 | 206) {
        return Err(DownloadError::Status(status));
    }
    if status == 200 {
        have = 0; // server ignored Range: restart from scratch
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(have > 0)
        .truncate(have == 0)
        .open(&part)
        .await?;
    let total = resp.content_length().map_or(0, |l| l + have);
    let mut done = have;
    while let Some(chunk) = resp.chunk().await.map_err(|e| DownloadError::Http(e.to_string()))? {
        if cancel.load(Ordering::Relaxed) {
            return Err(DownloadError::Cancelled);
        }
        file.write_all(&chunk).await?;
        done += chunk.len() as u64;
        progress(done, total);
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(DownloadError::Cancelled);
    }
    file.flush().await?;
    drop(file);

    let part_for_hash = part.clone();
    let (actual, size) = tokio::task::spawn_blocking(move || -> std::io::Result<(String, u64)> {
        let mut f = std::fs::File::open(&part_for_hash)?;
        let mut h = Sha256::new();
        let n = std::io::copy(&mut f, &mut h)?;
        Ok((format!("{:x}", h.finalize()), n))
    })
    .await
    .map_err(|e| DownloadError::Http(e.to_string()))??;
    let bad = if let Some(expected) = expected_size.filter(|e| *e != size) {
        Some(DownloadError::SizeMismatch { expected, actual: size })
    } else if !actual.eq_ignore_ascii_case(expected_sha256) {
        Some(DownloadError::HashMismatch { expected: expected_sha256.to_string(), actual })
    } else {
        None
    };
    if let Some(err) = bad {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(err);
    }
    tokio::fs::rename(&part, dest).await?;
    Ok(())
}

/// Extracts only the archive members whose *file name* is in `wanted`, writing each to
/// `dest_dir/<wanted name>` (archive directories and `..` components are ignored, so path
/// traversal is impossible). Returns the written paths in `wanted` order.
pub fn extract_zip_members(
    zip_path: &Path,
    dest_dir: &Path,
    wanted: &[&str],
) -> Result<Vec<PathBuf>, DownloadError> {
    extract_zip_members_capped(zip_path, dest_dir, wanted, MAX_MEMBER_BYTES)
}

/// Largest uncompressed member accepted (zip-bomb guard; cuBLASLt is about 0.7 GB).
const MAX_MEMBER_BYTES: u64 = 1_500_000_000;

fn extract_zip_members_capped(
    zip_path: &Path,
    dest_dir: &Path,
    wanted: &[&str],
    max_member_bytes: u64,
) -> Result<Vec<PathBuf>, DownloadError> {
    let zerr = |e: zip::result::ZipError| DownloadError::Zip(e.to_string());
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip_path)?).map_err(zerr)?;
    std::fs::create_dir_all(dest_dir)?;
    let mut out = Vec::with_capacity(wanted.len());
    for name in wanted {
        let index = (0..archive.len()).find(|&i| {
            archive.by_index_raw(i).is_ok_and(|e| {
                !e.is_dir() && e.name().rsplit(['/', '\\']).next() == Some(*name)
            })
        });
        let index = index.ok_or_else(|| DownloadError::MemberNotFound((*name).to_string()))?;
        let entry = archive.by_index(index).map_err(zerr)?;
        if entry.size() > max_member_bytes {
            return Err(DownloadError::Zip(format!("member {name} exceeds {max_member_bytes} bytes")));
        }
        out.push(write_member(entry, dest_dir, name, max_member_bytes)?);
    }
    Ok(out)
}

/// Copies one archive member to `dest_dir/name` through `<name>.part`. The part file is renamed
/// into place only after a complete copy within `max_member_bytes`; any failure removes it, so a
/// truncated member never keeps its final name.
fn write_member(
    reader: impl std::io::Read,
    dest_dir: &Path,
    name: &str,
    max_member_bytes: u64,
) -> Result<PathBuf, DownloadError> {
    let target = dest_dir.join(name);
    let part = dest_dir.join(format!("{name}.part"));
    let result = (|| -> Result<(), DownloadError> {
        let mut file = std::fs::File::create(&part)?;
        // Also bound the real output in case the declared size lies.
        let written = std::io::copy(&mut reader.take(max_member_bytes + 1), &mut file)?;
        if written > max_member_bytes {
            return Err(DownloadError::Zip(format!("member {name} exceeds {max_member_bytes} bytes")));
        }
        file.sync_all()?;
        drop(file);
        std::fs::rename(&part, &target)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result.map(|()| target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::io::Write;
    use std::sync::atomic::Ordering;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn sha(b: &[u8]) -> String {
        format!("{:x}", Sha256::digest(b))
    }

    fn payload() -> Vec<u8> {
        (0..200_000u32).map(|i| (i % 251) as u8).collect()
    }

    /// Minimal HTTP server. `honor_range=false` always answers 200 with the full body.
    async fn serve(body: Vec<u8>, honor_range: bool) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut s, _) = listener.accept().await.unwrap();
                let body = body.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = s.read(&mut buf).await.unwrap();
                    let req = String::from_utf8_lossy(&buf[..n]).to_lowercase();
                    let start = req
                        .lines()
                        .find_map(|l| l.strip_prefix("range: bytes="))
                        .and_then(|r| r.trim_end_matches('-').parse::<usize>().ok())
                        .filter(|_| honor_range);
                    let head = match start {
                        Some(st) => format!(
                            "HTTP/1.1 206 Partial Content\r\ncontent-length: {}\r\ncontent-range: bytes {}-{}/{}\r\nconnection: close\r\n\r\n",
                            body.len() - st, st, body.len() - 1, body.len()
                        ),
                        None => format!(
                            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                            body.len()
                        ),
                    };
                    let _ = s.write_all(head.as_bytes()).await;
                    let _ = s.write_all(&body[start.unwrap_or(0)..]).await;
                });
            }
        });
        format!("http://{addr}/f.bin")
    }

    fn part_of(dest: &Path) -> PathBuf {
        let mut p = dest.as_os_str().to_owned();
        p.push(".part");
        PathBuf::from(p)
    }

    fn tempdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("liveaudio-dl-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[tokio::test]
    async fn full_download_verifies_and_renames() {
        let body = payload();
        let url = serve(body.clone(), true).await;
        let dest = tempdir("full").join("f.bin");
        let mut last = (0, 0);
        download_verified(&url, &dest, &sha(&body), Some(body.len() as u64), |d, t| last = (d, t), &AtomicBool::new(false))
            .await
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        assert!(!part_of(&dest).exists());
        assert_eq!(last, (body.len() as u64, body.len() as u64));
    }

    #[tokio::test]
    async fn resumes_from_partial_file() {
        let body = payload();
        let url = serve(body.clone(), true).await;
        let dest = tempdir("resume").join("f.bin");
        std::fs::write(part_of(&dest), &body[..70_000]).unwrap();
        let mut first = None;
        download_verified(&url, &dest, &sha(&body), None, |d, _| { first.get_or_insert(d); }, &AtomicBool::new(false))
            .await
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        assert!(first.unwrap() > 70_000, "progress must start after the resumed offset");
    }

    #[tokio::test]
    async fn server_ignoring_range_restarts() {
        let body = payload();
        let url = serve(body.clone(), false).await;
        let dest = tempdir("restart").join("f.bin");
        std::fs::write(part_of(&dest), b"garbage-prefix").unwrap();
        download_verified(&url, &dest, &sha(&body), None, |_, _| {}, &AtomicBool::new(false))
            .await
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
    }

    #[tokio::test]
    async fn hash_mismatch_deletes_part() {
        let body = payload();
        let url = serve(body.clone(), true).await;
        let dest = tempdir("mismatch").join("f.bin");
        let err = download_verified(&url, &dest, &sha(b"other"), None, |_, _| {}, &AtomicBool::new(false))
            .await
            .unwrap_err();
        assert!(matches!(err, DownloadError::HashMismatch { .. }));
        assert!(!part_of(&dest).exists());
        assert!(!dest.exists());
    }

    #[tokio::test]
    async fn cancel_stops_and_keeps_part() {
        let body = payload();
        let url = serve(body.clone(), true).await;
        let dest = tempdir("cancel").join("f.bin");
        let cancel = AtomicBool::new(false);
        let err = download_verified(&url, &dest, &sha(&body), None, |_, _| cancel.store(true, Ordering::Relaxed), &cancel)
            .await
            .unwrap_err();
        assert!(matches!(err, DownloadError::Cancelled));
        assert!(!dest.exists());
        assert!(part_of(&dest).exists());
    }

    #[test]
    fn zip_extracts_only_wanted_names_without_traversal() {
        let dir = tempdir("zip");
        let zp = dir.join("a.zip");
        {
            let mut w = zip::ZipWriter::new(std::fs::File::create(&zp).unwrap());
            let o = zip::write::SimpleFileOptions::default();
            for (name, data) in [("pkg/bin/a.dll", &b"A"[..]), ("pkg/other.txt", b"X"), ("../evil.dll", b"E")] {
                w.start_file(name, o).unwrap();
                w.write_all(data).unwrap();
            }
            w.finish().unwrap();
        }
        let out = dir.join("out");
        let got = extract_zip_members(&zp, &out, &["a.dll", "evil.dll"]).unwrap();
        assert_eq!(got, vec![out.join("a.dll"), out.join("evil.dll")]);
        assert_eq!(std::fs::read(out.join("evil.dll")).unwrap(), b"E");
        assert!(!out.join("other.txt").exists());
        assert!(!dir.join("evil.dll").exists());
        assert!(matches!(
            extract_zip_members(&zp, &out, &["missing.dll"]),
            Err(DownloadError::MemberNotFound(_))
        ));
    }

    #[test]
    fn zip_failed_member_leaves_no_final_or_part_file() {
        let dir = tempdir("zipcorrupt");
        let zp = dir.join("a.zip");
        let payload = b"PAYLOAD-PAYLOAD-PAYLOAD-PAYLOAD".to_vec();
        {
            let mut w = zip::ZipWriter::new(std::fs::File::create(&zp).unwrap());
            let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            w.start_file("bad.dll", o).unwrap();
            w.write_all(&payload).unwrap();
            w.finish().unwrap();
        }
        // Flip one stored data byte: the copy writes the whole member, then the CRC check fails.
        let mut raw = std::fs::read(&zp).unwrap();
        let at = raw.windows(payload.len()).position(|w| w == payload.as_slice()).unwrap();
        raw[at + 3] ^= 0xFF;
        std::fs::write(&zp, raw).unwrap();
        let out = dir.join("out");
        assert!(matches!(extract_zip_members(&zp, &out, &["bad.dll"]), Err(DownloadError::Zip(_) | DownloadError::Io(_))));
        assert!(!out.join("bad.dll").exists());
        assert!(!out.join("bad.dll.part").exists());
    }

    #[test]
    fn zip_rejects_oversized_member() {
        let dir = tempdir("zipbomb");
        let zp = dir.join("a.zip");
        {
            let mut w = zip::ZipWriter::new(std::fs::File::create(&zp).unwrap());
            w.start_file("big.dll", zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(&[0u8; 100]).unwrap();
            w.finish().unwrap();
        }
        let out = dir.join("out");
        assert!(matches!(
            extract_zip_members_capped(&zp, &out, &["big.dll"], 10),
            Err(DownloadError::Zip(_))
        ));
        assert!(extract_zip_members_capped(&zp, &out, &["big.dll"], 100).is_ok());
    }
}
