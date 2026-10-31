use super::SandboxConfig;
use std::process::Child;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_BASIC_LIMIT_INFORMATION,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_ALL_ACCESS};

/// Keeps the Job Object alive for the duration of the associated ScriptProcess.
/// When dropped, the HANDLE is closed which triggers KILL_ON_JOB_CLOSE.
pub struct SandboxGuard(HANDLE);

impl Drop for SandboxGuard {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

// HANDLE is a pointer-sized integer; it can safely be sent across threads.
unsafe impl Send for SandboxGuard {}
unsafe impl Sync for SandboxGuard {}

pub fn apply_post_spawn(child: &Child, cfg: &SandboxConfig) -> anyhow::Result<SandboxGuard> {
    let result = try_apply_job(child);
    match result {
        Ok(guard) => Ok(guard),
        Err(e) => {
            tracing::warn!(
                mod_id = %cfg.mod_id,
                "sandbox: Job Object setup failed (running without Windows sandbox): {e}"
            );
            Ok(SandboxGuard(HANDLE::default()))
        }
    }
}

fn try_apply_job(child: &Child) -> anyhow::Result<SandboxGuard> {
    unsafe {
        let job = CreateJobObjectW(None, None)?;

        let limits = JOBOBJECT_BASIC_LIMIT_INFORMATION {
            LimitFlags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
            ActiveProcessLimit: 4, // enough for Bun's internal worker threads
            ..Default::default()
        };
        let ext = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
            BasicLimitInformation: limits,
            ..Default::default()
        };
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &ext as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )?;

        let pid = child.id();
        let process = OpenProcess(PROCESS_ALL_ACCESS, false, pid)?;
        let assign_result = AssignProcessToJobObject(job, process);
        let _ = CloseHandle(process); // we only needed the handle for this call
        assign_result?;

        Ok(SandboxGuard(job))
    }
}
