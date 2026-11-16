//! OS-specific glue that the rest of the agent stays free of.

use std::process::Child;

/// Ties the worker's lifetime (and on Windows its memory) to the agent. Linux and macOS need
/// nothing here: the worker exits on stdin EOF, and on Linux also via PR_SET_PDEATHSIG.
#[cfg(not(windows))]
pub fn contain_worker(_child: &Child) {}

#[cfg(windows)]
pub fn contain_worker(child: &Child) {
    windows_job::assign(child);
}

#[cfg(windows)]
mod windows_job {
    //! A kill-on-close Job Object with a 1 GB process-memory cap: if the agent dies, Windows closes
    //! the job handle and kills the worker; allocations past the cap fail instead of swapping.
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    };

    struct Job(isize);
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    fn job() -> Option<&'static Job> {
        static JOB: OnceLock<Option<Job>> = OnceLock::new();
        JOB.get_or_init(|| unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return None;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_PROCESS_MEMORY;
            info.ProcessMemoryLimit = 1024 * 1024 * 1024;
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            Some(Job(handle as isize)) // intentionally never closed: closing it kills the worker
        })
        .as_ref()
    }

    pub fn assign(child: &std::process::Child) {
        if let Some(job) = job() {
            unsafe {
                AssignProcessToJobObject(job.0 as _, child.as_raw_handle() as _);
            }
        }
    }
}
