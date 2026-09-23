//! Installer-owned payload delivery. No process termination or configuration edits.
use crate::config_journal::ConfigJournal;
use cxweb_platform::{binary_update, state::StatePaths, target_path::TargetPathGuard};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(serde::Serialize)]
pub struct Report {
    installations: usize,
    replaced: usize,
    pub restart_required: bool,
}

pub async fn stage_installed_payload() -> Result<Report, &'static str> {
    tokio::task::spawn_blocking(|| {
        let executable = std::env::current_exe().map_err(|_| "E_RUNTIME_UPDATE_SOURCE")?;
        let source = executable
            .parent()
            .ok_or("E_RUNTIME_UPDATE_SOURCE")?
            .join("cxweb-daemon.exe");
        let roots = [
            StatePaths::installations().map_err(|_| "E_RUNTIME_UPDATE_STATE")?,
            StatePaths::legacy_state().map_err(|_| "E_RUNTIME_UPDATE_STATE")?,
        ];
        stage(&source, &roots)
    })
    .await
    .map_err(|_| "E_RUNTIME_UPDATE_WORKER")?
}

fn stage(source: &Path, roots: &[PathBuf]) -> Result<Report, &'static str> {
    let _source_guard =
        TargetPathGuard::capture(source, false).map_err(|_| "E_RUNTIME_UPDATE_SOURCE")?;
    let mut ids = BTreeSet::new();
    let mut targets = Vec::new();
    let mut root_guards = Vec::new();
    // Validate every target before replacing any payload. The integration
    // journal and exact OS scheduler receipt authorize only a fixed sibling.
    for root in roots {
        match std::fs::symlink_metadata(root) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("E_RUNTIME_UPDATE_STATE"),
            Ok(_) => (),
        }
        root_guards
            .push(TargetPathGuard::capture(root, true).map_err(|_| "E_RUNTIME_UPDATE_STATE")?);
        for (index, entry) in std::fs::read_dir(root)
            .map_err(|_| "E_RUNTIME_UPDATE_STATE")?
            .enumerate()
        {
            if index >= 256 {
                return Err("E_INSTALLED_LIMIT");
            }
            let entry = entry.map_err(|_| "E_RUNTIME_UPDATE_STATE")?;
            let name = entry.file_name();
            let Some(suffix) = name
                .to_str()
                .and_then(|name| name.strip_prefix("prepared-"))
            else {
                continue;
            };
            if suffix.len() != 32 || !suffix.bytes().all(|c| c.is_ascii_hexdigit()) {
                continue;
            }
            let directory = entry.path();
            let guard =
                TargetPathGuard::capture(&directory, true).map_err(|_| "E_RUNTIME_UPDATE_STATE")?;
            let lock = binary_update::lock(&directory).map_err(|_| "E_RUNTIME_UPDATE_BUSY")?;
            if let Some((id, destination)) = ConfigJournal::runtime_update_target(&directory)
                .map_err(|_| "E_RUNTIME_UPDATE_JOURNAL")?
            {
                if !ids.insert(id) {
                    return Err("E_INSTALLED_DUPLICATE");
                }
                targets.push((directory, destination, guard, lock));
            }
        }
    }
    let mut report = Report {
        installations: targets.len(),
        replaced: 0,
        restart_required: !targets.is_empty(),
    };
    for (directory, destination, guard, _lock) in targets {
        guard
            .verify_unchanged()
            .map_err(|_| "E_RUNTIME_UPDATE_STATE")?;
        let backup: PathBuf = directory.join(format!(
            "cxweb-daemon.previous-{:032x}.exe",
            rand::random::<u128>()
        ));
        if binary_update::replace(source, &destination, &backup)
            .map_err(|_| "E_RUNTIME_UPDATE_REPLACE")?
        {
            report.replaced += 1;
        }
    }
    // Older hosts cannot acknowledge a graceful upgrade. Conservatively
    // require restart even on an idempotent reinstall: a mapped old image
    // may still be running after an earlier successful payload replacement.
    Ok(report)
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
    #[ignore = "requires Windows Task Scheduler and rustc; isolated GUI-subsystem fixture only"]
    fn installed_update_preserves_journal_and_scheduler_and_uses_new_payload() {
        let root = std::env::temp_dir().join(format!(
            "cxweb-update-installation-{:032x}",
            rand::random::<u128>()
        ));
        cxweb_platform::state::protected_directory(&root).unwrap();
        let installations = root.join("installations");
        cxweb_platform::state::protected_directory(&installations).unwrap();
        let directory = installations.join(format!("prepared-{:032x}", rand::random::<u128>()));
        cxweb_platform::state::protected_directory(&directory).unwrap();
        let home = root.join("home");
        cxweb_platform::state::protected_directory(&home).unwrap();
        let config = home.join("config.toml");
        std::fs::write(&config, "# isolated user preference\nmodel = 'fixture'\n").unwrap();
        let source_code = root.join("fixture.rs");
        std::fs::write(&source_code, r##"#![windows_subsystem="windows"]
fn main() {
    let path = std::env::current_exe().unwrap();
    let bytes = std::fs::read(&path).unwrap();
    std::fs::write(path.parent().unwrap().join("fixture-version"), if bytes.ends_with(b"cxweb-v2") { b"v2" } else { b"v1" }).unwrap();
}"##).unwrap();
        let destination = directory.join("cxweb-daemon.exe");
        assert!(
            Command::new("rustc")
                .args(["--crate-name", "runtime_update_fixture"])
                .arg(&source_code)
                .arg("-o")
                .arg(&destination)
                .creation_flags(0x08000000)
                .stdout(Stdio::null())
                .status()
                .unwrap()
                .success()
        );
        let mut bytes = std::fs::read(&destination).unwrap();
        bytes.extend_from_slice(b"cxweb-v2");
        let source = root.join("cxweb-daemon.exe");
        std::fs::write(&source, bytes).unwrap();
        let mut journal =
            ConfigJournal::prepare_with_web_tools(&directory, &config, 12345, &"a".repeat(43))
                .unwrap();
        journal.record_catalog(vec![], vec![]).unwrap();
        let task = journal
            .prepare_scheduler(&destination)
            .unwrap()
            .register()
            .unwrap();
        journal.record_scheduler(&task).unwrap();
        journal.apply().unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let marker = directory.join("fixture-version");
            for _ in 0..250 {
                if std::fs::read(&marker).ok().as_deref() == Some(b"v1")
                    && !task.status().unwrap().running
                {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            assert_eq!(std::fs::read(&marker).unwrap(), b"v1");
            assert!(!task.status().unwrap().running);
            let original_config = std::fs::read(&config).unwrap();
            let original_journal = std::fs::read(directory.join("integration.json")).unwrap();
            let report = stage(&source, std::slice::from_ref(&installations)).unwrap();
            assert_eq!(report.installations, 1);
            assert_eq!(report.replaced, 1);
            assert!(report.restart_required);
            assert_eq!(std::fs::read(&config).unwrap(), original_config);
            assert_eq!(
                std::fs::read(directory.join("integration.json")).unwrap(),
                original_journal
            );
            let repeated = stage(&source, std::slice::from_ref(&installations)).unwrap();
            assert_eq!(repeated.replaced, 0);
            assert!(repeated.restart_required);
            task.start().unwrap();
            for _ in 0..250 {
                if std::fs::read(&marker).ok().as_deref() == Some(b"v2")
                    && !task.status().unwrap().running
                {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            assert_eq!(std::fs::read(&marker).unwrap(), b"v2");
            task.verify_executable(&destination).unwrap();
        }));
        for _ in 0..250 {
            if !task.status().unwrap().running {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        task.remove_stopped().unwrap();
        drop(journal);
        result.unwrap();
        let resolved = root.canonicalize().unwrap();
        assert!(resolved.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            resolved
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("cxweb-update-installation-")
        );
        std::fs::remove_dir_all(resolved).unwrap();
    }
}
