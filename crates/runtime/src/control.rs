//! Login and explicit qualification worker. Browser IPC stays off the reactor.
use crate::browser_scope::BrowserScope;
pub use crate::generation_handoff::GenerationSession;
use crate::generation_handoff::PreparedHandoff;
use crate::ledger::{Admission, Ledger};
use crate::qualification::Kind;
use cxweb_browser_adapter::{
    LoginObservation, ManagedBrowser, ManagedPage, ModelSurfaceDiagnostic, QualificationDiagnostic,
};
use cxweb_codex_adapter::envelope;
use cxweb_domain::{SessionKey, TurnState};
use cxweb_platform::state::{StatePaths, installed_browser};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

fn replacement_error(error: &std::io::Error) -> &'static str {
    match error.to_string().as_str() {
        "E_BROWSER_OTHER_PAGES" => "E_BROWSER_OTHER_PAGES",
        "E_BROWSER_BUSY" => "E_BROWSER_BUSY",
        _ => "E_BROWSER_RELEASE",
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlStatus {
    #[serde(default)]
    pub background_session: bool,
    #[serde(default)]
    pub native_text_report: Option<crate::native_probe::Report>,
    #[serde(default)]
    pub native_text_error: Option<String>,
    pub phase: String,
    pub browser_version: Option<String>,
    pub observation: Option<LoginObservation>,
    pub routing_installed: bool,
    pub live_qualified: bool,
    #[serde(default)]
    pub candidate_models: Vec<QualifiedModel>,
    #[serde(default)]
    pub temporary_chat_available: Option<bool>,
    #[serde(default)]
    pub model_discovery_diagnostic: Option<ModelSurfaceDiagnostic>,
    #[serde(default)]
    pub text_qualified_model: Option<String>,
    #[serde(default)]
    pub tool_qualified_model: Option<String>,
    #[serde(default)]
    pub qualification_evidence: Option<String>,
    #[serde(default)]
    pub qualification_diagnostic: Option<QualificationDiagnostic>,
    #[serde(default)]
    pub scope_diagnostic: Option<cxweb_browser_adapter::ScopeDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualifiedModel {
    pub id: String,
    pub label: String,
    pub selected: bool,
}
impl Default for ControlStatus {
    fn default() -> Self {
        Self {
            background_session: false,
            native_text_report: None,
            native_text_error: None,
            phase: "disconnected".into(),
            browser_version: None,
            observation: None,
            routing_installed: false,
            live_qualified: false,
            candidate_models: Vec::new(),
            temporary_chat_available: None,
            model_discovery_diagnostic: None,
            text_qualified_model: None,
            tool_qualified_model: None,
            qualification_evidence: None,
            qualification_diagnostic: None,
            scope_diagnostic: None,
        }
    }
}
type Reply = oneshot::Sender<Result<ControlStatus, &'static str>>;

pub(crate) fn authenticated_surface(observation: &LoginObservation) -> bool {
    observation.official_page
        && observation.composer
        && observation.account_surface
        && !observation.login_action
        && !observation.verification_required
}

fn qualification_failure_state(submission_intent: bool, error: &str) -> TurnState {
    let send_refused = matches!(
        error,
        "E_SEND_SURFACE" | "E_SEND_DISABLED" | "E_COMPOSER_MISMATCH" | "E_MODEL_SELECTION"
    );
    if submission_intent && !send_refused {
        TurnState::SubmissionUncertain
    } else {
        TurnState::Failed
    }
}

#[cfg(test)]
mod qualification_tests {
    use super::*;

    #[tokio::test]
    async fn native_setup_requires_background_proof_without_starting_a_client_or_changing_the_receipt()
     {
        use crate::control_protocol::LoginBackend;
        let (commands, mut incoming) = mpsc::channel(8);
        let control = Control { commands };
        let owner = crate::setup_owner::SetupOwner::new(control);
        let status = ControlStatus {
            phase: "tool_protocol_qualified".into(),
            tool_qualified_model: Some("webbridge/fixture".into()),
            ..Default::default()
        };
        let expected = status.clone();
        let worker = tokio::spawn(async move {
            for _ in 0..2 {
                let WorkerCommand::Snapshot(reply) = incoming.recv().await.unwrap() else {
                    panic!("only last-receipt reads are allowed");
                };
                reply.send(Ok(status.clone())).unwrap();
            }
        });
        let result = owner
            .native_text(crate::setup_owner::NativeTarget {
                client: "nonexistent".into(),
                home: "nonexistent".into(),
                cwd: "nonexistent".into(),
                route: "webbridge/fixture".into(),
            })
            .await
            .unwrap();
        worker.await.unwrap();
        assert_eq!(result.phase, expected.phase);
        assert_eq!(result.tool_qualified_model, expected.tool_qualified_model);
        assert_eq!(
            result.native_text_error.as_deref(),
            Some("E_NATIVE_TEST_BACKGROUND")
        );
        assert!(result.native_text_report.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn generation_handoff_waits_for_the_owner_and_preserves_requested_identity() {
        let (commands, mut incoming) = mpsc::channel(8);
        let control = Control { commands };
        let task = tokio::spawn(async move {
            control
                .take_generation("installation".into(), "webbridge/fixture".into())
                .await
        });
        let WorkerCommand::TakeGeneration {
            installation,
            route,
            reply,
        } = incoming.recv().await.unwrap()
        else {
            panic!("expected handoff")
        };
        assert_eq!(installation, "installation");
        assert_eq!(route, "webbridge/fixture");
        tokio::time::advance(std::time::Duration::from_secs(400)).await;
        tokio::task::yield_now().await;
        assert!(!task.is_finished());
        assert!(reply.send(Err("E_HANDOFF_UNQUALIFIED")).is_ok());
        assert!(matches!(task.await.unwrap(), Err("E_HANDOFF_UNQUALIFIED")));
    }

    #[tokio::test(start_paused = true)]
    async fn accepted_browser_operations_wait_for_the_worker_instead_of_fabricating_timeout_receipts()
     {
        let (commands, mut incoming) = mpsc::channel(8);
        let control = Control { commands };
        let mut tasks = Vec::new();
        for action in 0..5 {
            let control = control.clone();
            tasks.push(tokio::spawn(async move {
                match action {
                    0 => control.background().await,
                    1 => control.connect().await,
                    2 => control.status().await,
                    3 => control.qualify().await,
                    _ => control.qualify_tools().await,
                }
            }));
        }
        let mut pending = Vec::new();
        for _ in 0..5 {
            pending.push(incoming.recv().await.unwrap());
        }
        tokio::time::advance(std::time::Duration::from_secs(400)).await;
        tokio::task::yield_now().await;
        assert!(tasks.iter().all(|task| !task.is_finished()));
        for command in pending {
            let (WorkerCommand::Background(reply)
            | WorkerCommand::Connect(reply)
            | WorkerCommand::Status(reply)
            | WorkerCommand::Qualify(reply)
            | WorkerCommand::QualifyTurn(_, reply)) = command
            else {
                panic!("unexpected handoff")
            };
            reply.send(Ok(ControlStatus::default())).unwrap();
        }
        for task in tasks {
            assert!(task.await.unwrap().is_ok());
        }
    }

    #[test]
    fn verification_challenge_cannot_qualify_a_stale_authenticated_surface() {
        let mut observation = LoginObservation {
            verification_required: false,
            document_ready: true,
            browser_language: Some("en-US".into()),
            page_language: Some("en-US".into()),
            official_page: true,
            composer: true,
            account_surface: true,
            login_action: false,
            selected_label: None,
        };
        assert!(authenticated_surface(&observation));
        observation.verification_required = true;
        assert!(!authenticated_surface(&observation));
        observation.verification_required = false;
        observation.login_action = true;
        assert!(!authenticated_surface(&observation));
    }

    #[tokio::test]
    async fn known_send_refusal_is_terminal_without_becoming_uncertain_or_replayable() {
        for code in [
            "E_SEND_SURFACE",
            "E_SEND_DISABLED",
            "E_COMPOSER_MISMATCH",
            "E_MODEL_SELECTION",
        ] {
            let ledger = Ledger::in_memory();
            let scope = SessionKey {
                installation: "fixture".into(),
                native_session: "fixture".into(),
                account_scope: "fixture".into(),
                workspace_scope: "fixture".into(),
                route: "webbridge/fixture".into(),
                epoch: 0,
            };
            assert_eq!(
                ledger.admit("test", &scope, b"test").await.unwrap(),
                Admission::New
            );
            ledger
                .transition("test", TurnState::ObservedBaseline)
                .await
                .unwrap();
            ledger
                .transition("test", TurnState::Submitting)
                .await
                .unwrap();
            let terminal = qualification_failure_state(true, code);
            assert_eq!(terminal, TurnState::Failed);
            ledger.transition("test", terminal).await.unwrap();
            assert_eq!(ledger.recover().await.unwrap(), 0);
            assert_eq!(
                ledger.admit("test", &scope, b"test").await.unwrap(),
                Admission::Existing(TurnState::Failed)
            );
            assert!(
                ledger
                    .transition("test", TurnState::Submitting)
                    .await
                    .is_err()
            );
        }
    }

    #[test]
    fn unknown_and_post_click_failures_remain_uncertain_after_intent() {
        for code in [
            "E_SUBMISSION_UNCERTAIN",
            "E_USER_MESSAGE_MISMATCH",
            "E_QUALIFICATION_OBSERVE",
            "arbitrary backend error",
        ] {
            assert_eq!(
                qualification_failure_state(true, code),
                TurnState::SubmissionUncertain
            );
            assert_eq!(qualification_failure_state(false, code), TurnState::Failed);
        }
    }
}
enum WorkerCommand {
    Snapshot(Reply),
    TakeGeneration {
        installation: String,
        route: String,
        reply: oneshot::Sender<Result<Arc<GenerationSession>, &'static str>>,
    },
    Background(Reply),
    Connect(Reply),
    Status(Reply),
    Qualify(Reply),
    QualifyTurn(Kind, Reply),
}
#[derive(Clone)]
pub struct Control {
    commands: mpsc::Sender<WorkerCommand>,
}

impl Control {
    pub fn start() -> Result<Self, &'static str> {
        let paths = StatePaths::open().map_err(|_| "E_STATE_PERMISSIONS")?;
        let lock = paths.lock().map_err(|_| "E_ALREADY_RUNNING")?;
        let executable = installed_browser().map_err(|_| "E_BROWSER_RUNTIME_MISSING")?;
        let (commands, mut incoming) = mpsc::channel(8);
        let runtime = tokio::runtime::Handle::current();
        std::thread::Builder::new()
            .name("cxweb-browser-control".into())
            .spawn(move || {
                let mut instance_lock = Some(lock);
                let mut generation: Option<Arc<GenerationSession>> = None;
                let mut browser: Option<ManagedBrowser> = None;
                let mut page: Option<ManagedPage> = None;
                let mut status = ControlStatus::default();
                let mut observed_routes: Vec<(QualifiedModel, String)> = Vec::new();
                // Diagnostic scope is unique to this browser owner's lifetime.
                // Production activation supplies its persistent installation ID.
                let scope_installation =
                    format!("cxweb-qualification-{:032x}", rand::random::<u128>());
                let mut observed_scope: Option<BrowserScope> = None;
                while let Some(command) = incoming.blocking_recv() {
                    let command = match command {
                        WorkerCommand::Snapshot(reply) => {
                            let _ = reply.send(Ok(status.clone()));
                            continue;
                        }
                        WorkerCommand::TakeGeneration {
                            installation,
                            route,
                            reply,
                        } => {
                            if reply.is_closed() {
                                continue;
                            }
                            if let Some(session) = generation.as_ref() {
                                let _ = reply.send(session.replay(&installation, &route));
                                continue;
                            }
                            let prepared = (|| {
                                if instance_lock.is_none() {
                                    return Err("E_CONTROL_CLOSED");
                                }
                                PreparedHandoff::prepare(
                                    browser.as_mut().ok_or("E_LOGIN_REQUIRED")?,
                                    page.as_ref().ok_or("E_LOGIN_REQUIRED")?,
                                    &status,
                                    &observed_routes,
                                    (
                                        &scope_installation,
                                        observed_scope.as_ref().ok_or("E_SESSION_SCOPE")?,
                                    ),
                                    &installation,
                                    &route,
                                )
                            })();
                            let prepared = match prepared {
                                Ok(value) => value,
                                Err(code) => {
                                    let _ = reply.send(Err(code));
                                    continue;
                                }
                            };
                            // A completed transfer retains a receipt even when the
                            // requester disappears. Retrying returns the same owner.
                            let result = prepared
                                .start(
                                    browser.take().expect("prepared browser owner"),
                                    instance_lock.take().expect("prepared installation lock"),
                                )
                                .map(Arc::new);
                            page = None;
                            observed_routes.clear();
                            observed_scope = None;
                            match &result {
                                Ok(session) => {
                                    generation = Some(session.clone());
                                    status.phase = "generation_ready".into();
                                    status.background_session = true;
                                }
                                Err(_) => {
                                    status = ControlStatus {
                                        phase: "browser_unavailable".into(),
                                        ..Default::default()
                                    }
                                }
                            }
                            let _ = reply.send(result);
                            continue;
                        }
                        command => command,
                    };
                    let (connect, background, qualify, qualification_kind, reply) = match command {
                        WorkerCommand::Snapshot(_) | WorkerCommand::TakeGeneration { .. } => {
                            unreachable!("handled above")
                        }
                        WorkerCommand::Connect(reply) => (true, false, false, None, reply),
                        WorkerCommand::Background(reply) => (false, true, false, None, reply),
                        WorkerCommand::Status(reply) => (false, false, false, None, reply),
                        WorkerCommand::Qualify(reply) => (false, false, true, None, reply),
                        WorkerCommand::QualifyTurn(kind, reply) => {
                            (false, false, false, Some(kind), reply)
                        }
                    };
                    if reply.is_closed() {
                        continue;
                    }
                    if let Some(session) = generation.as_ref() {
                        if !session.driver.is_closed() {
                            let result =
                                if connect || background || qualify || qualification_kind.is_some()
                                {
                                    Err("E_BROWSER_IN_USE")
                                } else {
                                    Ok(status.clone())
                                };
                            let _ = reply.send(result);
                            continue;
                        }
                        generation = None;
                        status = ControlStatus::default();
                    }
                    if instance_lock.is_none() {
                        match paths.lock() {
                            Ok(lock) => instance_lock = Some(lock),
                            Err(_) => {
                                let _ = reply.send(Err("E_ALREADY_RUNNING"));
                                continue;
                            }
                        }
                    }
                    // Reconnect never replays a qualification request.
                    if connect && status.phase == "browser_unavailable" {
                        // A failed DOM/pipe observation cannot authorize killing
                        // a live browser and its uncertain qualification tabs.
                        match browser.as_ref().map(ManagedBrowser::has_exited) {
                            Some(Ok(false)) if page.is_some() => {
                                // Reobserve the same page below; do not recreate it.
                            }
                            Some(Err(_)) => {
                                let _ = reply.send(Err("E_BROWSER_RELEASE"));
                                continue;
                            }
                            None | Some(Ok(true)) | Some(Ok(false)) => {
                                if let Some(current) = browser.as_mut()
                                    && let Err(error) = current.close_for_replacement(None)
                                {
                                    let _ = reply.send(Err(replacement_error(&error)));
                                    continue;
                                }
                                page = None;
                                browser = None;
                                status = ControlStatus::default();
                                observed_routes.clear();
                                observed_scope = None;
                            }
                        }
                    }
                    if connect
                        && (page.is_none()
                            || (status.phase == "authenticating"
                                && page.as_ref().is_some_and(ManagedPage::is_hidden)))
                    {
                        if page.as_ref().is_some_and(ManagedPage::is_hidden) {
                            if let Some(current) = browser.as_mut()
                                && let Err(error) = current.close_for_replacement(page.as_ref())
                            {
                                let _ = reply.send(Err(replacement_error(&error)));
                                continue;
                            }
                            page = None;
                            browser = None;
                        }
                        let opened = (|| {
                            if browser.is_none() {
                                browser = Some(
                                    ManagedBrowser::launch(&executable, &paths.profile, true)
                                        .map_err(|_| "E_BROWSER_START")?,
                                );
                            }
                            let browser = browser.as_mut().ok_or("E_BROWSER_START")?;
                            if let Some(current) = page.as_ref() {
                                browser
                                    .close_page_checked(current)
                                    .map_err(|_| "E_BROWSER_RELEASE")?;
                                page = None;
                            }
                            let version = browser.version().map_err(|_| "E_BROWSER_PIPE")?;
                            status.browser_version = version["product"].as_str().map(str::to_owned);
                            page = Some(browser.open_login().map_err(|error| {
                                match error.to_string().as_str() {
                                    "E_HIDDEN_TARGET" => "E_HIDDEN_TARGET",
                                    "E_HIDDEN_ATTACH" => "E_HIDDEN_ATTACH",
                                    "E_HIDDEN_VIEWPORT" => "E_HIDDEN_VIEWPORT",
                                    _ => "E_BROWSER_LOGIN",
                                }
                            })?);
                            status = ControlStatus {
                                phase: "authenticating".into(),
                                browser_version: status.browser_version.take(),
                                ..Default::default()
                            };
                            observed_routes.clear();
                            observed_scope = None;
                            Ok(())
                        })();
                        if let Err(error) = opened {
                            // Keep a live owner on launch/navigation failure;
                            // dropping it would kill other retained targets.
                            status = ControlStatus {
                                phase: if browser.is_some() {
                                    "browser_unavailable"
                                } else {
                                    "disconnected"
                                }
                                .into(),
                                ..Default::default()
                            };
                            observed_routes.clear();
                            observed_scope = None;
                            let _ = reply.send(Err(error));
                            continue;
                        }
                        // Login is user-driven. Do not inspect its DOM while
                        // navigation/authentication is in progress. The user
                        // explicitly requests a status check after signing in.
                        let _ = reply.send(Ok(status.clone()));
                        continue;
                    }
                    if background {
                        let result = match (browser.as_mut(), page.as_ref()) {
                            (Some(browser), Some(page)) => browser
                                .close_login_window(page)
                                .map_err(|error| match error.to_string().as_str() {
                                    "E_BROWSER_BUSY" => "E_BROWSER_BUSY",
                                    "E_BROWSER_OTHER_PAGES" => "E_BROWSER_OTHER_PAGES",
                                    "E_LOGIN_REQUIRED" => "E_LOGIN_REQUIRED",
                                    _ => "E_BROWSER_RELEASE",
                                }),
                            (None, None) => Ok(()),
                            _ => Err("E_LOGIN_REQUIRED"),
                        };
                        if let Err(code) = result {
                            let _ = reply.send(Err(code));
                            continue;
                        }
                    }
                    let restore_background = (background && browser.is_none() && page.is_none())
                        || match (browser.as_mut(), page.as_ref()) {
                            (Some(browser), Some(current)) => {
                                matches!(browser.has_exited(), Ok(true))
                                    || matches!(browser.page_exists(current), Ok(false))
                            }
                            _ => false,
                        };
                    if restore_background {
                        if let Some(current) = browser.as_mut()
                            && let Err(error) = current.close_for_replacement(None)
                        {
                            let _ = reply.send(Err(replacement_error(&error)));
                            continue;
                        }
                        // A closed login window does not discard the saved
                        // session. Replace only a positively absent target;
                        // transport timeouts never trigger a second browser.
                        let restored = (|| {
                            page = None;
                            browser = None;
                            let mut managed =
                                ManagedBrowser::launch_offscreen(&executable, &paths.profile)
                                    .map_err(|_| std::io::Error::other("E_BROWSER_START"))?;
                            let version = managed.version()?;
                            let background_page = managed.open_background_session()?;
                            status = ControlStatus {
                                phase: "authenticating".into(),
                                background_session: true,
                                browser_version: version["product"].as_str().map(str::to_owned),
                                ..Default::default()
                            };
                            observed_routes.clear();
                            observed_scope = None;
                            browser = Some(managed);
                            Ok::<_, std::io::Error>(background_page)
                        })();
                        match restored {
                            Ok(background_page) => page = Some(background_page),
                            Err(error) => {
                                page = None;
                                status = ControlStatus {
                                    phase: "browser_unavailable".into(),
                                    ..Default::default()
                                };
                                observed_routes.clear();
                                observed_scope = None;
                                let _ = reply.send(Err(match error.to_string().as_str() {
                                    "E_HIDDEN_TARGET" => "E_HIDDEN_TARGET",
                                    "E_HIDDEN_ATTACH" => "E_HIDDEN_ATTACH",
                                    "E_HIDDEN_VIEWPORT" => "E_HIDDEN_VIEWPORT",
                                    "E_BACKGROUND_NAVIGATION" => "E_BACKGROUND_NAVIGATION",
                                    "E_BACKGROUND_WINDOW" => "E_BACKGROUND_WINDOW",
                                    "E_BROWSER_START" => "E_BROWSER_START",
                                    _ => "E_BROWSER_RELEASE",
                                }));
                                continue;
                            }
                        }
                    }
                    if let (Some(browser), Some(page)) = (browser.as_mut(), page.as_ref()) {
                        status.background_session = page.is_hidden();
                        // A fresh observation supersedes the previous text test,
                        // including when discovery exits early with an error.
                        status.text_qualified_model = None;
                        status.tool_qualified_model = None;
                        status.qualification_evidence = None;
                        match browser.login_observation(page) {
                            Ok(observation) => {
                                status.phase = if authenticated_surface(&observation) {
                                    "awaiting_qualification".into()
                                } else {
                                    "authenticating".into()
                                };
                                status.observation = Some(observation);
                                if status.phase != "awaiting_qualification" {
                                    observed_routes.clear();
                                    observed_scope = None;
                                    status.candidate_models.clear();
                                    status.text_qualified_model = None;
                                    status.qualification_evidence = None;
                                    status.temporary_chat_available = None;
                                }
                                if qualify && status.phase == "awaiting_qualification" {
                                    match browser.discover_models(page) {
                                        Ok(surface) => {
                                            let selected = surface
                                                .candidates
                                                .iter()
                                                .filter(|candidate| candidate.selected)
                                                .cloned()
                                                .collect::<Vec<_>>();
                                            let selection = if selected.len() == 1 {
                                                match browser
                                                    .select_candidate(page, &selected[0].identity)
                                                {
                                                    Ok(label) if label == selected[0].label => {
                                                        Ok(())
                                                    }
                                                    Ok(_) => Err("E_MODEL_LABEL"),
                                                    Err(error) => {
                                                        Err(match error.to_string().as_str() {
                                                            "E_MODEL_OPEN" => "E_MODEL_OPEN",
                                                            "E_MODEL_SELECT" => "E_MODEL_SELECT",
                                                            "E_MODEL_CLOSE" => "E_MODEL_CLOSE",
                                                            _ => "E_MODEL_SELECTION",
                                                        })
                                                    }
                                                }
                                            } else {
                                                Err("E_MODEL_SELECTION")
                                            };
                                            if let Err(code) = selection {
                                                observed_routes.clear();
                                                observed_scope = None;
                                                status.candidate_models.clear();
                                                status.temporary_chat_available = None;
                                                status.model_discovery_diagnostic =
                                                    Some(surface.diagnostic);
                                                let _ = reply.send(Err(code));
                                                continue;
                                            }
                                            let temporary_chat =
                                                browser.verify_temporary_chat().unwrap_or(false);
                                            status.scope_diagnostic =
                                                Some(match browser.account_scope(page) {
                                                    Ok(scope) => {
                                                        observed_scope =
                                                            BrowserScope::from_surface(
                                                                &scope_installation,
                                                                &scope,
                                                            )
                                                            .ok();
                                                        scope.diagnostic
                                                    }
                                                    Err(error) => {
                                                        observed_scope = None;
                                                        cxweb_browser_adapter::ScopeDiagnostic {
                                                            failure: Some(
                                                                match error.to_string().as_str() {
                                                                    "E_ACCOUNT_OPEN" => {
                                                                        "E_ACCOUNT_OPEN"
                                                                    }
                                                                    "E_ACCOUNT_MISSING" => {
                                                                        "E_ACCOUNT_MISSING"
                                                                    }
                                                                    "E_ACCOUNT_AMBIGUOUS" => {
                                                                        "E_ACCOUNT_AMBIGUOUS"
                                                                    }
                                                                    "E_ACCOUNT_READ" => {
                                                                        "E_ACCOUNT_READ"
                                                                    }
                                                                    "E_ACCOUNT_PARSE" => {
                                                                        "E_ACCOUNT_PARSE"
                                                                    }
                                                                    "E_ACCOUNT_SETTINGS_CLOSE" => {
                                                                        "E_ACCOUNT_SETTINGS_CLOSE"
                                                                    }
                                                                    _ => "E_ACCOUNT_SCOPE",
                                                                }
                                                                .into(),
                                                            ),
                                                            ..Default::default()
                                                        }
                                                    }
                                                });
                                            observed_routes = surface
                                                .candidates
                                                .into_iter()
                                                .map(|candidate| {
                                                    let mut hash = Sha256::new();
                                                    for value in
                                                        [&candidate.identity, &candidate.label]
                                                    {
                                                        hash.update(
                                                            (value.len() as u64).to_le_bytes(),
                                                        );
                                                        hash.update(value.as_bytes());
                                                    }
                                                    (
                                                        QualifiedModel {
                                                            id: format!(
                                                                "webbridge/{:x}",
                                                                hash.finalize()
                                                            )[..34]
                                                                .to_owned(),
                                                            label: candidate.label,
                                                            selected: candidate.selected,
                                                        },
                                                        candidate.identity,
                                                    )
                                                })
                                                .collect();
                                            status.candidate_models = observed_routes
                                                .iter()
                                                .map(|(model, _)| model.clone())
                                                .collect();
                                            status.text_qualified_model = None;
                                            status.qualification_evidence = None;
                                            status.temporary_chat_available = Some(temporary_chat);
                                            status.model_discovery_diagnostic =
                                                Some(surface.diagnostic);
                                            status.phase = if status.candidate_models.is_empty() {
                                                "discovery_failed"
                                            } else {
                                                "candidates_observed"
                                            }
                                            .into();
                                        }
                                        Err(error) => {
                                            status.candidate_models.clear();
                                            observed_routes.clear();
                                            observed_scope = None;
                                            status.temporary_chat_available = None;
                                            status.model_discovery_diagnostic =
                                                browser.model_diagnostic();
                                            let code = match error.to_string().as_str() {
                                                "E_LOGIN_REQUIRED" => "E_LOGIN_REQUIRED",
                                                "E_MODEL_OPEN" => "E_MODEL_OPEN",
                                                "E_MODEL_READ" => "E_MODEL_READ",
                                                "E_MODEL_CLOSE" => "E_MODEL_CLOSE",
                                                "E_MODEL_PARSE" => "E_MODEL_PARSE",
                                                "E_MODEL_RESULT" => "E_MODEL_RESULT",
                                                "E_MODEL_RESTORE" => "E_MODEL_RESTORE",
                                                "E_MODEL_FAMILY" => "E_MODEL_FAMILY",
                                                "E_MODEL_SELECT" => "E_MODEL_SELECT",
                                                "E_MODEL_SELECTION" => "E_MODEL_SELECTION",
                                                "E_BROWSER_BUSY" => "E_BROWSER_BUSY",
                                                _ => "E_MODEL_DISCOVERY",
                                            };
                                            let _ = reply.send(Err(code));
                                            continue;
                                        }
                                    }
                                }
                                if let Some(kind) = qualification_kind
                                    .filter(|_| status.phase == "awaiting_qualification")
                                {
                                    status.text_qualified_model = None;
                                    status.qualification_evidence = None;
                                    status.qualification_diagnostic = None;
                                    let selected = observed_routes
                                        .iter()
                                        .filter(|(model, _)| model.selected)
                                        .collect::<Vec<_>>();
                                    if selected.len() != 1 {
                                        let _ = reply.send(Err("E_MODEL_SELECTION"));
                                        continue;
                                    }
                                    let (model, identity) = selected[0];
                                    let Some(expected_scope) = observed_scope.as_ref() else {
                                        let _ = reply.send(Err("E_SESSION_SCOPE"));
                                        continue;
                                    };
                                    // Recheck the chosen route without traversing or changing
                                    // every other family immediately before a generation.
                                    let verified =
                                        browser.verify_candidate(page, identity, &model.label);
                                    status.model_discovery_diagnostic = browser.model_diagnostic();
                                    status.qualification_diagnostic =
                                        browser.qualification_diagnostic();
                                    if verified.is_err() {
                                        let _ = reply.send(Err("E_MODEL_SELECTION"));
                                        continue;
                                    }
                                    let nonce = format!("{:032x}", rand::random::<u128>());
                                    let request = match kind.request(&model.id) {
                                        Ok(request) => request,
                                        Err(_) => {
                                            let _ = reply.send(Err("E_QUALIFICATION_REQUEST"));
                                            continue;
                                        }
                                    };
                                    let prompt = match request.browser_prompt(&nonce, 512 * 1024) {
                                        Ok(prompt) => prompt,
                                        Err(_) => {
                                            let _ = reply.send(Err("E_QUALIFICATION_REQUEST"));
                                            continue;
                                        }
                                    };
                                    let intent = runtime.block_on(async {
                                        let ledger =
                                            Ledger::open(&paths.state.join("qualification.sqlite"))
                                                .await?;
                                        // Diagnostic only, bound to the actual observed UI scope.
                                        let scope = SessionKey {
                                            installation: scope_installation.clone(),
                                            native_session: nonce.clone(),
                                            account_scope: expected_scope.account.clone(),
                                            workspace_scope: expected_scope.workspace.clone(),
                                            route: model.id.clone(),
                                            epoch: 0,
                                        };
                                        if ledger.admit(&nonce, &scope, prompt.as_bytes()).await?
                                            != Admission::New
                                        {
                                            return Err("E_REQUEST_ALREADY_ADMITTED");
                                        }
                                        Ok(ledger)
                                    });
                                    let ledger = match intent {
                                        Ok(ledger) => ledger,
                                        Err(code) => {
                                            let _ = reply.send(Err(code));
                                            continue;
                                        }
                                    };
                                    let mut submission_intent = false;
                                    let result = browser.qualify_candidate(
                                        identity,
                                        &model.label,
                                        &prompt,
                                        || {
                                            runtime
                                                .block_on(async {
                                                    ledger
                                                        .transition(
                                                            &nonce,
                                                            TurnState::ObservedBaseline,
                                                        )
                                                        .await?;
                                                    ledger
                                                        .transition(&nonce, TurnState::Submitting)
                                                        .await
                                                })
                                                .map_err(std::io::Error::other)?;
                                            submission_intent = true;
                                            Ok(())
                                        },
                                        |browser, page| {
                                            let surface =
                                                browser.account_scope(page).map_err(|error| {
                                                    let code = match error.to_string().as_str() {
                                                        "E_ACCOUNT_OPEN" => "E_ACCOUNT_OPEN",
                                                        "E_ACCOUNT_SCOPE" => "E_ACCOUNT_SCOPE",
                                                        "E_ACCOUNT_READ" => "E_ACCOUNT_READ",
                                                        "E_ACCOUNT_PARSE" => "E_ACCOUNT_PARSE",
                                                        "E_ACCOUNT_SETTINGS_CLOSE" => {
                                                            "E_ACCOUNT_SETTINGS_CLOSE"
                                                        }
                                                        "E_MODEL_CLOSE" => "E_MODEL_CLOSE",
                                                        _ => "E_ACCOUNT_OPERATION",
                                                    };
                                                    status.scope_diagnostic = Some(
                                                        cxweb_browser_adapter::ScopeDiagnostic {
                                                            failure: Some(code.into()),
                                                            ..Default::default()
                                                        },
                                                    );
                                                    std::io::Error::other("E_SESSION_SCOPE")
                                                })?;
                                            let current = BrowserScope::from_surface(
                                                &scope_installation,
                                                &surface,
                                            );
                                            status.scope_diagnostic = Some(surface.diagnostic);
                                            if current
                                                .as_ref()
                                                .is_ok_and(|scope| scope == expected_scope)
                                            {
                                                Ok(())
                                            } else {
                                                Err(std::io::Error::other("E_SESSION_SCOPE"))
                                            }
                                        },
                                    );
                                    status.qualification_diagnostic =
                                        browser.qualification_diagnostic();
                                    status.model_discovery_diagnostic = browser.model_diagnostic();
                                    let outcome = match result {
                                        Ok(outcome) if outcome.candidate_label == model.label => {
                                            outcome
                                        }
                                        Ok(_) => {
                                            let _ = runtime.block_on(ledger.transition(
                                                &nonce,
                                                TurnState::SubmissionUncertain,
                                            ));
                                            let _ = reply.send(Err("E_MODEL_SELECTION"));
                                            continue;
                                        }
                                        Err(error) => {
                                            let terminal = qualification_failure_state(
                                                submission_intent,
                                                &error.to_string(),
                                            );
                                            let _ = runtime
                                                .block_on(ledger.transition(&nonce, terminal));
                                            let code = match error.to_string().as_str() {
                                                "E_SUBMISSION_UNCERTAIN" => {
                                                    "E_SUBMISSION_UNCERTAIN"
                                                }
                                                "E_QUALIFICATION_TIMEOUT" => {
                                                    "E_QUALIFICATION_TIMEOUT"
                                                }
                                                "E_TEMPORARY_CHAT" => "E_TEMPORARY_CHAT",
                                                "E_SESSION_SCOPE" => "E_SESSION_SCOPE",
                                                "E_SEND_SURFACE" => "E_SEND_SURFACE",
                                                "E_SEND_DISABLED" => "E_SEND_DISABLED",
                                                "E_COMPOSER_MISMATCH" => "E_COMPOSER_MISMATCH",
                                                "E_MODEL_SELECTION" => "E_MODEL_SELECTION",
                                                "E_QUALIFICATION_SELECT" => {
                                                    "E_QUALIFICATION_SELECT"
                                                }
                                                "E_QUALIFICATION_BASELINE" => {
                                                    "E_QUALIFICATION_BASELINE"
                                                }
                                                "E_QUALIFICATION_INSERT" => {
                                                    "E_QUALIFICATION_INSERT"
                                                }
                                                "E_QUALIFICATION_OBSERVE" => {
                                                    "E_QUALIFICATION_OBSERVE"
                                                }
                                                "E_TURN_AMBIGUOUS" => "E_TURN_AMBIGUOUS",
                                                "E_TURN_ATTRIBUTION" => "E_TURN_ATTRIBUTION",
                                                "E_USER_MESSAGE_MISMATCH" => {
                                                    "E_USER_MESSAGE_MISMATCH"
                                                }
                                                "E_MODEL_FIDELITY" => "E_MODEL_FIDELITY",
                                                "E_INVALID_TOOL_ENVELOPE" => {
                                                    "E_INVALID_TOOL_ENVELOPE"
                                                }
                                                _ => "E_LIVE_QUALIFICATION",
                                            };
                                            let _ = reply.send(Err(code));
                                            continue;
                                        }
                                    };
                                    let recorded = runtime.block_on(async {
                                        ledger.transition(&nonce, TurnState::Submitted).await?;
                                        ledger.transition(&nonce, TurnState::Generating).await
                                    });
                                    if let Err(code) = recorded {
                                        let _ = reply.send(Err(code));
                                        continue;
                                    }
                                    let valid = envelope::validate(
                                        outcome.response.as_bytes(),
                                        &request.context(&nonce),
                                    );
                                    if !valid.as_ref().is_ok_and(|output| kind.accepts(output)) {
                                        let _ = runtime
                                            .block_on(ledger.transition(&nonce, TurnState::Failed));
                                        let _ = reply.send(Err("E_QUALIFICATION_PROTOCOL"));
                                        continue;
                                    }
                                    if let Err(code) = runtime
                                        .block_on(ledger.transition(&nonce, TurnState::Completed))
                                    {
                                        let _ = reply.send(Err(code));
                                        continue;
                                    }
                                    let mut evidence = Sha256::new();
                                    let evidence_parts: [&[u8]; 4] = [
                                        model.id.as_bytes(),
                                        model.label.as_bytes(),
                                        outcome.response.as_bytes(),
                                        b"cxweb-browser-adapter-v1",
                                    ];
                                    for value in evidence_parts {
                                        evidence.update((value.len() as u64).to_le_bytes());
                                        evidence.update(value);
                                    }
                                    match kind {
                                        Kind::Text => {
                                            status.text_qualified_model = Some(model.id.clone())
                                        }
                                        Kind::Tools => {
                                            status.tool_qualified_model = Some(model.id.clone())
                                        }
                                    }
                                    status.qualification_evidence =
                                        Some(format!("{:x}", evidence.finalize()));
                                    status.phase = match kind {
                                        Kind::Text => "text_qualified",
                                        Kind::Tools => "tool_protocol_qualified",
                                    }
                                    .into();
                                }
                            }
                            Err(_) => {
                                status.phase = "browser_unavailable".into();
                                status.observation = None;
                                status.text_qualified_model = None;
                                status.qualification_evidence = None;
                                status.candidate_models.clear();
                                observed_routes.clear();
                                observed_scope = None;
                            }
                        }
                    }
                    let _ = reply.send(Ok(status.clone()));
                }
                // The daemon owns this worker; closing a remote UI does not drop it.
                if let Some(mut browser) = browser {
                    let _ = browser.close();
                }
            })
            .map_err(|_| "E_BROWSER_WORKER")?;
        Ok(Self { commands })
    }
    /// Last worker receipt, without re-observing or clearing qualification.
    pub(crate) async fn snapshot(&self) -> Result<ControlStatus, &'static str> {
        let (reply, receive) = oneshot::channel();
        self.commands
            .try_send(WorkerCommand::Snapshot(reply))
            .map_err(|_| "E_CONTROL_BUSY")?;
        receive.await.map_err(|_| "E_CONTROL_CLOSED")?
    }
    pub async fn connect(&self) -> Result<ControlStatus, &'static str> {
        self.request(true).await
    }
    /// Internal daemon handoff, not a desktop IPC operation or activation grant.
    /// The selected route must pass protocol qualification in background mode.
    /// Native coding, picker and configuration gates remain the caller's duty.
    pub async fn take_generation(
        &self,
        installation: String,
        route: String,
    ) -> Result<Arc<GenerationSession>, &'static str> {
        let (reply, receive) = oneshot::channel();
        self.commands
            .try_send(WorkerCommand::TakeGeneration {
                installation,
                route,
                reply,
            })
            .map_err(|_| "E_CONTROL_BUSY")?;
        receive.await.map_err(|_| "E_CONTROL_CLOSED")?
    }
    pub async fn background(&self) -> Result<ControlStatus, &'static str> {
        let (reply, receive) = oneshot::channel();
        self.commands
            .try_send(WorkerCommand::Background(reply))
            .map_err(|_| "E_CONTROL_BUSY")?;
        // The accepted operation owns its receipt until the worker actually
        // finishes. A caller's observation deadline must not mark running work
        // failed while the browser continues changing selections in the queue.
        receive.await.map_err(|_| "E_CONTROL_CLOSED")?
    }
    pub async fn status(&self) -> Result<ControlStatus, &'static str> {
        self.request(false).await
    }
    pub async fn qualify(&self) -> Result<ControlStatus, &'static str> {
        let (reply, receive) = oneshot::channel();
        self.commands
            .try_send(WorkerCommand::Qualify(reply))
            .map_err(|_| "E_CONTROL_BUSY")?;
        receive.await.map_err(|_| "E_CONTROL_CLOSED")?
    }
    pub async fn qualify_text(&self) -> Result<ControlStatus, &'static str> {
        self.qualify_turn(Kind::Text).await
    }
    pub async fn qualify_tools(&self) -> Result<ControlStatus, &'static str> {
        self.qualify_turn(Kind::Tools).await
    }
    async fn qualify_turn(&self, kind: Kind) -> Result<ControlStatus, &'static str> {
        let (reply, receive) = oneshot::channel();
        self.commands
            .try_send(WorkerCommand::QualifyTurn(kind, reply))
            .map_err(|_| "E_CONTROL_BUSY")?;
        receive.await.map_err(|_| "E_CONTROL_CLOSED")?
    }
    async fn request(&self, connect: bool) -> Result<ControlStatus, &'static str> {
        let (reply, receive) = oneshot::channel();
        self.commands
            .try_send(if connect {
                WorkerCommand::Connect(reply)
            } else {
                WorkerCommand::Status(reply)
            })
            .map_err(|_| "E_CONTROL_BUSY")?;
        receive.await.map_err(|_| "E_CONTROL_CLOSED")?
    }
}
