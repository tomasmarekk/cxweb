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
        FOLDERID_LocalAppData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86,
        SHGetKnownFolderPath,
    },
};

struct LocalAllocation(*mut std::ffi::c_void);
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

fn current_sid() -> io::Result<String> {
    // SAFETY: API outputs are valid owned handles or allocated strings. Buffer is
    // aligned and sized using the API, and remains live while its SID is read.
    unsafe {
        let mut token = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
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

fn descriptor() -> io::Result<LocalAllocation> {
    let sid = current_sid()?;
    let sddl = wide(OsStr::new(&format!(
        "O:{sid}D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;{sid})"
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

fn descriptor_text(descriptor: PSECURITY_DESCRIPTOR) -> io::Result<String> {
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
    if descriptor_text(expected.0)? != descriptor_text(actual.0)? {
        return Err(io::Error::other("E_STATE_PERMISSIONS"));
    }
    Ok(())
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
    pub fn open() -> io::Result<Self> {
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
