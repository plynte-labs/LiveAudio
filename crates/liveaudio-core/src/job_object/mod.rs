// SPDX-License-Identifier: MIT

//! Windows Job Object wrapper guaranteeing zero zombie child processes.
//!
//! Configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, ensuring that whenever
//! the parent process terminates, crashes, or is killed, Windows kernel immediately
//! terminates all associated child processes.

#[cfg(windows)]
mod windows_impl {
    use std::io;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    pub struct JobObject {
        handle: HANDLE,
    }

    // Safety: Windows kernel job object handles are safe to send and share across threads.
    unsafe impl Send for JobObject {}
    unsafe impl Sync for JobObject {}

    impl JobObject {
        /// Creates a new Windows Job Object configured to kill all processes when closed.
        pub fn create() -> io::Result<Self> {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return Err(io::Error::last_os_error());
                }

                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

                let res = SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const std::ffi::c_void,
                    std::mem::size_of_val(&info) as u32,
                );

                if res == 0 {
                    let err = io::Error::last_os_error();
                    CloseHandle(handle);
                    return Err(err);
                }

                Ok(Self { handle })
            }
        }

        /// Assign a running standard library child process to the Job Object.
        pub fn assign_child(&self, child: &std::process::Child) -> io::Result<()> {
            let raw_handle = child.as_raw_handle() as HANDLE;
            unsafe {
                let res = AssignProcessToJobObject(self.handle, raw_handle);
                if res == 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(())
                }
            }
        }

        /// Assign an arbitrary process handle to the Job Object.
        pub fn assign_raw_handle(&self, raw_handle: HANDLE) -> io::Result<()> {
            unsafe {
                let res = AssignProcessToJobObject(self.handle, raw_handle);
                if res == 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(())
                }
            }
        }

        /// Assign a process by PID to the Job Object.
        pub fn assign_process_id(&self, pid: u32) -> io::Result<()> {
            use windows_sys::Win32::System::Threading::{
                OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
            };
            unsafe {
                let proc = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
                if proc.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let res = AssignProcessToJobObject(self.handle, proc);
                CloseHandle(proc);
                if res == 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(())
                }
            }
        }
    }

    impl Drop for JobObject {
        fn drop(&mut self) {
            unsafe {
                if !self.handle.is_null() {
                    CloseHandle(self.handle);
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod non_windows_impl {
    use std::io;

    pub struct JobObject;

    impl JobObject {
        pub fn create() -> io::Result<Self> {
            Ok(Self)
        }

        pub fn assign_child(&self, _child: &std::process::Child) -> io::Result<()> {
            Ok(())
        }

        pub fn assign_process_id(&self, _pid: u32) -> io::Result<()> {
            Ok(())
        }
    }
}

#[cfg(windows)]
pub use windows_impl::JobObject;

#[cfg(not(windows))]
pub use non_windows_impl::JobObject;
