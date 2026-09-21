//! Read-only owner/DACL validation for private configuration and selected paths.
//! Path policy permits public readers; private configuration remains restricted.
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
        GetAce, GetSecurityDescriptorControl, INHERIT_ONLY_ACE, IsValidAcl, IsValidSid,
        MapGenericMask, OWNER_SECURITY_INFORMATION, PSID, SE_DACL_PROTECTED, SecurityImpersonation,
        TOKEN_DUPLICATE, TOKEN_QUERY,
    },
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, DELETE, FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_ALL_ACCESS,
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_EXECUTE,
        FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FILE_TRAVERSE, FILE_WRITE_ATTRIBUTES, FILE_WRITE_EA,
        GetFileInformationByHandle,
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
    dacl_protected: bool,
}
impl AccessSnapshot {
    #[cfg(test)]
    pub(crate) fn fixture_descriptor(&self) -> String {
        self.descriptor
            .replace(&current_sid().expect("fixture user"), "CURRENT_USER")
    }

    pub(crate) fn owner(&self) -> &str {
        &self.owner
    }
    pub(crate) fn descriptor_differs(&self, other: &Self) -> bool {
        file_policy(&self.descriptor) != file_policy(&other.descriptor)
    }

    pub(crate) fn is_legacy_replacement_merge(&self, other: &Self) -> bool {
        legacy_replacement_merge(&self.descriptor, &other.descriptor)
    }

