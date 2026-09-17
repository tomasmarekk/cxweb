//! Durable preparation and recovery evidence for a selected configuration file.
//! Not enabled by the desktop controller: client/runtime preflight is still required.
//! The private journal contains the original configuration and live route capability.
use cxweb_codex_adapter::{config::RoutePatch, strict_json};
use cxweb_platform::{
    atomic_file::Snapshot,
    state::{StatePaths, protected_directory},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Prepared,
    ConfigApplied,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Recovery {
    /// Full preflight is required before any new installation attempt.
    Original,
    /// Route is on disk; this says nothing about daemon health or client readiness.
    Candidate,
    /// Preserve user changes. A later disconnect must use key-level three-way undo.
    Changed,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    id: String,
    target: PathBuf,
    phase: Phase,
    original_existed: bool,
    original: String,
    candidate: String,
    original_sha256: String,
    candidate_sha256: String,
    port: u16,
    capability: String,
}

pub struct ConfigJournal {
    _lock: File,
    record: Record,
    journal: Snapshot,
    // Only the original live preflight can commit. Reopening never silently
    // recaptures a changed file identity or resumes an interrupted installation.
    prepared: Option<Snapshot>,
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn invalid() -> io::Error {
    io::Error::other("E_INTEGRATION_JOURNAL")
}
fn acquire(directory: &Path) -> io::Result<File> {
    protected_directory(directory)?;
    StatePaths {
        root: directory.into(),
        profile: directory.into(),
        state: directory.into(),
    }
    .lock()
}
fn replace(snapshot: &Snapshot, bytes: &[u8]) -> io::Result<()> {
    let name = format!(".cxweb-{:032x}.tmp", rand::random::<u128>());
    let staged = snapshot.stage(&name, bytes)?;
    snapshot.commit(&staged, bytes)
}

impl ConfigJournal {
    /// Caller has selected/qualified this home and owns the installation lock.
    /// A second lock serializes this journal's writers. Real config is untouched
    /// until the complete private recovery record has been flushed and replaced.
    pub fn prepare(
        directory: &Path,
        target: &Path,
        port: u16,
        capability: &str,
    ) -> io::Result<Self> {
        let lock = acquire(directory)?;
        let journal = Snapshot::capture(&directory.join("integration.json"))?;
        if journal.existed() {
            return Err(io::Error::other("E_INTEGRATION_EXISTS"));
        }
        let prepared = Snapshot::capture(target)?;
        if prepared.path() == journal.path() || prepared.original().len() > 512 * 1024 {
            return Err(invalid());
        }
        let original = std::str::from_utf8(prepared.original()).map_err(|_| invalid())?;
        let (_, candidate) = RoutePatch::plan(original, port, capability).map_err(|_| invalid())?;
        let record = Record {
            version: 1,
            id: format!("{:032x}", rand::random::<u128>()),
            target: prepared.path().into(),
            phase: Phase::Prepared,
            original_existed: prepared.existed(),
            original: original.into(),
            original_sha256: hash(original.as_bytes()),
            candidate_sha256: hash(candidate.as_bytes()),
            candidate,
            port,
            capability: capability.into(),
        };
        replace(
            &journal,
            &serde_json::to_vec(&record).map_err(|_| invalid())?,
        )?;
        let journal = Snapshot::capture(journal.path())?;
        Ok(Self {
            _lock: lock,
            record,
            journal,
            prepared: Some(prepared),
        })
    }

    /// Never takes a write destination from a journal alone. The independently
    /// selected target must agree, and the entire plan must reproduce exactly.
    pub fn reopen(directory: &Path, target: &Path) -> io::Result<Self> {
        let lock = acquire(directory)?;
        let journal = Snapshot::capture(&directory.join("integration.json"))?;
        let value =
            strict_json::parse(journal.original(), 2 * 1024 * 1024).map_err(|_| invalid())?;
        let record: Record = serde_json::from_value(value).map_err(|_| invalid())?;
        let selected = Snapshot::capture(target)?;
        if record.version != 1
            || record.id.len() != 32
            || !record.id.bytes().all(|b| b.is_ascii_hexdigit())
            || record.target != selected.path()
            || record.target == journal.path()
            || (!record.original_existed && !record.original.is_empty())
            || record.original.len() > 512 * 1024
            || hash(record.original.as_bytes()) != record.original_sha256
            || hash(record.candidate.as_bytes()) != record.candidate_sha256
        {
            return Err(invalid());
        }
        let (_, candidate) = RoutePatch::plan(&record.original, record.port, &record.capability)
            .map_err(|_| invalid())?;
        if candidate != record.candidate {
            return Err(invalid());
        }
        Ok(Self {
            _lock: lock,
            record,
            journal,
            prepared: None,
        })
    }

    pub fn phase(&self) -> Phase {
        self.record.phase
    }

    pub fn recovery(&self) -> io::Result<Recovery> {
        let current = Snapshot::capture(&self.record.target)?;
        if current.existed() && current.original() == self.record.candidate.as_bytes() {
            Ok(Recovery::Candidate)
        } else if current.existed() == self.record.original_existed
            && current.original() == self.record.original.as_bytes()
        {
            Ok(Recovery::Original)
        } else {
            Ok(Recovery::Changed)
        }
    }

    pub fn apply(&mut self) -> io::Result<()> {
        self.journal.verify_unchanged()?;
        let prepared = self
            .prepared
            .take()
            .ok_or_else(|| io::Error::other("E_PREFLIGHT_REQUIRED"))?;
        // The staged basename is deterministically bound to the durable receipt.
        let name = format!(".cxweb-{}-config.tmp", self.record.id);
        let staged = prepared.stage(&name, self.record.candidate.as_bytes())?;
        prepared.commit(&staged, self.record.candidate.as_bytes())?;
        self.record.phase = Phase::ConfigApplied;
        let result = replace(
            &self.journal,
            &serde_json::to_vec(&self.record).map_err(|_| invalid())?,
        );
        if result.is_err() {
            self.record.phase = Phase::Prepared;
        }
        result?;
        self.journal = Snapshot::capture(self.journal.path())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const CAP: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    struct Fixture {
        root: PathBuf,
        state: PathBuf,
        target: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("cxweb-journal-{:032x}", rand::random::<u128>()));
            protected_directory(&root).unwrap();
            let state = root.join("state");
            let target = root.join("config.toml");
            Self {
                root,
                state,
                target,
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.root).unwrap();
        }
    }
    #[test]
    fn prepared_is_durable_before_config_and_cannot_resume_without_preflight() {
        let f = Fixture::new();
        let journal = ConfigJournal::prepare(&f.state, &f.target, 12345, CAP).unwrap();
        assert!(!f.target.exists());
        assert!(ConfigJournal::reopen(&f.state, &f.target).is_err());
        drop(journal);
        let mut recovered = ConfigJournal::reopen(&f.state, &f.target).unwrap();
        assert_eq!(recovered.phase(), Phase::Prepared);
        assert_eq!(recovered.recovery().unwrap(), Recovery::Original);
        assert!(recovered.apply().is_err());
        assert!(!f.target.exists());
    }
    #[test]
    fn committed_config_and_later_user_edits_have_distinct_recovery() {
        let f = Fixture::new();
        let mut journal = ConfigJournal::prepare(&f.state, &f.target, 12345, CAP).unwrap();
        journal.apply().unwrap();
        drop(journal);
        let journal = ConfigJournal::reopen(&f.state, &f.target).unwrap();
        assert_eq!(journal.phase(), Phase::ConfigApplied);
        assert_eq!(journal.recovery().unwrap(), Recovery::Candidate);
        std::fs::write(&f.target, "# user's later edit\nmodel='native'\n").unwrap();
        assert_eq!(journal.recovery().unwrap(), Recovery::Changed);
        assert!(
            std::fs::read_to_string(&f.target)
                .unwrap()
                .contains("user's later edit")
        );
    }
    #[test]
    fn crash_between_config_and_receipt_is_detected_without_rewriting() {
        let f = Fixture::new();
        let journal = ConfigJournal::prepare(&f.state, &f.target, 12345, CAP).unwrap();
        let prepared = journal.prepared.as_ref().unwrap();
        let staged = prepared
            .stage(".cxweb-crash.tmp", journal.record.candidate.as_bytes())
            .unwrap();
        prepared
            .commit(&staged, journal.record.candidate.as_bytes())
            .unwrap();
        drop(journal);
        let recovered = ConfigJournal::reopen(&f.state, &f.target).unwrap();
        assert_eq!(recovered.phase(), Phase::Prepared);
        assert_eq!(recovered.recovery().unwrap(), Recovery::Candidate);
    }
    #[test]
    fn corrupt_or_redirected_receipt_is_rejected_and_user_edits_survive() {
        let f = Fixture::new();
        std::fs::write(&f.target, "model='native'\n").unwrap();
        let mut journal = ConfigJournal::prepare(&f.state, &f.target, 12345, CAP).unwrap();
        std::fs::write(&f.target, "model='user-edit'\n").unwrap();
        assert!(journal.apply().is_err());
        assert_eq!(journal.recovery().unwrap(), Recovery::Changed);
        drop(journal);
        assert!(ConfigJournal::reopen(&f.state, &f.root.join("other.toml")).is_err());
        std::fs::write(f.state.join("integration.json"), b"{broken").unwrap();
        assert!(ConfigJournal::reopen(&f.state, &f.target).is_err());
        assert_eq!(
            std::fs::read_to_string(&f.target).unwrap(),
            "model='user-edit'\n"
        );
    }

    #[test]
    fn replacing_journal_or_same_content_config_invalidates_live_preflight() {
        for replace_journal in [false, true] {
            let f = Fixture::new();
            std::fs::write(&f.target, "model='native'\n").unwrap();
            let mut journal = ConfigJournal::prepare(&f.state, &f.target, 12345, CAP).unwrap();
            let changed = if replace_journal {
                f.state.join("integration.json")
            } else {
                f.target.clone()
            };
            let bytes = std::fs::read(&changed).unwrap();
            std::fs::rename(&changed, f.root.join("keep-old-identity")).unwrap();
            std::fs::write(&changed, bytes).unwrap();
            assert!(journal.apply().is_err());
            assert_eq!(
                std::fs::read_to_string(&f.target).unwrap(),
                "model='native'\n"
            );
        }
    }

    #[test]
    fn absent_and_empty_are_distinct_and_modified_candidates_are_rejected() {
        let f = Fixture::new();
        let journal = ConfigJournal::prepare(&f.state, &f.target, 12345, CAP).unwrap();
        std::fs::write(&f.target, "").unwrap();
        assert_eq!(journal.recovery().unwrap(), Recovery::Changed);
        drop(journal);
        let path = f.state.join("integration.json");
        let mut record: Record = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        record.candidate.push_str("approval_policy='never'\n");
        record.candidate_sha256 = hash(record.candidate.as_bytes());
        std::fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(ConfigJournal::reopen(&f.state, &f.target).is_err());
        assert_eq!(std::fs::read(&f.target).unwrap(), b"");
    }
}
