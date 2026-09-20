//! Explicit installed-family qualification. Never publish labels as evidence.
use crate::{
    browser_scope::BrowserScope,
    ledger::{Admission, Ledger},
    managed_driver::{Binding, ReasoningVariant, observed_effort},
    qualification::Kind,
};
use cxweb_browser_adapter::{ManagedBrowser, ManagedPage, ModelCandidate};
use cxweb_codex_adapter::envelope;
use cxweb_domain::{SessionKey, TurnState};
use sha2::{Digest, Sha256};
use std::{io, path::Path};
use tokio_util::sync::CancellationToken;

#[derive(Default, serde::Serialize)]
struct Report {
    observed_labels: Vec<String>,
    completed: Vec<(&'static str, String)>,
    failure: Option<&'static str>,
    finished: bool,
    current: Option<(&'static str, String)>,
    stage: Option<&'static str>,
    scope: Option<cxweb_browser_adapter::ScopeDiagnostic>,
    attribution: Option<cxweb_browser_adapter::QualificationDiagnostic>,
}
impl Report {
    fn save(&self, directory: &Path) -> Result<(), &'static str> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|_| "E_REASONING_REPORT")?;
        std::fs::write(directory.join("reasoning-qualification-report.json"), bytes)
            .map_err(|_| "E_REASONING_REPORT")
    }
}

fn check_scope(
    browser: &mut ManagedBrowser,
    page: &ManagedPage,
    binding: &Binding,
) -> io::Result<()> {
    let surface = browser.account_scope(page).map_err(|error| {
        io::Error::other(crate::managed_driver::browser_error(
            &error,
            "E_SESSION_SCOPE",
        ))
    })?;
    let scope =
        BrowserScope::from_surface(&binding.installation, &surface).map_err(io::Error::other)?;
    if scope.account != binding.account || scope.workspace != binding.workspace {
        return Err(io::Error::other("E_SESSION_SCOPE"));
    }
    Ok(())
}

fn candidates(
    binding: &Binding,
    observed: Vec<ModelCandidate>,
) -> Result<Vec<ModelCandidate>, &'static str> {
    let route = binding
        .routes
        .first()
        .filter(|_| binding.routes.len() == 1)
        .ok_or("E_MODEL_UNAVAILABLE")?;
    let family = route
        .label
        .rsplit_once(" · ")
        .ok_or("E_MODEL_UNAVAILABLE")?
        .0;
    if observed.len() != 5
        || !observed
            .iter()
            .any(|candidate| candidate.identity == route.identity && candidate.label == route.label)
    {
        return Err("E_REASONING_DISCOVERY");
    }
    let mut efforts = std::collections::BTreeSet::new();
    for candidate in &observed {
        if candidate.label.rsplit_once(" · ").map(|label| label.0) != Some(family)
            || !efforts.insert(observed_effort(&candidate.label)?)
        {
            return Err("E_REASONING_DISCOVERY");
        }
    }
    if efforts
        != ["low", "medium", "high", "xhigh", "max"]
            .into_iter()
            .collect()
    {
        return Err("E_REASONING_DISCOVERY");
    }
    Ok(observed)
}

pub(crate) fn qualify(
    browser: &mut ManagedBrowser,
    binding: &Binding,
    directory: &Path,
    cancel: &CancellationToken,
) -> Result<Binding, &'static str> {
    let mut report = Report::default();
    report.save(directory)?;
    let result = qualify_inner(browser, binding, directory, cancel, &mut report);
    report.finished = true;
    report.failure = result.as_ref().err().copied();
    // Preserve an unconfirmed-cleanup error even if diagnostics cannot be saved.
    let saved = report.save(directory);
    result.and_then(|binding| saved.map(|_| binding))
}