    /// Apply the already-reviewed destination DACL to the held replacement only.
    /// ReplaceFile can merge ACLs, so the replacement starts with the same
    /// reviewed grants. The caller checks it before publication and restores
    /// only a recognized legacy duplication after the Windows replacement.
    pub(crate) fn prepare_replacement(&self, file: &File) -> io::Result<()> {
        use windows_sys::Win32::Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
                SetSecurityInfo,
            },
            GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
            PROTECTED_DACL_SECURITY_INFORMATION, SE_DACL_PROTECTED,
            UNPROTECTED_DACL_SECURITY_INFORMATION,
        };
        let sddl: Vec<u16> = self.descriptor.encode_utf16().chain(Some(0)).collect();
        let mut parsed = null_mut();
        // SAFETY: descriptor text comes from a reviewed OS snapshot. The parsed
        // allocation backs every pointer until SetSecurityInfo returns. The
        // caller holds this identity-verified replacement with WRITE_DAC access.
        unsafe {
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut parsed,
                null_mut(),
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let parsed = LocalAllocation(parsed);
            let mut acl = null_mut();
            let mut present = 0;
            let mut defaulted = 0;
            let mut control = 0;
            let mut revision = 0;
            if GetSecurityDescriptorDacl(parsed.0, &mut present, &mut acl, &mut defaulted) == 0
                || GetSecurityDescriptorControl(parsed.0, &mut control, &mut revision) == 0
            {
                return Err(io::Error::last_os_error());
            }
            if present == 0 || acl.is_null() {
                return Err(refused());
            }
            let protection = if control & SE_DACL_PROTECTED != 0 {
                PROTECTED_DACL_SECURITY_INFORMATION
            } else {
                UNPROTECTED_DACL_SECURITY_INFORMATION
            };
            let status = SetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | protection,
                null_mut(),
                null_mut(),
                acl,
                null_mut(),
            );
            if status != 0 {
                return Err(io::Error::from_raw_os_error(status as i32));
            }
        }
        Ok(())
    }
    pub(crate) fn directory(path: &Path) -> io::Result<Self> {
        Self::directory_policy(path, false)
    }
    pub(crate) fn native_directory(path: &Path) -> io::Result<Self> {
        Self::directory_policy(path, true)
    }
    /// An installation index may be readable by native sandbox accounts. Its
    /// descendants containing state still require their own private DACLs.
    pub(crate) fn protected_container(path: &Path) -> io::Result<Self> {
        let snapshot = Self::native_directory(path)?;
        let user = current_sid()?;
        // SDDL can abbreviate the built-in administrator's owner SID as LA.
        // Compare the actual SID and control bit, never its display spelling.
        if snapshot.owner != user || !snapshot.dacl_protected {
            return Err(refused());
        }
        Ok(snapshot)
    }
    fn directory_policy(path: &Path, native: bool) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        Self::capture_policy(&file, true, false, native, false)
    }

    pub(crate) fn capture(file: &File, directory: bool) -> io::Result<Self> {
        Self::capture_policy(file, directory, false, false, false)
    }

    pub(crate) fn capture_native(file: &File, directory: bool) -> io::Result<Self> {
        Self::capture_policy(file, directory, false, true, false)
    }

    /// Existing path components may be publicly readable. Unlike private
    /// configuration objects, they need no write access for the current user.
    pub(crate) fn capture_path(file: &File, directory: bool) -> io::Result<Self> {
        Self::capture_policy(file, directory, true, true, false)
    }

    /// Only for an ancestor whose next child is held against rename/deletion.
    /// Sibling creation and directory attributes cannot replace that child.
    /// NTFS refuses installing a reparse point on a nonempty directory.
    pub(crate) fn capture_held_ancestor(file: &File) -> io::Result<Self> {
        let mut filesystem = [0u16; 32];
        // SAFETY: live file handle, optional outputs are null, the filesystem
        // output buffer is writable and its capacity is supplied in WCHARs.
        let ntfs = unsafe {
            windows_sys::Win32::Storage::FileSystem::GetVolumeInformationByHandleW(
                file.as_raw_handle(),
                null_mut(),
                0,
                null_mut(),
                null_mut(),
                null_mut(),
                filesystem.as_mut_ptr(),
                filesystem.len() as u32,
            ) != 0
                && filesystem[..5] == [b'N' as u16, b'T' as u16, b'F' as u16, b'S' as u16, 0]
        };
        Self::capture_policy(file, true, true, true, ntfs)
    }

    fn capture_policy(
        file: &File,
        directory: bool,
        path_policy: bool,
        readable: bool,
        held_child: bool,
    ) -> io::Result<Self> {
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
                        || (!path_policy && info.dwFileAttributes & FILE_ATTRIBUTE_READONLY != 0)))
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
            let mut control = 0;
            let mut revision = 0;
            if GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision) == 0 {
                return Err(io::Error::last_os_error());
            }
            let owner = sid_text(owner)?;
            // Elevated Windows tools can create an Administrators-owned file
            // inside a user-owned directory. AccessCheck still requires the
            // current (possibly filtered) token to have all needed rights.
            if !trusted(&owner, &user, path_policy) {
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
                let mut mask = u32::from_le_bytes(bytes[4..8].try_into().expect("ACE mask"));
                MapGenericMask(&mut mask, &mapping());
                let inherit_only = u32::from(header.AceFlags) & INHERIT_ONLY_ACE != 0;
                // A deny ACE never authorizes an otherwise unsafe allow ACE.
                // Do not infer arbitrary group membership or grant exceptions
                // for sandbox accounts. Unknown ACE layouts fail above.
                let safe_public = FILE_GENERIC_READ
                    | FILE_GENERIC_EXECUTE
                    | if directory { FILE_ADD_SUBDIRECTORY } else { 0 }
                    | if held_child {
                        FILE_ADD_FILE | FILE_WRITE_ATTRIBUTES | FILE_WRITE_EA
                    } else {
                        0
                    };
                if header.AceType == 0
                    && !trusted(&trustee, &user, path_policy)
                    && !(path_policy && inherit_only)
                    && !(readable
                        && mask
                            & !(if path_policy {
                                safe_public
                            } else {
                                FILE_GENERIC_READ | FILE_GENERIC_EXECUTE
                            })
                            == 0)
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
            let mapping = mapping();
            let wanted = if path_policy {
                FILE_GENERIC_READ
            } else if directory {
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
                dacl_protected: control & SE_DACL_PROTECTED != 0,
            })
        }
    }
}

