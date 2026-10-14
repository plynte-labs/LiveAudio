// SPDX-License-Identifier: MIT

//! Single-owner service lock in the data home (Python `liveaudio/service/lock.py` parity).
//!
//! Whoever runs the service pipeline (desktop `start_service`, CLI `serve`) holds it. Exclusivity
//! comes from a non-blocking OS lock on byte 0 (the OS releases it when the owner exits); the
//! `{"pid": N}` content is diagnostic metadata rewritten by each new owner.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub const SERVICE_LOCK_NAME: &str = "service.lock";

/// Held service lock; released on drop.
#[derive(Debug)]
pub struct ServiceLock {
    file: File,
}

/// A live process currently holding the service lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceLockHolder {
    /// PID from the lock metadata; `None` when it is missing or unreadable.
    pub pid: Option<u32>,
}

impl ServiceLock {
    /// Takes the service lock in `dir` and records this process's PID.
    /// Returns `None` when another owner holds it; an existing lock is never deleted or replaced.
    pub fn try_acquire(dir: &Path) -> io::Result<Option<Self>> {
        std::fs::create_dir_all(dir)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join(SERVICE_LOCK_NAME))?;
        if !try_lock_first_byte(&file)? {
            return Ok(None);
        }
        let mut lock = Self { file };
        lock.write_pid(std::process::id())?;
        Ok(Some(lock))
    }

    fn write_pid(&mut self, pid: u32) -> io::Result<()> {
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        write!(self.file, "{{\"pid\": {pid}}}")?;
        self.file.flush()
    }
}

impl Drop for ServiceLock {
    fn drop(&mut self) {
        unlock_first_byte(&self.file);
    }
}

/// Reports the live holder of the service lock in `dir`, or `None` when nobody holds it.
/// The PID file alone is never trusted: a stale file left by a dead owner reports `None`.
pub fn service_lock_holder(dir: &Path) -> io::Result<Option<ServiceLockHolder>> {
    let mut file = match File::open(dir.join(SERVICE_LOCK_NAME)) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if try_lock_first_byte(&file)? {
        // Nobody holds it: release the probe lock (pid metadata, if any, is stale).
        unlock_first_byte(&file);
        return Ok(None);
    }
    Ok(Some(ServiceLockHolder {
        pid: read_pid(&mut file),
    }))
}

/// Reads the holder's PID. Byte 0 is the locked byte (unreadable on Windows while held) and is
/// always `{` in the `{"pid": N}` format, so the JSON is rebuilt from offset 1.
fn read_pid(file: &mut File) -> Option<u32> {
    let mut rest = String::new();
    file.seek(SeekFrom::Start(1)).ok()?;
    file.read_to_string(&mut rest).ok()?;
    let value: serde_json::Value = serde_json::from_str(&format!("{{{rest}")).ok()?;
    u32::try_from(value.get("pid")?.as_u64()?).ok()
}

/// Locks byte 0 like Python `msvcrt.locking(..., 1)`, so the two implementations exclude
/// each other and the rest of the file stays readable for the holder query.
#[cfg(windows)]
fn try_lock_first_byte(file: &File) -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::ERROR_LOCK_VIOLATION;
    use windows_sys::Win32::Storage::FileSystem::{
        LockFileEx, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY,
    };
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let mut overlapped = OVERLAPPED::default();
    // SAFETY: the handle is owned by `file` and outlives the call; `overlapped` is zeroed (offset 0).
    let locked = unsafe {
        LockFileEx(
            file.as_raw_handle(),
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            0,
            1,
            0,
            &mut overlapped,
        )
    };
    if locked != 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(ERROR_LOCK_VIOLATION as i32) {
        Ok(false)
    } else {
        Err(error)
    }
}

#[cfg(windows)]
fn unlock_first_byte(file: &File) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::UnlockFileEx;
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let mut overlapped = OVERLAPPED::default();
    // SAFETY: same handle and range as `try_lock_first_byte`; failure only means it was not held.
    unsafe {
        UnlockFileEx(file.as_raw_handle(), 0, 1, 0, &mut overlapped);
    }
}

/// `flock` is per open file description, so a second open in the same process also conflicts.
#[cfg(not(windows))]
fn try_lock_first_byte(file: &File) -> io::Result<bool> {
    match file.try_lock() {
        Ok(()) => Ok(true),
        Err(std::fs::TryLockError::WouldBlock) => Ok(false),
        Err(std::fs::TryLockError::Error(error)) => Err(error),
    }
}

#[cfg(not(windows))]
fn unlock_first_byte(file: &File) {
    let _ = file.unlock();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_HOME: AtomicU64 = AtomicU64::new(0);

    fn isolated_home() -> PathBuf {
        let home = std::env::temp_dir().join(format!(
            "liveaudio-service-lock-test-{}-{}",
            std::process::id(),
            NEXT_HOME.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&home);
        home
    }

    #[test]
    fn service_lock_admits_a_single_owner_until_released() {
        let home = isolated_home();
        let first = ServiceLock::try_acquire(&home)
            .expect("lock file opens")
            .expect("first owner acquires the lock");
        assert!(
            ServiceLock::try_acquire(&home)
                .expect("lock file opens")
                .is_none(),
            "a second owner must not acquire a held lock"
        );
        drop(first);
        assert!(
            ServiceLock::try_acquire(&home)
                .expect("lock file opens")
                .is_some(),
            "the lock is released when its owner drops it"
        );
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn service_lock_writes_python_compatible_pid_metadata() {
        let home = isolated_home();
        let lock = ServiceLock::try_acquire(&home)
            .expect("lock file opens")
            .expect("owner acquires the lock");
        drop(lock);
        let content = std::fs::read_to_string(home.join(SERVICE_LOCK_NAME)).expect("read lock");
        assert_eq!(content, format!("{{\"pid\": {}}}", std::process::id()));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn holder_query_reports_live_holder_pid_and_nothing_after_release() {
        let home = isolated_home();
        assert_eq!(service_lock_holder(&home).expect("query"), None);

        let lock = ServiceLock::try_acquire(&home)
            .expect("lock file opens")
            .expect("owner acquires the lock");
        assert_eq!(
            service_lock_holder(&home).expect("query"),
            Some(ServiceLockHolder {
                pid: Some(std::process::id())
            })
        );
        assert!(
            ServiceLock::try_acquire(&home).expect("open").is_none(),
            "the query must not release or steal the holder's lock"
        );

        drop(lock);
        assert_eq!(
            service_lock_holder(&home).expect("query"),
            None,
            "a pid file without a live lock holder is stale"
        );
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn holder_query_ignores_stale_pid_file_and_new_owner_rewrites_it() {
        let home = isolated_home();
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(
            home.join(SERVICE_LOCK_NAME),
            "{\"pid\": 4294967295, \"extra\": 1}",
        )
        .unwrap();
        assert_eq!(service_lock_holder(&home).expect("query"), None);

        let _lock = ServiceLock::try_acquire(&home)
            .expect("lock file opens")
            .expect("stale metadata does not block a new owner");
        assert_eq!(
            service_lock_holder(&home).expect("query"),
            Some(ServiceLockHolder {
                pid: Some(std::process::id())
            })
        );
        drop(_lock);
        let _ = std::fs::remove_dir_all(home);
    }
}
