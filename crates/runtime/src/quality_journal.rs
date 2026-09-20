//! Content-free first-attempt evidence. Persist before external work, never retry
//! an interrupted case, and bind a run to one immutable execution environment.
use cxweb_codex_adapter::quality_corpus::{
    self, Assessment,
    tally::{Failure, Outcome, Report, Tally},
};
use cxweb_platform::{
    atomic_file::Snapshot, state::protected_directory, target_path::TargetPathGuard,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

const SPACING: u64 = 60;
const RATE_BACKOFF: u64 = 900;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub corpus_sha256: String,
    pub runtime_sha256: String,
    /// Hash of installation, account, workspace and epoch, never raw account data.
    pub scope_sha256: String,
    /// Hash of the selected model identity and label.
    pub route_sha256: String,
    pub browser_version: String,
    pub effort: String,
}

impl Identity {
    fn validate(&self) -> Result<(), &'static str> {
        let manifest = quality_corpus::manifest()?;
        if manifest["corpus_sha256"].as_str() != Some(&self.corpus_sha256)
            || [&self.runtime_sha256, &self.scope_sha256, &self.route_sha256]
                .iter()
                .any(|hash| hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()))
            || !matches!(
                self.effort.as_str(),
                "low" | "medium" | "high" | "xhigh" | "max"
            )
            || self.browser_version.is_empty()
            || self.browser_version.len() > 128
            || self.browser_version.chars().any(char::is_control)
        {
            return Err("E_QUALITY_IDENTITY");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResultRecord {
    Response {
        protocol_valid: bool,
        task_passed: bool,
    },
    Failure {
        reason: Failure,
    },
}
impl ResultRecord {
    fn outcome(&self) -> Outcome {
        match *self {
            Self::Response {
                protocol_valid,
                task_passed,
            } => Outcome::Response(Assessment {
                protocol_valid,
                task_passed,
                failure: None,
            }),
            Self::Failure { reason } => Outcome::Failure(reason),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    pub started_at: u64,
    pub submitted_at: Option<u64>,
    pub finished_at: Option<u64>,
    pub result: Option<ResultRecord>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    identity: Identity,
    not_before: u64,
    attempts: BTreeMap<String, Attempt>,
}

/// The OS releases the exclusive lock even if the owner crashes. The next owner
/// converts unfinished attempts to failures before another case can be admitted.
pub struct Journal {
    path: PathBuf,
    guard: TargetPathGuard,
    record: Record,
    persisted: Option<Vec<u8>>,
    _lock: File,
}

impl Journal {
    pub fn open(
        directory: &Path,
        run: &str,
        identity: Identity,
        now: u64,
    ) -> Result<Self, &'static str> {
        identity.validate()?;
        if run.is_empty()
            || run.len() > 64
            || !run
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err("E_QUALITY_RUN");
        }
        let directory = directory.join(format!("quality-{run}"));
        protected_directory(&directory).map_err(|_| "E_QUALITY_PATH")?;
        let guard = TargetPathGuard::capture(&directory, true).map_err(|_| "E_QUALITY_PATH")?;
        let lock_path = directory.join("owner.lock");
        // Snapshot validates the leaf and ancestors before opening an existing lock.
        Snapshot::capture(&lock_path).map_err(|_| "E_QUALITY_PATH")?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(&lock_path)
            .map_err(|_| "E_QUALITY_BUSY")?;
        guard.verify_unchanged().map_err(|_| "E_QUALITY_PATH")?;
        let path = directory.join("attempts.json");
        let snapshot = Snapshot::capture(&path).map_err(|_| "E_QUALITY_PATH")?;
        let record = if snapshot.existed() {
            let record: Record =
                serde_json::from_slice(snapshot.original()).map_err(|_| "E_QUALITY_RECORD")?;
            if record.version != 1 || record.identity != identity {
                return Err("E_QUALITY_IDENTITY_CHANGED");
            }
            record
        } else {
            Record {
                version: 1,
                identity,
                not_before: 0,
                attempts: BTreeMap::new(),
            }
        };
        validate(&record)?;
        let mut journal = Self {
            path,
            guard,
            record,
            persisted: snapshot.existed().then(|| snapshot.original().to_vec()),
            _lock: lock,
        };
        let mut recovered = journal.record.clone();
        let mut interrupted = false;
        for attempt in recovered.attempts.values_mut() {
            if attempt.result.is_none() {
                attempt.finished_at = Some(
                    now.max(attempt.started_at)
                        .max(attempt.submitted_at.unwrap_or(0)),
                );
                attempt.result = Some(ResultRecord::Failure {
                    reason: Failure::Interrupted,
                });
                interrupted = true;
            }
        }
        if interrupted {
            recovered.not_before = recovered.not_before.max(now.saturating_add(RATE_BACKOFF));
        }
        if interrupted || !snapshot.existed() {
            journal.save(recovered)?;
        }
        Ok(journal)
    }

    fn save(&mut self, record: Record) -> Result<(), &'static str> {
        validate(&record)?;
        self.guard
            .verify_unchanged()
            .map_err(|_| "E_QUALITY_PATH")?;
        let snapshot = Snapshot::capture(&self.path).map_err(|_| "E_QUALITY_PATH")?;
        if snapshot.existed() != self.persisted.is_some()
            || self
                .persisted
                .as_deref()
                .is_some_and(|bytes| bytes != snapshot.original())
        {
            return Err("E_QUALITY_CHANGED");
        }
        let bytes = serde_json::to_vec_pretty(&record).map_err(|_| "E_QUALITY_RECORD")?;
        let name = format!(".cxweb-quality-{:032x}.tmp", rand::random::<u128>());
        let staged = snapshot
            .stage(&name, &bytes)
            .map_err(|_| "E_QUALITY_WRITE")?;
        snapshot
            .commit(&staged, &bytes)
            .map_err(|_| "E_QUALITY_WRITE")?;
        // Re-read after replacement before acknowledging a durable transition.
        let committed = Snapshot::capture(&self.path).map_err(|_| "E_QUALITY_WRITE")?;
        if committed.original() != bytes {
            return Err("E_QUALITY_CHANGED");
        }
        self.persisted = Some(bytes);
        self.record = record;
        Ok(())
    }

    /// Returns the next frozen case only after its first attempt is durable.
    pub fn begin_next(&mut self, now: u64) -> Result<Option<quality_corpus::Case>, &'static str> {
        if self
            .record
            .attempts
            .values()
            .any(|attempt| attempt.result.is_none())
        {
            return Err("E_QUALITY_PENDING");
        }
        if now < self.record.not_before {
            return Err("E_QUALITY_BACKOFF");
        }
        let Some(case) = quality_corpus::cases()
            .into_iter()
            .find(|case| !self.record.attempts.contains_key(&case.id))
        else {
            return Ok(None);
        };
        let mut record = self.record.clone();
        record.attempts.insert(
            case.id.clone(),
            Attempt {
                started_at: now,
                submitted_at: None,
                finished_at: None,
                result: None,
            },
        );
        self.save(record)?;
        Ok(Some(case))
    }

    pub fn submitting(&mut self, id: &str, now: u64) -> Result<(), &'static str> {
        let mut record = self.record.clone();
        let attempt = record.attempts.get_mut(id).ok_or("E_QUALITY_ATTEMPT")?;
        if attempt.result.is_some() || attempt.submitted_at.is_some() || now < attempt.started_at {
            return Err("E_QUALITY_ATTEMPT");
        }
        attempt.submitted_at = Some(now);
        self.save(record)
    }

    pub fn finish(&mut self, id: &str, result: ResultRecord, now: u64) -> Result<(), &'static str> {
        let mut record = self.record.clone();
        let attempt = record.attempts.get_mut(id).ok_or("E_QUALITY_ATTEMPT")?;
        if attempt.result.is_some()
            || now < attempt.started_at
            || now < attempt.submitted_at.unwrap_or(0)
            || (matches!(result, ResultRecord::Response { .. }) && attempt.submitted_at.is_none())
        {
            return Err("E_QUALITY_ATTEMPT");
        }
        let backoff = if matches!(
            result,
            ResultRecord::Failure {
                reason: Failure::RateLimited
            }
        ) {
            RATE_BACKOFF
        } else {
            SPACING
        };
        attempt.finished_at = Some(now);
        attempt.result = Some(result);
        record.not_before = now.saturating_add(backoff);
        self.save(record)
    }

    pub fn report(&self) -> Result<serde_json::Value, &'static str> {
        Ok(serde_json::json!({
            "version": quality_corpus::VERSION,
            "identity": self.record.identity,
            "not_before": self.record.not_before,
            "attempts": self.record.attempts,
            "summary": validate(&self.record)?,
            "native_client_exercised": false,
            "account_plan_category": null,
        }))
    }
}

