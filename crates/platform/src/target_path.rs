//! Existing local target paths, inspected before canonicalization.
//! This guards object identity during a read-only preflight, not ancestor ACLs
//! or a future configuration transaction. Reparse points are never qualified.
use std::{
    fs::{File, OpenOptions},
    io,
    mem::zeroed,
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::{Component, Path, PathBuf, Prefix},
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ,
    FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileInformationByHandle,
};

fn invalid() -> io::Error {
    io::Error::other("E_TARGET_PATH_IDENTITY")
}

fn components(path: &Path) -> io::Result<Vec<PathBuf>> {
    let mut parts = path.components();
    let Some(Component::Prefix(prefix)) = parts.next() else {
        return Err(invalid());
    };
    if !matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
        || parts.next() != Some(Component::RootDir)
    {
        return Err(invalid());
    }
    let mut current = PathBuf::from(prefix.as_os_str());
    current.push("\\");
    let mut paths = vec![current.clone()];
    for part in parts {
        let Component::Normal(name) = part else {
            return Err(invalid());
        };
        let name_text = name.to_string_lossy();
        if name_text.contains([':', '\0']) || name_text.ends_with(['.', ' ']) {
            return Err(invalid());
        }
        current.push(name);
        paths.push(current.clone());
    }
    Ok(paths)
}

#[derive(PartialEq)]
struct Identity([u32; 3]);

fn identity(file: &File, directory: bool) -> io::Result<Identity> {
    // SAFETY: the owned handle is live and the POD output is writable.
    unsafe {
        let mut info: BY_HANDLE_FILE_INFORMATION = zeroed();
        if GetFileInformationByHandle(file.as_raw_handle(), &mut info) == 0 {
            return Err(io::Error::last_os_error());
        }
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory
            || (!directory && info.nNumberOfLinks != 1)
        {
            return Err(invalid());
        }
        Ok(Identity([
            info.dwVolumeSerialNumber,
            info.nFileIndexHigh,
            info.nFileIndexLow,
        ]))
    }
}

fn open(path: &Path, directory: bool) -> io::Result<File> {
    OpenOptions::new()
        .access_mode(if directory {
            FILE_READ_ATTRIBUTES
        } else {
            FILE_GENERIC_READ
        })
        // Retain rename/delete exclusion for every ancestor. The executable
        // additionally excludes writers while its fingerprint is being used.
        .share_mode(FILE_SHARE_READ | if directory { FILE_SHARE_WRITE } else { 0 })
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

pub struct TargetPathGuard {
    entries: Vec<(PathBuf, File, Identity, bool)>,
}

impl TargetPathGuard {
    /// Accepts existing drive-absolute paths only. Open each component without
    /// following its reparse point before inspecting any descendant. Handles
    /// stay owned until this guard is dropped; no privileges or ACLs are changed.
    pub fn capture(path: &Path, directory: bool) -> io::Result<Self> {
        let paths = components(path)?;
        let last = paths.len() - 1;
        let mut entries = Vec::with_capacity(paths.len());
        for (index, path) in paths.into_iter().enumerate() {
            let directory = index != last || directory;
            let file = open(&path, directory)?;
            let id = identity(&file, directory)?;
            entries.push((path, file, id, directory));
        }
        let guard = Self { entries };
        guard.verify_unchanged()?;
        Ok(guard)
    }

    /// Recheck both the held objects and each path's current resolution. This
    /// detects persistent in-place reparse changes; it is not filesystem CAS.
    pub fn verify_unchanged(&self) -> io::Result<()> {
        for (path, file, expected, directory) in &self.entries {
            if identity(file, *directory)? != *expected
                || identity(&open(path, *directory)?, *directory)? != *expected
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_ambiguous_and_non_local_names_before_opening() {
        for path in [
            "relative",
            "C:relative",
            "\\rooted",
            "C:\\safe\\..\\target",
            "C:\\safe\\file:stream",
            "C:\\safe\\trailing.",
            "C:\\safe\\trailing ",
            "\\\\server\\share\\target",
            "\\\\.\\C:\\target",
            "\\\\?\\UNC\\server\\share\\target",
        ] {
            assert!(components(Path::new(path)).is_err(), "{path}");
        }
        assert_eq!(components(Path::new("C:\\safe\\target")).unwrap().len(), 3);
        assert_eq!(
            components(Path::new("\\\\?\\C:\\safe\\target"))
                .unwrap()
                .len(),
            3
        );
    }

    #[test]
    fn holds_ancestors_and_executable_until_inspection_finishes() {
        let root = std::env::temp_dir().join(format!(
            "cxweb-target-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let nested = root.join("nested");
        std::fs::create_dir(&nested).unwrap();
        let target = nested.join("fixture.exe");
        std::fs::write(&target, "fixture").unwrap();
        let guard = TargetPathGuard::capture(&target, false).unwrap();
        assert!(std::fs::rename(&nested, root.join("renamed")).is_err());
        assert!(std::fs::write(&target, "replacement").is_err());
        assert!(TargetPathGuard::capture(&target, true).is_err());
        guard.verify_unchanged().unwrap();
        drop(guard);
        let link = nested.join("hardlink");
        std::fs::hard_link(&target, &link).unwrap();
        assert!(TargetPathGuard::capture(&target, false).is_err());
        std::fs::remove_file(link).unwrap();
        std::fs::remove_file(target).unwrap();
        std::fs::remove_dir(nested).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
