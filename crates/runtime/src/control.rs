//! Non-generative login worker. All browser IPC runs off the UI/async reactor.
use cxweb_browser_adapter::{
    LoginObservation, ManagedBrowser, ManagedPage, ModelSurfaceDiagnostic,
};
use cxweb_platform::state::{StatePaths, installed_browser};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlStatus {
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
            phase: "disconnected".into(),
            browser_version: None,
            observation: None,
            routing_installed: false,
            live_qualified: false,
            candidate_models: Vec::new(),
            temporary_chat_available: None,
            model_discovery_diagnostic: None,
        }
    }
}
type Reply = oneshot::Sender<Result<ControlStatus, &'static str>>;
enum WorkerCommand {
    Connect(Reply),
    Status(Reply),
    Qualify(Reply),
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
        std::thread::Builder::new()
            .name("cxweb-browser-control".into())
            .spawn(move || {
                let _instance_lock = lock;
                let mut browser: Option<ManagedBrowser> = None;
                let mut page: Option<ManagedPage> = None;
                let mut status = ControlStatus::default();
                while let Some(command) = incoming.blocking_recv() {
                    let (connect, qualify, reply) = match command {
                        WorkerCommand::Connect(reply) => (true, false, reply),
                        WorkerCommand::Status(reply) => (false, false, reply),
                        WorkerCommand::Qualify(reply) => (false, true, reply),
                    };
                    if reply.is_closed() {
                        continue;
                    }
                    // Reconnect is an explicit UI action. This controller never
                    // submits a generation, so replacing its failed login page
                    // cannot repeat an uncertain request.
                    if connect && status.phase == "browser_unavailable" {
                        page = None;
                        browser = None;
                        status = ControlStatus::default();
                    }
                    if connect && page.is_none() {
                        let opened = (|| {
                            if browser.is_none() {
                                browser = Some(
                                    ManagedBrowser::launch(&executable, &paths.profile, true)
                                        .map_err(|_| "E_BROWSER_START")?,
                                );
                            }
                            let browser = browser.as_mut().ok_or("E_BROWSER_START")?;
                            let version = browser.version().map_err(|_| "E_BROWSER_PIPE")?;
                            status.browser_version = version["product"].as_str().map(str::to_owned);
                            page = Some(browser.open_login().map_err(|_| "E_BROWSER_LOGIN")?);
                            status.phase = "authenticating".into();
                            Ok(())
                        })();
                        if let Err(error) = opened {
                            page = None;
                            browser = None;
                            status = ControlStatus::default();
                            let _ = reply.send(Err(error));
                            continue;
                        }
                        // Login is user-driven. Do not inspect its DOM while
                        // navigation/authentication is in progress. The user
                        // explicitly requests a status check after signing in.
                        let _ = reply.send(Ok(status.clone()));
                        continue;
                    }
                    if let (Some(browser), Some(page)) = (browser.as_mut(), page.as_ref()) {
                        match browser.login_observation(page) {
                            Ok(observation) => {
                                status.phase = if observation.official_page
                                    && observation.composer
                                    && observation.account_surface
                                    && !observation.login_action
                                {
                                    "awaiting_qualification".into()
                                } else {
                                    "authenticating".into()
                                };
                                status.observation = Some(observation);
                                if qualify && status.phase == "awaiting_qualification" {
                                    match browser.discover_models(page) {
                                        Ok(surface) => {
                                            let selected = surface
                                                .candidates
                                                .iter()
                                                .filter(|candidate| candidate.selected)
                                                .cloned()
                                                .collect::<Vec<_>>();
                                            if selected.len() != 1
                                                || !matches!(
                                                    browser.select_candidate(
                                                        page,
                                                        &selected[0].identity
                                                    ),
                                                    Ok(label) if label == selected[0].label
                                                )
                                            {
                                                status.candidate_models.clear();
                                                status.temporary_chat_available = None;
                                                status.model_discovery_diagnostic =
                                                    Some(surface.diagnostic);
                                                let _ = reply.send(Err("E_MODEL_SELECTION"));
                                                continue;
                                            }
                                            let temporary_chat =
                                                browser.verify_temporary_chat().unwrap_or(false);
                                            status.candidate_models = surface
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
                                                    QualifiedModel {
                                                        id: format!(
                                                            "webbridge/{:x}",
                                                            hash.finalize()
                                                        )[..34]
                                                            .to_owned(),
                                                        label: candidate.label,
                                                        selected: candidate.selected,
                                                    }
                                                })
                                                .collect();
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
                                            status.temporary_chat_available = None;
                                            status.model_discovery_diagnostic = None;
                                            let code = match error.to_string().as_str() {
                                                "E_LOGIN_REQUIRED" => "E_LOGIN_REQUIRED",
                                                "E_MODEL_OPEN" => "E_MODEL_OPEN",
                                                "E_MODEL_READ" => "E_MODEL_READ",
                                                "E_MODEL_CLOSE" => "E_MODEL_CLOSE",
                                                "E_MODEL_PARSE" => "E_MODEL_PARSE",
                                                "E_MODEL_RESULT" => "E_MODEL_RESULT",
                                                _ => "E_MODEL_DISCOVERY",
                                            };
                                            let _ = reply.send(Err(code));
                                            continue;
                                        }
                                    }
                                }
                            }
                            Err(_) => {
                                status.phase = "browser_unavailable".into();
                                status.observation = None;
                            }
                        }
                    }
                    let _ = reply.send(Ok(status.clone()));
                }
                // There are no generation requests in this login-only controller.
                // The daemon owns this worker; closing a remote UI does not drop it.
                if let Some(mut browser) = browser {
                    let _ = browser.close();
                }
            })
            .map_err(|_| "E_BROWSER_WORKER")?;
        Ok(Self { commands })
    }
    pub async fn connect(&self) -> Result<ControlStatus, &'static str> {
        self.request(true).await
    }
    pub async fn status(&self) -> Result<ControlStatus, &'static str> {
        self.request(false).await
    }
    pub async fn qualify(&self) -> Result<ControlStatus, &'static str> {
        let (reply, receive) = oneshot::channel();
        self.commands
            .try_send(WorkerCommand::Qualify(reply))
            .map_err(|_| "E_CONTROL_BUSY")?;
        tokio::time::timeout(Duration::from_secs(30), receive)
            .await
            .map_err(|_| "E_CONTROL_TIMEOUT")?
            .map_err(|_| "E_CONTROL_CLOSED")?
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
        tokio::time::timeout(Duration::from_secs(30), receive)
            .await
            .map_err(|_| "E_CONTROL_TIMEOUT")?
            .map_err(|_| "E_CONTROL_CLOSED")?
    }
}