fn validate(record: &Record) -> Result<Report, &'static str> {
    let mut tally = Tally::default();
    let mut pending = 0;
    for (id, attempt) in &record.attempts {
        tally.begin(id)?;
        if attempt
            .submitted_at
            .is_some_and(|time| time < attempt.started_at)
            || attempt.finished_at.is_some_and(|time| {
                time < attempt.started_at || time < attempt.submitted_at.unwrap_or(0)
            })
            || attempt.result.is_some() != attempt.finished_at.is_some()
        {
            return Err("E_QUALITY_RECORD");
        }
        if let Some(result) = &attempt.result {
            if matches!(result, ResultRecord::Response { .. }) && attempt.submitted_at.is_none() {
                return Err("E_QUALITY_RECORD");
            }
            tally.finish(id, result.outcome())?;
        } else {
            pending += 1;
        }
    }
    if pending > 1 {
        return Err("E_QUALITY_RECORD");
    }
    Ok(tally.report())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (PathBuf, Identity) {
        let path =
            std::env::temp_dir().join(format!("cxweb-quality-{:032x}", rand::random::<u128>()));
        protected_directory(&path).unwrap();
        (
            path,
            Identity {
                corpus_sha256: quality_corpus::manifest().unwrap()["corpus_sha256"]
                    .as_str()
                    .unwrap()
                    .into(),
                runtime_sha256: "1".repeat(64),
                scope_sha256: "2".repeat(64),
                route_sha256: "3".repeat(64),
                browser_version: "Chrome/153.0.8010.48".into(),
                effort: "xhigh".into(),
            },
        )
    }

    #[test]
    fn start_and_submission_are_on_disk_before_return_and_external_edits_are_preserved() {
        let (path, identity) = setup();
        let mut journal = Journal::open(&path, "durable", identity, 1).unwrap();
        let case = journal.begin_next(1).unwrap().unwrap();
        let read = || -> Record {
            serde_json::from_slice(
                &std::fs::read(path.join("quality-durable/attempts.json")).unwrap(),
            )
            .unwrap()
        };
        assert!(read().attempts[&case.id].result.is_none());
        journal.submitting(&case.id, 2).unwrap();
        assert_eq!(read().attempts[&case.id].submitted_at, Some(2));
        std::fs::write(&journal.path, b"external change").unwrap();
        assert_eq!(
            journal.finish(
                &case.id,
                ResultRecord::Failure {
                    reason: Failure::Timeout
                },
                3
            ),
            Err("E_QUALITY_CHANGED")
        );
        assert_eq!(std::fs::read(&journal.path).unwrap(), b"external change");
        drop(journal);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn crash_after_submission_is_terminal_and_cannot_be_retried() {
        let (path, identity) = setup();
        let mut journal = Journal::open(&path, "crash", identity.clone(), 10).unwrap();
        let first = journal.begin_next(10).unwrap().unwrap();
        journal.submitting(&first.id, 11).unwrap();
        assert!(Journal::open(&path, "crash", identity.clone(), 12).is_err());
        drop(journal);
        let mut journal = Journal::open(&path, "crash", identity, 12).unwrap();
        let report = journal.report().unwrap();
        assert_eq!(report["summary"]["counts"]["attempted"], 1);
        assert_eq!(report["summary"]["failures"]["interrupted"], 1);
        assert!(matches!(journal.begin_next(13), Err("E_QUALITY_BACKOFF")));
        let next = journal.begin_next(912).unwrap().unwrap();
        assert_ne!(next.id, first.id);
        assert!(journal.submitting(&first.id, 913).is_err());
        assert!(
            journal
                .finish(
                    &first.id,
                    ResultRecord::Failure {
                        reason: Failure::Timeout
                    },
                    914
                )
                .is_err()
        );
        drop(journal);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn changed_identity_does_not_recover_or_mutate_existing_attempts() {
        let (path, identity) = setup();
        let mut journal = Journal::open(&path, "fixed", identity.clone(), 1).unwrap();
        journal.begin_next(1).unwrap();
        let record_path = journal.path.clone();
        drop(journal);
        let before = std::fs::read(&record_path).unwrap();
        let mut changed = identity;
        changed.effort = "high".into();
        assert!(matches!(
            Journal::open(&path, "fixed", changed, 2),
            Err("E_QUALITY_IDENTITY_CHANGED")
        ));
        assert_eq!(before, std::fs::read(record_path).unwrap());
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn rate_limit_backoff_and_first_attempt_result_survive_reopening() {
        let (path, identity) = setup();
        let mut journal = Journal::open(&path, "limited", identity.clone(), 1).unwrap();
        let case = journal.begin_next(1).unwrap().unwrap();
        journal
            .finish(
                &case.id,
                ResultRecord::Failure {
                    reason: Failure::RateLimited,
                },
                2,
            )
            .unwrap();
        drop(journal);
        let mut journal = Journal::open(&path, "limited", identity, 3).unwrap();
        assert!(matches!(journal.begin_next(901), Err("E_QUALITY_BACKOFF")));
        let next = journal.begin_next(902).unwrap().unwrap();
        assert_ne!(next.id, case.id);
        assert!(
            journal
                .finish(
                    &next.id,
                    ResultRecord::Response {
                        protocol_valid: true,
                        task_passed: true
                    },
                    903
                )
                .is_err()
        );
        journal.submitting(&next.id, 903).unwrap();
        assert!(
            journal
                .finish(
                    &next.id,
                    ResultRecord::Response {
                        protocol_valid: false,
                        task_passed: true
                    },
                    904
                )
                .is_err()
        );
        journal
            .finish(
                &next.id,
                ResultRecord::Response {
                    protocol_valid: true,
                    task_passed: false,
                },
                904,
            )
            .unwrap();
        let report = journal.report().unwrap();
        assert_eq!(report["summary"]["counts"]["attempted"], 2);
        assert_eq!(report["summary"]["counts"]["protocol_valid"], 1);
        assert_eq!(report["summary"]["counts"]["task_passed"], 0);
        assert_eq!(report["summary"]["failures"]["rate_limited"], 1);
        assert_eq!(report["native_client_exercised"], false);
        drop(journal);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn unknown_case_and_inconsistent_record_are_refused_without_repair() {
        let (path, identity) = setup();
        let journal = Journal::open(&path, "corrupt", identity.clone(), 1).unwrap();
        let record_path = journal.path.clone();
        let mut record = journal.record.clone();
        drop(journal);
        record.attempts.insert(
            "unknown-case".into(),
            Attempt {
                started_at: 1,
                submitted_at: None,
                finished_at: None,
                result: None,
            },
        );
        let bytes = serde_json::to_vec(&record).unwrap();
        std::fs::write(&record_path, &bytes).unwrap();
        assert!(Journal::open(&path, "corrupt", identity, 2).is_err());
        assert_eq!(bytes, std::fs::read(record_path).unwrap());
        std::fs::remove_dir_all(path).unwrap();
    }
}
