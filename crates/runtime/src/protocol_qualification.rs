//! One frozen protocol case per command in the installed browser owner.
//! This generates responses but does not execute fixture tools or publish gates.
use crate::{
    managed_driver::Binding,
    quality_journal::{Identity, Journal, ResultRecord},
};
use cxweb_browser_adapter::ManagedBrowser;
use cxweb_codex_adapter::quality_corpus::{self, tally::Failure};
use cxweb_platform::atomic_file::Snapshot;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub run: String,
    pub effort: String,
}

fn now() -> Result<u64, &'static str> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| "E_QUALITY_CLOCK")
}
fn hash(value: impl Serialize) -> Result<String, &'static str> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&value).map_err(|_| "E_QUALITY_IDENTITY")?)
    ))
}

pub(crate) fn run(
    browser: &mut ManagedBrowser,
    binding: &Binding,
    directory: &Path,
    target: &Target,
    cancel: &CancellationToken,
) -> Result<(), &'static str> {
    if cancel.is_cancelled() {
        return Err("E_CANCELLED");
    }
    let route = binding
        .routes
        .first()
        .filter(|_| binding.routes.len() == 1)
        .ok_or("E_MODEL_UNAVAILABLE")?;
    let selection = route.selection(Some(&target.effort))?;
    let executable = std::env::current_exe().map_err(|_| "E_QUALITY_RUNTIME")?;
    let _executable_guard =
        cxweb_platform::target_path::TargetPathGuard::capture(&executable, false)
            .map_err(|_| "E_QUALITY_RUNTIME")?;
    let runtime_sha256 = format!(
        "{:x}",
        Sha256::digest(std::fs::read(&executable).map_err(|_| "E_QUALITY_RUNTIME")?)
    );
    let version = browser.version().map_err(|_| "E_QUALITY_BROWSER")?;
    let identity = Identity {
        corpus_sha256: quality_corpus::manifest()?["corpus_sha256"]
            .as_str()
            .ok_or("E_QUALITY_IDENTITY")?
            .into(),
        runtime_sha256,
        scope_sha256: hash((
            &binding.installation,
            &binding.account,
            &binding.workspace,
            binding.epoch,
        ))?,
        route_sha256: hash((&route.id, &selection.identity, &selection.label))?,
        browser_version: version["product"]
            .as_str()
            .ok_or("E_QUALITY_BROWSER")?
            .into(),
        effort: target.effort.clone(),
    };
    let mut journal = Journal::open(directory, &target.run, identity, now()?)?;
    let result = execute_next(
        browser,
        binding,
        &selection.identity,
        &selection.label,
        &route.id,
        &mut journal,
        cancel,
    );
    // The journal is authoritative. This separate, content-free report can be
    // regenerated after a crash; it never controls admission or accounting.
    let report_path = directory
        .join(format!("quality-{}", target.run))
        .join("report.json");
    let snapshot = Snapshot::capture(&report_path).map_err(|_| "E_QUALITY_REPORT")?;
    let bytes = serde_json::to_vec_pretty(&journal.report()?).map_err(|_| "E_QUALITY_REPORT")?;
    let staged = snapshot
        .stage(
            &format!(".cxweb-quality-{:032x}.tmp", rand::random::<u128>()),
            &bytes,
        )
        .map_err(|_| "E_QUALITY_REPORT")?;
    snapshot
        .commit(&staged, &bytes)
        .map_err(|_| "E_QUALITY_REPORT")?;
    result
}

fn execute_next(
    browser: &mut ManagedBrowser,
    binding: &Binding,
    identity: &str,
    label: &str,
    model: &str,
    journal: &mut Journal,
    cancel: &CancellationToken,
) -> Result<(), &'static str> {
    let Some(case) = journal.begin_next(now()?)? else {
        return Ok(());
    };
    let nonce = format!("{:032x}", rand::random::<u128>());
    let response = (|| {
        let request = case.request(model)?;
        let prompt = request.browser_prompt(&nonce, 512 * 1024)?;
        browser
            .qualify_cancellable(
                (identity, label, &prompt),
                || {
                    journal
                        .submitting(&case.id, now().map_err(io::Error::other)?)
                        .map_err(io::Error::other)
                },
                |browser, page| crate::reasoning_qualification::check_scope(browser, page, binding),
                || cancel.is_cancelled(),
            )
            .map_err(|error| match error.to_string().as_str() {
                "E_CANCELLED" => "E_CANCELLED",
                "E_BROWSER_RATE_LIMITED" => "E_BROWSER_RATE_LIMITED",
                "E_QUALIFICATION_TIMEOUT" => "E_QUALIFICATION_TIMEOUT",
                "E_WEB_CLEANUP_UNCONFIRMED" => "E_WEB_CLEANUP_UNCONFIRMED",
                "E_SESSION_SCOPE" => "E_SESSION_SCOPE",
                _ => "E_QUALITY_BROWSER",
            })
    })();
    let (record, error) = match response {
        Ok(outcome) if outcome.candidate_label == label => {
            match case.assess(model, &nonce, outcome.response.as_bytes()) {
                Ok(assessment) => (
                    ResultRecord::Response {
                        protocol_valid: assessment.protocol_valid,
                        task_passed: assessment.task_passed,
                    },
                    None,
                ),
                Err(code) => (
                    ResultRecord::Failure {
                        reason: Failure::Transport,
                    },
                    Some(code),
                ),
            }
        }
        Ok(_) => (
            ResultRecord::Failure {
                reason: Failure::WrongRoute,
            },
            Some("E_QUALITY_ROUTE"),
        ),
        Err(code) => (
            ResultRecord::Failure {
                reason: match code {
                    "E_CANCELLED" => Failure::Cancelled,
                    "E_BROWSER_RATE_LIMITED" => Failure::RateLimited,
                    "E_QUALIFICATION_TIMEOUT" => Failure::Timeout,
                    "E_SESSION_SCOPE" => Failure::WrongRoute,
                    _ => Failure::Transport,
                },
            },
            Some(code),
        ),
    };
    journal.finish(&case.id, record, now()?)?;
    error.map_or(Ok(()), Err)
}
