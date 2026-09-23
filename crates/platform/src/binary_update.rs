//! Replace an owned executable without terminating its already mapped process.
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::Path,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_SHARE_DELETE, FILE_SHARE_READ, GetFileInformationByHandle, ReplaceFileW,
};

const LIMIT: u64 = 128 * 1024 * 1024;
pub fn lock(directory: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(directory.join("runtime-update.lock"))?;
    identity(&file)?;
    Ok(file)
}
fn invalid() -> io::Error {
    io::Error::other("E_RUNTIME_UPDATE_FILE")
}
fn identity(file: &File) -> io::Result<[u32; 3]> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the owned file handle is live and the output is writable.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 || info.nNumberOfLinks != 1 {
        return Err(invalid());
    }
    Ok([
        info.dwVolumeSerialNumber,
        info.nFileIndexHigh,
        info.nFileIndexLow,
    ])
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
    if bytes.len() as u64 > LIMIT || !bytes.starts_with(b"MZ") {
        return Err(invalid());
    }
    Ok(bytes)
}
fn wide(path: &Path) -> io::Result<Vec<u16>> {
    let mut value: Vec<_> = path.as_os_str().encode_wide().collect();
    if value.contains(&0) {
        return Err(invalid());
    }
    value.push(0);
    Ok(value)
}

/// Caller selects an installation-owned destination and unique sibling backup.
/// A running old image continues unchanged. Its next launch uses the new file.
/// Backups are retained, including if Windows reports an incomplete replacement.
pub fn replace(source: &Path, destination: &Path, backup: &Path) -> io::Result<bool> {
    let parent = destination.parent().ok_or_else(invalid)?;
    if backup.parent() != Some(parent) || backup == destination {
        return Err(invalid());
    }
    let parent_guard = crate::target_path::TargetPathGuard::capture(parent, true)?;
    let _access = crate::config_access::AccessSnapshot::directory(parent)?;
    let source_guard = crate::target_path::TargetPathGuard::capture(source, false)?;
    let expected = read(&mut File::open(source)?)?;
    let mut current = open(destination)?;
    let original_id = identity(&current)?;
    let _destination_access = crate::config_access::AccessSnapshot::capture(&current, false)?;
    let original = read(&mut current)?;
    if original == expected {
        return Ok(false);
    }
    if std::fs::symlink_metadata(backup).is_ok() {
        return Err(invalid());
    }
    let staged = backup.with_extension("next");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
        .open(&staged)?;
    file.write_all(&expected)?;
    file.sync_all()?;
    let staged_id = identity(&file)?;
    drop(file);
    let staged_guard = open(&staged)?;
    if identity(&staged_guard)? != staged_id || read(&mut open(&staged)?)? != expected {
        return Err(invalid());
    }
    source_guard.verify_unchanged()?;
    parent_guard.verify_unchanged()?;
    if identity(&open(destination)?)? != original_id {
        return Err(invalid());
    }
    // ReplaceFile needs write access to both file objects. Keep ancestor and
    // source guards, but release our read-only file handles immediately before
    // replacement. The caller serializes updates in the private directory.
    drop(current);
    drop(staged_guard);
    let destination_wide = wide(destination)?;
    let staged_wide = wide(&staged)?;
    let backup_wide = wide(backup)?;
    // SAFETY: strings are NUL-terminated, handles pin inspected ancestors and
    // source, and reserved arguments are null. No process is stopped.
    let replaced = unsafe {
        ReplaceFileW(
            destination_wide.as_ptr(),
            staged_wide.as_ptr(),
            backup_wide.as_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if replaced == 0 {
        let error = io::Error::last_os_error();
        // Some Windows replacement failures can leave the old name moved to
        // the backup. Restore only that exact original object into an absent
        // destination; never overwrite an unexpected file during recovery.
        if !destination.try_exists()? && identity(&open(backup)?)? == original_id {
            std::fs::rename(backup, destination)?;
        }
        return Err(error);
    }
    if identity(&open(backup)?)? != original_id || read(&mut open(destination)?)? != expected {
        return Err(invalid());
    }
    parent_guard.verify_unchanged()?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::windows::process::CommandExt,
        process::{Command, Stdio},
        time::Duration,
    };

    #[test]
    #[ignore = "resident subprocess used only by the executable replacement test"]
    fn resident_fixture() {
        let marker = std::env::var_os("CXWEB_UPDATE_TEST_READY").expect("fixture marker");
        std::fs::write(marker, b"ready").unwrap();
        std::thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn replacement_preserves_running_image_and_next_launch_uses_new_file() {
        let root = std::env::temp_dir().join(format!(
            "cxweb-update-{}-{}",
            std::process::id(),
            crate::clock::utc_timestamp()
                .unwrap()
                .replace([':', '.'], "")
        ));
        crate::state::protected_directory(&root).unwrap();
        let executable = std::env::current_exe().unwrap();
        let source = root.join("new.exe");
        let destination = root.join("runtime.exe");
        let backup = root.join("previous.exe");
        let marker = root.join("ready");
        let original = std::fs::read(&executable).unwrap();
        let mut updated = original.clone();
        updated.extend_from_slice(b"cxweb update fixture overlay");
        std::fs::write(&source, &updated).unwrap();
        std::fs::write(&destination, &original).unwrap();
        let mut child = Command::new(&destination)
            .args([
                "--exact",
                "binary_update::tests::resident_fixture",
                "--ignored",
            ])
            .env("CXWEB_UPDATE_TEST_READY", &marker)
            .creation_flags(0x08000000)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let result = std::panic::catch_unwind(|| {
            for _ in 0..100 {
                if marker.exists() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            assert!(marker.exists());
            assert!(replace(&source, &destination, &backup).unwrap());
            assert_eq!(std::fs::read(&destination).unwrap(), updated);
            assert_eq!(std::fs::read(&backup).unwrap(), original);
            assert!(!replace(&source, &destination, &root.join("unused.exe")).unwrap());
            assert!(
                Command::new(&destination)
                    .arg("--list")
                    .creation_flags(0x08000000)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .unwrap()
                    .success()
            );
        });
        let still_running = child.try_wait().unwrap().is_none();
        if still_running {
            child.kill().unwrap();
        }
        child.wait().unwrap();
        result.unwrap();
        assert!(still_running);
        for name in ["new.exe", "runtime.exe", "previous.exe", "ready"] {
            std::fs::remove_file(root.join(name)).unwrap();
        }
        std::fs::remove_dir(root).unwrap();
    }
}
