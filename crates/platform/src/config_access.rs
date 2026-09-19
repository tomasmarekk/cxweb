//! Read-only validation of a selected configuration object and its parent.
//! Administrators and SYSTEM are trusted OS principals; other grants fail closed.
use crate::state::{LocalAllocation, current_sid, descriptor_text};
use std::{
    fs::{File, OpenOptions},
    io,
    mem::zeroed,
    os::windows::{
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr::null_mut,
};
use windows_sys::Win32::{
    Security::{
        AccessCheck,
        Authorization::{ConvertSidToStringSidW, GetSecurityInfo, SE_FILE_OBJECT},
        DACL_SECURITY_INFORMATION, DuplicateToken, GENERIC_MAPPING, GROUP_SECURITY_INFORMATION,
        GetAce, INHERIT_ONLY_ACE, IsValidAcl, IsValidSid, OWNER_SECURITY_INFORMATION, PSID,
        SecurityImpersonation, TOKEN_DUPLICATE, TOKEN_QUERY,
    },
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, DELETE, FILE_ADD_FILE, FILE_ALL_ACCESS,
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_EXECUTE,
        FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FILE_TRAVERSE, GetFileInformationByHandle,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

fn refused() -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, "E_CONFIG_PERMISSIONS")
}

/// Caller supplies a valid SID backed by an OS-owned security descriptor.
unsafe fn sid_text(sid: PSID) -> io::Result<String> {
    // SAFETY: caller guarantees SID storage; Windows validates and allocates the
    // terminated output. LocalAllocation releases it after the string is copied.
    unsafe {
        if sid.is_null() || IsValidSid(sid) == 0 {
            return Err(refused());
        }
        let mut output = null_mut();
        if ConvertSidToStringSidW(sid, &mut output) == 0 {
            return Err(io::Error::last_os_error());
        }
        let _allocation = LocalAllocation(output.cast());
        let mut length = 0;
        while *output.add(length) != 0 {
            length += 1;
        }
        Ok(String::from_utf16_lossy(std::slice::from_raw_parts(
            output, length,
        )))
    }
}

#[derive(PartialEq, Eq)]
pub(crate) struct AccessSnapshot {
    identity: [u32; 3],
    descriptor: String,
    owner: String,
}
impl AccessSnapshot {
    pub(crate) fn owner(&self) -> &str {
        &self.owner
    }
    pub(crate) fn descriptor_differs(&self, other: &Self) -> bool {
        file_policy(&self.descriptor) != file_policy(&other.descriptor)
    }
    pub(crate) fn directory(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        Self::capture(&file, true)
    }