fn mapping() -> GENERIC_MAPPING {
    GENERIC_MAPPING {
        GenericRead: FILE_GENERIC_READ,
        GenericWrite: FILE_GENERIC_WRITE,
        GenericExecute: FILE_GENERIC_EXECUTE,
        GenericAll: FILE_ALL_ACCESS,
    }
}

fn trusted(sid: &str, user: &str, path_policy: bool) -> bool {
    sid == user || matches!(sid, "S-1-5-18" | "S-1-5-32-544")
        // The Windows Modules Installer owns system ancestors. This exact
        // service SID is trusted only for path components, never config files.
        || (path_policy && sid == "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464")
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

// Legacy ACLs can be copied by ReplaceFile as explicit ACEs and then inherit
// their existing entries again. Accept only this exact duplication pattern for
// restoration, never an arbitrary post-commit policy change or extra trustee.
fn legacy_replacement_merge(before: &str, after: &str) -> bool {
    let Some((owner, acl)) = before.split_once("D:") else {
        return false;
    };
    if !acl.starts_with('(') {
        return false;
    }
    let original = file_policy(before);
    let (_, acl) = original.split_once("D:").expect("normalized descriptor");
    let mut explicit = String::new();
    let mut inherited = String::new();
    for ace in acl.split('(').skip(1) {
        let fields: Vec<_> = ace.split(';').collect();
        if fields.len() != 6 {
            return false;
        }
        if fields[1].contains("ID") {
            inherited.push('(');
            inherited.push_str(ace);
        }
        explicit.push('(');
        for (index, field) in fields.iter().enumerate() {
            if index != 0 {
                explicit.push(';');
            }
            if index == 1 {
                explicit.push_str(&field.replace("ID", ""));
            } else {
                explicit.push_str(field);
            }
        }
    }
    !inherited.is_empty() && file_policy(after) == format!("{owner}D:{explicit}{inherited}")
}

#[cfg(test)]
mod tests {
    use super::file_policy;

    #[test]
    fn legacy_merge_requires_exact_owner_order_masks_and_inherited_duplicates() {
        let before = "O:BAD:(A;ID;FA;;;SY)(A;ID;FA;;;LA)";
        let merged = "O:BAD:AI(A;;FA;;;SY)(A;;FA;;;LA)(A;ID;FA;;;SY)(A;ID;FA;;;LA)";
        assert!(super::legacy_replacement_merge(before, merged));
        for changed in [
            merged.replace("O:BA", "O:SY"),
            merged.replace("D:AI", "D:PAI"),
            merged.replace(";FA;", ";FR;"),
            merged.replace(";;;LA", ";;;WD"),
            merged.replace("A;;", "D;;"),
            merged.replace("A;ID;", "A;;"),
            format!("{merged}(A;;FA;;;WD)"),
        ] {
            assert!(!super::legacy_replacement_merge(before, &changed));
        }
        assert!(!super::legacy_replacement_merge(
            &before.replace("D:", "D:AI"),
            merged
        ));
        assert!(!super::legacy_replacement_merge(
            &before.replace("D:", "D:P"),
            merged
        ));
    }

    #[test]
    fn system_ancestor_trust_does_not_expand_private_configuration_trust() {
        let user = "S-1-5-21-1-2-3-1000";
        for principal in [user, "S-1-5-18", "S-1-5-32-544"] {
            assert!(super::trusted(principal, user, false));
            assert!(super::trusted(principal, user, true));
        }
        let installer = "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464";
        assert!(super::trusted(installer, user, true));
        assert!(!super::trusted(installer, user, false));
        for principal in [
            "S-1-5-21-1-2-3-1001",
            "S-1-5-32-545",
            "S-1-1-0",
            "S-1-5-80-1",
            "S-1-15-3-1",
        ] {
            assert!(!super::trusted(principal, user, true));
        }
    }

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
