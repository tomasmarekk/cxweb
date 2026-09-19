//! Versioned private control commands. Operation receipts belong to one runtime
//! instance; stale desktop clients cannot replay mutations into a replacement.
use crate::{
    control::{Control, ControlStatus},
    lifecycle::{DisconnectController, DisconnectState},
};
use cxweb_platform::control_pipe::{self, ControlListener};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    future::Future,
    io,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

const VERSION: u32 = 1;
const RECEIPTS: usize = 256;
const EXCHANGE: Duration = Duration::from_secs(2);
pub type Work = Pin<Box<dyn Future<Output = Result<DisconnectState, &'static str>> + Send>>;

pub trait Lifecycle: Send + Sync + 'static {
    fn state(&self) -> DisconnectState;
    fn disconnect(&self) -> Work;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginAction {
    Background,
    Connect,
    Refresh,
    Qualify,
    QualifyText,
    QualifyTools,
}
pub type LoginWork = Pin<Box<dyn Future<Output = Result<ControlStatus, &'static str>> + Send>>;
pub trait LoginBackend: Send + Sync + 'static {
    fn request(&self, action: LoginAction) -> LoginWork;
}
impl LoginBackend for Control {
    fn request(&self, action: LoginAction) -> LoginWork {
        let control = self.clone();
        Box::pin(async move {
            match action {
                LoginAction::Background => control.background().await,
                LoginAction::Connect => control.connect().await,
                LoginAction::Refresh => control.status().await,
                LoginAction::Qualify => control.qualify().await,
                LoginAction::QualifyText => control.qualify_text().await,
                LoginAction::QualifyTools => control.qualify_tools().await,
            }
        })
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum OperationKind {
    Disconnect,
    Login(LoginAction),
}
impl Lifecycle for DisconnectController {
    fn state(&self) -> DisconnectState {
        *self.subscribe().borrow()
    }
    fn disconnect(&self) -> Work {
        let controller = self.clone();
        Box::pin(async move { controller.disconnect(Duration::from_secs(30)).await })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub command: Command,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Status {},
    BrowserStatus {},
    Browser {
        instance: String,
        operation: String,
        action: LoginAction,
    },
    Disconnect {
        instance: String,
        operation: String,
    },
    Operation {
        instance: String,
        operation: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reply {
    BrowserStatus {
        version: u32,
        instance: String,
        status: Box<ControlStatus>,
    },
    Status {
        version: u32,
        instance: String,
        state: DisconnectState,
    },
    Operation {
        operation: String,
        outcome: Outcome,
    },
    Error {
        code: ErrorCode,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
    Running {},
    Completed { result: DisconnectState },
    Failed {},
    LoginCompleted { status: Box<ControlStatus> },
    LoginFailed { code: String },
}

pub(crate) fn login_error(code: &str) -> &'static str {
    match code {
        "E_BROWSER_START" => "E_BROWSER_START",
        "E_BROWSER_LOGIN" => "E_BROWSER_LOGIN",
        "E_BROWSER_PIPE" => "E_BROWSER_PIPE",
        "E_HIDDEN_TARGET" => "E_HIDDEN_TARGET",
        "E_HIDDEN_ATTACH" => "E_HIDDEN_ATTACH",
        "E_HIDDEN_VIEWPORT" => "E_HIDDEN_VIEWPORT",
        "E_BACKGROUND_NAVIGATION" => "E_BACKGROUND_NAVIGATION",
        "E_BACKGROUND_WINDOW" => "E_BACKGROUND_WINDOW",
        "E_SUBMISSION_UNCERTAIN" => "E_SUBMISSION_UNCERTAIN",
        "E_QUALIFICATION_TIMEOUT" => "E_QUALIFICATION_TIMEOUT",
        "E_TEMPORARY_CHAT" => "E_TEMPORARY_CHAT",
        "E_SEND_SURFACE" => "E_SEND_SURFACE",
        "E_SEND_DISABLED" => "E_SEND_DISABLED",
        "E_COMPOSER_MISMATCH" => "E_COMPOSER_MISMATCH",
        "E_LIVE_QUALIFICATION" => "E_LIVE_QUALIFICATION",
        "E_TURN_AMBIGUOUS" => "E_TURN_AMBIGUOUS",
        "E_TURN_ATTRIBUTION" => "E_TURN_ATTRIBUTION",
        "E_USER_MESSAGE_MISMATCH" => "E_USER_MESSAGE_MISMATCH",
        "E_MODEL_FIDELITY" => "E_MODEL_FIDELITY",
        "E_INVALID_TOOL_ENVELOPE" => "E_INVALID_TOOL_ENVELOPE",
        "E_QUALIFICATION_SELECT" => "E_QUALIFICATION_SELECT",
        "E_QUALIFICATION_BASELINE" => "E_QUALIFICATION_BASELINE",
        "E_QUALIFICATION_INSERT" => "E_QUALIFICATION_INSERT",
        "E_QUALIFICATION_OBSERVE" => "E_QUALIFICATION_OBSERVE",
        "E_MODEL_SELECTION" => "E_MODEL_SELECTION",
        "E_MODEL_SELECT" => "E_MODEL_SELECT",
        "E_MODEL_LABEL" => "E_MODEL_LABEL",
        "E_BROWSER_BUSY" => "E_BROWSER_BUSY",
        "E_BROWSER_RELEASE" => "E_BROWSER_RELEASE",
        "E_LOGIN_REQUIRED" => "E_LOGIN_REQUIRED",
        "E_QUALIFICATION_PROTOCOL" => "E_QUALIFICATION_PROTOCOL",
        "E_MODEL_OPEN" => "E_MODEL_OPEN",
        "E_MODEL_READ" => "E_MODEL_READ",
        "E_MODEL_CLOSE" => "E_MODEL_CLOSE",
        "E_MODEL_PARSE" => "E_MODEL_PARSE",
        "E_MODEL_RESULT" => "E_MODEL_RESULT",
        "E_MODEL_RESTORE" => "E_MODEL_RESTORE",
        _ => "E_LOGIN_OPERATION",
    }
}

#[test]
fn login_failures_never_export_arbitrary_backend_text() {
    for code in [
        "E_TURN_AMBIGUOUS",
        "E_TURN_ATTRIBUTION",
        "E_USER_MESSAGE_MISMATCH",
        "E_MODEL_FIDELITY",
        "E_INVALID_TOOL_ENVELOPE",
    ] {
        assert_eq!(login_error(code), code);
    }
    assert_eq!(
        login_error("E_QUALIFICATION_PROTOCOL"),
        "E_QUALIFICATION_PROTOCOL"
    );
    assert_eq!(
        login_error("E_QUALIFICATION_INSERT"),
        "E_QUALIFICATION_INSERT"
    );
    assert_eq!(login_error("private account data"), "E_LOGIN_OPERATION");
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    Protocol,
    Version,
    Instance,
    OperationId,
    UnknownOperation,
    Busy,
    Capacity,
    Unsupported,
    OperationConflict,
}

#[derive(Clone)]
pub struct Service {
    instance: String,
    backend: Option<Arc<dyn Lifecycle>>,
    login: Option<Arc<dyn LoginBackend>>,
    login_status: Arc<Mutex<ControlStatus>>,
    receipts: Arc<Mutex<HashMap<String, (OperationKind, Outcome)>>>,
}
impl Service {
    pub fn new(backend: Arc<dyn Lifecycle>) -> Self {
        Self {
            instance: format!("{:032x}", rand::random::<u128>()),
            backend: Some(backend),
            login: None,
            login_status: Arc::default(),
            receipts: Arc::default(),
        }
    }
    pub fn login(backend: Arc<dyn LoginBackend>) -> Self {
        Self {
            instance: format!("{:032x}", rand::random::<u128>()),
            backend: None,
            login: Some(backend),
            login_status: Arc::default(),
            receipts: Arc::default(),
        }
    }
    fn handle(&self, bytes: &[u8]) -> Reply {
        let request = cxweb_codex_adapter::strict_json::parse(bytes, 64 * 1024)
            .ok()
            .and_then(|value| serde_json::from_value::<Request>(value).ok());
        let Some(request) = request else {
            return error(ErrorCode::Protocol);
        };
        if request.version != VERSION {
            return error(ErrorCode::Version);
        }
        let (instance, operation, start) = match request.command {
            Command::Status {} => {
                let Some(backend) = &self.backend else {
                    return error(ErrorCode::Unsupported);
                };
                return Reply::Status {
                    version: VERSION,
                    instance: self.instance.clone(),
                    state: backend.state(),
                };
            }
            Command::BrowserStatus {} => {
                if self.login.is_none() {
                    return error(ErrorCode::Unsupported);
                }
                return Reply::BrowserStatus {
                    version: VERSION,
                    instance: self.instance.clone(),
                    status: self
                        .login_status
                        .lock()
                        .expect("login status lock poisoned")
                        .clone()
                        .into(),
                };
            }
            Command::Browser {
                instance,
                operation,
                action,
            } => (instance, operation, Some(OperationKind::Login(action))),
            Command::Disconnect {
                instance,
                operation,
            } => (instance, operation, Some(OperationKind::Disconnect)),
            Command::Operation {
                instance,
                operation,
            } => (instance, operation, None),
        };
        if instance != self.instance {
            return error(ErrorCode::Instance);
        }
        if operation.len() != 32
            || !operation
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return error(ErrorCode::OperationId);
        }
        let mut receipts = self.receipts.lock().expect("control receipt lock poisoned");
        if let Some((kind, outcome)) = receipts.get(&operation) {
            if start.is_some_and(|requested| requested != *kind) {
                return error(ErrorCode::OperationConflict);
            }
            return Reply::Operation {
                operation,
                outcome: outcome.clone(),
            };
        }
        let Some(kind) = start else {
            return error(ErrorCode::UnknownOperation);
        };
        if (kind == OperationKind::Disconnect && self.backend.is_none())
            || (matches!(kind, OperationKind::Login(_)) && self.login.is_none())
        {
            return error(ErrorCode::Unsupported);
        }
        if receipts
            .values()
            .any(|(_, outcome)| *outcome == Outcome::Running {})
        {
            return error(ErrorCode::Busy);
        }
        // Never evict a receipt and accidentally re-execute its operation ID.
        if receipts.len() >= RECEIPTS {
            return error(ErrorCode::Capacity);
        }
        receipts.insert(operation.clone(), (kind, Outcome::Running {}));
        let backend = self.backend.clone();
        let login = self.login.clone();
        let status = self.login_status.clone();
        let completed = self.receipts.clone();
        let id = operation.clone();
        // Ownership transfers before replying. A disconnected/slow UI cannot
        // cancel a mutation, and a worker panic produces only a fixed error.
        tokio::spawn(async move {
            let worker = tokio::spawn(async move {
                match kind {
                    OperationKind::Disconnect => match backend
                        .expect("validated disconnect backend")
                        .disconnect()
                        .await
                    {
                        Ok(result) => Outcome::Completed { result },
                        Err(_) => Outcome::Failed {},
                    },
                    OperationKind::Login(action) => match login
                        .expect("validated login backend")
                        .request(action)
                        .await
                    {
                        Ok(result) => {
                            *status.lock().expect("login status lock poisoned") = result.clone();
                            Outcome::LoginCompleted {
                                status: Box::new(result),
                            }
                        }
                        Err(code) => {
                            let mut cached = status.lock().expect("login status lock poisoned");
                            cached.text_qualified_model = None;
                            cached.tool_qualified_model = None;
                            cached.qualification_evidence = None;
                            cached.qualification_diagnostic = None;
                            cached.phase = "awaiting_qualification".into();
                            Outcome::LoginFailed {
                                code: login_error(code).to_owned(),
                            }
                        }
                    },
                }
            });
            let outcome = worker.await.unwrap_or(Outcome::Failed {});
            completed
                .lock()
                .expect("control receipt lock poisoned")
                .insert(id, (kind, outcome));
        });
        Reply::Operation {
            operation,
            outcome: Outcome::Running {},
        }
    }

    /// Stopping control admission does not shut down the gateway or cancel an
    /// accepted lifecycle operation. The daemon owner retains those resources.
    pub async fn serve(
        &self,
        mut listener: ControlListener,
        stop: CancellationToken,
    ) -> io::Result<()> {
        loop {
            let mut pipe = tokio::select! {
                biased;
                _ = stop.cancelled() => return Ok(()),
                accepted = listener.accept() => accepted?,
            };
            // One exchange per connection, bounded in time and memory. A stalled
            // peer cannot occupy control admission indefinitely.
            let exchange = async {
                let bytes = control_pipe::read_frame(&mut pipe).await?;
                let reply = self.handle(&bytes);
                let bytes = serde_json::to_vec(&reply).map_err(io::Error::other)?;
                control_pipe::write_frame(&mut pipe, &bytes).await?;
                // Closing a Windows server handle can discard unread output.
                // Retain it until the client confirms receipt (or the bounded
                // exchange expires). This never controls operation lifetime.
                if control_pipe::read_frame(&mut pipe).await? != b"ack" {
                    return Err(io::Error::other("E_CONTROL_PROTOCOL"));
                }
                Ok::<(), io::Error>(())
            };
            tokio::select! {
                biased;
                _ = stop.cancelled() => return Ok(()),
                _ = tokio::time::timeout(EXCHANGE, exchange) => {},
            }
        }
    }
}
fn error(code: ErrorCode) -> Reply {
    Reply::Error { code }
}

/// A client timeout is an unknown outcome. Reuse the same operation ID (and
/// runtime instance) when querying/retrying; never synthesize a replacement ID.
pub async fn exchange(installation: &str, request: &Request) -> io::Result<Reply> {
    tokio::time::timeout(EXCHANGE, async {
        let mut pipe = loop {
            match control_pipe::connect(installation) {
                // Windows ERROR_PIPE_BUSY: the preceding client is finishing.
                Err(error) if error.raw_os_error() == Some(231) => {
                    tokio::time::sleep(Duration::from_millis(10)).await
                }
                result => break result?,
            }
        };
        let bytes = serde_json::to_vec(request).map_err(io::Error::other)?;
        control_pipe::write_frame(&mut pipe, &bytes).await?;
        let bytes = control_pipe::read_frame(&mut pipe).await?;
        control_pipe::write_frame(&mut pipe, b"ack").await?;
        let value = cxweb_codex_adapter::strict_json::parse(&bytes, 64 * 1024)
            .map_err(|_| io::Error::other("E_CONTROL_PROTOCOL"))?;
        let reply: Reply =
            serde_json::from_value(value).map_err(|_| io::Error::other("E_CONTROL_PROTOCOL"))?;
        let matches = match (&request.command, &reply) {
            (_, Reply::Error { .. }) => true,
            (Command::Status {}, Reply::Status { version, .. }) => *version == VERSION,
            (Command::BrowserStatus {}, Reply::BrowserStatus { version, .. }) => {
                *version == VERSION
            }
            (
                Command::Disconnect { operation, .. }
                | Command::Operation { operation, .. }
                | Command::Browser { operation, .. },
                Reply::Operation {
                    operation: received,
                    ..
                },
            ) => operation == received,
            _ => false,
        };
        if !matches {
            return Err(io::Error::other("E_CONTROL_PROTOCOL"));
        }
        Ok(reply)
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "E_CONTROL_TIMEOUT"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Semaphore;
    struct Backend {
        calls: AtomicUsize,
        release: Arc<Semaphore>,
    }
    impl Lifecycle for Backend {
        fn state(&self) -> DisconnectState {
            DisconnectState::Idle
        }
        fn disconnect(&self) -> Work {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let release = self.release.clone();
            Box::pin(async move {
                release.acquire().await.unwrap().forget();
                Ok(DisconnectState::PendingRestart)
            })
        }
    }
    fn fixture() -> (Service, Arc<Backend>) {
        let backend = Arc::new(Backend {
            calls: AtomicUsize::new(0),
            release: Arc::new(Semaphore::new(0)),
        });
        (Service::new(backend.clone()), backend)
    }
    fn request(command: Command) -> Request {
        Request {
            version: VERSION,
            command,
        }
    }
    fn dispatch(service: &Service, command: Command) -> Reply {
        service.handle(&serde_json::to_vec(&request(command)).unwrap())
    }
    #[tokio::test]
    async fn failed_text_test_receipt_is_replayed_without_resubmission() {
        struct Failing(AtomicUsize);
        impl LoginBackend for Failing {
            fn request(&self, action: LoginAction) -> LoginWork {
                assert_eq!(action, LoginAction::QualifyText);
                self.0.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Err("E_SUBMISSION_UNCERTAIN") })
            }
        }
        let backend = Arc::new(Failing(AtomicUsize::new(0)));
        let service = Service::login(backend.clone());
        service.login_status.lock().unwrap().text_qualified_model = Some("stale".into());
        let command = || Command::Browser {
            instance: service.instance.clone(),
            operation: "a".repeat(32),
            action: LoginAction::QualifyText,
        };
        assert!(matches!(
            dispatch(&service, command()),
            Reply::Operation {
                outcome: Outcome::Running {},
                ..
            }
        ));
        let terminal = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let result = dispatch(&service, command());
                if !matches!(
                    result,
                    Reply::Operation {
                        outcome: Outcome::Running {},
                        ..
                    }
                ) {
                    break result;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            terminal,
            Reply::Operation {
                operation: "a".repeat(32),
                outcome: Outcome::LoginFailed {
                    code: "E_SUBMISSION_UNCERTAIN".into()
                },
            }
        );
        assert_eq!(dispatch(&service, command()), terminal);
        assert_eq!(backend.0.load(Ordering::SeqCst), 1);
        assert!(
            service
                .login_status
                .lock()
                .unwrap()
                .text_qualified_model
                .is_none()
        );
    }
    #[tokio::test]
    async fn rejects_ambiguous_remote_and_stale_commands_without_mutation() {
        let (service, backend) = fixture();
        for bytes in [
            br#"{"version":1,"version":1,"command":{"type":"status"}}"#.as_slice(),
            br#"{"version":1,"command":{"type":"status","path":"secret"}}"#,
            br#"{"version":1,"command":{"type":"evaluate","script":"secret"}}"#,
            br#"{"version":1,"command":{"type":"shutdown"}}"#,
        ] {
            assert_eq!(service.handle(bytes), error(ErrorCode::Protocol));
        }
        assert_eq!(
            service.handle(br#"{"version":2,"command":{"type":"status"}}"#),
            error(ErrorCode::Version)
        );
        assert_eq!(
            dispatch(
                &service,
                Command::Disconnect {
                    instance: "stale".into(),
                    operation: "a".repeat(32)
                }
            ),
            error(ErrorCode::Instance)
        );
        assert_eq!(
            dispatch(
                &service,
                Command::Disconnect {
                    instance: service.instance.clone(),
                    operation: "bad".into()
                }
            ),
            error(ErrorCode::OperationId)
        );
        assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn lost_reply_retries_execute_once_and_receipts_are_not_evicted() {
        let (service, backend) = fixture();
        let command = || Command::Disconnect {
            instance: service.instance.clone(),
            operation: "a".repeat(32),
        };
        let first = dispatch(&service, command());
        assert_eq!(dispatch(&service, command()), first);
        assert_eq!(
            dispatch(
                &service,
                Command::Browser {
                    instance: service.instance.clone(),
                    operation: "a".repeat(32),
                    action: LoginAction::Refresh,
                }
            ),
            error(ErrorCode::OperationConflict)
        );
        assert_eq!(
            dispatch(&service, Command::BrowserStatus {}),
            error(ErrorCode::Unsupported)
        );
        assert_eq!(
            dispatch(
                &service,
                Command::Disconnect {
                    instance: service.instance.clone(),
                    operation: "b".repeat(32)
                }
            ),
            error(ErrorCode::Busy)
        );
        backend.release.add_permits(1);
        tokio::time::timeout(EXCHANGE, async {
            loop {
                if dispatch(&service, command())
                    == (Reply::Operation {
                        operation: "a".repeat(32),
                        outcome: Outcome::Completed {
                            result: DisconnectState::PendingRestart,
                        },
                    })
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
        for n in 0..RECEIPTS {
            service.receipts.lock().unwrap().insert(
                format!("{n:032x}"),
                (OperationKind::Disconnect, Outcome::Failed {}),
            );
        }
        assert_eq!(
            dispatch(
                &service,
                Command::Disconnect {
                    instance: service.instance.clone(),
                    operation: "b".repeat(32)
                }
            ),
            error(ErrorCode::Capacity)
        );
        assert!(matches!(
            dispatch(&service, command()),
            Reply::Operation {
                outcome: Outcome::Completed { .. },
                ..
            }
        ));
    }

    #[tokio::test]
    async fn actual_pipe_recovers_after_stalled_client_and_keeps_accepted_work() {
        let (service, backend) = fixture();
        let installation = format!("{:032x}", rand::random::<u128>());
        let listener = control_pipe::listen(&installation).unwrap();
        let stop = CancellationToken::new();
        let server = tokio::spawn({
            let service = service.clone();
            let stop = stop.clone();
            async move { service.serve(listener, stop).await }
        });
        let mut stalled = control_pipe::connect(&installation).unwrap();
        use tokio::io::AsyncWriteExt;
        stalled.write_all(&[1, 0]).await.unwrap();
        tokio::time::sleep(EXCHANGE + Duration::from_millis(100)).await;
        drop(stalled);
        let status = exchange(&installation, &request(Command::Status {}))
            .await
            .unwrap();
        assert_eq!(
            status,
            Reply::Status {
                version: VERSION,
                instance: service.instance.clone(),
                state: DisconnectState::Idle
            }
        );
        let mut slow_reader = control_pipe::connect(&installation).unwrap();
        control_pipe::write_frame(
            &mut slow_reader,
            &serde_json::to_vec(&request(Command::Status {})).unwrap(),
        )
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let buffered = control_pipe::read_frame(&mut slow_reader).await.unwrap();
        assert_eq!(serde_json::from_slice::<Reply>(&buffered).unwrap(), status);
        control_pipe::write_frame(&mut slow_reader, b"ack")
            .await
            .unwrap();
        drop(slow_reader);
        let op = "c".repeat(32);
        let accepted = exchange(
            &installation,
            &request(Command::Disconnect {
                instance: service.instance.clone(),
                operation: op.clone(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(
            accepted,
            Reply::Operation {
                operation: op.clone(),
                outcome: Outcome::Running {}
            }
        );
        stop.cancel();
        server.await.unwrap().unwrap();
        backend.release.add_permits(1);
        tokio::time::timeout(EXCHANGE, async {
            loop {
                if matches!(
                    dispatch(
                        &service,
                        Command::Operation {
                            instance: service.instance.clone(),
                            operation: op.clone()
                        }
                    ),
                    Reply::Operation {
                        outcome: Outcome::Completed { .. },
                        ..
                    }
                ) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn client_rejects_wrong_operation_reply() {
        let installation = format!("{:032x}", rand::random::<u128>());
        let mut listener = control_pipe::listen(&installation).unwrap();
        let server = tokio::spawn(async move {
            let mut pipe = listener.accept().await.unwrap();
            control_pipe::read_frame(&mut pipe).await.unwrap();
            let reply = Reply::Operation {
                operation: "b".repeat(32),
                outcome: Outcome::Running {},
            };
            control_pipe::write_frame(&mut pipe, &serde_json::to_vec(&reply).unwrap())
                .await
                .unwrap();
            // Keep the server handle alive until the client has consumed its
            // response: dropping a Windows pipe may discard unread data.
            let _ = control_pipe::read_frame(&mut pipe).await;
        });
        let result = exchange(
            &installation,
            &request(Command::Disconnect {
                instance: "a".repeat(32),
                operation: "a".repeat(32),
            }),
        )
        .await;
        assert_eq!(result.unwrap_err().to_string(), "E_CONTROL_PROTOCOL");
        server.await.unwrap();
    }
}
