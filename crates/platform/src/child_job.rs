//! Contain subprocesses created by a reviewed diagnostic backend after attachment.
use std::{
    io,
    mem::{size_of, zeroed},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr::null,
    time::Duration,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
    QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
};

pub struct ChildJob(OwnedHandle);
impl ChildJob {
    /// Attach before initializing the backend or asking it to execute tools.
    /// This is not a suspended-process launcher and makes no claim about children
    /// a different executable could create before this call.
    pub fn attach(child: &tokio::process::Child) -> io::Result<Self> {
        let process = child
            .raw_handle()
            .ok_or_else(|| io::Error::other("E_CHILD_HANDLE"))?;
        // SAFETY: the borrowed Child keeps its process handle alive throughout
        // assignment. Each created job handle is immediately owned and the
        // information buffer has the API-prescribed type and byte length.
        unsafe {
            let raw = CreateJobObjectW(null(), null());
            if raw.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = OwnedHandle::from_raw_handle(raw);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                raw,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
                || AssignProcessToJobObject(raw, process) == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(job))
        }
    }
    fn active(&self) -> io::Result<u32> {
        // SAFETY: self retains the live job handle; the writable accounting
        // buffer matches the selected information class and its byte length.
        unsafe {
            let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = zeroed();
            if QueryInformationJobObject(
                self.0.as_raw_handle(),
                JobObjectBasicAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                std::ptr::null_mut(),
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(info.ActiveProcesses)
        }
    }
    pub async fn terminate_and_wait(&self) -> io::Result<()> {
        // SAFETY: self exclusively owns this diagnostic job, which contains
        // only its attached child and descendants created after attachment.
        if unsafe { TerminateJobObject(self.0.as_raw_handle(), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        tokio::time::timeout(Duration::from_secs(5), async {
            while self.active()? != 0 {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Ok(())
        })
        .await
        .map_err(|_| io::Error::other("E_CHILD_CLEANUP_TIMEOUT"))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    #[tokio::test]
    async fn attached_backend_descendants_are_terminated_and_drained() {
        let executable = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let mut child = tokio::process::Command::new(executable)
            .args(["-NoProfile", "-NonInteractive", "-Command", "$null=[Console]::ReadLine(); Start-Process -WindowStyle Hidden -FilePath $env:COMSPEC -ArgumentList '/d /c ping -n 60 127.0.0.1 >NUL' -Wait"])
            .creation_flags(0x08000000).kill_on_drop(true)
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
        let job = ChildJob::attach(&child).unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"start\n")
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while job.active().unwrap() < 2 {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        job.terminate_and_wait().await.unwrap();
        assert_eq!(job.active().unwrap(), 0);
        assert!(!child.wait().await.unwrap().success());
    }
}
