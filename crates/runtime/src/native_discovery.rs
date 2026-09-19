//! Read-only executable inventory. Cached App binaries are candidates, not proof
//! of which backend a running GUI uses. No discovered executable/script runs.
use crate::native_preflight::{fingerprint, reviewed};
use cxweb_platform::target_path::TargetPathGuard;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

const MAX_PATH_ENTRIES: usize = 256;
const MAX_APP_ENTRIES: usize = 64;
const MAX_CANDIDATES: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    PathExecutable,
    NpmInstallation,
    DesktopBackendCache,
}

#[derive(Serialize)]
pub struct Candidate {
    pub executable: PathBuf,
    pub sources: Vec<Source>,
    pub executable_sha256: String,
    pub reviewed_build: Option<&'static str>,
    pub catalog_codec: Option<&'static str>,
}

#[derive(Serialize)]
pub struct Report {
    pub candidates: Vec<Candidate>,
    pub diagnostics: Vec<&'static str>,
    pub target_selection_required: bool,
    pub activation_eligible: bool,
    pub executed_processes: u32,
}

struct Sources {
    path: Vec<PathBuf>,
    npm: Option<PathBuf>,
    app_cache: Option<PathBuf>,
}

fn environment_directory(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

pub async fn discover() -> Report {
    let path = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();
    inventory(Sources {
        path,
        npm: environment_directory("APPDATA").map(|root| root.join("npm")),
        app_cache: environment_directory("LOCALAPPDATA").map(|root| root.join("OpenAI/Codex/bin")),
    })
    .await
}

fn note(diagnostics: &mut Vec<&'static str>, code: &'static str) {
    if !diagnostics.contains(&code) {
        diagnostics.push(code);
    }
}

fn npm_paths(root: &Path) -> [PathBuf; 3] {
    // Read-only locations from the installed npm loader; never execute codex.cmd,
    // codex.ps1, codex.js or arbitrary package-manager hooks to resolve a wrapper.
    let package = root.join("node_modules/@openai/codex");
    [
        package.join(
            "node_modules/@openai/codex-win32-x64/vendor/x86_64-pc-windows-msvc/bin/codex.exe",
        ),
        root.join(
            "node_modules/@openai/codex-win32-x64/vendor/x86_64-pc-windows-msvc/bin/codex.exe",
        ),
        package.join("vendor/x86_64-pc-windows-msvc/bin/codex.exe"),
    ]
}

async fn inventory(sources: Sources) -> Report {
    let mut diagnostics = Vec::new();
    let mut paths = Vec::new();
    if sources.path.len() > MAX_PATH_ENTRIES {
        note(&mut diagnostics, "E_DISCOVERY_PATH_LIMIT");
    }
    for root in sources.path.into_iter().take(MAX_PATH_ENTRIES) {
        // Empty and relative PATH entries must not resolve against the project.
        if !root.is_absolute() {
            note(&mut diagnostics, "E_DISCOVERY_RELATIVE_PATH");
            continue;
        }
        paths.push((root.join("codex.exe"), Source::PathExecutable));
        paths.extend(npm_paths(&root).map(|path| (path, Source::NpmInstallation)));
    }
    if let Some(root) = sources.npm {
        paths.extend(npm_paths(&root).map(|path| (path, Source::NpmInstallation)));
    }
    if let Some(root) = sources.app_cache {
        match TargetPathGuard::capture(&root, true) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => note(&mut diagnostics, "E_DISCOVERY_TARGET_IDENTITY"),
            Ok(_guard) => match std::fs::read_dir(&root) {
                Err(_) => note(&mut diagnostics, "E_DISCOVERY_APP_CACHE"),
                Ok(entries) => {
                    for (index, entry) in entries.enumerate() {
                        if index == MAX_APP_ENTRIES {
                            note(&mut diagnostics, "E_DISCOVERY_APP_LIMIT");
                            break;
                        }
                        match entry {
                            Ok(entry) => {
                                let name = entry.file_name();
                                if name.to_str().is_some_and(|name| {
                                    name.len() == 16 && name.bytes().all(|b| b.is_ascii_hexdigit())
                                }) {
                                    paths.push((
                                        entry.path().join("codex.exe"),
                                        Source::DesktopBackendCache,
                                    ));
                                }
                            }
                            Err(_) => note(&mut diagnostics, "E_DISCOVERY_APP_CACHE"),
                        }
                    }
                }
            },
        }
    }
    let mut candidates: BTreeMap<PathBuf, Candidate> = BTreeMap::new();
    for (path, source) in paths {
        let guard = match TargetPathGuard::capture(&path, false) {
            Ok(guard) => guard,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                note(&mut diagnostics, "E_DISCOVERY_TARGET_IDENTITY");
                continue;
            }
        };
        let Ok(path) = path.canonicalize() else {
            note(&mut diagnostics, "E_DISCOVERY_TARGET_IDENTITY");
            continue;
        };
        if let Some(candidate) = candidates.get_mut(&path) {
            if !candidate.sources.contains(&source) {
                candidate.sources.push(source);
            }
            continue;
        }
        if candidates.len() == MAX_CANDIDATES {
            note(&mut diagnostics, "E_DISCOVERY_CANDIDATE_LIMIT");
            break;
        }
        let Ok(hash) = fingerprint(&path).await else {
            note(&mut diagnostics, "E_DISCOVERY_EXECUTABLE");
            continue;
        };
        if guard.verify_unchanged().is_err() {
            note(&mut diagnostics, "E_DISCOVERY_TARGET_CHANGED");
            continue;
        }
        let qualification = reviewed(&hash);
        candidates.insert(
            path.clone(),
            Candidate {
                executable: path,
                sources: vec![source],
                executable_sha256: hash,
                reviewed_build: qualification.map(|(build, _)| build),
                catalog_codec: qualification.map(|(_, codec)| codec.id()),
            },
        );
    }
    diagnostics.sort_unstable();
    Report {
        candidates: candidates.into_values().collect(),
        diagnostics,
        target_selection_required: true,
        activation_eligible: false,
        executed_processes: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn separates_cached_backends_and_deduplicates_without_running_wrappers() {
        let root =
            std::env::temp_dir().join(format!("cxweb-discovery-{:032x}", rand::random::<u128>()));
        let npm = root.join("npm");
        let cache = root.join("cache");
        let cli = npm_paths(&npm)[0].clone();
        let app = cache.join("0123456789abcdef/codex.exe");
        for path in [&cli, &app] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"Not an executable; must never run").unwrap();
        }
        let wrapper = npm.join("codex.cmd");
        std::fs::write(&wrapper, "@echo off\r\nexit /b 99\r\n").unwrap();
        let report = inventory(Sources {
            path: vec![
                PathBuf::from("relative"),
                npm.clone(),
                cli.parent().unwrap().into(),
            ],
            npm: Some(npm),
            app_cache: Some(cache),
        })
        .await;
        assert_eq!(report.candidates.len(), 2);
        let selected = report
            .candidates
            .iter()
            .find(|c| c.sources.contains(&Source::NpmInstallation))
            .unwrap();
        assert!(selected.sources.contains(&Source::PathExecutable));
        assert!(
            report
                .candidates
                .iter()
                .any(|c| c.sources == [Source::DesktopBackendCache])
        );
        assert!(
            report
                .candidates
                .iter()
                .all(|c| c.reviewed_build.is_none() && c.catalog_codec.is_none())
        );
        assert_eq!(report.diagnostics, ["E_DISCOVERY_RELATIVE_PATH"]);
        assert!(report.target_selection_required && !report.activation_eligible);
        assert_eq!(report.executed_processes, 0);
        // Delete only our enumerated files/directories, never follow candidate links.
        std::fs::remove_file(wrapper).unwrap();
        for path in [cli, app] {
            std::fs::remove_file(&path).unwrap();
            let mut parent = path.parent().unwrap();
            while parent != root {
                std::fs::remove_dir(parent).unwrap();
                parent = parent.parent().unwrap();
            }
        }
        std::fs::remove_dir(root).unwrap();
    }

    #[tokio::test]
    async fn discovery_bounds_path_enumeration_and_does_not_guess_missing_installations() {
        let report = inventory(Sources {
            path: vec![PathBuf::new(); MAX_PATH_ENTRIES + 1],
            npm: None,
            app_cache: None,
        })
        .await;
        assert!(report.candidates.is_empty());
        assert_eq!(
            report.diagnostics,
            ["E_DISCOVERY_PATH_LIMIT", "E_DISCOVERY_RELATIVE_PATH"]
        );
        assert!(report.target_selection_required && !report.activation_eligible);
    }
}
