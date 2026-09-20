//! Per-user directories with a protected DACL; existing foreign ACLs are refused.
use std::{
    ffi::OsStr,
    fs::{File, OpenOptions},
    io,
    mem::size_of,
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::{Path, PathBuf},
    ptr::null_mut,
};
use windows_sys::Win32::{
    Foundation::{ERROR_ALREADY_EXISTS, LocalFree},
    Security::{
        Authorization::{
            ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW,
            ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW,
            SDDL_REVISION_1, SE_FILE_OBJECT,
        },
        DACL_SECURITY_INFORMATION, GetTokenInformation, OWNER_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    Storage::FileSystem::{CreateDirectoryW, FILE_ATTRIBUTE_REPARSE_POINT},
    System::{
        Com::CoTaskMemFree,
        Threading::{GetCurrentProcess, OpenProcessToken},
    },
    UI::Shell::{
        FOLDERID_LocalAppData, FOLDERID_Profile, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86,
        SHGetKnownFolderPath,
    },
};

pub(crate) struct LocalAllocation(pub(crate) *mut std::ffi::c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: every instance wraps a successful LocalAlloc-family API result.
        unsafe {
            LocalFree(self.0);
        }
    }
}
fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut value: Vec<u16> = value.encode_wide().collect();
    if value.contains(&0) {
        return Err(io::Error::other("E_STATE_PATH"));
    }
    value.push(0);
    Ok(value)
}
unsafe fn read_wide(pointer: *const u16) -> String {
    let mut length = 0;
    // SAFETY: caller guarantees an allocated NUL-terminated Windows string.
    unsafe {
        while *pointer.add(length) != 0 {
            length += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length))
    }
}

pub(crate) fn sid_for_process(
    process: windows_sys::Win32::Foundation::HANDLE,
) -> io::Result<String> {
    // SAFETY: API outputs are valid owned handles or allocated strings. Buffer is
    // aligned and sized using the API, and remains live while its SID is read.
    unsafe {
        let mut token = null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = OwnedHandle::from_raw_handle(token);
        let mut size = 0;
        GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut size);
        let mut buffer = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
        if GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            size,
            &mut size,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
        let mut sid = null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut sid) == 0 {
            return Err(io::Error::last_os_error());
        }
        let _allocation = LocalAllocation(sid.cast());
        Ok(read_wide(sid))
    }
}

pub(crate) fn current_sid() -> io::Result<String> {
    // SAFETY: returns the always-valid pseudo handle for the calling process.
    sid_for_process(unsafe { GetCurrentProcess() })
}

pub(crate) fn verify_process_user(pid: u32) -> io::Result<()> {
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    // SAFETY: limited query access to an OS-reported peer PID. Successful handle
    // is owned immediately; no process memory is read and no token is duplicated.
    let process = unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        OwnedHandle::from_raw_handle(handle)
    };
    if sid_for_process(process.as_raw_handle())? != current_sid()? {
        return Err(io::Error::other("E_CONTROL_PEER"));
    }
    Ok(())
}

pub(crate) fn descriptor() -> io::Result<LocalAllocation> {
    descriptor_with_owner(None, true)
}

fn descriptor_with_owner(owner: Option<&str>, directory: bool) -> io::Result<LocalAllocation> {
    let sid = current_sid()?;
    let owner = owner.unwrap_or(&sid);
    if owner != sid && owner != "S-1-5-18" && owner != "S-1-5-32-544" {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "E_CONFIG_OWNER",
        ));
    }
    let inheritance = if directory { "OICI" } else { "" };
    let sddl = wide(OsStr::new(&format!(
        "O:{owner}D:P(A;{inheritance};FA;;;SY)(A;{inheritance};FA;;;{sid})"
    )))?;
    let mut descriptor = null_mut();
    // SAFETY: terminated UTF-16 input and writable descriptor output. Returned
    // allocation is released via LocalFree after directory creation/verification.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(LocalAllocation(descriptor))
}

/// Preserve a reviewed destination owner on the private replacement. Windows
/// refuses an owner the current token cannot assign; no privilege is enabled.
pub(crate) fn create_private_file_with_owner(path: &Path, owner: Option<&str>) -> io::Result<File> {
    use windows_sys::Win32::{
        Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_NORMAL},
    };
    let descriptor = descriptor_with_owner(owner, false)?;
    let path = wide(path.as_os_str())?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    // SAFETY: terminated path and live descriptor; successful handle is uniquely
    // transferred to File, failure never constructs an owner for an invalid handle.
    unsafe {
        let handle = CreateFileW(
            path.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            &attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        Ok(File::from(OwnedHandle::from_raw_handle(handle)))
    }
}

