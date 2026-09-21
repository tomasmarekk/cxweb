//! Owned Windows browser processes: private pipe transport or user-operated login.
use std::{
    fs::File,
    io,
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation, WAIT_OBJECT_0, WAIT_TIMEOUT},
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
            ResumeThread, STARTF_USESHOWWINDOW, STARTUPINFOEXW, STARTUPINFOW, TerminateProcess,
            UpdateProcThreadAttribute, WaitForSingleObject,
        },
    },
    UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNORMAL},
};

pub struct BrowserProcess {
    // Closing the job ends only the child tree created here, even after a crash.
    _job: OwnedHandle,
    _process: OwnedHandle,
    pid: u32,
    pub input: File,
    pub output: File,
}

/// User-operated authentication window. Deliberately exposes no browser transport.
pub struct LoginBrowser(BrowserProcess);

impl LoginBrowser {
    pub fn launch(executable: &Path, profile: &Path) -> io::Result<Self> {
        launch_mode(executable, profile, true, false, false, true).map(Self)
    }

    pub fn has_exited(&self) -> io::Result<bool> {
        self.0.has_exited()
    }
}

impl BrowserProcess {
    pub fn pid(&self) -> u32 {
        self.pid
    }
    pub fn has_exited(&self) -> io::Result<bool> {
        // SAFETY: this owns the process handle; the zero timeout never blocks.
        match unsafe { WaitForSingleObject(self._process.as_raw_handle(), 0) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(io::Error::other("E_BROWSER_RELEASE")),
        }
    }
    pub fn background_bounds() -> (i32, i32, i32, i32) {
        crate::browser_window::bounds()
    }

    pub fn login_bounds() -> (i32, i32, i32, i32) {
        crate::browser_window::login_bounds()
    }

    pub fn park_windows(&self) -> io::Result<(usize, bool, bool)> {
        // SAFETY: this owns a live process handle; a zero timeout never blocks.
        if unsafe { WaitForSingleObject(self._process.as_raw_handle(), 0) } != WAIT_TIMEOUT {
            return Err(io::Error::other("E_BROWSER_RELEASE"));
        }
        crate::browser_window::park(self.pid)
    }
    pub fn wait_for_exit(&self) -> io::Result<()> {
        // SAFETY: the owned process handle stays alive for this bounded wait.
        if unsafe { WaitForSingleObject(self._process.as_raw_handle(), 10_000) } == WAIT_OBJECT_0 {
            Ok(())
        } else {
            Err(io::Error::other("E_BROWSER_RELEASE"))
        }
    }
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
    launch_mode(executable, profile, visible, !visible, false, false)
}

pub fn launch_offscreen(executable: &Path, profile: &Path) -> io::Result<BrowserProcess> {
    launch_mode(executable, profile, false, false, true, false)
}

fn launch_mode(
    executable: &Path,
    profile: &Path,
    visible: bool,
    headless: bool,
    offscreen: bool,
    manual_login: bool,
) -> io::Result<BrowserProcess> {
    let executable = browser_path(executable)?;
    let profile = browser_path(profile)?;
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
    let mode = if headless {
        " --headless=new"
    } else if offscreen {
        " --disable-backgrounding-occluded-windows --disable-renderer-backgrounding"
    } else {
        ""
    };
    let command = if manual_login {
        login_command(exe_string, profile_string)
    } else {
        format!(
            "\"{exe_string}\" --user-data-dir=\"{profile_string}\" --remote-debugging-pipe --remote-debugging-io-pipes={},{} --no-first-run --no-default-browser-check --no-startup-window --lang=en-US --accept-lang=en-US,en{mode}",
            handles[0] as usize as u32, handles[1] as usize as u32
        )
    };
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
        startup.StartupInfo.cb = if manual_login {
            size_of::<STARTUPINFOW>()
        } else {
            size_of::<STARTUPINFOEXW>()
        } as u32;
        startup.StartupInfo.dwFlags = STARTF_USESHOWWINDOW;
        startup.StartupInfo.wShowWindow = if visible { SW_SHOWNORMAL } else { SW_HIDE } as u16;
        startup.lpAttributeList = if manual_login {
            null_mut()
        } else {
            attributes.ptr()
        };
        let mut info: PROCESS_INFORMATION = zeroed();
        if CreateProcessW(
            application.as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            i32::from(!manual_login),
            CREATE_SUSPENDED
                | CREATE_NO_WINDOW
                | if manual_login {
                    0
                } else {
                    EXTENDED_STARTUPINFO_PRESENT
                },
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

fn login_command(executable: &str, profile: &str) -> String {
    format!(
        "\"{executable}\" --user-data-dir=\"{profile}\" --no-first-run --no-default-browser-check --disable-background-mode --lang=en-US --accept-lang=en-US,en --new-window https://chatgpt.com/"
    )
}

// Rust canonicalization returns a verbatim Windows path. Keep canonical path
// validation, but supply Chromium the ordinary spelling used by a manual launch.
fn browser_path(path: &Path) -> io::Result<PathBuf> {
    let canonical = path.canonicalize()?;
    let text = canonical
        .to_str()
        .ok_or_else(|| io::Error::other("invalid browser path"))?;
    let ordinary = if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{unc}"))
    } else if let Some(disk) = text.strip_prefix(r"\\?\") {
        PathBuf::from(disk)
    } else {
        canonical.clone()
    };
    if !ordinary.is_absolute() || ordinary.canonicalize()? != canonical {
        return Err(io::Error::other("browser path identity changed"));
    }
    Ok(ordinary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authentication_uses_ordinary_english_chrome_without_automation() {
        let command = login_command(
            r"C:\Program Files\Chrome\chrome.exe",
            r"C:\cxweb data\profile",
        );
        assert!(command.starts_with(
            r#""C:\Program Files\Chrome\chrome.exe" --user-data-dir="C:\cxweb data\profile" "#
        ));
        assert!(command.contains("--lang=en-US --accept-lang=en-US,en"));
        assert!(command.contains("--disable-background-mode"));
        assert!(command.ends_with("--new-window https://chatgpt.com/"));
        for flag in [
            "debugging",
            "automation",
            "headless",
            "no-startup-window",
            "user-agent",
            "disable-blink",
        ] {
            assert!(!command.contains(flag), "unexpected login flag: {flag}");
        }
    }

    #[test]
    fn chromium_paths_keep_identity_without_verbatim_prefixes() {
        let path = std::env::temp_dir().join(format!("cxweb path 🦀 {}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        let canonical = path.canonicalize().unwrap();
        assert!(canonical.to_str().unwrap().starts_with(r"\\?\"));
        let ordinary = browser_path(&path).unwrap();
        assert!(!ordinary.to_str().unwrap().starts_with(r"\\?\"));
        assert_eq!(ordinary.canonicalize().unwrap(), canonical);
        assert_eq!(browser_path(&canonical).unwrap(), ordinary);
        std::fs::remove_dir(path).unwrap();
    }
}
