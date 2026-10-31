//! Process-tree control for supervised services.
//!
//! Apache on Windows runs a parent and a worker process; killing only the parent can
//! leave the worker holding the port. Each service therefore runs in its own Windows
//! Job Object (killed as a unit, and automatically if LampRabit itself dies) or, on
//! Unix, its own process group.

use tokio::process::{Child, Command};

pub struct ProcGroup {
    #[cfg(windows)]
    job: usize,
    #[cfg(unix)]
    pgid: i32,
}

/// Must be called on the command before spawning.
pub fn prepare(cmd: &mut Command) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        cmd.process_group(0);
    }
}

#[cfg(windows)]
impl ProcGroup {
    pub fn attach(child: &Child) -> std::io::Result<Self> {
        use std::{io::Error, mem, ptr};
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
            },
        };

        let process = child
            .raw_handle()
            .ok_or_else(|| Error::other("process already exited"))?;
        unsafe {
            let job = CreateJobObjectW(ptr::null(), ptr::null());
            if job.is_null() {
                return Err(Error::last_os_error());
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) != 0
                && AssignProcessToJobObject(job, process as _) != 0;
            if !ok {
                let err = Error::last_os_error();
                CloseHandle(job);
                return Err(err);
            }
            Ok(Self { job: job as usize })
        }
    }

    /// Windows console services have no portable "please stop" signal.
    pub fn signal_term(&self) -> bool {
        false
    }

    pub fn kill(&self) {
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job as _, 1);
        }
    }
}

#[cfg(windows)]
impl Drop for ProcGroup {
    fn drop(&mut self) {
        // KILL_ON_JOB_CLOSE takes down anything still in the job.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.job as _);
        }
    }
}

#[cfg(unix)]
impl ProcGroup {
    pub fn attach(child: &Child) -> std::io::Result<Self> {
        let pid = child
            .id()
            .ok_or_else(|| std::io::Error::other("process already exited"))?;
        Ok(Self { pgid: pid as i32 })
    }

    pub fn signal_term(&self) -> bool {
        unsafe { libc::kill(-self.pgid, libc::SIGTERM) == 0 }
    }

    pub fn kill(&self) {
        unsafe {
            libc::kill(-self.pgid, libc::SIGKILL);
        }
    }
}
