//! Compare immediately before a Windows replacement. This is not filesystem CAS.
//! The caller must journal original bytes and the staged path before commit.
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    mem::zeroed,
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, DELETE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DISPOSITION_INFO,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FileDispositionInfo, GetFileInformationByHandle, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    ReplaceFileW, SetFileInformationByHandle,
};

const LIMIT: u64 = 2 * 1024 * 1024;

fn file_access(
    file: &File,
    native_config: bool,
) -> io::Result<crate::config_access::AccessSnapshot> {
    if native_config {
        crate::config_access::AccessSnapshot::capture_native(file, false)
    } else {
        crate::config_access::AccessSnapshot::capture(file, false)
    }
}

#[derive(PartialEq)]
struct Identity {
    volume: u32,
    high: u32,
    low: u32,
}

fn identity(file: &File) -> io::Result<Identity> {
    // SAFETY: valid live handle and writable POD output for the duration of call.
    unsafe {
        let mut info: BY_HANDLE_FILE_INFORMATION = zeroed();
        if GetFileInformationByHandle(file.as_raw_handle(), &mut info) == 0 {
            return Err(io::Error::last_os_error());
        }
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 || info.nNumberOfLinks != 1 {
            return Err(io::Error::other("E_CONFIG_FILE_IDENTITY"));
        }
        Ok(Identity {
            volume: info.dwVolumeSerialNumber,
            high: info.nFileIndexHigh,
            low: info.nFileIndexLow,
        })
    }
}

fn open(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}
fn read(file: &mut File) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > LIMIT {
        return Err(io::Error::other("E_CONFIG_TOO_LARGE"));
    }
    Ok(bytes)
}
fn wide(path: &Path) -> io::Result<Vec<u16>> {
    let mut text: Vec<_> = path.as_os_str().encode_wide().collect();
    if text.contains(&0) {
        return Err(io::Error::other("E_CONFIG_PATH"));
    }
    text.push(0);
    Ok(text)
}

pub struct Snapshot {
    native_config: bool,
    selected_path: PathBuf,
    path: PathBuf,
    identity: Option<Identity>,
    bytes: Vec<u8>,
    access: Option<crate::config_access::AccessSnapshot>,
    parent_access: crate::config_access::AccessSnapshot,
    ancestor_access: Option<crate::target_path::PathAccessSnapshot>,
}
impl Snapshot {
    /// Inspect the selected path before canonicalization, refusing reparse
    /// points in every ancestor as well as reparse/hard-linked destination files.
    /// Caller must separately qualify the selected Codex home and its ownership.
    pub fn capture(path: &Path) -> io::Result<Self> {
        Self::capture_policy(path, false)
    }

    /// Native Codex configuration/cache may be read by sandbox principals.
    /// Existing readers are preserved; foreign writes/ownership changes remain
    /// forbidden. Use only with a gateway that also verifies the Windows peer.
    pub fn capture_native_config(path: &Path) -> io::Result<Self> {
        Self::capture_policy(path, true)
    }

