//! Windows Job Object confinement (WIN-04, REQ-EV-0221).
//!
//! Every session child is assigned to a job object with
//! `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, so the whole tree a command started
//! — a ConPTY shell's children, a detached `start /b` grandchild — ends with
//! the session: at cancel/timeout by [`Job::terminate`], at natural exit by
//! the broker taking the job in `finish`, and when this broker dies at all,
//! by the kernel closing the job's last handle. This is the Windows answer
//! to the Unix process group (`process_group(0)` + `killpg`), inside the
//! existing broker — not a second process manager (docs/21).

// SAFETY exception (workspace `unsafe_code = "deny"`, as for the Unix
// killpg in broker.rs): this module's unsafe blocks wrap Windows job-object
// calls on handles it created itself, and its unsafe trait impls move an
// opaque kernel handle across threads. Each item carries its own SAFETY note.
#[cfg(windows)]
#[allow(unsafe_code)]
mod imp {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA};

    /// An owned job handle. Dropping it closes the job; because the job's
    /// only handle is owned here, the kernel ends every confined process
    /// (`KILL_ON_JOB_CLOSE`) — the broker-crash safety net.
    pub(crate) struct Job(HANDLE);

    // SAFETY: a HANDLE is an opaque kernel object id, not a pointer into
    // memory; moving it across threads and closing it from another thread is
    // governed by the kernel's reference count, not by thread ownership.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    /// Confine `pid` — and everything it spawns from now on — to a new job
    /// that dies with the returned handle. `None` only when a Windows call
    /// refuses (the session then stays unconfined and the explicit kill
    /// paths still run `taskkill /T`, as before).
    pub(crate) fn confine(pid: u32) -> Option<Job> {
        // SAFETY exception (workspace `unsafe_code = "deny"`, noted in
        // Cargo.toml): job-object calls on handles this function created or
        // opened itself, on a pid the broker spawned.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                eprintln!(
                    "modbit-execd: job: CreateJobObjectW failed: {}",
                    std::io::Error::last_os_error()
                );
                return None;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let shaped = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if shaped == 0 {
                eprintln!(
                    "modbit-execd: job: SetInformationJobObject failed: {}",
                    std::io::Error::last_os_error()
                );
                CloseHandle(job);
                return None;
            }
            let proc = OpenProcess(PROCESS_SET_QUOTA, 0, pid);
            if proc.is_null() {
                eprintln!(
                    "modbit-execd: job: OpenProcess({pid}) failed: {}",
                    std::io::Error::last_os_error()
                );
                CloseHandle(job);
                return None;
            }
            let assigned = AssignProcessToJobObject(job, proc);
            CloseHandle(proc);
            if assigned == 0 {
                eprintln!(
                    "modbit-execd: job: AssignProcessToJobObject({pid}) failed: {}",
                    std::io::Error::last_os_error()
                );
                CloseHandle(job);
                return None;
            }
            Some(Job(job))
        }
    }

    impl Job {
        /// End every process in the job now (cancel, timeout, session exit).
        pub(crate) fn terminate(&self) {
            // SAFETY: the handle is owned and was created by `confine`.
            unsafe {
                TerminateJobObject(self.0, 1);
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: owned handle; closing it is what makes the kernel end
            // the confined tree if this broker dies without a clean kill.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(windows)]
pub(crate) use imp::{Job, confine};

/// The Unix arm of the boundary is the process group set at spawn and
/// `killpg`'d on cancel (broker.rs); no job exists there.
#[cfg(not(windows))]
pub(crate) struct Job;

#[cfg(not(windows))]
pub(crate) fn confine(_pid: u32) -> Option<Job> {
    None
}

#[cfg(not(windows))]
impl Job {
    pub(crate) fn terminate(&self) {}
}
