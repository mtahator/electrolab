//! Child processes tied to the app's lifetime by a Job Object (ADR-009).
//!
//! The job has "kill on close": when the app exits in any way — even a crash or Task Manager —
//! Windows closes the job handle and ends every child. Children get no console window, null
//! stdin, and stdout/stderr only into a log file; no other handles are inherited.

use std::fs::OpenOptions;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};

use ep_os::{ChildProcess, OsError, ProcessSpawner, ProcessSpec, Result};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

/// Owns the job handle; dropping it (or the app exiting) ends all children.
#[derive(Debug)]
struct Job(HANDLE);

// SAFETY: a job handle is a kernel handle usable from any thread.
#[allow(unsafe_code)]
unsafe impl Send for Job {}
// SAFETY: as above; the Win32 job functions are thread-safe.
#[allow(unsafe_code)]
unsafe impl Sync for Job {}

impl Drop for Job {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: we own the handle and close it exactly once.
        unsafe { CloseHandle(self.0) };
    }
}

impl Job {
    #[allow(unsafe_code)]
    fn new() -> std::io::Result<Self> {
        // SAFETY: null attributes and name create an unnamed job with default security.
        let h = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if h.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let job = Self(h);
        // SAFETY: all-zero is a valid JOBOBJECT_EXTENDED_LIMIT_INFORMATION (no limits).
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let size = u32::try_from(std::mem::size_of_val(&info)).unwrap_or(u32::MAX);
        // SAFETY: `info` is the structure this information class expects, with its true size.
        let ok = unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&raw const info).cast(),
                size,
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(job)
    }

    #[allow(unsafe_code)]
    fn assign(&self, child: &Child) -> std::io::Result<()> {
        // SAFETY: both handles are valid for the duration of the call.
        let ok = unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle()) };
        if ok == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

/// Spawns children into one kill-on-close job.
#[derive(Debug)]
pub struct JobSpawner {
    job: Job,
}

impl JobSpawner {
    /// Create the job.
    ///
    /// # Errors
    /// Windows refused to create the job.
    pub fn new() -> Result<Self> {
        Ok(Self { job: Job::new()? })
    }
}

struct WinChild(Child);

impl ChildProcess for WinChild {
    fn id(&self) -> u32 {
        self.0.id()
    }
    fn try_wait(&mut self) -> Result<Option<i32>> {
        Ok(self.0.try_wait()?.map(|s| s.code().unwrap_or(-1)))
    }
    fn kill(&mut self) -> Result<()> {
        match self.0.kill() {
            Ok(()) => {
                let _ = self.0.wait();
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => Ok(()), // already exited
            Err(e) => Err(e.into()),
        }
    }
}

impl ProcessSpawner for JobSpawner {
    fn spawn(&self, spec: &ProcessSpec) -> Result<Box<dyn ChildProcess>> {
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args)
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null());
        if let Some(dir) = &spec.cwd {
            cmd.current_dir(dir);
        }
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        if let Some(log) = &spec.log_file {
            let f = OpenOptions::new().create(true).append(true).open(log)?;
            cmd.stdout(f.try_clone()?).stderr(f);
        } else {
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
        let mut child = cmd.spawn()?;
        if let Err(e) = self.job.assign(&child) {
            let _ = child.kill();
            return Err(OsError::Io(e));
        }
        Ok(Box::new(WinChild(child)))
    }
}