    fn capture_policy(path: &Path, native_config: bool) -> io::Result<Self> {
        let selected_path = path.to_path_buf();
        let selected_parent = path
            .parent()
            .ok_or_else(|| io::Error::other("E_CONFIG_PATH"))?;
        let ancestors = crate::target_path::TargetPathGuard::capture(selected_parent, true)?;
        let parent = selected_parent.canonicalize()?;
        let name = path
            .file_name()
            .ok_or_else(|| io::Error::other("E_CONFIG_PATH"))?;
        let name_text = name.to_string_lossy();
        if name_text.contains([':', '\0']) || name_text.ends_with(['.', ' ']) {
            return Err(io::Error::other("E_CONFIG_PATH"));
        }
        let path = parent.join(name);
        let parent_access = if native_config {
            crate::config_access::AccessSnapshot::native_directory(&parent)?
        } else {
            crate::config_access::AccessSnapshot::directory(&parent)?
        };
        let snapshot = match open(&path) {
            Ok(mut file) => {
                let identity = identity(&file)?;
                let access = file_access(&file, native_config)?;
                let bytes = read(&mut file)?;
                Ok(Self {
                    native_config,
                    selected_path,
                    path,
                    identity: Some(identity),
                    bytes,
                    access: Some(access),
                    parent_access,
                    ancestor_access: None,
                })
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self {
                native_config,
                selected_path,
                path,
                identity: None,
                bytes: Vec::new(),
                access: None,
                parent_access,
                ancestor_access: None,
            }),
            Err(error) => Err(error),
        }?;
        ancestors.verify_unchanged()?;
        Ok(snapshot)
    }
    pub fn original(&self) -> &[u8] {
        &self.bytes
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn existed(&self) -> bool {
        self.identity.is_some()
    }
    pub fn verify_unchanged(&self) -> io::Result<()> {
        if let Some(access) = &self.ancestor_access {
            crate::target_path::TargetPathGuard::capture(self.selected_parent()?, true)?
                .verify_access(access)?;
        }
        let current = Self::capture_policy(&self.selected_path, self.native_config)?;
        if current.path != self.path
            || current.identity != self.identity
            || current.bytes != self.bytes
            || current.access != self.access
            || current.parent_access != self.parent_access
        {
            return Err(io::Error::other("E_CONFIG_CHANGED"));
        }
        Ok(())
    }

    /// Bind this snapshot to current qualified ancestor ownership and ACLs.
    /// Repeated calls verify the existing evidence rather than accepting changed
    /// permissions. Mutation methods retain/check it until their OS operation
    /// completes. This changes no permissions and persists no security descriptor.
    pub fn require_ancestor_access(&mut self) -> io::Result<()> {
        let ancestors = self.guard_parent()?;
        if self.ancestor_access.is_none() {
            self.ancestor_access = Some(ancestors.capture_access()?);
        }
        Ok(())
    }

    fn selected_parent(&self) -> io::Result<&Path> {
        self.selected_path
            .parent()
            .ok_or_else(|| io::Error::other("E_CONFIG_PATH"))
    }

    fn verify_parent(&self, ancestors: &crate::target_path::TargetPathGuard) -> io::Result<()> {
        ancestors.verify_unchanged()?;
        if let Some(access) = &self.ancestor_access {
            ancestors.verify_access(access)?;
        }
        Ok(())
    }

    /// Retain rename/delete exclusion through each mutation, without pinning
    /// user directories for the entire lifetime of a configuration journal.
    /// Qualified snapshots also retain their exact ancestor access requirement.
    fn guard_parent(&self) -> io::Result<crate::target_path::TargetPathGuard> {
        let ancestors =
            crate::target_path::TargetPathGuard::capture(self.selected_parent()?, true)?;
        self.verify_unchanged()?;
        self.verify_parent(&ancestors)?;
        Ok(ancestors)
    }

    /// Deletes only the verified file through its handle. While the handle is
    /// open, writers and renames are denied; a replacement at this path is never
    /// selected by a later path-based delete. Caller proves semantic ownership.
    pub fn remove(&self) -> io::Result<()> {
        let ancestors = self.guard_parent()?;
        let expected = self
            .identity
            .as_ref()
            .ok_or_else(|| io::Error::other("E_CONFIG_CHANGED"))?;
        let mut file = OpenOptions::new()
            .access_mode(FILE_GENERIC_READ | DELETE)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&self.path)?;
        if &identity(&file)? != expected
            || read(&mut file)? != self.bytes
            || Some(file_access(&file, self.native_config)?) != self.access
        {
            return Err(io::Error::other("E_CONFIG_CHANGED"));
        }
        let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
        self.verify_parent(&ancestors)?;
        // SAFETY: valid exclusively mutable file handle and correctly sized POD
        // input. Deletion applies to this checked file object upon handle close.
        if unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle(),
                FileDispositionInfo,
                (&disposition as *const FILE_DISPOSITION_INFO).cast(),
                std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Creates a caller-named same-directory private file with create-new semantics.
    /// Caller records its exact path and candidate hash in the durable journal.
    pub fn stage(&self, name: &str, candidate: &[u8]) -> io::Result<PathBuf> {
        let ancestors = self.guard_parent()?;
        if !name.starts_with(".cxweb-")
            || !name.ends_with(".tmp")
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
            || candidate.len() as u64 > LIMIT
        {
            return Err(io::Error::other("E_CONFIG_STAGE"));
        }
        let path = self.path.with_file_name(name);
        self.verify_parent(&ancestors)?;
        let mut file = crate::state::create_private_file_with_owner(
            &path,
            self.access.as_ref().map(|access| access.owner()),
        )?;
        file.write_all(candidate)?;
        file.sync_all()?;
        self.verify_parent(&ancestors)?;
        Ok(path)
    }

    /// Does not delete staging data on failure: journal recovery owns that data.
    /// Denies in-place writes while checking/replacing an existing destination.
    /// Another writer can still rename it in the final check/replace interval;
    /// full editor qualification and post-commit journal verification are required.
    pub fn commit(&self, staged: &Path, candidate: &[u8]) -> io::Result<()> {
        let ancestors = self.guard_parent()?;
        if staged.parent() != self.path.parent()
            || !staged
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(".cxweb-"))
        {
            return Err(io::Error::other("E_CONFIG_STAGE"));
        }
        let mut file = open(staged)?;
        identity(&file)?;
        crate::config_access::AccessSnapshot::capture(&file, false)?;
        if read(&mut file)? != candidate {
            return Err(io::Error::other("E_CONFIG_STAGE_CHANGED"));
        }
        drop(file); // ReplaceFile needs exclusive access to the replacement file.
        let guard = match (&self.identity, open(&self.path)) {
            (Some(expected), Ok(mut current)) => {
                if &identity(&current)? != expected
                    || read(&mut current)? != self.bytes
                    || Some(file_access(&current, self.native_config)?) != self.access
                {
                    return Err(io::Error::other("E_CONFIG_CHANGED"));
                }
                Some(current)
            }
            (None, Err(error)) if error.kind() == io::ErrorKind::NotFound => None,
            (_, Err(error)) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => return Err(io::Error::other("E_CONFIG_CHANGED")),
        };
        let destination = wide(&self.path)?;
        let source = wide(staged)?;
        self.verify_parent(&ancestors)?;
        // SAFETY: live NUL-terminated paths. No ignore-ACL flags and no replace
        // flag for a previously absent file. Any API error requires recovery.
        let success = unsafe {
            if guard.is_some() {
                ReplaceFileW(
                    destination.as_ptr(),
                    source.as_ptr(),
                    null(),
                    0,
                    null_mut(),
                    null_mut(),
                )
            } else {
                MoveFileExW(
                    source.as_ptr(),
                    destination.as_ptr(),
                    MOVEFILE_WRITE_THROUGH,
                )
            }
        };
        if success == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut committed = open(&self.path)?;
        identity(&committed)?;
        let committed_access = file_access(&committed, self.native_config)?;
        if self
            .access
            .as_ref()
            .is_some_and(|before| before.descriptor_differs(&committed_access))
        {
            return Err(io::Error::other("E_CONFIG_POST_COMMIT_PERMISSIONS"));
        }
        if read(&mut committed)? != candidate {
            return Err(io::Error::other("E_CONFIG_POST_COMMIT_CHANGED"));
        }
        self.verify_parent(&ancestors)?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn native_config_preserves_readers_without_relaxing_private_files_or_allowing_writers() {
        let fixture = Fixture::new();
        let config = fixture.config();
        std::fs::write(&config, b"original").unwrap();
        set_fixture_acl(&fixture.0, Some("(A;;FA;;;CURRENT_USER)(A;;FRFX;;;WD)"));
        set_fixture_acl(&config, Some("(A;;FA;;;CURRENT_USER)(A;;FRFX;;;WD)"));
        assert!(Snapshot::capture(&config).is_err());
        let before = Snapshot::capture_native_config(&config).unwrap();
        let staged = before.stage(".cxweb-native-read.tmp", b"updated").unwrap();
        before.commit(&staged, b"updated").unwrap();
        let after = Snapshot::capture_native_config(&config).unwrap();
        assert_eq!(after.original(), b"updated");
        assert!(
            !before
                .access
                .as_ref()
                .unwrap()
                .descriptor_differs(after.access.as_ref().unwrap())
        );
        assert!(Snapshot::capture(&config).is_err());
        set_fixture_acl(&config, Some("(A;;FA;;;CURRENT_USER)(A;;FW;;;WD)"));
        assert!(Snapshot::capture_native_config(&config).is_err());
        assert!(after.remove().is_err());
        set_fixture_acl(&config, Some("(A;;FA;;;CURRENT_USER)"));
        set_fixture_acl(&fixture.0, Some("(A;;FA;;;CURRENT_USER)(A;;FW;;;WD)"));
        assert!(Snapshot::capture_native_config(&config).is_err());
        set_fixture_acl(&fixture.0, Some("(A;;FA;;;CURRENT_USER)"));
    }

    pub(crate) fn set_fixture_acl(path: &Path, grants: Option<&str>) {
        use windows_sys::Win32::Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
                SE_FILE_OBJECT, SetNamedSecurityInfoW,
            },
            DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl,
            PROTECTED_DACL_SECURITY_INFORMATION,
        };
        let mut path = wide(path).unwrap();
        let mut acl = null_mut();
        let mut parsed = null_mut();
        // SAFETY: paths are unique test-owned objects. Optional SDDL is a fixed
        // test policy with the OS-provided SID; returned descriptor remains live
        // through SetNamedSecurityInfo and is freed by LocalAllocation.
        unsafe {
            if let Some(grants) = grants {
                let sid = crate::state::current_sid().unwrap();
                let sddl: Vec<u16> = format!("D:P{}\0", grants.replace("CURRENT_USER", &sid))
                    .encode_utf16()
                    .collect();
                assert_ne!(
                    ConvertStringSecurityDescriptorToSecurityDescriptorW(
                        sddl.as_ptr(),
                        SDDL_REVISION_1,
                        &mut parsed,
                        null_mut()
                    ),
                    0
                );
                let mut present = 0;
                let mut defaulted = 0;
                assert_ne!(
                    GetSecurityDescriptorDacl(parsed, &mut present, &mut acl, &mut defaulted),
                    0
                );
                assert_ne!(present, 0);
            }
            let _parsed = crate::state::LocalAllocation(parsed);
            assert_eq!(
                SetNamedSecurityInfoW(
                    path.as_mut_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    acl,
                    null_mut()
                ),
                0
            );
        }
    }

    const PRIVATE_ACL: &str = "(A;OICI;FA;;;SY)(A;OICI;FA;;;CURRENT_USER)";
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            Self::under(&std::env::temp_dir())
        }
        fn under(base: &Path) -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let path = base.join(format!(
                "cxweb-atomic-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            crate::state::protected_directory(&path).unwrap();
            Self(path)
        }
        fn config(&self) -> PathBuf {
            self.0.join("config.toml")
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            // Permission tests restore only this unique fixture before cleanup.
            set_fixture_acl(&self.0, Some(PRIVATE_ACL));
            for entry in std::fs::read_dir(&self.0).unwrap() {
                let path = entry.unwrap().path();
                set_fixture_acl(&path, Some(PRIVATE_ACL));
                let mut permissions = std::fs::metadata(&path).unwrap().permissions();
                // This module is Windows-only: clearing the fixture's read-only
                // attribute does not change its private DACL.
                #[allow(clippy::permissions_set_readonly_false)]
                permissions.set_readonly(false);
                std::fs::set_permissions(path, permissions).unwrap();
            }
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    struct Junction(PathBuf);
    impl Junction {
        fn new(path: &Path, target: &Path) -> Self {
            use std::os::windows::process::CommandExt;
            let status = std::process::Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "New-Item -ItemType Junction -Path $env:CXWEB_TEST_LINK -Target $env:CXWEB_TEST_TARGET -ErrorAction Stop | Out-Null",
                ])
                .env("CXWEB_TEST_LINK", path)
                .env("CXWEB_TEST_TARGET", target)
                .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
                .status()
                .unwrap();
            assert!(status.success());
            Self(path.to_path_buf())
        }
    }
    impl Drop for Junction {
        fn drop(&mut self) {
            // Remove only the test-owned junction itself, never its target.
            std::fs::remove_dir(&self.0).unwrap();
        }
    }

    #[test]
    fn junction_parents_and_ancestors_are_refused_before_canonicalization() {
        let fixture = Fixture::new();
        let target = Fixture::new();
        let nested = target.0.join("nested");
        crate::state::protected_directory(&nested).unwrap();
        std::fs::write(target.config(), b"original").unwrap();
        let link = fixture.0.join("link");
        let _junction = Junction::new(&link, &target.0);
        assert!(Snapshot::capture(&link.join("config.toml")).is_err());
        assert!(Snapshot::capture(&link.join("nested/config.toml")).is_err());
        assert_eq!(std::fs::read(target.config()).unwrap(), b"original");
        assert!(!nested.join("config.toml").exists());
    }

    #[test]
    fn a_redirected_selected_parent_cannot_stage_commit_or_remove() {
        for existed in [false, true] {
            let fixture = Fixture::new();
            let parent = fixture.0.join("home");
            let moved = fixture.0.join("moved");
            crate::state::protected_directory(&parent).unwrap();
            let config = parent.join("config.toml");
            if existed {
                std::fs::write(&config, b"original").unwrap();
            }
            let snapshot = Snapshot::capture(&config).unwrap();
            let staged = snapshot.stage(".cxweb-before.tmp", b"candidate").unwrap();
            // Keep the same config/parent objects and ACLs, but redirect the
            // selected path through a new junction after the initial capture.
            std::fs::rename(&parent, &moved).unwrap();
            let _junction = Junction::new(&parent, &moved);
            assert!(snapshot.verify_unchanged().is_err());
            assert!(snapshot.stage(".cxweb-after.tmp", b"candidate").is_err());
            assert!(snapshot.commit(&staged, b"candidate").is_err());
            assert!(snapshot.remove().is_err());
            assert!(!moved.join(".cxweb-after.tmp").exists());
            assert_eq!(
                std::fs::read(moved.join(".cxweb-before.tmp")).unwrap(),
                b"candidate"
            );
            assert_eq!(moved.join("config.toml").exists(), existed);
            if existed {
                assert_eq!(
                    std::fs::read(moved.join("config.toml")).unwrap(),
                    b"original"
                );
            }
        }
    }

    #[test]
    fn operation_guard_pins_ancestors_only_until_the_operation_finishes() {
        let fixture = Fixture::new();
        let ancestor = fixture.0.join("ancestor");
        let parent = ancestor.join("home");
        std::fs::create_dir(&ancestor).unwrap();
        crate::state::protected_directory(&parent).unwrap();
        let snapshot = Snapshot::capture(&parent.join("config.toml")).unwrap();
        let guard = snapshot.guard_parent().unwrap();
        assert!(std::fs::rename(&parent, ancestor.join("renamed")).is_err());
        assert!(std::fs::rename(&ancestor, fixture.0.join("renamed")).is_err());
        drop(guard);
        std::fs::rename(&ancestor, fixture.0.join("renamed")).unwrap();
        assert!(snapshot.verify_unchanged().is_err());
    }

    #[test]
    fn qualification_refuses_an_unsafe_ancestor_before_staging_or_acl_repair() {
        let fixture = Fixture::new();
        let parent = fixture.0.join("home");
        crate::state::protected_directory(&parent).unwrap();
        let config = parent.join("config.toml");
        std::fs::write(&config, b"original").unwrap();
        // Expose only the fixture ancestor, preserving the private child's ACL.
        set_fixture_acl(&fixture.0, Some("(A;;FA;;;CURRENT_USER)(A;;FW;;;WD)"));
        let mut snapshot = Snapshot::capture(&config).unwrap();
        assert!(snapshot.require_ancestor_access().is_err());
        assert!(snapshot.ancestor_access.is_none());
        assert_eq!(std::fs::read(&config).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(&parent).unwrap().count(), 1);
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS)
            .open(&fixture.0)
            .unwrap();
        assert!(crate::config_access::AccessSnapshot::capture_path(&file, true).is_err());
    }

    #[test]
    #[ignore = "requires CXWEB_QUALIFIED_FIXTURE_ROOT on a path whose complete ancestor chain qualifies; never changes host ancestor ACLs"]
    fn qualified_mutations_recheck_evidence_and_never_rebase_a_changed_acl() {
        let base = PathBuf::from(
            std::env::var_os("CXWEB_QUALIFIED_FIXTURE_ROOT")
                .expect("select an existing qualified test directory"),
        );
        crate::target_path::TargetPathGuard::capture(&base, true)
            .unwrap()
            .capture_access()
            .expect("test root ancestor permissions must qualify");
        for existed in [false, true] {
            let fixture = Fixture::under(&base);
            let parent = fixture.0.join("home");
            crate::state::protected_directory(&parent).unwrap();
            let config = parent.join("config.toml");
            if existed {
                std::fs::write(&config, b"original").unwrap();
            }
            let mut snapshot = Snapshot::capture(&config).unwrap();
            snapshot.require_ancestor_access().unwrap();
            snapshot.require_ancestor_access().unwrap();
            let staged = snapshot
                .stage(".cxweb-qualified.tmp", b"candidate")
                .unwrap();
            snapshot.commit(&staged, b"candidate").unwrap();
            let mut snapshot = Snapshot::capture(&config).unwrap();
            snapshot.require_ancestor_access().unwrap();
            let staged = snapshot.stage(".cxweb-rejected.tmp", b"changed").unwrap();
            // Change an earlier ancestor, keeping the immediate parent and file
            // private and unchanged. The old unqualified snapshot accepted this.
            set_fixture_acl(&fixture.0, Some("(A;;FA;;;CURRENT_USER)(A;;FR;;;SY)"));
            assert!(snapshot.require_ancestor_access().is_err());
            assert!(snapshot.stage(".cxweb-changed.tmp", b"changed").is_err());
            assert!(snapshot.commit(&staged, b"changed").is_err());
            assert!(snapshot.remove().is_err());
            assert_eq!(std::fs::read(&config).unwrap(), b"candidate");
            assert_eq!(std::fs::read(&staged).unwrap(), b"changed");
            let mut fresh = Snapshot::capture(&config).unwrap();
            fresh.require_ancestor_access().unwrap();
            fresh.remove().unwrap();
            assert!(!config.exists());
        }
    }

    #[test]
    fn path_permissions_allow_readers_but_refuse_foreign_mutation() {
        fn inspect(
            path: &Path,
            directory: bool,
        ) -> io::Result<crate::config_access::AccessSnapshot> {
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(
                    windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS
                        | FILE_FLAG_OPEN_REPARSE_POINT,
                )
                .open(path)?;
            crate::config_access::AccessSnapshot::capture_path(&file, directory)
        }
        let fixture = Fixture::new();
        std::fs::write(fixture.config(), b"unchanged").unwrap();
        for directory in [false, true] {
            let path = if directory {
                &fixture.0
            } else {
                &fixture.config()
            };
            for grant in ["FR", "GRGX", "0x1200a9"] {
                set_fixture_acl(
                    path,
                    Some(&format!("(A;;FA;;;CURRENT_USER)(A;;{grant};;;WD)")),
                );
                assert!(inspect(path, directory).is_ok(), "{directory} {grant}");
            }
            for grant in [
                "FW", "GW", "GA", "SD", "WD", "WO", "0x40", "0x100", "0x10", "0x2",
            ] {
                set_fixture_acl(
                    path,
                    Some(&format!("(A;;FA;;;CURRENT_USER)(A;;{grant};;;WD)")),
                );
                assert!(inspect(path, directory).is_err(), "{directory} {grant}");
            }
            // A deny does not make a broad foreign allow qualify, and a NULL
            // DACL must never be confused with an empty restricted DACL.
            set_fixture_acl(path, Some("(D;;GW;;;WD)(A;;FA;;;CURRENT_USER)(A;;GA;;;WD)"));
            assert!(inspect(path, directory).is_err());
            set_fixture_acl(path, None);
            assert!(inspect(path, directory).is_err());
            set_fixture_acl(path, Some(PRIVATE_ACL));
        }
        // Existing ancestors may allow creation of sibling directories. This
        // same bit means append-data on files and must not qualify there.
        for directory in [false, true] {
            let path = if directory {
                &fixture.0
            } else {
                &fixture.config()
            };
            set_fixture_acl(path, Some("(A;;FA;;;CURRENT_USER)(A;;0x4;;;WD)"));
            assert_eq!(inspect(path, directory).is_ok(), directory);
            set_fixture_acl(path, Some(PRIVATE_ACL));
        }
        assert_eq!(std::fs::read(fixture.config()).unwrap(), b"unchanged");
    }

    #[test]
    fn path_permissions_use_effective_aces_and_preserve_descriptor_changes() {
        fn inspect(path: &Path) -> crate::config_access::AccessSnapshot {
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS)
                .open(path)
                .unwrap();
            crate::config_access::AccessSnapshot::capture_path(&file, true).unwrap()
        }
        let fixture = Fixture::new();
        set_fixture_acl(&fixture.0, Some("(A;;FA;;;CURRENT_USER)(A;OICIIO;GA;;;WD)"));
        let before = inspect(&fixture.0);
        assert!(before == inspect(&fixture.0));
        set_fixture_acl(&fixture.0, Some("(A;;FA;;;CURRENT_USER)(A;;FR;;;WD)"));
        assert!(before != inspect(&fixture.0));
        // The private-config policy stays stricter even though this directory
        // can be a readable ancestor of a separately protected target.
        assert!(Snapshot::capture(&fixture.config()).is_err());
    }

    #[test]
    fn foreign_read_grants_null_dacl_and_denied_writes_never_qualify() {
        for acl in [
            Some("(A;;FA;;;CURRENT_USER)(A;;FR;;;WD)"),
            Some("(A;;FA;;;CURRENT_USER)(A;;FW;;;BU)"),
            Some("(D;;0x2;;;CURRENT_USER)(A;;FA;;;CURRENT_USER)"),
            None,
        ] {
            let fixture = Fixture::new();
            std::fs::write(fixture.config(), b"original").unwrap();
            set_fixture_acl(&fixture.config(), acl);
            assert!(Snapshot::capture(&fixture.config()).is_err());
            // The validator neither repairs the ACL nor changes file content.
            assert_eq!(std::fs::read(fixture.config()).unwrap(), b"original");
        }
        let fixture = Fixture::new();
        set_fixture_acl(&fixture.0, Some("(A;;FA;;;CURRENT_USER)(A;;FR;;;WD)"));
        assert!(Snapshot::capture(&fixture.config()).is_err());
        assert!(!fixture.config().exists());
    }

    #[test]
    fn changed_trusted_file_acl_blocks_commit_and_remove_without_repair() {
        let fixture = Fixture::new();
        std::fs::write(fixture.config(), b"original").unwrap();
        let snapshot = Snapshot::capture(&fixture.config()).unwrap();
        let staged = snapshot.stage(".cxweb-acl.tmp", b"candidate").unwrap();
        set_fixture_acl(
            &fixture.config(),
            Some("(A;;FA;;;SY)(A;;FA;;;CURRENT_USER)(A;;FR;;;BA)"),
        );
        let changed = Snapshot::capture(&fixture.config()).unwrap();
        assert!(snapshot.verify_unchanged().is_err());
        assert!(snapshot.commit(&staged, b"candidate").is_err());
        assert!(snapshot.remove().is_err());
        changed.verify_unchanged().unwrap();
        assert_eq!(std::fs::read(fixture.config()).unwrap(), b"original");
    }

    #[test]
    fn changed_parent_or_exposed_staging_blocks_replacement() {
        for change_parent in [true, false] {
            let fixture = Fixture::new();
            let snapshot = Snapshot::capture(&fixture.config()).unwrap();
            let staged = snapshot.stage(".cxweb-acl.tmp", b"candidate").unwrap();
            if change_parent {
                set_fixture_acl(
                    &fixture.0,
                    Some("(A;;FA;;;SY)(A;;FA;;;CURRENT_USER)(A;;FR;;;BA)"),
                );
            } else {
                set_fixture_acl(&staged, Some("(A;;FA;;;CURRENT_USER)(A;;FR;;;WD)"));
            }
            assert!(snapshot.commit(&staged, b"candidate").is_err());
            assert!(!fixture.config().exists());
        }
    }

    #[test]
    fn readonly_attributes_are_not_removed_to_force_an_installation() {
        let fixture = Fixture::new();
        std::fs::write(fixture.config(), b"original").unwrap();
        let mut permissions = std::fs::metadata(fixture.config()).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(fixture.config(), permissions).unwrap();
        assert!(Snapshot::capture(&fixture.config()).is_err());
        assert!(
            std::fs::metadata(fixture.config())
                .unwrap()
                .permissions()
                .readonly()
        );
    }
    #[test]
    fn replacing_private_owned_file_preserves_protected_acl() {
        for legacy_inheritance_flags in [false, true] {
            let fixture = Fixture::new();
            if legacy_inheritance_flags {
                std::fs::write(fixture.config(), b"legacy").unwrap();
                set_fixture_acl(&fixture.config(), Some(PRIVATE_ACL));
            }
            for text in [b"first".as_slice(), b"second".as_slice()] {
                let snapshot = Snapshot::capture(&fixture.config()).unwrap();
                let staged = snapshot.stage(".cxweb-private.tmp", text).unwrap();
                snapshot.commit(&staged, text).unwrap();
            }
        }
    }
    #[test]
    fn parent_need_not_allow_deletion_or_subdirectory_creation() {
        let fixture = Fixture::new();
        // Read + add file + traverse only; child files inherit full access.
        set_fixture_acl(
            &fixture.0,
            Some("(A;;0x1200ab;;;CURRENT_USER)(A;OICIIO;FA;;;CURRENT_USER)(A;OICI;FA;;;SY)"),
        );
        let snapshot = Snapshot::capture(&fixture.config()).unwrap();
        let staged = snapshot.stage(".cxweb-private.tmp", b"candidate").unwrap();
        snapshot.commit(&staged, b"candidate").unwrap();
        Snapshot::capture(&fixture.config())
            .unwrap()
            .remove()
            .unwrap();
    }
    #[test]
    fn existing_and_absent_destinations_commit_the_flushed_candidate() {
        for existing in [false, true] {
            let fixture = Fixture::new();
            if existing {
                std::fs::write(fixture.config(), b"model = 'native'\n").unwrap();
            }
            let snapshot = Snapshot::capture(&fixture.config()).unwrap();
            assert_eq!(snapshot.existed(), existing);
            let staged = snapshot
                .stage(".cxweb-test.tmp", b"model = 'changed'\n")
                .unwrap();
            snapshot.commit(&staged, b"model = 'changed'\n").unwrap();
            assert_eq!(
                std::fs::read(fixture.config()).unwrap(),
                b"model = 'changed'\n"
            );
            assert!(!staged.exists());
        }
    }
    #[test]
    fn edited_content_and_replaced_identity_are_preserved_on_conflict() {
        for identical_bytes in [false, true] {
            let fixture = Fixture::new();
            std::fs::write(fixture.config(), b"original").unwrap();
            let snapshot = Snapshot::capture(&fixture.config()).unwrap();
            let staged = snapshot.stage(".cxweb-test.tmp", b"candidate").unwrap();
            let current: &[u8] = if identical_bytes {
                // Keep the old inode alive so Windows cannot recycle its ID.
                std::fs::rename(fixture.config(), fixture.0.join("editor-backup")).unwrap();
                b"original"
            } else {
                b"edited"
            };
            std::fs::write(fixture.config(), current).unwrap();
            assert!(snapshot.commit(&staged, b"candidate").is_err());
            assert_eq!(std::fs::read(fixture.config()).unwrap(), current);
            assert!(staged.exists());
        }
    }
    #[test]
    fn concurrent_creation_and_changed_staging_never_overwrite_user_data() {
        let fixture = Fixture::new();
        let snapshot = Snapshot::capture(&fixture.config()).unwrap();
        let staged = snapshot.stage(".cxweb-test.tmp", b"candidate").unwrap();
        std::fs::write(fixture.config(), b"user created").unwrap();
        assert!(snapshot.commit(&staged, b"candidate").is_err());
        assert_eq!(std::fs::read(fixture.config()).unwrap(), b"user created");
        let snapshot = Snapshot::capture(&fixture.config()).unwrap();
        std::fs::write(&staged, b"changed staging").unwrap();
        assert!(snapshot.commit(&staged, b"candidate").is_err());
        assert_eq!(std::fs::read(fixture.config()).unwrap(), b"user created");
        assert!(snapshot.stage("../outside.tmp", b"candidate").is_err());
    }
    #[test]
    fn hard_links_and_active_in_place_writers_are_refused() {
        let fixture = Fixture::new();
        std::fs::write(fixture.config(), b"original").unwrap();
        let writer = OpenOptions::new()
            .write(true)
            .open(fixture.config())
            .unwrap();
        assert!(Snapshot::capture(&fixture.config()).is_err());
        drop(writer);
        std::fs::hard_link(fixture.config(), fixture.0.join("linked")).unwrap();
        assert!(Snapshot::capture(&fixture.config()).is_err());
    }

    #[test]
    fn removal_checks_bytes_identity_and_active_writer_before_handle_deletion() {
        let fixture = Fixture::new();
        std::fs::write(fixture.config(), b"owned").unwrap();
        let snapshot = Snapshot::capture(&fixture.config()).unwrap();
        std::fs::write(fixture.config(), b"user edited").unwrap();
        assert!(snapshot.remove().is_err());
        let snapshot = Snapshot::capture(&fixture.config()).unwrap();
        std::fs::rename(fixture.config(), fixture.0.join("old-file")).unwrap();
        std::fs::write(fixture.config(), b"user edited").unwrap();
        assert!(snapshot.remove().is_err());
        let snapshot = Snapshot::capture(&fixture.config()).unwrap();
        let writer = OpenOptions::new()
            .write(true)
            .open(fixture.config())
            .unwrap();
        assert!(snapshot.remove().is_err());
        drop(writer);
        snapshot.remove().unwrap();
        assert!(!fixture.config().exists());
        assert!(fixture.0.join("old-file").exists());
    }
}
