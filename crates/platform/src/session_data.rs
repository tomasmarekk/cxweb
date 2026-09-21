//! Clear only the dedicated browser profile after its owner has released it.
use crate::{state::StatePaths, target_path::TargetPathGuard};
use std::{fs, io, os::windows::fs::MetadataExt, path::Path};
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

/// No caller-supplied path can select a personal browser or native Codex data.
pub fn clear_local_session() -> io::Result<()> {
    clear(&StatePaths::open()?)
}

fn clear(paths: &StatePaths) -> io::Result<()> {
    let _owner = paths
        .lock()
        .map_err(|_| io::Error::other("E_SESSION_IN_USE"))?;
    let guard = TargetPathGuard::capture(&paths.profile, true)?;
    guard.capture_access()?;
    let mut remaining = 100_000;
    validate_tree(&paths.profile, 0, &mut remaining)?;
    for entry in fs::read_dir(&paths.profile)? {
        let entry = entry?;
        let path = entry.path();
        guard.verify_unchanged()?;
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::other("E_SESSION_PATH"));
        }
        if metadata.is_dir() {
            // Windows std removal uses handles and does not follow symlinks.
            // Keep the verified profile root pinned throughout child removal.
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    guard.verify_unchanged()?;
    if fs::read_dir(&paths.profile)?.next().transpose()?.is_some() {
        return Err(io::Error::other("E_SESSION_CLEAR_INCOMPLETE"));
    }
    Ok(())
}

fn validate_tree(path: &Path, depth: usize, remaining: &mut usize) -> io::Result<()> {
    if depth > 128 {
        return Err(io::Error::other("E_SESSION_LIMIT"));
    }
    for entry in fs::read_dir(path)? {
        *remaining = remaining
            .checked_sub(1)
            .ok_or_else(|| io::Error::other("E_SESSION_LIMIT"))?;
        let path = entry?.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::other("E_SESSION_PATH"));
        }
        if metadata.is_dir() {
            // Pin this directory while enumerating its children.
            let _guard = TargetPathGuard::capture(&path, true)?;
            validate_tree(&path, depth + 1, remaining)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::protected_directory;
    use std::path::PathBuf;

    struct Fixture {
        root: PathBuf,
        paths: StatePaths,
    }
    impl Fixture {
        fn new() -> Self {
            let root = StatePaths::installations().unwrap().join(format!(
                "cxweb-session-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            protected_directory(&root).unwrap();
            let paths = StatePaths {
                root: root.clone(),
                profile: root.join("browser-profile"),
                state: root.join("state"),
            };
            protected_directory(&paths.profile).unwrap();
            protected_directory(&paths.state).unwrap();
            fs::create_dir(paths.profile.join("Default")).unwrap();
            fs::write(paths.profile.join("Default/Cookies"), b"session fixture").unwrap();
            fs::write(paths.profile.join("Local State"), b"key fixture").unwrap();
            fs::write(
                paths.state.join("retained-journal"),
                b"compatibility fixture",
            )
            .unwrap();
            fs::write(root.join("auth.json"), b"native auth fixture").unwrap();
            Self { root, paths }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }

    #[test]
    fn clears_profile_only_and_repeated_clear_is_safe() {
        let f = Fixture::new();
        clear(&f.paths).unwrap();
        clear(&f.paths).unwrap();
        assert!(fs::read_dir(&f.paths.profile).unwrap().next().is_none());
        assert_eq!(
            fs::read(f.root.join("auth.json")).unwrap(),
            b"native auth fixture"
        );
        assert_eq!(
            fs::read(f.paths.state.join("retained-journal")).unwrap(),
            b"compatibility fixture"
        );
    }

    #[test]
    fn active_owner_prevents_any_deletion() {
        let f = Fixture::new();
        let _owner = f.paths.lock().unwrap();
        assert_eq!(clear(&f.paths).unwrap_err().to_string(), "E_SESSION_IN_USE");
        assert_eq!(
            fs::read(f.paths.profile.join("Default/Cookies")).unwrap(),
            b"session fixture"
        );
    }

    #[test]
    fn shared_file_is_unlinked_without_modifying_other_data() {
        let f = Fixture::new();
        fs::hard_link(
            f.root.join("auth.json"),
            f.paths.profile.join("linked-file"),
        )
        .unwrap();
        clear(&f.paths).unwrap();
        assert_eq!(
            fs::read(f.root.join("auth.json")).unwrap(),
            b"native auth fixture"
        );
    }

    #[test]
    fn reparse_descendant_is_rejected_before_deleting_any_profile_data() {
        let f = Fixture::new();
        let external = Fixture::new();
        let _junction = crate::atomic_file::tests::Junction::new(
            &f.paths.profile.join("Default/redirect"),
            &external.root,
        );
        assert_eq!(clear(&f.paths).unwrap_err().to_string(), "E_SESSION_PATH");
        assert_eq!(
            fs::read(f.paths.profile.join("Default/Cookies")).unwrap(),
            b"session fixture"
        );
        assert_eq!(
            fs::read(external.root.join("auth.json")).unwrap(),
            b"native auth fixture"
        );
    }
}