pub(crate) fn descriptor_text(descriptor: PSECURITY_DESCRIPTOR) -> io::Result<String> {
    let mut output = null_mut();
    // SAFETY: descriptor originates from successful Windows security API calls.
    unsafe {
        if ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            SDDL_REVISION_1,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut output,
            null_mut(),
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let _allocation = LocalAllocation(output.cast());
        Ok(read_wide(output))
    }
}

/// Atomically creates a protected directory or verifies the existing exact owner
/// and DACL. Never relaxes permissions or repairs a foreign directory in place.
pub fn protected_directory(path: &Path) -> io::Result<()> {
    let expected = descriptor()?;
    let path_wide = wide(path.as_os_str())?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: expected.0,
        bInheritHandle: 0,
    };
    // SAFETY: initialized descriptor remains alive during the directory call.
    if unsafe { CreateDirectoryW(path_wide.as_ptr(), &attributes) } == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_ALREADY_EXISTS as i32) {
            return Err(error);
        }
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other("E_STATE_REPARSE_POINT"));
    }
    let mut actual = null_mut();
    // SAFETY: terminated path and allocated security descriptor output. No ACLs
    // are changed during inspection, and null unused outputs are documented.
    let status = unsafe {
        GetNamedSecurityInfoW(
            path_wide.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut actual,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    let actual = LocalAllocation(actual);
    if !private_descriptor_matches(&descriptor_text(expected.0)?, &descriptor_text(actual.0)?) {
        return Err(io::Error::other("E_STATE_PERMISSIONS"));
    }
    Ok(())
}

fn private_descriptor_matches(expected: &str, actual: &str) -> bool {
    // Windows can retain SE_DACL_AUTO_INHERITED after copying an ACL. With
    // SE_DACL_PROTECTED still present this is bookkeeping, not an inherited
    // permission. Owner and every explicit ACE must still match exactly.
    expected == actual.replacen("D:PAI(", "D:P(", 1)
}

/// The installation index contains only protected child directories. Native
/// sandbox setup can add read/traverse access to this ancestor. Permit that
/// without editing its ACL; reject foreign writes and unprotected inheritance.
/// Never use this policy for journals, staging, browser profiles or state.
fn installation_container(path: &Path) -> io::Result<()> {
    match protected_directory(path) {
        Ok(()) => Ok(()),
        Err(error) if error.to_string() == "E_STATE_PERMISSIONS" => {
            crate::config_access::AccessSnapshot::protected_container(path)?;
            Ok(())
        }
        Err(error) => Err(error),
    }
}

pub struct StatePaths {
    pub root: PathBuf,
    pub profile: PathBuf,
    pub state: PathBuf,
}

/// Development runtime discovery; distribution must bundle a separately
/// qualified browser payload. Personal browser profiles are never inspected.
pub fn installed_browser() -> io::Result<PathBuf> {
    for (folder, relative) in [
        (
            &FOLDERID_ProgramFiles,
            "Google/Chrome/Application/chrome.exe",
        ),
        (
            &FOLDERID_ProgramFilesX86,
            "Google/Chrome/Application/chrome.exe",
        ),
        (
            &FOLDERID_ProgramFilesX86,
            "Microsoft/Edge/Application/msedge.exe",
        ),
    ] {
        let mut location = null_mut();
        // SAFETY: fixed OS-known folder IDs; successful output uses CoTaskMemFree.
        unsafe {
            if SHGetKnownFolderPath(folder, 0, null_mut(), &mut location) >= 0 {
                let path = PathBuf::from(read_wide(location)).join(relative);
                CoTaskMemFree(location.cast());
                if path.is_file() {
                    return path.canonicalize();
                }
            }
        }
    }
    Err(io::Error::other("E_BROWSER_RUNTIME_MISSING"))
}
impl StatePaths {
    /// Persistent executables and journals use private child directories under
    /// this protected index in the OS-selected user profile. AppData may carry
    /// broader write grants; installation must not rewrite those system ACLs.
    pub fn installations() -> io::Result<PathBuf> {
        let mut location = null_mut();
        // SAFETY: fixed known-folder ID; the successful string is copied before
        // releasing the CoTaskMem allocation. No caller controls this path.
        let root = unsafe {
            if SHGetKnownFolderPath(&FOLDERID_Profile, 0, null_mut(), &mut location) < 0 {
                return Err(io::Error::other("E_USER_PROFILE"));
            }
            let path = PathBuf::from(read_wide(location)).join(".cxweb-runtime");
            CoTaskMemFree(location.cast());
            path
        };
        installation_container(&root)?;
        let guard = crate::target_path::TargetPathGuard::capture(&root, true)?;
        guard.capture_access()?;
        Ok(root)
    }
    pub fn open() -> io::Result<Self> {
        // An MSIX parent's filesystem virtualization is inherited by children.
        // LocalAppData can therefore name different files in a packaged desktop
        // process and an ordinary scheduled task, even when both report the same
        // known-folder and browser profile paths. Keep all active application
        // data alongside the context-independent installation directory.
        let root = Self::installations()?.join("data");
        protected_directory(&root)?;
        let profile = root.join("browser-profile");
        let state = root.join("state");
        protected_directory(&profile)?;
        protected_directory(&state)?;
        Ok(Self {
            root,
            profile,
            state,
        })
    }

    /// Inventory only: legacy registrations stay at their original paths.
    /// Never import a profile implicitly from an ambiguous virtualized view.
    pub fn legacy_state() -> io::Result<PathBuf> {
        let mut location = null_mut();
        // SAFETY: fixed known-folder ID and CoTaskMem-owned output. No environment
        // variable or repository path chooses the application's data directory.
        let root = unsafe {
            if SHGetKnownFolderPath(&FOLDERID_LocalAppData, 0, null_mut(), &mut location) < 0 {
                return Err(io::Error::other("E_LOCAL_APP_DATA"));
            }
            let path = PathBuf::from(read_wide(location));
            CoTaskMemFree(location.cast());
            path.join("cxweb")
        };
        Ok(root.join("state"))
    }
    pub fn lock(&self) -> io::Result<File> {
        let path = self.state.join("instance.lock");
        if let Ok(metadata) = std::fs::symlink_metadata(&path)
            && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err(io::Error::other("E_STATE_REPARSE_POINT"));
        }
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_acl_accepts_bookkeeping_without_accepting_inherited_or_foreign_access() {
        let expected = "O:ownerD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;owner)";
        assert!(private_descriptor_matches(expected, expected));
        assert!(private_descriptor_matches(
            expected,
            &expected.replace("D:P(", "D:PAI(")
        ));
        for changed in [
            expected.replace("D:P(", "D:AI("),
            expected.replace("O:owner", "O:other"),
            expected.replace("FA;;;owner", "FR;;;owner"),
            format!("{expected}(A;OICI;FR;;;WD)"),
            expected.replace("A;OICI;FA;;;owner", "A;OICIID;FA;;;owner"),
        ] {
            assert!(!private_descriptor_matches(expected, &changed));
        }
    }
    fn path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "cxweb-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    #[test]
    fn protected_directory_can_be_reopened_but_inherited_permissions_are_rejected() {
        let protected = path("protected");
        protected_directory(&protected).unwrap();
        protected_directory(&protected).unwrap();
        std::fs::remove_dir(protected).unwrap();
        let foreign = path("foreign");
        std::fs::create_dir(&foreign).unwrap();
        assert!(protected_directory(&foreign).is_err());
        std::fs::remove_dir(foreign).unwrap();
    }
    #[test]
    fn readable_installation_index_keeps_state_private_and_refuses_foreign_writes() {
        let root = path("readable-index");
        installation_container(&root).unwrap();
        let private = "(A;OICI;FA;;;SY)(A;OICI;FA;;;CURRENT_USER)";
        crate::atomic_file::tests::set_fixture_acl(
            &root,
            Some(&format!("{private}(A;OICI;FRFX;;;WD)")),
        );
        installation_container(&root).unwrap();
        assert!(protected_directory(&root).is_err());
        let child = root.join("private-data");
        protected_directory(&child).unwrap();
        protected_directory(&child).unwrap();
        for mask in ["FW", "WD", "WO", "SD", "DC", "FA"] {
            crate::atomic_file::tests::set_fixture_acl(
                &root,
                Some(&format!("{private}(A;OICI;{mask};;;WD)")),
            );
            assert!(installation_container(&root).is_err(), "{mask}");
            protected_directory(&child).unwrap();
        }
        crate::atomic_file::tests::set_fixture_acl(&root, Some(private));
        std::fs::remove_dir(child).unwrap();
        std::fs::remove_dir(root).unwrap();
        let inherited = path("inherited-index");
        std::fs::create_dir(&inherited).unwrap();
        assert!(installation_container(&inherited).is_err());
        std::fs::remove_dir(inherited).unwrap();
    }
    #[test]
    fn instance_lock_is_exclusive_and_releases_on_drop() {
        let root = path("lock");
        protected_directory(&root).unwrap();
        let paths = StatePaths {
            root: root.clone(),
            profile: root.clone(),
            state: root.clone(),
        };
        let lock = paths.lock().unwrap();
        assert!(paths.lock().is_err());
        drop(lock);
        drop(paths.lock().unwrap());
        std::fs::remove_file(root.join("instance.lock")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
