//! Durable preparation and recovery evidence for a selected configuration file.
//! The setup owner supplies a verified target; the journal never discovers one.
//! The private journal contains the original configuration and live route capability.
use cxweb_codex_adapter::{config::RoutePatch, strict_json};
use cxweb_platform::{
    atomic_file::Snapshot,
    scheduled_runtime::{RegisteredRuntime, RegistrationReceipt, TaskPlan, task_name},
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
    Disconnecting,
    ConfigRestored,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Recovery {
    /// Full preflight is required before any new installation attempt.
    Original,
    /// Route is on disk; this says nothing about daemon health or client readiness.
    Candidate,
    /// Preserve user changes. A later disconnect must use key-level three-way undo.
    Changed,
    /// The recorded disconnect result is on disk; existing clients may still
    /// need the native-only compatibility listener until they restart.
    Restored,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Undo {
    before: String,
    before_existed: bool,
    after: Option<String>,
    published_routes: Vec<String>,
    native_models: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
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
    #[serde(default)]
    undo: Option<Undo>,
    #[serde(default)]
    catalog: Option<CatalogReceipt>,
    #[serde(default)]
    scheduler: Option<SchedulerRecord>,
    #[serde(default)]
    web: Option<serde_json::Value>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SchedulerRecord {
    name: String,
    planned_xml: String,
    receipt: Option<RegistrationReceipt>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogReceipt {
    published: Vec<String>,
    native: Vec<String>,
}
impl CatalogReceipt {
    fn valid(&self) -> bool {
        use cxweb_domain::OWNED_MODEL_PREFIX;
        let ids = self
            .published
            .iter()
            .chain(&self.native)
            .collect::<Vec<_>>();
        self.published.len() <= 256
            && self.native.len() <= 1024
            && ids
                .iter()
                .all(|id| !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control))
            && ids.iter().collect::<std::collections::HashSet<_>>().len() == ids.len()
            && self
                .published
                .iter()
                .all(|id| id.starts_with(OWNED_MODEL_PREFIX) && id.len() > OWNED_MODEL_PREFIX.len())
            && self
                .native
                .iter()
                .all(|id| !id.starts_with(OWNED_MODEL_PREFIX))
    }
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
    /// Read only the local attachment identity from an atomic journal snapshot.
    /// This does not acquire the writer's lock, inspect auth files or authorize a
    /// config write. Original config and route capabilities never leave here.
    pub(crate) fn control_target(directory: &Path) -> io::Result<Option<(String, PathBuf)>> {
        let _guard = cxweb_platform::target_path::TargetPathGuard::capture(directory, true)?;
        protected_directory(directory)?;
        let snapshot = Snapshot::capture(&directory.join("integration.json"))?;
        if !snapshot.existed() {
            return Ok(None);
        }
        let value =
            strict_json::parse(snapshot.original(), 2 * 1024 * 1024).map_err(|_| invalid())?;
        let record: Record = serde_json::from_value(value).map_err(|_| invalid())?;
        if !matches!(record.version, 1 | 2)
            || record.id.len() != 32
            || !record
                .id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || !record.target.is_absolute()
            || record
                .target
                .file_name()
                .is_none_or(|name| name != "config.toml")
            || hash(record.original.as_bytes()) != record.original_sha256
            || hash(record.candidate.as_bytes()) != record.candidate_sha256
            || plan_record(&record)?.1 != record.candidate
        {
            return Err(invalid());
        }
        // A started activation can retain a host or scheduler before config is
        // committed. Keep it discoverable for recovery/disconnect; diagnostic
        // reservations without a bound provider remain invisible.
        if record.phase == Phase::Prepared && record.web.is_none() && record.scheduler.is_none() {
            return Ok(None);
        }
        Ok(Some((
            record.id,
            record.target.parent().ok_or_else(invalid)?.into(),
        )))
    }

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
            version: 2,
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
            undo: None,
            catalog: None,
            scheduler: None,
            web: None,
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
        if !matches!(record.version, 1 | 2)
            || record.id.len() != 32
            || !record.id.bytes().all(|b| b.is_ascii_hexdigit())
            || record.target != selected.path()
            || record.target == journal.path()
            || (!record.original_existed && !record.original.is_empty())
            || record.original.len() > 512 * 1024
            || hash(record.original.as_bytes()) != record.original_sha256
            || hash(record.candidate.as_bytes()) != record.candidate_sha256
            || record
                .catalog
                .as_ref()
                .is_some_and(|catalog| !catalog.valid())
        {
            return Err(invalid());
        }
        let (_, candidate) = plan_record(&record)?;
        if candidate != record.candidate {
            return Err(invalid());
        }
        if let Some(scheduler) = &record.scheduler {
            if scheduler.name != task_name(&record.id)?
                || scheduler.planned_xml.is_empty()
                || scheduler.planned_xml.len() > 128 * 1024
                || scheduler.planned_xml.contains('\0')
            {
                return Err(invalid());
            }
            if let Some(receipt) = &scheduler.receipt
                && !receipt.matches_plan(&record.id, &scheduler.planned_xml)?
            {
                return Err(invalid());
            }
        }
        match (&record.undo, record.phase) {
            (None, Phase::Prepared | Phase::ConfigApplied) => {}
            (Some(undo), Phase::Disconnecting | Phase::ConfigRestored) => {
                if undo.before.len() > 512 * 1024
                    || (!undo.before_existed && !undo.before.is_empty())
                    || undo.after
                        != restored_text(
                            &record,
                            &undo.before,
                            undo.before_existed,
                            &undo.published_routes,
                            &undo.native_models,
                        )?
                {
                    return Err(invalid());
                }
            }
            _ => return Err(invalid()),
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

    pub(crate) fn directory(&self) -> &Path {
        self.journal.path().parent().expect("journal directory")
    }

    pub fn installation_id(&self) -> &str {
        &self.record.id
    }

    /// Persist the plan before creating any external startup entry. An uncertain
    /// registration leaves this record pending; never overwrite/adopt a task by
    /// name alone after a crash.
    pub fn prepare_scheduler(&mut self, executable: &Path) -> io::Result<TaskPlan> {
        if self.record.phase != Phase::Prepared
            || self.prepared.is_none()
            || self.record.scheduler.is_some()
            || self.record.catalog.is_none()
        {
            return Err(io::Error::other("E_SUPERVISION_STATE"));
        }
        let plan = TaskPlan::new(
            &self.record.id,
            executable,
            self.journal.path().parent().ok_or_else(invalid)?,
            &self.record.target,
        )?;
        let mut next = self.record.clone();
        next.scheduler = Some(SchedulerRecord {
            name: plan.name().into(),
            planned_xml: plan.xml().into(),
            receipt: None,
        });
        self.store(next)?;
        Ok(plan)
    }

    pub fn record_scheduler(&mut self, task: &RegisteredRuntime) -> io::Result<()> {
        let scheduler = self.record.scheduler.as_ref().ok_or_else(invalid)?;
        let receipt = task.receipt();
        if scheduler.receipt.is_some()
            || !receipt.matches_plan(&self.record.id, &scheduler.planned_xml)?
        {
            return Err(io::Error::other("E_SUPERVISION_STATE"));
        }
        // Validate live identity again before accepting the durable receipt.
        receipt.reopen(&self.record.id)?;
        let mut next = self.record.clone();
        next.scheduler.as_mut().ok_or_else(invalid)?.receipt = Some(receipt);
        self.store(next)
    }

    pub fn registered_scheduler(&self) -> io::Result<RegisteredRuntime> {
        self.journal.verify_unchanged()?;
        self.record
            .scheduler
            .as_ref()
            .and_then(|record| record.receipt.as_ref())
            .ok_or_else(|| io::Error::other("E_SUPERVISION_PENDING"))?
            .reopen(&self.record.id)
    }

    /// Persist exact, previously qualified catalog IDs before exposing the route
    /// to clients. This records evidence supplied by qualification; it cannot
    /// qualify a model or authorize browser generation by itself.
    pub fn record_catalog(
        &mut self,
        published: Vec<String>,
        native: Vec<String>,
    ) -> io::Result<()> {
        if self.record.phase != Phase::Prepared
            || self.prepared.is_none()
            || self.record.catalog.is_some()
        {
            return Err(io::Error::other("E_CATALOG_RECEIPT_STATE"));
        }
        let catalog = CatalogReceipt { published, native };
        if !catalog.valid() {
            return Err(io::Error::other("E_CATALOG_RECEIPT"));
        }
        let mut next = self.record.clone();
        next.catalog = Some(catalog);
        self.store(next)
    }

    /// Only the live activation owner records previously qualified browser and
    /// client bindings. This record contains hashes, never account credentials.
    pub(crate) fn record_web(&mut self, receipt: crate::web_recovery::Receipt) -> io::Result<()> {
        if self.record.phase != Phase::Prepared
            || self.prepared.is_none()
            || self.record.web.is_some()
        {
            return Err(invalid());
        }
        let (published, _) = self.catalog_receipt()?;
        receipt
            .validate(&self.record.id, &published)
            .map_err(io::Error::other)?;
        let mut next = self.record.clone();
        next.web = Some(serde_json::to_value(receipt).map_err(|_| invalid())?);
        self.store(next)
    }

    /// Malformed or obsolete web evidence cannot stop native forwarding or undo.
    /// Reopening a prepared/interrupted install never authorizes web activation.
    pub(crate) fn web_recovery(&self) -> io::Result<Option<crate::web_recovery::Receipt>> {
        self.journal.verify_unchanged()?;
        if self.record.phase != Phase::ConfigApplied {
            return Ok(None);
        }
        let Some(raw) = self.record.web.clone() else {
            return Ok(None);
        };
        let Ok(receipt) = serde_json::from_value::<crate::web_recovery::Receipt>(raw) else {
            return Ok(None);
        };
        let (published, _) = self.catalog_receipt()?;
        if receipt.validate(&self.record.id, &published).is_err() {
            return Ok(None);
        }
        let current = Snapshot::capture(&self.record.target)?;
        let text = std::str::from_utf8(current.original()).map_err(|_| invalid())?;
        if !plan_record(&self.record)?.0.can_resume(text) {
            return Ok(None);
        }
        Ok(Some(receipt))
    }

    pub(crate) fn catalog_receipt(&self) -> io::Result<(Vec<String>, Vec<String>)> {
        let receipt = self
            .record
            .catalog
            .as_ref()
            .ok_or_else(|| io::Error::other("E_CATALOG_RECEIPT_MISSING"))?;
        Ok((receipt.published.clone(), receipt.native.clone()))
    }

    pub(crate) fn runtime_route(&self) -> (u16, &str, &str) {
        (self.record.port, &self.record.capability, &self.record.id)
    }

    /// Match the exact live listener, including its private capability, without
    /// exporting that capability through lifecycle status or diagnostics.
    pub fn routes_to(&self, base_url: &str) -> bool {
        // The recovered listener serves both layouts under the same capability;
        // this does not rewrite or upgrade the original configuration receipt.
        ["/v1", "/backend-api/codex"].iter().any(|suffix| {
            base_url
                == format!(
                    "http://127.0.0.1:{}/wb/{}{suffix}",
                    self.record.port, self.record.capability
                )
        })
    }

    pub fn recovery(&self) -> io::Result<Recovery> {
        let current = Snapshot::capture(&self.record.target)?;
        if let Some(undo) = &self.record.undo
            && matches_result(&current, undo.after.as_deref())
        {
            return Ok(Recovery::Restored);
        }
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

    /// Production activation requires current access evidence for both writes.
    /// A read-only preflight report or durable journal cannot supply this proof.
    /// Refusal happens before taking the prepared snapshot or creating staging.
    pub(crate) fn apply_qualified(&mut self) -> io::Result<()> {
        self.journal.verify_unchanged()?;
        self.prepared
            .as_ref()
            .ok_or_else(|| io::Error::other("E_PREFLIGHT_REQUIRED"))?
            .verify_unchanged()?;
        self.prepared
            .as_mut()
            .ok_or_else(|| io::Error::other("E_PREFLIGHT_REQUIRED"))?
            .require_ancestor_access()
            .map_err(|_| io::Error::other("E_ACTIVATION_TARGET_PERMISSIONS"))?;
        self.journal
            .require_ancestor_access()
            .map_err(|_| io::Error::other("E_ACTIVATION_TARGET_PERMISSIONS"))?;
        // Native clients cache catalog rows independently of the route URL.
        // Remove only the selected home's regenerable cache, through a checked
        // handle. Failure leaves config unapplied; no auth/history is touched.
        invalidate_model_cache(self.record.target.parent().ok_or_else(invalid)?)
            .map_err(|_| io::Error::other("E_CATALOG_CACHE"))?;
        self.apply()
    }

    /// Low-level journal transaction; production activation uses apply_qualified.
    /// This entry point also supports isolated journal/recovery fixtures.
    pub fn apply(&mut self) -> io::Result<()> {
        self.journal.verify_unchanged()?;
        if self
            .record
            .scheduler
            .as_ref()
            .is_some_and(|scheduler| scheduler.receipt.is_none())
        {
            return Err(io::Error::other("E_SUPERVISION_PENDING"));
        }
        if self.record.scheduler.is_some() {
            self.registered_scheduler()?;
        }
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

    fn store(&mut self, next: Record) -> io::Result<()> {
        self.journal.verify_unchanged()?;
        replace(
            &self.journal,
            &serde_json::to_vec(&next).map_err(|_| invalid())?,
        )?;
        self.journal = Snapshot::capture(self.journal.path())?;
        self.record = next;
        Ok(())
    }

    fn prepare_disconnect(
        &mut self,
        published: &[String],
        native: &[String],
    ) -> io::Result<Snapshot> {
        self.journal.verify_unchanged()?;
        let current = Snapshot::capture(&self.record.target)?;
        if current.original().len() > 512 * 1024 {
            return Err(invalid());
        }
        let before = std::str::from_utf8(current.original()).map_err(|_| invalid())?;
        let after = restored_text(&self.record, before, current.existed(), published, native)?;
        let mut next = self.record.clone();
        next.phase = Phase::Disconnecting;
        next.undo = Some(Undo {
            before: before.into(),
            before_existed: current.existed(),
            after,
            published_routes: published.into(),
            native_models: native.into(),
        });
        self.prepared = None;
        self.store(next)?;
        Ok(current)
    }

    fn apply_disconnect(&self, current: &Snapshot) -> io::Result<()> {
        self.journal.verify_unchanged()?;
        let undo = self.record.undo.as_ref().ok_or_else(invalid)?;
        match &undo.after {
            Some(text) if current.original() != text.as_bytes() || !current.existed() => {
                replace(current, text.as_bytes())?
            }
            None if current.existed() => current.remove()?,
            _ => current.verify_unchanged()?,
        }
        if !matches_result(
            &Snapshot::capture(&self.record.target)?,
            undo.after.as_deref(),
        ) {
            return Err(io::Error::other("E_CONFIG_POST_COMMIT_CHANGED"));
        }
        Ok(())
    }

    /// Call after rejecting/draining web work. Persist exact catalog receipts
    /// supplied by the qualified runtime, then undo only still-owned keys.
    /// This does not stop the listener or declare already-running clients updated.
    pub fn disconnect(&mut self, published: &[String], native: &[String]) -> io::Result<()> {
        if self.record.undo.is_some() && self.recovery()? == Recovery::Restored {
            let mut next = self.record.clone();
            next.phase = Phase::ConfigRestored;
            return self.store(next);
        }
        if self.record.phase == Phase::ConfigRestored {
            return Err(io::Error::other("E_CONFIG_CHANGED"));
        }
        // After a crash this is a fresh three-way plan against current bytes,
        // never a replay of the old whole-file candidate over later user edits.
        let current = self.prepare_disconnect(published, native)?;
        self.apply_disconnect(&current)?;
        let mut next = self.record.clone();
        next.phase = Phase::ConfigRestored;
        self.store(next)
    }
}

fn invalidate_model_cache(home: &Path) -> io::Result<()> {
    let cache = Snapshot::capture(&home.join("models_cache.json"))?;
    if cache.existed() {
        cache.remove()?;
    }
    Ok(())
}

fn matches_result(current: &Snapshot, text: Option<&str>) -> bool {
    match text {
        Some(text) => current.existed() && current.original() == text.as_bytes(),
        None => !current.existed(),
    }
}

fn plan_record(record: &Record) -> io::Result<(RoutePatch, String)> {
    match record.version {
        1 => RoutePatch::legacy_plan(&record.original, record.port, &record.capability),
        2 => RoutePatch::plan(&record.original, record.port, &record.capability),
        _ => return Err(invalid()),
    }
    .map_err(|_| invalid())
}

fn restored_text(
    record: &Record,
    current: &str,
    existed: bool,
    published: &[String],
    native: &[String],
) -> io::Result<Option<String>> {
    let (patch, _) = plan_record(record)?;
    let restored = patch
        .remove_with_selection(current, published, native)
        .map_err(|_| io::Error::other("E_CONFIG_CONFLICT"))?;
    if !existed || (!record.original_existed && restored.is_empty() && current != restored) {
        Ok(None)
    } else {
        Ok(Some(restored))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refresh_removes_only_the_selected_homes_model_cache_and_refuses_links() {
        let f = Fixture::new();
        let cache = f.root.join("models_cache.json");
        let auth = f.root.join("auth.json");
        let history = f.root.join("history.jsonl");
        std::fs::write(&auth, "synthetic auth fixture").unwrap();
        std::fs::write(&history, "synthetic history fixture").unwrap();
        invalidate_model_cache(&f.root).unwrap();
        std::fs::write(&cache, "synthetic old catalog").unwrap();
        invalidate_model_cache(&f.root).unwrap();
        assert!(!cache.exists());
        assert_eq!(
            std::fs::read_to_string(&auth).unwrap(),
            "synthetic auth fixture"
        );
        assert_eq!(
            std::fs::read_to_string(&history).unwrap(),
            "synthetic history fixture"
        );
        std::fs::hard_link(&auth, &cache).unwrap();
        assert!(invalidate_model_cache(&f.root).is_err());
        assert!(cache.exists());
        assert_eq!(
            std::fs::read_to_string(&auth).unwrap(),
            "synthetic auth fixture"
        );
    }
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
    fn qualified_apply_refuses_changed_input_without_consuming_preparation() {
        let f = Fixture::new();
        std::fs::write(&f.target, "model = 'native'\n").unwrap();
        let mut journal = ConfigJournal::prepare(&f.state, &f.target, 12345, CAP).unwrap();
        let receipt = std::fs::read(f.state.join("integration.json")).unwrap();
        std::fs::write(&f.target, "model = 'user-edit'\n").unwrap();
        assert_eq!(
            journal.apply_qualified().unwrap_err().to_string(),
            "E_CONFIG_CHANGED"
        );
        assert_eq!(journal.phase(), Phase::Prepared);
        assert!(journal.prepared.is_some());
        assert_eq!(
            std::fs::read(f.state.join("integration.json")).unwrap(),
            receipt
        );
        assert_eq!(
            std::fs::read_to_string(&f.target).unwrap(),
            "model = 'user-edit'\n"
        );
        assert!(
            !f.root
                .join(format!(".cxweb-{}-config.tmp", journal.installation_id()))
                .exists()
        );
    }

    #[test]
    fn web_recovery_requires_applied_unchanged_ownership_and_valid_private_evidence() {
        let f = Fixture::new();
        let mut journal = ConfigJournal::prepare(&f.state, &f.target, 12345, CAP).unwrap();
        journal
            .record_catalog(
                vec!["webbridge/fixture".into()],
                vec!["native-fixture".into()],
            )
            .unwrap();
        let receipt = crate::web_recovery::Receipt::fixture(journal.installation_id());
        assert!(
            journal
                .record_web(crate::web_recovery::Receipt::fixture("other"))
                .is_err()
        );
        journal.record_web(receipt.clone()).unwrap();
        assert!(journal.record_web(receipt.clone()).is_err());
        assert!(journal.web_recovery().unwrap().is_none());
        journal.apply().unwrap();
        assert!(journal.web_recovery().unwrap().is_some());
        let installed = std::fs::read_to_string(&f.target).unwrap();
        drop(journal);
        let mut journal = ConfigJournal::reopen(&f.state, &f.target).unwrap();
        assert!(journal.web_recovery().unwrap().is_some());
        assert!(journal.record_web(receipt).is_err());
        std::fs::write(
            &f.target,
            format!("{installed}# unrelated edit\ntheme = 'dark'\n"),
        )
        .unwrap();
        assert!(journal.web_recovery().unwrap().is_some());
        std::fs::write(
            &f.target,
            installed.replace(CAP, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        )
        .unwrap();
        assert!(journal.web_recovery().unwrap().is_none());
        std::fs::write(&f.target, &installed).unwrap();
        let path = f.state.join("integration.json");
        let mut raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        drop(journal);
        raw["web"]["builds"] = serde_json::json!(["unreviewed-build"]);
        std::fs::write(&path, raw.to_string()).unwrap();
        let mut journal = ConfigJournal::reopen(&f.state, &f.target).unwrap();
        assert!(journal.web_recovery().unwrap().is_none());
        // Invalid web evidence does not prevent safe native-only recovery/undo.
        assert_eq!(journal.recovery().unwrap(), Recovery::Candidate);
        journal
            .disconnect(&["webbridge/fixture".into()], &["native-fixture".into()])
            .unwrap();
        assert!(journal.web_recovery().unwrap().is_none());
        assert!(!f.target.exists());
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
    fn legacy_journal_reopens_and_removes_its_original_route_without_migration() {
        let f = Fixture::new();
        let journal = ConfigJournal::prepare(&f.state, &f.target, 12345, CAP).unwrap();
        assert_eq!(journal.record.version, 2);
        let mut old = journal.record.clone();
        old.version = 1;
        old.candidate = RoutePatch::legacy_plan("", 12345, CAP).unwrap().1;
        old.candidate_sha256 = hash(old.candidate.as_bytes());
        old.phase = Phase::ConfigApplied;
        drop(journal);
        let path = f.state.join("integration.json");
        std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
        std::fs::write(&f.target, &old.candidate).unwrap();
        let mut recovered = ConfigJournal::reopen(&f.state, &f.target).unwrap();
        assert_eq!(recovered.recovery().unwrap(), Recovery::Candidate);
        assert_eq!(std::fs::read_to_string(&f.target).unwrap(), old.candidate);
        assert!(recovered.routes_to(&format!(
            "http://127.0.0.1:12345/wb/{CAP}/backend-api/codex"
        )));
        recovered.disconnect(&[], &[]).unwrap();
        assert!(!f.target.exists());
        drop(recovered);
        assert_eq!(
            ConfigJournal::reopen(&f.state, &f.target)
                .unwrap()
                .recovery()
                .unwrap(),
            Recovery::Restored
        );
        // A version flip cannot disguise a different candidate plan.
        old.version = 2;
        std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
        assert!(ConfigJournal::reopen(&f.state, &f.target).is_err());
    }
    #[test]
    fn catalog_receipt_is_validated_durable_and_immutable_after_preparation() {
        let f = Fixture::new();
        let mut journal = ConfigJournal::prepare(&f.state, &f.target, 12345, CAP).unwrap();
        assert!(journal.catalog_receipt().is_err());
        assert!(
            journal
                .record_catalog(vec!["native".into()], vec![])
                .is_err()
        );
        assert!(
            journal
                .record_catalog(vec!["webbridge/a".into(); 2], vec![])
                .is_err()
        );
        assert!(
            journal
                .record_catalog(vec![], vec!["webbridge/a".into()])
                .is_err()
        );
        journal
            .record_catalog(vec!["webbridge/a".into()], vec!["native".into()])
            .unwrap();
        assert!(journal.record_catalog(vec![], vec![]).is_err());
        assert!(!f.target.exists());
        journal.apply().unwrap();
        drop(journal);
        let mut journal = ConfigJournal::reopen(&f.state, &f.target).unwrap();
        assert_eq!(
            journal.catalog_receipt().unwrap(),
            (vec!["webbridge/a".to_owned()], vec!["native".to_owned()])
        );
        assert!(journal.record_catalog(vec![], vec![]).is_err());
        drop(journal);
        let path = f.state.join("integration.json");
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        value["catalog"]["native"] = serde_json::json!(["webbridge/foreign"]);
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(ConfigJournal::reopen(&f.state, &f.target).is_err());
    }
    #[test]
    fn scheduler_plan_is_durable_before_registration_and_blocks_uncertain_apply() {
        let f = Fixture::new();
        let mut journal = ConfigJournal::prepare(&f.state, &f.target, 12345, CAP).unwrap();
        assert!(
            journal
                .prepare_scheduler(&std::env::current_exe().unwrap())
                .is_err()
        );
        journal.record_catalog(vec![], vec![]).unwrap();
        assert!(ConfigJournal::control_target(&f.state).unwrap().is_none());
        let plan = journal
            .prepare_scheduler(&std::env::current_exe().unwrap())
            .unwrap();
        assert_eq!(
            ConfigJournal::control_target(&f.state).unwrap().unwrap().0,
            journal.installation_id()
        );
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(f.state.join("integration.json")).unwrap())
                .unwrap();
        assert_eq!(value["scheduler"]["name"], plan.name());
        assert_eq!(value["scheduler"]["planned_xml"], plan.xml());
        assert!(value["scheduler"]["receipt"].is_null());
        assert_eq!(
            journal.apply().unwrap_err().to_string(),
            "E_SUPERVISION_PENDING"
        );
        assert!(!f.target.exists());
        drop(journal);
        let recovered = ConfigJournal::reopen(&f.state, &f.target).unwrap();
        assert!(recovered.registered_scheduler().is_err());
        drop(recovered);
        let mut redirected = value;
        redirected["scheduler"]["name"] = serde_json::json!("cxweb-foreign-installation");
        std::fs::write(
            f.state.join("integration.json"),
            serde_json::to_vec(&redirected).unwrap(),
        )
        .unwrap();
        assert!(ConfigJournal::reopen(&f.state, &f.target).is_err());
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

    #[test]
    fn disconnect_restores_owned_selection_and_preserves_user_changes() {
        let f = Fixture::new();
        std::fs::write(&f.target, "# keep\nmodel='native'\n").unwrap();
        let mut journal = ConfigJournal::prepare(&f.state, &f.target, 12345, CAP).unwrap();
        journal.apply().unwrap();
        let changed = journal
            .record
            .candidate
            .replace("'native'", "'webbridge/test'")
            + "# later\nother=true\n";
        std::fs::write(&f.target, changed).unwrap();
        journal
            .disconnect(&["webbridge/test".into()], &["native".into()])
            .unwrap();
        assert_eq!(journal.phase(), Phase::ConfigRestored);
        let result = std::fs::read_to_string(&f.target).unwrap();
        assert!(
            result.contains("# keep")
                && result.contains("# later")
                && result.contains("other=true")
        );
        assert!(result.contains("model = \"native\"") || result.contains("model=\"native\""));
        assert!(!result.contains("openai_base_url"));
        drop(journal);
        let mut reopened = ConfigJournal::reopen(&f.state, &f.target).unwrap();
        assert_eq!(reopened.recovery().unwrap(), Recovery::Restored);
        reopened.disconnect(&[], &[]).unwrap();
        assert_eq!(std::fs::read_to_string(&f.target).unwrap(), result);
    }

    #[test]
    fn disconnect_removes_only_new_owned_only_files() {
        for case in 0..4 {
            let f = Fixture::new();
            if case == 1 {
                std::fs::write(&f.target, "").unwrap();
            }
            let mut journal = ConfigJournal::prepare(&f.state, &f.target, 12345, CAP).unwrap();
            if case == 3 {
                // An editor created an empty file before cxweb ever applied.
                std::fs::write(&f.target, "").unwrap();
            } else {
                journal.apply().unwrap();
                if case == 2 {
                    std::fs::write(
                        &f.target,
                        format!("{}# user comment\n", journal.record.candidate),
                    )
                    .unwrap();
                }
            }
            journal.disconnect(&[], &[]).unwrap();
            assert_eq!(f.target.exists(), case != 0);
            assert_eq!(journal.recovery().unwrap(), Recovery::Restored);
            if case == 2 {
                assert!(
                    std::fs::read_to_string(&f.target)
                        .unwrap()
                        .contains("user comment")
                );
            }
        }
    }

    #[test]
    fn disconnect_crashes_before_and_after_mutation_recover_without_erasing_edits() {
        for after_mutation in [false, true] {
            let f = Fixture::new();
            let mut journal = ConfigJournal::prepare(&f.state, &f.target, 12345, CAP).unwrap();
            journal.apply().unwrap();
            let current = journal.prepare_disconnect(&[], &[]).unwrap();
            if after_mutation {
                journal.apply_disconnect(&current).unwrap();
            }
            drop(journal);
            let mut recovered = ConfigJournal::reopen(&f.state, &f.target).unwrap();
            assert_eq!(recovered.phase(), Phase::Disconnecting);
            if after_mutation {
                assert_eq!(recovered.recovery().unwrap(), Recovery::Restored);
            } else {
                std::fs::write(
                    &f.target,
                    format!("{}# post-crash edit\n", recovered.record.candidate),
                )
                .unwrap();
            }
            recovered.disconnect(&[], &[]).unwrap();
            assert_eq!(recovered.phase(), Phase::ConfigRestored);
            if !after_mutation {
                assert!(
                    std::fs::read_to_string(&f.target)
                        .unwrap()
                        .contains("post-crash edit")
                );
            } else {
                assert!(!f.target.exists());
            }
        }
    }

    #[test]
    fn foreign_route_and_edit_after_disconnect_preparation_survive() {
        let f = Fixture::new();
        let mut journal = ConfigJournal::prepare(&f.state, &f.target, 12345, CAP).unwrap();
        journal.apply().unwrap();
        let current = journal.prepare_disconnect(&[], &[]).unwrap();
        let foreign = "openai_base_url='https://example.com'\n";
        std::fs::write(&f.target, foreign).unwrap();
        assert!(journal.apply_disconnect(&current).is_err());
        assert!(journal.disconnect(&[], &[]).is_err());
        assert_eq!(std::fs::read_to_string(&f.target).unwrap(), foreign);
    }
}