fn qualify_inner(
    browser: &mut ManagedBrowser,
    binding: &Binding,
    directory: &Path,
    cancel: &CancellationToken,
    report: &mut Report,
) -> Result<Binding, &'static str> {
    if cancel.is_cancelled() {
        return Err("E_CANCELLED");
    }
    let page = browser
        .open_temporary_chat()
        .map_err(|error| crate::managed_driver::temporary_chat_error(&error.to_string()))?;
    let discovered = (|| {
        check_scope(browser, &page, binding)
            .map_err(|error| crate::managed_driver::browser_error(&error, "E_SESSION_SCOPE"))?;
        let route = binding.routes.first().ok_or("E_MODEL_UNAVAILABLE")?;
        let label = browser
            .select_candidate(&page, &route.identity)
            .map_err(|_| "E_MODEL_SELECTION")?;
        if label != route.label {
            return Err("E_MODEL_SELECTION");
        }
        let surface = browser
            .discover_current_family(&page)
            .map_err(|_| "E_REASONING_DISCOVERY")?;
        report.observed_labels = surface
            .candidates
            .iter()
            .map(|candidate| candidate.label.clone())
            .collect();
        report.save(directory)?;
        check_scope(browser, &page, binding)
            .map_err(|error| crate::managed_driver::browser_error(&error, "E_SESSION_SCOPE"))?;
        candidates(binding, surface.candidates)
    })();
    browser
        .close_page_checked(&page)
        .map_err(|_| "E_WEB_CLEANUP_UNCONFIRMED")?;
    let discovered = discovered?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "E_REASONING_WORKER")?;
    let ledger = runtime.block_on(Ledger::open(
        &directory.join("reasoning-qualification.sqlite"),
    ))?;
    let mut next = binding.clone();
    next.routes[0].reasoning.clear();
    for candidate in discovered {
        // The default already has durable text/tools evidence. Re-observation
        // above checks that its identity and label have not changed.
        if candidate.identity == binding.routes[0].identity {
            continue;
        }
        let mut evidence = Sha256::new();
        for kind in [Kind::Text, Kind::Tools] {
            if cancel.is_cancelled() {
                return Err("E_CANCELLED");
            }
            report.current = Some((
                if kind == Kind::Text { "text" } else { "tools" },
                candidate.label.clone(),
            ));
            report.stage = Some("preparing");
            report.save(directory)?;
            let nonce = format!("{:032x}", rand::random::<u128>());
            let request = kind.request(&binding.routes[0].id)?;
            let prompt = request.browser_prompt(&nonce, 512 * 1024)?;
            let scope = SessionKey {
                installation: binding.installation.clone(),
                native_session: nonce.clone(),
                account_scope: binding.account.clone(),
                workspace_scope: binding.workspace.clone(),
                route: binding.routes[0].id.clone(),
                epoch: binding.epoch,
            };
            if runtime.block_on(ledger.admit(&nonce, &scope, prompt.as_bytes()))? != Admission::New
            {
                return Err("E_REQUEST_ALREADY_ADMITTED");
            }
            let mut sent = false;
            let stage = std::cell::Cell::new("account_before_send");
            let scope_checks = std::cell::Cell::new(0);
            let result = browser.qualify_cancellable(
                (&candidate.identity, &candidate.label, &prompt),
                || {
                    runtime
                        .block_on(async {
                            ledger
                                .transition(&nonce, TurnState::ObservedBaseline)
                                .await?;
                            ledger.transition(&nonce, TurnState::Submitting).await
                        })
                        .map_err(io::Error::other)?;
                    sent = true;
                    stage.set("submitting");
                    Ok(())
                },
                |browser, page| {
                    stage.set(if scope_checks.get() == 0 {
                        "account_before_send"
                    } else {
                        "account_after_completion"
                    });
                    scope_checks.set(scope_checks.get() + 1);
                    check_scope(browser, page, binding)
                },
                || cancel.is_cancelled(),
            );
            report.stage = Some(stage.get());
            report.scope = browser.scope_diagnostic();
            report.attribution = browser.qualification_diagnostic();
            let outcome = match result {
                Ok(outcome) => outcome,
                Err(error) => {
                    let terminal = if sent {
                        TurnState::SubmissionUncertain
                    } else {
                        TurnState::Failed
                    };
                    let recorded = runtime.block_on(ledger.transition(&nonce, terminal));
                    let code = match error.to_string().as_str() {
                        "E_CANCELLED" => "E_CANCELLED",
                        "E_WEB_CLEANUP_UNCONFIRMED" => "E_WEB_CLEANUP_UNCONFIRMED",
                        "E_SESSION_SCOPE" => "E_SESSION_SCOPE",
                        "E_BROWSER_RATE_LIMITED" => "E_BROWSER_RATE_LIMITED",
                        "E_QUALIFICATION_TIMEOUT" => "E_QUALIFICATION_TIMEOUT",
                        _ => "E_REASONING_QUALIFICATION",
                    };
                    if code == "E_WEB_CLEANUP_UNCONFIRMED" {
                        return Err(code);
                    }
                    recorded?;
                    return Err(code);
                }
            };
            runtime.block_on(async {
                ledger.transition(&nonce, TurnState::Submitted).await?;
                ledger.transition(&nonce, TurnState::Generating).await
            })?;
            if outcome.candidate_label != candidate.label
                || !envelope::validate(outcome.response.as_bytes(), &request.context(&nonce))
                    .as_ref()
                    .is_ok_and(|output| kind.accepts(output))
            {
                runtime.block_on(ledger.transition(&nonce, TurnState::Failed))?;
                return Err("E_QUALIFICATION_PROTOCOL");
            }
            runtime.block_on(ledger.transition(&nonce, TurnState::Completed))?;
            report.completed.push((
                if kind == Kind::Text { "text" } else { "tools" },
                candidate.label.clone(),
            ));
            report.save(directory)?;
            for part in [
                candidate.identity.as_bytes(),
                candidate.label.as_bytes(),
                nonce.as_bytes(),
                outcome.response.as_bytes(),
            ] {
                evidence.update((part.len() as u64).to_le_bytes());
                evidence.update(part);
            }
        }
        next.routes[0].reasoning.push(ReasoningVariant {
            effort: observed_effort(&candidate.label)?.into(),
            identity: candidate.identity,
            label: candidate.label,
            protocol_evidence: format!("{:x}", evidence.finalize()),
        });
    }
    next.validate()?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_requires_all_distinct_observed_choices_and_the_installed_default() {
        let observed = ["Instant", "Medium", "High", "Extra High", "6 PRO"]
            .into_iter()
            .enumerate()
            .map(|(position, label)| ModelCandidate {
                identity: serde_json::json!(["reasoning-slider-v2", "Latest", 1, 5, position + 1])
                    .to_string(),
                label: format!("Latest · {label}"),
                selected: position == 3,
            })
            .collect::<Vec<_>>();
        let binding = Binding {
            installation: "fixture".into(),
            account: "a".repeat(64),
            workspace: "b".repeat(64),
            epoch: 0,
            routes: vec![crate::managed_driver::Route {
                id: "webbridge/fixture".into(),
                identity: observed[3].identity.clone(),
                label: observed[3].label.clone(),
                effort: Some("xhigh".into()),
                reasoning: vec![],
            }],
        };
        assert_eq!(candidates(&binding, observed.clone()).unwrap(), observed);
        let mut short_pro = observed.clone();
        short_pro[4].label = "Latest · Pro".into();
        assert!(candidates(&binding, short_pro).is_ok());
        for mutation in 0..5 {
            let mut changed = observed.clone();
            match mutation {
                0 => {
                    changed.pop();
                }
                1 => changed[0].label = "Other · Instant".into(),
                2 => changed[0].label = "Latest · High".into(),
                3 => changed[0].label = "Latest · Unknown".into(),
                _ => changed[3].identity = "different".into(),
            }
            assert!(candidates(&binding, changed).is_err());
        }
    }
}
