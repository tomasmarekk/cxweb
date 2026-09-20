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
    Service::login(Arc::new(crate::setup_owner::SetupOwner::new(control)))
        .serve(listener, CancellationToken::new())
        .await
        .map_err(|_| "E_CONTROL_LISTENER")
}

enum Action {
    Browser(LoginAction),
    NativeText(crate::setup_owner::NativeTarget),
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
    pub async fn qualify_tools(&self) -> Result<ControlStatus, &'static str> {
        self.perform(LoginAction::QualifyTools).await
    }
    pub async fn background(&self) -> Result<ControlStatus, &'static str> {
        self.perform(LoginAction::Background).await
    }
    pub async fn status(&self, refresh: bool) -> Result<ControlStatus, &'static str> {
        if refresh {
            return self.perform(LoginAction::Refresh).await;
        }
        // Cached status must remain readable while another request waits for
        // native generation, including from a reopened control window.
        Ok(self.attach().await?.1)
    }
    pub async fn cancel_native(
        &self,
        instance: String,
        operation: String,
    ) -> Result<(), &'static str> {
        // Target the exact operation observed by the UI. Never attach/restart or
        // substitute the currently running operation if that receipt is stale.
        let reply = control_protocol::exchange(
            &self.channel,
            &Request {
                version: 1,
                command: Command::CancelNative {
                    instance,
                    operation,
                },
            },
        )
        .await
        .map_err(|_| "E_CONTROL_UNAVAILABLE")?;
        match reply {
            Reply::Operation { .. } => Ok(()),
            _ => Err("E_RUNTIME_PROTOCOL"),
        }
    }
    pub async fn native_text(
        &self,
        target: crate::setup_owner::NativeTarget,
    ) -> Result<ControlStatus, &'static str> {
        self.perform_action(Action::NativeText(target)).await
    }
    async fn perform(&self, action: LoginAction) -> Result<ControlStatus, &'static str> {
        self.perform_action(Action::Browser(action)).await
    }
    async fn perform_action(&self, action: Action) -> Result<ControlStatus, &'static str> {
        let _request = self.serial.lock().await;
        let (instance, _) = self.attach().await?;
        let operation = format!("{:032x}", rand::random::<u128>());
        let duration = match &action {
            Action::NativeText(_) => Duration::from_secs(900),
            Action::Browser(LoginAction::QualifyText | LoginAction::QualifyTools) => {
                Duration::from_secs(335)
            }
            Action::Browser(LoginAction::Qualify) => Duration::from_secs(125),
            _ => Duration::from_secs(35),
        };
        let command = match action {
            Action::Browser(action) => Command::Browser {
                instance: instance.clone(),
                operation: operation.clone(),
                action,
            },
            Action::NativeText(target) => Command::NativeText {
                instance: instance.clone(),
                operation: operation.clone(),
                target,
            },
        };
        let request = Request {
            version: 1,
            command,
        };
        let mut response = control_protocol::exchange(&self.channel, &request).await;
        let deadline = tokio::time::Instant::now() + duration;
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
                LoginAction::Refresh | LoginAction::Background => {
                    self.refreshes.fetch_add(1, Ordering::SeqCst);
                }
                LoginAction::Qualify => {
                    self.refreshes.fetch_add(1, Ordering::SeqCst);
                }
                LoginAction::QualifyText | LoginAction::QualifyTools => {
                    self.refreshes.fetch_add(1, Ordering::SeqCst);
                }
            }
            self.started.notify_one();
            let finish = self.finish.clone();
            Box::pin(async move {
                finish.acquire().await.unwrap().forget();
                Ok(ControlStatus {
                    background_session: action == LoginAction::Background,
                    phase: match action {
                        LoginAction::Connect => "authenticating",
                        LoginAction::Refresh | LoginAction::Background => "awaiting_qualification",
                        LoginAction::Qualify => "candidates_observed",
                        LoginAction::QualifyText => "text_qualified",
                        LoginAction::QualifyTools => "tool_protocol_qualified",
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
    async fn background_transition_is_cached_without_reopening_login_or_qualifying_routes() {
        let (channel, browser, stop, task) = fixture();
        browser.finish.add_permits(1);
        let status = client(&channel).background().await.unwrap();
        assert!(status.background_session);
        assert!(!status.routing_installed && !status.live_qualified);
        assert_eq!(client(&channel).status(false).await.unwrap(), status);
        assert_eq!(browser.connects.load(Ordering::SeqCst), 0);
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
    async fn native_test_over_private_pipe_finishes_after_its_desktop_waiter_closes() {
        struct Native {
            started: Notify,
            finish: Arc<Semaphore>,
            calls: AtomicUsize,
        }
        impl LoginBackend for Native {
            fn request(&self, _: LoginAction) -> LoginWork {
                panic!("no browser observation requested");
            }
            fn native_text(
                &self,
                target: crate::setup_owner::NativeTarget,
                _cancellation: CancellationToken,
            ) -> LoginWork {
                assert_eq!(target.route, "webbridge/fixture");
                self.calls.fetch_add(1, Ordering::SeqCst);
                self.started.notify_one();
                let finish = self.finish.clone();
                Box::pin(async move {
                    finish.acquire().await.unwrap().forget();
                    Ok(ControlStatus {
                        phase: "generation_ready".into(),
                        background_session: true,
                        native_text_report: Some(crate::native_probe::Report {
                            client_build: "fixture".into(),
                            catalog_codec: "fixture".into(),
                            executable_sha256: "a".repeat(64),
                            exact_text_received: true,
                            native_tools_executed: 0,
                            browser_reused: true,
                            routing_installed: false,
                            actual_picker_verified: false,
                        }),
                        ..Default::default()
                    })
                })
            }
        }
        let channel = format!("{:032x}", rand::random::<u128>());
        let listener = control_pipe::listen(&channel).unwrap();
        let backend = Arc::new(Native {
            started: Notify::new(),
            finish: Arc::new(Semaphore::new(0)),
            calls: AtomicUsize::new(0),
        });
        let service = Service::login(backend.clone());
        let stop = CancellationToken::new();
        let server = tokio::spawn({
            let stop = stop.clone();
            async move { service.serve(listener, stop).await }
        });
        let remote = client(&channel);
        let waiting = tokio::spawn(async move {
            remote
                .native_text(crate::setup_owner::NativeTarget {
                    client: r"C:\fixture\codex.exe".into(),
                    home: r"C:\fixture\home".into(),
                    cwd: r"C:\fixture\workspace".into(),
                    route: "webbridge/fixture".into(),
                })
                .await
        });
        tokio::time::timeout(Duration::from_secs(2), backend.started.notified())
            .await
            .unwrap();
        waiting.abort();
        assert!(waiting.await.unwrap_err().is_cancelled());
        backend.finish.add_permits(1);
        let reopened = client(&channel);
        let status = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let status = reopened.status(false).await.unwrap();
                if status.native_text_report.is_some() {
                    break status;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(status.phase, "generation_ready");
        assert!(status.native_text_report.unwrap().exact_text_received);
        assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
        stop.cancel();
        server.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn reopened_ui_cancels_exact_native_operation_and_waits_for_cleanup() {
        struct Native {
            started: Notify,
            cancelled: Arc<Notify>,
            cleanup: Arc<Semaphore>,
            calls: AtomicUsize,
        }
        impl LoginBackend for Native {
            fn request(&self, _: LoginAction) -> LoginWork {
                panic!("no browser observation requested");
            }
            fn native_text(
                &self,
                _: crate::setup_owner::NativeTarget,
                cancellation: CancellationToken,
            ) -> LoginWork {
                self.calls.fetch_add(1, Ordering::SeqCst);
                self.started.notify_one();
                let cancelled = self.cancelled.clone();
                let cleanup = self.cleanup.clone();
                Box::pin(async move {
                    cancellation.cancelled().await;
                    cancelled.notify_one();
                    cleanup.acquire().await.unwrap().forget();
                    Ok(ControlStatus {
                        phase: "generation_ready".into(),
                        background_session: true,
                        native_text_error: Some("E_NATIVE_PROBE_CANCELLED".into()),
                        ..Default::default()
                    })
                })
            }
        }
        let channel = format!("{:032x}", rand::random::<u128>());
        let listener = control_pipe::listen(&channel).unwrap();
        let backend = Arc::new(Native {
            started: Notify::new(),
            cancelled: Arc::new(Notify::new()),
            cleanup: Arc::new(Semaphore::new(0)),
            calls: AtomicUsize::new(0),
        });
        let service = Service::login(backend.clone());
        let stop = CancellationToken::new();
        let server = tokio::spawn({
            let stop = stop.clone();
            async move { service.serve(listener, stop).await }
        });
        let remote = Arc::new(client(&channel));
        let target = crate::setup_owner::NativeTarget {
            client: r"C:\fixture\codex.exe".into(),
            home: r"C:\fixture\home".into(),
            cwd: r"C:\fixture\workspace".into(),
            route: "webbridge/fixture".into(),
        };
        let waiting = tokio::spawn({
            let remote = remote.clone();
            let target = target.clone();
            async move { remote.native_text(target).await }
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            backend.started.notified().await;
            // Reading from the same client must not wait for its mutation lock.
            let operation = remote
                .status(false)
                .await
                .unwrap()
                .native_operation
                .unwrap();
            assert!(!operation.cancellation_requested);
            waiting.abort();
            assert!(waiting.await.unwrap_err().is_cancelled());
            let reopened = client(&channel);
            assert!(
                reopened
                    .cancel_native("0".repeat(32), operation.operation.clone())
                    .await
                    .is_err()
            );
            assert!(
                reopened
                    .cancel_native(operation.instance.clone(), "0".repeat(32))
                    .await
                    .is_err()
            );
            assert!(
                !reopened
                    .status(false)
                    .await
                    .unwrap()
                    .native_operation
                    .unwrap()
                    .cancellation_requested
            );
            reopened
                .cancel_native(operation.instance.clone(), operation.operation.clone())
                .await
                .unwrap();
            backend.cancelled.notified().await;
            assert!(
                reopened
                    .status(false)
                    .await
                    .unwrap()
                    .native_operation
                    .unwrap()
                    .cancellation_requested
            );
            assert_eq!(
                reopened.native_text(target.clone()).await.unwrap_err(),
                "E_CONTROL_BUSY"
            );
            reopened
                .cancel_native(operation.instance.clone(), operation.operation.clone())
                .await
                .unwrap();
            backend.cleanup.add_permits(1);
            loop {
                let status = reopened.status(false).await.unwrap();
                if status.native_operation.is_none() {
                    assert_eq!(
                        status.native_text_error.as_deref(),
                        Some("E_NATIVE_PROBE_CANCELLED")
                    );
                    break;
                }
                tokio::task::yield_now().await;
            }
            assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
            let next = tokio::spawn({
                let remote = remote.clone();
                async move { remote.native_text(target).await }
            });
            backend.started.notified().await;
            let next_operation = reopened
                .status(false)
                .await
                .unwrap()
                .native_operation
                .unwrap();
            assert_ne!(operation.operation, next_operation.operation);
            // Replaying a completed receipt cannot cancel the next operation.
            reopened
                .cancel_native(operation.instance, operation.operation)
                .await
                .unwrap();
            assert!(
                !reopened
                    .status(false)
                    .await
                    .unwrap()
                    .native_operation
                    .unwrap()
                    .cancellation_requested
            );
            reopened
                .cancel_native(next_operation.instance, next_operation.operation)
                .await
                .unwrap();
            backend.cancelled.notified().await;
            backend.cleanup.add_permits(1);
            assert_eq!(
                next.await.unwrap().unwrap().native_text_error.as_deref(),
                Some("E_NATIVE_PROBE_CANCELLED")
            );
            assert_eq!(backend.calls.load(Ordering::SeqCst), 2);
        })
        .await
        .unwrap();
        stop.cancel();
        server.await.unwrap().unwrap();
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
