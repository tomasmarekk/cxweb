//! Chromium's private Windows pipe transport with an explicit handle allowlist.
use std::{
    fs::File,
    io,
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation},
    Security::SECURITY_ATTRIBUTES,
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        },
        Pipes::CreatePipe,
        Threading::{
            CREATE_NO_WINDOW, CREATE_SUSPENDED, CreateProcessW, DeleteProcThreadAttributeList,
            EXTENDED_STARTUPINFO_PRESENT, InitializeProcThreadAttributeList,
            LPPROC_THREAD_ATTRIBUTE_LIST, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION,
            ResumeThread, STARTF_USESHOWWINDOW, STARTUPINFOEXW, TerminateProcess,
            UpdateProcThreadAttribute,
        },
    },
    UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNORMAL},
};

pub struct BrowserProcess {
    // Closing the job ends only the child tree created here, even after a crash.
    _job: OwnedHandle,
    _process: OwnedHandle,
    pub pid: u32,
    pub input: File,
    pub output: File,
}

fn wide(value: &std::ffi::OsStr) -> io::Result<Vec<u16>> {
    let mut bytes: Vec<u16> = value.encode_wide().collect();
    if bytes.contains(&0) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "embedded NUL"));
    }
    bytes.push(0);
    Ok(bytes)
}

fn pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let mut reader = null_mut();
    let mut writer = null_mut();
    let sa = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    // SAFETY: valid output pointers and initialized security descriptor; successful
    // handles are uniquely transferred to RAII owners immediately.
    unsafe {
        if CreatePipe(&mut reader, &mut writer, &sa, 0) == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((
            OwnedHandle::from_raw_handle(reader),
            OwnedHandle::from_raw_handle(writer),
        ))
    }
}

struct Attributes {
    storage: Vec<usize>,
}
impl Attributes {
    fn new(handles: &mut [HANDLE]) -> io::Result<Self> {
        let mut size = 0;
        // SAFETY: null first call requests the required allocation size.
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut size);
        }
        let mut this = Self {
            storage: vec![0usize; size.div_ceil(size_of::<usize>())],
        };
        // SAFETY: storage is aligned, sized by the API, and remains alive for the
        // entire attribute-list lifetime. Caller keeps handle array alive through spawn.
        unsafe {
            if InitializeProcThreadAttributeList(this.ptr(), 1, 0, &mut size) == 0 {
                // Do not run DeleteProcThreadAttributeList for an uninitialized list.
                this.storage.clear();
                return Err(io::Error::last_os_error());
            }
            if UpdateProcThreadAttribute(
                this.ptr(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_mut_ptr().cast(),
                std::mem::size_of_val(handles),
                null_mut(),
                null(),
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(this)
    }
    fn ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.storage.as_mut_ptr().cast()
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        if !self.storage.is_empty() {
            // SAFETY: this owns one successfully initialized list.
            unsafe {
                DeleteProcThreadAttributeList(self.ptr());
            }
        }
    }
}

/// `profile` must be an application-owned, dedicated directory. This function
/// never attaches to an existing browser or accepts arbitrary debugging flags.
pub fn launch(executable: &Path, profile: &Path, visible: bool) -> io::Result<BrowserProcess> {
    let executable = executable.canonicalize()?;
    let profile = profile.canonicalize()?;
    let exe_string = executable
        .to_str()
        .ok_or_else(|| io::Error::other("invalid executable path"))?;
    let profile_string = profile
        .to_str()
        .ok_or_else(|| io::Error::other("invalid profile path"))?;
    if exe_string.contains('"') || profile_string.contains('"') {
        return Err(io::Error::other("invalid path quoting"));
    }
    let (child_in, parent_in) = pipe()?;
    let (parent_out, child_out) = pipe()?;
    for handle in [&parent_in, &parent_out] {
        // SAFETY: live owned pipe handle; parent ends must never be inherited.
        if unsafe { SetHandleInformation(handle.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    let mut handles = [child_in.as_raw_handle(), child_out.as_raw_handle()];
    let mut attributes = Attributes::new(&mut handles)?;
    let command = format!(
        "\"{exe_string}\" --user-data-dir=\"{profile_string}\" --remote-debugging-pipe --remote-debugging-io-pipes={},{} --no-first-run --no-default-browser-check about:blank",
        handles[0] as usize as u32, handles[1] as usize as u32
    );
    let mut command = wide(command.as_ref())?;
    let application = wide(executable.as_os_str())?;
    // SAFETY: Win32 structures are POD with documented zero defaults. Every OS
    // handle is owned before a subsequent fallible operation. The child is
    // suspended until containment is established, then only its main thread resumes.
    unsafe {
        let job_raw = CreateJobObjectW(null(), null());
        if job_raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = OwnedHandle::from_raw_handle(job_raw);
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job_raw,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut startup: STARTUPINFOEXW = zeroed();
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESHOWWINDOW;
        startup.StartupInfo.wShowWindow = if visible { SW_SHOWNORMAL } else { SW_HIDE } as u16;
        startup.lpAttributeList = attributes.ptr();
        let mut info: PROCESS_INFORMATION = zeroed();
        if CreateProcessW(
            application.as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            1,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED | CREATE_NO_WINDOW,
            null(),
            null(),
            &startup.StartupInfo,
            &mut info,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let process = OwnedHandle::from_raw_handle(info.hProcess);
        let thread = OwnedHandle::from_raw_handle(info.hThread);
        if AssignProcessToJobObject(job_raw, info.hProcess) == 0 {
            let error = io::Error::last_os_error();
            TerminateProcess(info.hProcess, 1);
            return Err(error);
        }
        if ResumeThread(thread.as_raw_handle()) == u32::MAX {
            return Err(io::Error::last_os_error());
        }
        Ok(BrowserProcess {
            _job: job,
            _process: process,
            pid: info.dwProcessId,
            input: File::from(parent_in),
            output: File::from(parent_out),
        })
    }
}
