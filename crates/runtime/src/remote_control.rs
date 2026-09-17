//! Desktop and diagnostic attachment to the separate login/qualification runtime.
use crate::{
    control::{Control, ControlStatus},
    control_protocol::{self, Command, ErrorCode, LoginAction, Outcome, Reply, Request, Service},
};
use cxweb_platform::control_pipe;
use sha2::{Digest, Sha256};
use std::os::windows::process::CommandExt;
use std::{
    path::PathBuf,
    process::{Command as ProcessCommand, Stdio},
    sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub fn login_channel() -> String {
    format!("{:x}", Sha256::digest(b"cxweb/login/runtime/v1"))[..32].to_owned()
}

pub async fn serve_login() -> Result<(), &'static str> {
    let control = Control::start()?;
    let listener = control_pipe::listen(&login_channel()).map_err(|_| "E_CONTROL_LISTENER")?;
    Service::login(Arc::new(control))
        .serve(listener, CancellationToken::new())
        .await
        .map_err(|_| "E_CONTROL_LISTENER")
}

pub struct RemoteControl {
    executable: Option<PathBuf>,
    channel: String,
    serial: Mutex<()>,
}
impl RemoteControl {
    pub fn new() -> Result<Self, &'static str> {
        let desktop = std::env::current_exe().map_err(|_| "E_RUNTIME_PATH")?;
        let executable = desktop
            .parent()
            .ok_or("E_RUNTIME_PATH")?
            .join("cxweb-daemon.exe");
        Ok(Self {
            executable: Some(executable),
            channel: login_channel(),
            serial: Mutex::new(()),
        })
    }
    async fn snapshot(&self) -> std::io::Result<Reply> {
        control_protocol::exchange(
            &self.channel,
            &Request {
                version: 1,
                command: Command::BrowserStatus {},
            },
        )
        .await
    }
    async fn attach(&self) -> Result<(String, ControlStatus), &'static str> {
        let reply = match self.snapshot().await {
            Ok(reply) => reply,
            // Only a missing endpoint permits a launch. Busy, slow or rejected
            // peers never trigger a competing process or a forced restart.
            Err(error) if error.raw_os_error() == Some(2) => {
                let executable = self
                    .executable
                    .as_ref()
                    .filter(|path| path.is_file())
                    .ok_or("E_RUNTIME_MISSING")?;
                let mut child = ProcessCommand::new(executable)
                    .arg("--login-runtime")
                    .creation_flags(0x08000000) // CREATE_NO_WINDOW; no shell or inherited terminal.
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|_| "E_RUNTIME_START")?;
                let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
                loop {
                    if let Ok(reply) = self.snapshot().await {
                        break reply;
                    }
                    if child.try_wait().map_err(|_| "E_RUNTIME_START")?.is_some() {
                        // Another desktop may have won the single-instance lock.
                        if let Ok(reply) = self.snapshot().await {
                            break reply;
                        }
                        return Err("E_RUNTIME_START");
                    }
                    if tokio::time::Instant::now() >= deadline {
                        return Err("E_CONTROL_TIMEOUT");
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                // Dropping Child does not terminate it. The process, profile and
                // browser outlive this client/window and are protected by the
                // runtime's own installation lock and browser job object.
            }
            Err(_) => return Err("E_CONTROL_UNAVAILABLE"),
        };
        match reply {
            Reply::BrowserStatus {
                instance, status, ..
            } => Ok((instance, *status)),
            _ => Err("E_RUNTIME_PROTOCOL"),
        }
    }
    pub async fn connect(&self) -> Result<ControlStatus, &'static str> {
        self.perform(LoginAction::Connect).await
    }
    pub async fn qualify(&self) -> Result<ControlStatus, &'static str> {
        self.perform(LoginAction::Qualify).await
    }
    pub async fn qualify_text(&self) -> Result<ControlStatus, &'static str> {
        self.perform(LoginAction::QualifyText).await
    }
    pub async fn status(&self, refresh: bool) -> Result<ControlStatus, &'static str> {
        if refresh {
            return self.perform(LoginAction::Refresh).await;
        }
        let _request = self.serial.lock().await;
        Ok(self.attach().await?.1)
    }
    async fn perform(&self, action: LoginAction) -> Result<ControlStatus, &'static str> {
        let _request = self.serial.lock().await;
        let (instance, _) = self.attach().await?;
        let operation = format!("{:032x}", rand::random::<u128>());
        let request = Request {
            version: 1,
            command: Command::Browser {
                instance: instance.clone(),
                operation: operation.clone(),
                action,
            },
        };
        let mut response = control_protocol::exchange(&self.channel, &request).await;
        let deadline = tokio::time::Instant::now()
            + if action == LoginAction::QualifyText {
                Duration::from_secs(335)
            } else {
                Duration::from_secs(35)
            };
        loop {
            match response {
                Ok(Reply::Operation {
                    outcome: Outcome::LoginCompleted { status },
                    ..
                }) => return Ok(*status),
                Ok(Reply::Operation {
                    outcome: Outcome::LoginFailed { code },
                    ..
                }) => return Err(control_protocol::login_error(&code)),
                Ok(Reply::Operation {
                    outcome: Outcome::Failed {},
                    ..
                }) => return Err("E_LOGIN_OPERATION"),
                Ok(Reply::Operation {
                    outcome: Outcome::Running {},
                    ..
                })
                | Err(_) => {}
                Ok(Reply::Error {
                    code: ErrorCode::UnknownOperation,
                }) => {
                    // An unacknowledged send may never have been admitted. The
                    // same ID and instance make this retry safe and bounded.
                    if tokio::time::Instant::now() >= deadline {
                        return Err("E_CONTROL_TIMEOUT");
                    }
                    response = control_protocol::exchange(&self.channel, &request).await;
                    continue;
                }
                Ok(Reply::Error {
                    code: ErrorCode::Busy,
                }) => return Err("E_CONTROL_BUSY"),
                _ => return Err("E_RUNTIME_PROTOCOL"),
            }
            if tokio::time::Instant::now() >= deadline {
                return Err("E_CONTROL_TIMEOUT");
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            response = control_protocol::exchange(
                &self.channel,
                &Request {
                    version: 1,
                    command: Command::Operation {
                        instance: instance.clone(),
                        operation: operation.clone(),
                    },
                },
            )
            .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_protocol::{LoginBackend, LoginWork};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::{Notify, Semaphore};
    struct Browser {
        connects: AtomicUsize,
        refreshes: AtomicUsize,
        started: Notify,
        finish: Arc<Semaphore>,
    }
    impl LoginBackend for Browser {
        fn request(&self, action: LoginAction) -> LoginWork {
            match action {
                LoginAction::Connect => {
                    self.connects.fetch_add(1, Ordering::SeqCst);
                }
                LoginAction::Refresh => {
                    self.refreshes.fetch_add(1, Ordering::SeqCst);
                }
                LoginAction::Qualify => {
                    self.refreshes.fetch_add(1, Ordering::SeqCst);
                }
                LoginAction::QualifyText => {
                    self.refreshes.fetch_add(1, Ordering::SeqCst);
                }
            }
            self.started.notify_one();
            let finish = self.finish.clone();
            Box::pin(async move {
                finish.acquire().await.unwrap().forget();
                Ok(ControlStatus {
                    phase: match action {
                        LoginAction::Connect => "authenticating",
                        LoginAction::Refresh => "awaiting_qualification",
                        LoginAction::Qualify => "candidates_observed",
                        LoginAction::QualifyText => "text_qualified",
                    }
                    .into(),
                    ..ControlStatus::default()
                })
            })
        }
    }
    fn client(channel: &str) -> RemoteControl {
        RemoteControl {
            executable: None,
            channel: channel.into(),
            serial: Mutex::new(()),
        }
    }
    fn fixture() -> (
        String,
        Arc<Browser>,
        CancellationToken,
        tokio::task::JoinHandle<std::io::Result<()>>,
    ) {
        let channel = format!("{:032x}", rand::random::<u128>());
        let listener = control_pipe::listen(&channel).unwrap();
        let browser = Arc::new(Browser {
            connects: AtomicUsize::new(0),
            refreshes: AtomicUsize::new(0),
            started: Notify::new(),
            finish: Arc::new(Semaphore::new(0)),
        });
        let service = Service::login(browser.clone());
        let stop = CancellationToken::new();
        let task = tokio::spawn({
            let stop = stop.clone();
            async move { service.serve(listener, stop).await }
        });
        (channel, browser, stop, task)
    }
    #[tokio::test]
    async fn reopening_desktop_uses_cached_state_without_touching_browser() {
        let (channel, browser, stop, task) = fixture();
        let first = client(&channel);
        assert_eq!(first.status(false).await.unwrap().phase, "disconnected");
        assert_eq!(browser.connects.load(Ordering::SeqCst), 0);
        browser.finish.add_permits(1);
        assert_eq!(first.connect().await.unwrap().phase, "authenticating");
        drop(first);
        let reopened = client(&channel);
        assert_eq!(
            reopened.status(false).await.unwrap().phase,
            "authenticating"
        );
        assert_eq!(browser.connects.load(Ordering::SeqCst), 1);
        assert_eq!(browser.refreshes.load(Ordering::SeqCst), 0);
        browser.finish.add_permits(1);
        assert_eq!(
            reopened.status(true).await.unwrap().phase,
            "awaiting_qualification"
        );
        assert_eq!(browser.refreshes.load(Ordering::SeqCst), 1);
        stop.cancel();
        task.await.unwrap().unwrap();
    }
    #[tokio::test]
    async fn closing_ui_waiter_keeps_admitted_login_operation_alive() {
        let (channel, browser, stop, task) = fixture();
        let first = client(&channel);
        let waiting = tokio::spawn(async move { first.connect().await });
        browser.started.notified().await;
        waiting.abort();
        assert!(waiting.await.unwrap_err().is_cancelled());
        browser.finish.add_permits(1);
        let reopened = client(&channel);
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if reopened.status(false).await.unwrap().phase == "authenticating" {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(browser.connects.load(Ordering::SeqCst), 1);
        stop.cancel();
        task.await.unwrap().unwrap();
    }
    #[tokio::test]
    async fn slow_existing_peer_does_not_trigger_another_launch() {
        let channel = format!("{:032x}", rand::random::<u128>());
        let mut listener = control_pipe::listen(&channel).unwrap();
        let waiting = tokio::spawn(async move {
            let _peer = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
        });
        assert_eq!(
            client(&channel).status(false).await.unwrap_err(),
            "E_CONTROL_UNAVAILABLE"
        );
        waiting.abort();
        let _ = waiting.await;
        let absent = format!("{:032x}", rand::random::<u128>());
        assert_eq!(
            client(&absent).status(false).await.unwrap_err(),
            "E_RUNTIME_MISSING"
        );
    }
}
