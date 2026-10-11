// SPDX-License-Identifier: MIT

/// Check whether a given parent process PID is still running.
#[cfg(windows)]
pub fn is_parent_alive(parent_pid: u32) -> bool {
    if parent_pid == 0 {
        return false;
    }
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    const STILL_ACTIVE: u32 = 259;

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, parent_pid);
        if handle.is_null() {
            return false;
        }

        let mut exit_code: u32 = 0;
        let res = GetExitCodeProcess(handle, &mut exit_code);
        CloseHandle(handle);

        res != 0 && exit_code == STILL_ACTIVE
    }
}

#[cfg(unix)]
pub fn is_parent_alive(parent_pid: u32) -> bool {
    if parent_pid == 0 {
        return false;
    }
    // Sending signal 0 checks for process existence without affecting it
    unsafe { libc::kill(parent_pid as i32, 0) == 0 }
}

#[cfg(all(test, unix))]
mod tests {
    use super::is_parent_alive;

    #[test]
    fn zero_parent_pid_is_not_alive() {
        assert!(!is_parent_alive(0));
    }
}
