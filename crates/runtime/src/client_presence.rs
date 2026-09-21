//! Presence in supported current-user installation locations, not compatibility.
//! No discovered binary, shim, package hook or native authentication file runs.
use cxweb_platform::target_path::TargetPathGuard;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const MAX_PATH_ENTRIES: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Presence {
    #[default]
    Unknown,
    Installed,
    NotInstalled,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Observation {
    pub cli: Presence,
    pub app: Presence,
    pub observed_at: Option<String>,
}

#[derive(Default)]
pub(crate) struct Cache {
    checked: Option<Instant>,
    observation: Observation,
}

impl Cache {
    pub fn check(&mut self) -> Observation {
        self.check_with(Instant::now(), observe)
    }

    fn check_with(&mut self, now: Instant, probe: impl FnOnce() -> Observation) -> Observation {
        if self
            .checked
            .is_none_or(|checked| now.duration_since(checked) >= Duration::from_secs(30))
        {
            self.observation = probe();
            // Interval begins after the potentially slow filesystem inspection.
            self.checked = Some(Instant::now());
        }
        self.observation.clone()
    }
}

fn combine(first: Presence, second: Presence) -> Presence {
    use Presence::*;
    match (first, second) {
        (Installed, _) | (_, Installed) => Installed,
        (Unknown, _) | (_, Unknown) => Unknown,
        _ => NotInstalled,
    }
}

fn executable(path: &Path) -> Presence {
    match TargetPathGuard::capture(path, false) {
        Ok(guard) if guard.verify_unchanged().is_ok() => Presence::Installed,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Presence::NotInstalled,
        _ => Presence::Unknown,
    }
}

fn cli_presence(path: Option<Vec<PathBuf>>, npm: Option<PathBuf>) -> Presence {
    use Presence::*;
    let mut result = if path.is_some() && npm.is_some() {
        NotInstalled
    } else {
        Unknown
    };
    let path = path.unwrap_or_default();
    if path.len() > MAX_PATH_ENTRIES {
        result = Unknown;
    }
    let roots = path.into_iter().take(MAX_PATH_ENTRIES).chain(npm);
    for root in roots {
        if !root.is_absolute() {
            result = combine(result, Unknown);
            continue;
        }
        for candidate in
            std::iter::once(root.join("codex.exe")).chain(crate::native_discovery::npm_paths(&root))
        {
            result = combine(result, executable(&candidate));
            if result == Installed {
                return result;
            }
        }
        // An unresolved shim can point outside the known layout. Its existence
        // prevents an absence claim; never execute or parse its instructions.
        for shim in ["codex.cmd", "codex.ps1", "codex"] {
            if executable(&root.join(shim)) != NotInstalled {
                result = combine(result, Unknown);
            }
        }
    }
    result
}

fn observe() -> Observation {
    let path = std::env::var_os("PATH").map(|value| std::env::split_paths(&value).collect());
    let npm = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .map(|root| root.join("npm"));
    Observation {
        cli: cli_presence(path, npm),
        app: match cxweb_platform::package_inventory::codex_registered() {
            Ok(true) => Presence::Installed,
            Ok(false) => Presence::NotInstalled,
            Err(_) => Presence::Unknown,
        },
        observed_at: cxweb_platform::clock::utc_timestamp(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_present_and_unresolved_shims_remain_distinct() {
        use Presence::*;
        let root =
            std::env::temp_dir().join(format!("cxweb-presence-{:032x}", rand::random::<u128>()));
        std::fs::create_dir(&root).unwrap();
        assert_eq!(
            cli_presence(Some(vec![root.clone()]), Some(root.clone())),
            NotInstalled
        );
        assert_eq!(cli_presence(None, Some(root.clone())), Unknown);
        assert_eq!(
            cli_presence(Some(vec![PathBuf::from("relative")]), Some(root.clone())),
            Unknown
        );
        assert_eq!(
            cli_presence(
                Some(vec![root.clone(); MAX_PATH_ENTRIES + 1]),
                Some(root.clone())
            ),
            Unknown
        );
        let shim = root.join("codex.cmd");
        std::fs::write(&shim, "must never execute").unwrap();
        assert_eq!(
            cli_presence(Some(vec![root.clone()]), Some(root.clone())),
            Unknown
        );
        let exe = root.join("codex.exe");
        std::fs::write(&exe, "presence does not qualify an executable").unwrap();
        assert_eq!(
            cli_presence(Some(vec![root.clone()]), Some(root.clone())),
            Installed
        );
        std::fs::remove_file(exe).unwrap();
        std::fs::remove_file(shim).unwrap();
        std::fs::create_dir(root.join("codex.exe")).unwrap();
        assert_eq!(
            cli_presence(Some(vec![root.clone()]), Some(root.clone())),
            Unknown
        );
        std::fs::remove_dir(root.join("codex.exe")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn observations_are_cached_and_keep_their_own_timestamp() {
        let mut cache = Cache::default();
        let first = cache.check_with(Instant::now(), || Observation {
            cli: Presence::Installed,
            app: Presence::NotInstalled,
            observed_at: Some("2026-09-20T00:00:00.000Z".into()),
        });
        assert_eq!(
            cache.check_with(Instant::now(), || panic!("must not poll again")),
            first
        );
        cache.checked = Some(Instant::now() - Duration::from_secs(31));
        let second = cache.check_with(Instant::now(), Observation::default);
        assert_eq!(second, Observation::default());
    }
}
