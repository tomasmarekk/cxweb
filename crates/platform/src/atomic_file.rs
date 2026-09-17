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
    BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_SHARE_DELETE, FILE_SHARE_READ, GetFileInformationByHandle, MOVEFILE_WRITE_THROUGH,
    MoveFileExW, ReplaceFileW,
};

const LIMIT: u64 = 2 * 1024 * 1024;

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
    path: PathBuf,
    identity: Option<Identity>,
    bytes: Vec<u8>,
}
impl Snapshot {
    /// Uses a canonical parent and refuses reparse points and hard-linked files.
    /// Caller must separately qualify the selected Codex home and its ownership.
    pub fn capture(path: &Path) -> io::Result<Self> {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("E_CONFIG_PATH"))?
            .canonicalize()?;
        let name = path
            .file_name()
            .ok_or_else(|| io::Error::other("E_CONFIG_PATH"))?;
        if name.to_string_lossy().contains(':') {
            return Err(io::Error::other("E_CONFIG_PATH"));
        }
        let path = parent.join(name);
        match open(&path) {
            Ok(mut file) => {
                let identity = identity(&file)?;
                let bytes = read(&mut file)?;
                Ok(Self {
                    path,
                    identity: Some(identity),
                    bytes,
                })
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self {
                path,
                identity: None,
                bytes: Vec::new(),
            }),
            Err(error) => Err(error),
        }
    }
    pub fn original(&self) -> &[u8] {
        &self.bytes
    }
    pub fn existed(&self) -> bool {
        self.identity.is_some()
    }

    /// Creates a caller-named same-directory private file with create-new semantics.
    /// Caller records its exact path and candidate hash in the durable journal.
    pub fn stage(&self, name: &str, candidate: &[u8]) -> io::Result<PathBuf> {
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
        let mut file = crate::state::create_private_file(&path)?;
        file.write_all(candidate)?;
        file.sync_all()?;
        Ok(path)
    }

    /// Does not delete staging data on failure: journal recovery owns that data.
    /// Denies in-place writes while checking/replacing an existing destination.
    /// Another writer can still rename it in the final check/replace interval;
    /// full editor qualification and post-commit journal verification are required.
    pub fn commit(&self, staged: &Path, candidate: &[u8]) -> io::Result<()> {
        if staged.parent() != self.path.parent()
            || !staged
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(".cxweb-"))
        {
            return Err(io::Error::other("E_CONFIG_STAGE"));
        }
        let mut file = open(staged)?;
        identity(&file)?;
        if read(&mut file)? != candidate {
            return Err(io::Error::other("E_CONFIG_STAGE_CHANGED"));
        }
        drop(file); // ReplaceFile needs exclusive access to the replacement file.
        let guard = match (&self.identity, open(&self.path)) {
            (Some(expected), Ok(mut current)) => {
                if &identity(&current)? != expected || read(&mut current)? != self.bytes {
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
        if read(&mut committed)? != candidate {
            return Err(io::Error::other("E_CONFIG_POST_COMMIT_CHANGED"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "cxweb-atomic-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
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
            std::fs::remove_dir_all(&self.0).unwrap();
        }
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
}