    pub(crate) fn capture(file: &File, directory: bool) -> io::Result<Self> {
        let user = current_sid()?;
        // SAFETY: all descriptor/ACL/SID pointers below remain inside the live
        // allocation from GetSecurityInfo. GetAce offsets are checked before SID
        // inspection. Token handles are owned immediately and never impersonate
        // a thread or enable privileges; AccessCheck only evaluates permissions.
        unsafe {
            let mut info: BY_HANDLE_FILE_INFORMATION = zeroed();
            if GetFileInformationByHandle(file.as_raw_handle(), &mut info) == 0 {
                return Err(io::Error::last_os_error());
            }
            if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
                || (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory
                || (!directory
                    && (info.nNumberOfLinks != 1
                        || info.dwFileAttributes & FILE_ATTRIBUTE_READONLY != 0))
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "E_CONFIG_OBJECT",
                ));
            }
            let mut owner = null_mut();
            let mut acl = null_mut();
            let mut descriptor = null_mut();
            let status = GetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut owner,
                null_mut(),
                &mut acl,
                null_mut(),
                &mut descriptor,
            );
            if status != 0 {
                return Err(io::Error::from_raw_os_error(status as i32));
            }
            let descriptor = LocalAllocation(descriptor);
            let owner = sid_text(owner)?;
            // Elevated Windows tools can create an Administrators-owned file
            // inside a user-owned directory. AccessCheck still requires the
            // current (possibly filtered) token to have all needed rights.
            if owner != user && owner != "S-1-5-18" && owner != "S-1-5-32-544" {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "E_CONFIG_OWNER",
                ));
            }
            if acl.is_null() || IsValidAcl(acl) == 0 {
                return Err(refused());
            }
            for index in 0..(*acl).AceCount {
                let mut ace = null_mut();
                if GetAce(acl, u32::from(index), &mut ace) == 0 {
                    return Err(io::Error::last_os_error());
                }
                let header = &*ace.cast::<windows_sys::Win32::Security::ACE_HEADER>();
                // Only ordinary allow/deny ACEs have this reviewed SID layout.
                if !matches!(header.AceType, 0 | 1) || header.AceSize < 16 {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "E_CONFIG_ACE",
                    ));
                }
                let bytes =
                    std::slice::from_raw_parts(ace.cast::<u8>(), usize::from(header.AceSize));
                let sid_size = 8 + usize::from(bytes[9]) * 4;
                if 8 + sid_size > bytes.len() {
                    return Err(refused());
                }
                let trustee = sid_text(ace.cast::<u8>().add(8).cast())?;
                if header.AceType == 0
                    && trustee != user
                    && trustee != "S-1-5-18"
                    && trustee != "S-1-5-32-544"
                    && !(trustee == "S-1-3-0" && u32::from(header.AceFlags) & INHERIT_ONLY_ACE != 0)
                {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "E_CONFIG_PRINCIPAL",
                    ));
                }
            }
            let mut primary = null_mut();
            if OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_QUERY | TOKEN_DUPLICATE,
                &mut primary,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let primary = OwnedHandle::from_raw_handle(primary);
            let mut token = null_mut();
            if DuplicateToken(primary.as_raw_handle(), SecurityImpersonation, &mut token) == 0 {
                return Err(io::Error::last_os_error());
            }
            let token = OwnedHandle::from_raw_handle(token);
            let mapping = GENERIC_MAPPING {
                GenericRead: FILE_GENERIC_READ,
                GenericWrite: FILE_GENERIC_WRITE,
                GenericExecute: FILE_GENERIC_EXECUTE,
                GenericAll: FILE_ALL_ACCESS,
            };
            let wanted = if directory {
                // Staging needs child creation/traversal, never parent deletion.
                FILE_GENERIC_READ | FILE_ADD_FILE | FILE_TRAVERSE
            } else {
                FILE_GENERIC_READ | FILE_GENERIC_WRITE | DELETE
            };
            let mut privileges = [0u64; 1024];
            let mut size = size_of_val(&privileges) as u32;
            let mut granted = 0;
            let mut allowed = 0;
            if AccessCheck(
                descriptor.0,
                token.as_raw_handle(),
                wanted,
                &mapping,
                privileges.as_mut_ptr().cast(),
                &mut size,
                &mut granted,
                &mut allowed,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            if allowed == 0 || granted & wanted != wanted {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "E_CONFIG_ACCESS",
                ));
            }
            Ok(Self {
                identity: [
                    info.dwVolumeSerialNumber,
                    info.nFileIndexHigh,
                    info.nFileIndexLow,
                ],
                descriptor: descriptor_text(descriptor.0)?,
                owner,
            })
        }
    }
}

// Only for post-replacement comparison of non-directory files. Windows may add
// DACL auto-inherited bookkeeping and strip child-propagation flags from file
// ACEs. A file has no children; IO still changes effective access and must stay.
// Preserve owner, protection, inheritance requests, ACE order/type/mask/trustee,
// inherited provenance and every other flag. Pre-write snapshots remain exact.
fn file_policy(text: &str) -> String {
    let (owner, dacl) = text.split_once("D:").unwrap_or((text, ""));
    let mut entries = dacl.split('(');
    let mut result = format!(
        "{owner}D:{}",
        entries.next().unwrap_or("").replace("AI", "")
    );
    for ace in entries {
        result.push('(');
        for (index, field) in ace.split(';').enumerate() {
            if index != 0 {
                result.push(';');
            }
            if index == 1 {
                result.push_str(&field.replace("OI", "").replace("CI", "").replace("NP", ""));
            } else {
                result.push_str(field);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::file_policy;

    #[test]
    fn replacement_normalization_preserves_effective_access_and_protection() {
        let original = "O:SYD:P(A;OICINP;FA;;;SY)(A;ID;FR;;;BA)";
        let normalized = "O:SYD:PAI(A;;FA;;;SY)(A;ID;FR;;;BA)";
        assert_eq!(file_policy(original), file_policy(normalized));
        for changed in [
            normalized.replace("D:PAI", "D:AI"),
            normalized.replace("D:PAI", "D:PARAI"),
            normalized.replace("O:SY", "O:BA"),
            normalized.replace("A;;", "A;IO;"),
            normalized.replace("A;;", "D;;"),
            normalized.replace(";FA;", ";FR;"),
            normalized.replace(";ID;", ";;"),
            normalized.replace(";;;BA", ";;;WD"),
        ] {
            assert_ne!(file_policy(original), file_policy(&changed));
        }
    }
}
