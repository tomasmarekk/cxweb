//! Non-generative login worker. All browser IPC runs off the UI/async reactor.
use cxweb_browser_adapter::{LoginObservation, ManagedBrowser, ManagedPage};
use cxweb_platform::state::{StatePaths, installed_browser};
use serde::Serialize;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

#[derive(Clone, Serialize)]
pub struct ControlStatus {
    pub phase: &'static str,
    pub browser_version: Option<String>,
    pub observation: Option<LoginObservation>,
    pub routing_installed: bool,
    pub live_qualified: bool,
}
impl Default for ControlStatus {
    fn default() -> Self {
        Self {
            phase: "disconnected",
            browser_version: None,
            observation: None,
            routing_installed: false,
            live_qualified: false,
        }
    }
}
type Reply = oneshot::Sender<Result<ControlStatus, &'static str>>;
enum Command {
    Connect(Reply),
    Status(Reply),
}
#[derive(Clone)]
pub struct Control {
    commands: mpsc::Sender<Command>,
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
                    let (connect, reply) = match command {
                        Command::Connect(reply) => (true, reply),
                        Command::Status(reply) => (false, reply),
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
                            status.phase = "authenticating";
                            Ok(())
                        })();
                        if let Err(error) = opened {
                            page = None;
                            browser = None;
                            status = ControlStatus::default();
                            let _ = reply.send(Err(error));
                            continue;
                        }
                    }
                    if let (Some(browser), Some(page)) = (browser.as_mut(), page.as_ref()) {
                        match browser.login_observation(page) {
                            Ok(observation) => {
                                status.phase = if observation.official_page
                                    && observation.composer
                                    && observation.account_surface
                                    && !observation.login_action
                                {
                                    "awaiting_qualification"
                                } else {
                                    "authenticating"
                                };
                                status.observation = Some(observation);
                            }
                            Err(_) => {
                                status.phase = "browser_unavailable";
                                status.observation = None;
                            }
                        }
                    }
                    let _ = reply.send(Ok(status.clone()));
                }
                // There are no generation requests in this login-only controller.
                // Production daemon ownership is a separate integration milestone.
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
    async fn request(&self, connect: bool) -> Result<ControlStatus, &'static str> {
        let (reply, receive) = oneshot::channel();
        self.commands
            .try_send(if connect {
                Command::Connect(reply)
            } else {
                Command::Status(reply)
            })
            .map_err(|_| "E_CONTROL_BUSY")?;
        tokio::time::timeout(Duration::from_secs(30), receive)
            .await
            .map_err(|_| "E_CONTROL_TIMEOUT")?
            .map_err(|_| "E_CONTROL_CLOSED")?
    }
}
