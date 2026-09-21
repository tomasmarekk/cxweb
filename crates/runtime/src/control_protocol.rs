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
    fn web_login(&self, _finish: bool) -> Work {
        Box::pin(async { Err("E_WEB_RECOVERY_STATE") })
    }
    fn qualify_protocol(&self, _target: crate::protocol_qualification::Target) -> Work {
        Box::pin(async { Err("E_QUALITY_UNSUPPORTED") })
    }
    fn verify_compaction(&self, _target: Option<crate::native_probe::CheckpointTarget>) -> Work {
        Box::pin(async { Err("E_COMPACTION_UNQUALIFIED") })
    }
    fn reasoning_status(&self) -> Option<Vec<ReasoningFamily>> {
        None
    }
    fn qualify_reasoning(&self) -> Work {
        Box::pin(async { Err("E_REASONING_UNSUPPORTED") })
    }
    fn retry_web(&self) -> Work {
        Box::pin(async { Err("E_WEB_RECOVERY_NOT_RETRYABLE") })
    }
    fn state(&self) -> DisconnectState;
    fn health(&self) -> cxweb_domain::health::Health {
        cxweb_domain::health::Health::default()
    }
    fn checked_health(
        &self,
    ) -> Pin<Box<dyn Future<Output = cxweb_domain::health::Health> + Send + '_>> {
        Box::pin(async move { self.health() })
    }
    fn disconnect(&self) -> Work;
    fn disconnect_when_idle(&self) -> Work {
        Box::pin(async { Err("E_IDLE_DISCONNECT_UNSUPPORTED") })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginAction {
    ResetTest,
    Background,
    Connect,
    Refresh,
    Qualify,
    QualifyText,
    QualifyTools,
}
pub type LoginWork = Pin<Box<dyn Future<Output = Result<ControlStatus, &'static str>> + Send>>;
pub trait LoginBackend: Send + Sync + 'static {
    fn activate(&self, _target: crate::setup_owner::ActivationTarget) -> LoginWork {
        Box::pin(async { Err("E_ACTIVATION_UNSUPPORTED") })
    }
    fn request(&self, action: LoginAction) -> LoginWork;
    fn native_text(
        &self,
        _target: crate::setup_owner::NativeTarget,
        _cancellation: CancellationToken,
    ) -> LoginWork {
        Box::pin(async { Err("E_NATIVE_TEST_FAILED") })
    }
}
impl LoginBackend for Control {
    fn request(&self, action: LoginAction) -> LoginWork {
        let control = self.clone();
        Box::pin(async move {
            match action {
                LoginAction::ResetTest => control.reset_test().await,
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
#[derive(Clone, PartialEq, Eq)]
enum OperationKind {
    WebLogin(bool),
    QualifyProtocol(crate::protocol_qualification::Target),
    VerifyCompaction(Option<crate::native_probe::CheckpointTarget>),
    QualifyReasoning,
    Activate(crate::setup_owner::ActivationTarget),
    RetryWeb,
    Disconnect,
    DisconnectWhenIdle,
    Login(LoginAction),
    NativeText(crate::setup_owner::NativeTarget),
}
impl Lifecycle for DisconnectController {
    fn web_login(&self, finish: bool) -> Work {
        let controller = self.clone();
        Box::pin(async move { controller.web_login(finish).await })
    }
    fn qualify_protocol(&self, target: crate::protocol_qualification::Target) -> Work {
        let controller = self.clone();
        Box::pin(async move { controller.qualify_protocol(target).await })
    }
    fn verify_compaction(&self, target: Option<crate::native_probe::CheckpointTarget>) -> Work {
        let controller = self.clone();
        Box::pin(async move { controller.verify_compaction(target).await })
    }
    fn reasoning_status(&self) -> Option<Vec<ReasoningFamily>> {
        self.reasoning_status()
    }
    fn qualify_reasoning(&self) -> Work {
        let controller = self.clone();
        Box::pin(async move { controller.qualify_reasoning().await })
    }
    fn retry_web(&self) -> Work {
        let controller = self.clone();
        Box::pin(async move { controller.retry_web().await })
    }
    fn disconnect_when_idle(&self) -> Work {
        let controller = self.clone();
        Box::pin(async move {
            controller
                .disconnect_when_idle(Duration::from_secs(30))
                .await
        })
    }
    fn health(&self) -> cxweb_domain::health::Health {
        DisconnectController::health(self)
    }
    fn checked_health(
        &self,
    ) -> Pin<Box<dyn Future<Output = cxweb_domain::health::Health> + Send + '_>> {
        Box::pin(DisconnectController::checked_health(self))
    }
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
    WebLogin {
        instance: String,
        operation: String,
        finish: bool,
    },
    QualifyProtocol {
        instance: String,
        operation: String,
        target: crate::protocol_qualification::Target,
    },
    VerifyCompaction {
        instance: String,
        operation: String,
        #[serde(default)]
        target: Option<crate::native_probe::CheckpointTarget>,
    },
    ReasoningStatus {},
    QualifyReasoning {
        instance: String,
        operation: String,
    },
    Activate {
        instance: String,
        operation: String,
        target: crate::setup_owner::ActivationTarget,
    },
    RetryWeb {
        instance: String,
        operation: String,
    },
    Status {},
    Health {},
    BrowserStatus {},
    Browser {
        instance: String,
        operation: String,
        action: LoginAction,
    },
    CancelNative {
        instance: String,
        operation: String,
    },
    NativeText {
        instance: String,
        operation: String,
        target: crate::setup_owner::NativeTarget,
    },
    Disconnect {
        instance: String,
        operation: String,
    },
    DisconnectWhenIdle {
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
    ReasoningStatus {
        version: u32,
        instance: String,
        families: Option<Vec<ReasoningFamily>>,
    },
    Health {
        version: u32,
        instance: String,
        health: Box<cxweb_domain::health::Health>,
    },
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

/// Public picker metadata only. Never includes browser identities or account scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReasoningFamily {
    pub model: String,
    pub name: String,
    pub levels: Vec<cxweb_codex_adapter::catalog_codec::ReasoningLevel>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Outcome {
    ReasoningFailed { code: ReasoningFailure },
    Running {},
    ActiveWork {},
    Completed { result: DisconnectState },
    Failed {},
    LoginCompleted { status: Box<ControlStatus> },
    LoginFailed { code: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningFailure {
    RateLimited,
    Discovery,
    Protocol,
    Timeout,
    Scope,
    Cleanup,
    Cancelled,
    Publication,
    Unavailable,
}
impl ReasoningFailure {
    fn from_code(code: &str) -> Self {
        match code {
            "E_BROWSER_RATE_LIMITED" => Self::RateLimited,
            "E_REASONING_DISCOVERY" | "E_MODEL_UNAVAILABLE" | "E_MODEL_SELECTION" => {
                Self::Discovery
            }
            "E_QUALIFICATION_PROTOCOL" => Self::Protocol,
            "E_QUALIFICATION_TIMEOUT" => Self::Timeout,
            "E_SESSION_SCOPE" => Self::Scope,
            "E_WEB_CLEANUP_UNCONFIRMED" => Self::Cleanup,
            "E_CANCELLED" => Self::Cancelled,
            "E_WEB_RECOVERY_RECEIPT" => Self::Publication,
            _ => Self::Unavailable,
        }
    }
    pub fn code(&self) -> &'static str {
        match self {
            Self::RateLimited => "E_BROWSER_RATE_LIMITED",
            Self::Discovery => "E_REASONING_DISCOVERY",
            Self::Protocol => "E_QUALIFICATION_PROTOCOL",
            Self::Timeout => "E_QUALIFICATION_TIMEOUT",
            Self::Scope => "E_SESSION_SCOPE",
            Self::Cleanup => "E_WEB_CLEANUP_UNCONFIRMED",
            Self::Cancelled => "E_CANCELLED",
            Self::Publication => "E_WEB_RECOVERY_RECEIPT",
            Self::Unavailable => "E_REASONING_QUALIFICATION",
        }
    }
}

pub(crate) fn login_error(code: &str) -> &'static str {
    match code {
        "E_BROWSER_RATE_LIMITED" => "E_BROWSER_RATE_LIMITED",
        "E_ALREADY_RUNNING" => "E_ALREADY_RUNNING",
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
        "E_SESSION_SCOPE" => "E_SESSION_SCOPE",
        "E_SEND_SURFACE" => "E_SEND_SURFACE",
        "E_SEND_DISABLED" => "E_SEND_DISABLED",
        "E_COMPOSER_MISMATCH" => "E_COMPOSER_MISMATCH",
        "E_LIVE_QUALIFICATION" => "E_LIVE_QUALIFICATION",
        "E_TURN_AMBIGUOUS" => "E_TURN_AMBIGUOUS",
        "E_TURN_ATTRIBUTION" => "E_TURN_ATTRIBUTION",
        "E_USER_MESSAGE_MISMATCH" => "E_USER_MESSAGE_MISMATCH",
        "E_MODEL_FIDELITY" => "E_MODEL_FIDELITY",
        "E_INVALID_TOOL_ENVELOPE" => "E_INVALID_TOOL_ENVELOPE",
        "E_TOOL_ENVELOPE_FENCED" => "E_TOOL_ENVELOPE_FENCED",
        "E_QUALIFICATION_SELECT" => "E_QUALIFICATION_SELECT",
        "E_QUALIFICATION_BASELINE" => "E_QUALIFICATION_BASELINE",
        "E_QUALIFICATION_INSERT" => "E_QUALIFICATION_INSERT",
        "E_QUALIFICATION_OBSERVE" => "E_QUALIFICATION_OBSERVE",
        "E_MODEL_SELECTION" => "E_MODEL_SELECTION",
        "E_MODEL_SELECT" => "E_MODEL_SELECT",
        "E_MODEL_LABEL" => "E_MODEL_LABEL",
        "E_BROWSER_BUSY" => "E_BROWSER_BUSY",
        "E_BROWSER_OTHER_PAGES" => "E_BROWSER_OTHER_PAGES",
        "E_BROWSER_IN_USE" => "E_BROWSER_IN_USE",
        "E_BROWSER_RELEASE" => "E_BROWSER_RELEASE",
        "E_LOGIN_REQUIRED" => "E_LOGIN_REQUIRED",
        "E_QUALIFICATION_PROTOCOL" => "E_QUALIFICATION_PROTOCOL",
        "E_MODEL_OPEN" => "E_MODEL_OPEN",
        "E_MODEL_READ" => "E_MODEL_READ",
        "E_MODEL_CLOSE" => "E_MODEL_CLOSE",
        "E_MODEL_PARSE" => "E_MODEL_PARSE",
        "E_MODEL_RESULT" => "E_MODEL_RESULT",
        "E_MODEL_RESTORE" => "E_MODEL_RESTORE",
        "E_MODEL_FAMILY" => "E_MODEL_FAMILY",
        "E_MODEL_DISCOVERY" => "E_MODEL_DISCOVERY",
        "E_CONTROL_TIMEOUT" => "E_CONTROL_TIMEOUT",
        "E_CONTROL_CLOSED" => "E_CONTROL_CLOSED",
        "E_CONTROL_BUSY" => "E_CONTROL_BUSY",
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
        "E_TOOL_ENVELOPE_FENCED",
        "E_BROWSER_OTHER_PAGES",
        "E_BROWSER_IN_USE",
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
    native_cancellations: Arc<Mutex<HashMap<String, CancellationToken>>>,
}
impl Service {
    pub fn new(backend: Arc<dyn Lifecycle>) -> Self {
        Self {
            instance: format!("{:032x}", rand::random::<u128>()),
            backend: Some(backend),
            login: None,
            login_status: Arc::default(),
            receipts: Arc::default(),
            native_cancellations: Arc::default(),
        }
    }
    pub fn login(backend: Arc<dyn LoginBackend>) -> Self {
        Self {
            instance: format!("{:032x}", rand::random::<u128>()),
            backend: None,
            login: Some(backend),
            login_status: Arc::default(),
            receipts: Arc::default(),
            native_cancellations: Arc::default(),
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
        let cancel_native = matches!(&request.command, Command::CancelNative { .. });
        let (instance, operation, start) = match request.command {
            Command::QualifyProtocol {
                instance,
                operation,
                target,
            } => (
                instance,
                operation,
                Some(OperationKind::QualifyProtocol(target)),
            ),
            Command::ReasoningStatus {} => {
                let Some(backend) = &self.backend else {
                    return error(ErrorCode::Unsupported);
                };
                return Reply::ReasoningStatus {
                    version: VERSION,
                    instance: self.instance.clone(),
                    families: backend.reasoning_status(),
                };
            }
            Command::VerifyCompaction {
                instance,
                operation,
                target,
            } => (
                instance,
                operation,
                Some(OperationKind::VerifyCompaction(target)),
            ),
            Command::QualifyReasoning {
                instance,
                operation,
            } => (instance, operation, Some(OperationKind::QualifyReasoning)),
            Command::WebLogin {
                instance,
                operation,
                finish,
            } => (instance, operation, Some(OperationKind::WebLogin(finish))),
            Command::RetryWeb {
                instance,
                operation,
            } => (instance, operation, Some(OperationKind::RetryWeb)),
            Command::Health {} => {
                let Some(backend) = &self.backend else {
                    return error(ErrorCode::Unsupported);
                };
                return Reply::Health {
                    version: VERSION,
                    instance: self.instance.clone(),
                    health: Box::new(backend.health()),
                };
            }
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
                let receipts = self.receipts.lock().expect("control receipt lock poisoned");
                let mut status = self
                    .login_status
                    .lock()
                    .expect("login status lock poisoned")
                    .clone();
                status.native_operation = receipts.iter().find_map(|(id, (kind, outcome))| {
                    (matches!(kind, OperationKind::NativeText(_))
                        && *outcome == Outcome::Running {})
                    .then(|| crate::control::NativeOperation {
                        instance: self.instance.clone(),
                        operation: id.clone(),
                        cancellation_requested: self
                            .native_cancellations
                            .lock()
                            .expect("cancellation lock poisoned")
                            .get(id)
                            .is_some_and(CancellationToken::is_cancelled),
                    })
                });
                return Reply::BrowserStatus {
                    version: VERSION,
                    instance: self.instance.clone(),
                    status: Box::new(status),
                };
            }
            Command::CancelNative {
                instance,
                operation,
            } => (instance, operation, None),
            Command::Browser {
                instance,
                operation,
                action,
            } => (instance, operation, Some(OperationKind::Login(action))),
            Command::NativeText {
                instance,
                operation,
                target,
            } => (instance, operation, Some(OperationKind::NativeText(target))),
            Command::Activate {
                instance,
                operation,
                target,
            } => (instance, operation, Some(OperationKind::Activate(target))),
            Command::Disconnect {
                instance,
                operation,
            } => (instance, operation, Some(OperationKind::Disconnect)),
            Command::DisconnectWhenIdle {
                instance,
                operation,
            } => (instance, operation, Some(OperationKind::DisconnectWhenIdle)),
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
            if cancel_native {
                if !matches!(kind, OperationKind::NativeText(_)) {
                    return error(ErrorCode::OperationConflict);
                }
                if let Some(cancel) = self
                    .native_cancellations
                    .lock()
                    .expect("cancellation lock poisoned")
                    .get(&operation)
                {
                    cancel.cancel();
                }
            }
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
        if (matches!(
            kind,
            OperationKind::Disconnect
                | OperationKind::DisconnectWhenIdle
                | OperationKind::RetryWeb
                | OperationKind::WebLogin(_)
                | OperationKind::QualifyReasoning
                | OperationKind::QualifyProtocol(_)
                | OperationKind::VerifyCompaction(_)
        ) && self.backend.is_none())
            || (matches!(
                &kind,
                OperationKind::Login(_) | OperationKind::NativeText(_) | OperationKind::Activate(_)
            ) && self.login.is_none())
        {
            return error(ErrorCode::Unsupported);
        }
        if receipts.values().any(|(running, outcome)| {
            *outcome == Outcome::Running {}
                && !(kind == OperationKind::Disconnect
                    && matches!(
                        running,
                        OperationKind::QualifyReasoning
                            | OperationKind::QualifyProtocol(_)
                            | OperationKind::VerifyCompaction(_)
                    ))
        }) {
            return error(ErrorCode::Busy);
        }
        // Never evict a receipt and accidentally re-execute its operation ID.
        if receipts.len() >= RECEIPTS {
            return error(ErrorCode::Capacity);
        }
        receipts.insert(operation.clone(), (kind.clone(), Outcome::Running {}));
        let cancellation = CancellationToken::new();
        if matches!(&kind, OperationKind::NativeText(_)) {
            let mut cached = self
                .login_status
                .lock()
                .expect("login status lock poisoned");
            cached.native_text_report = None;
            cached.native_text_error = None;
            self.native_cancellations
                .lock()
                .expect("cancellation lock poisoned")
                .insert(operation.clone(), cancellation.clone());
        }
        let cancellations = self.native_cancellations.clone();
        let backend = self.backend.clone();
        let login = self.login.clone();
        let status = self.login_status.clone();
        let completed = self.receipts.clone();
        let id = operation.clone();
        // Ownership transfers before replying. A disconnected/slow UI cannot
        // cancel a mutation, and a worker panic produces only a fixed error.
        tokio::spawn(async move {
            let work_kind = kind.clone();
            let worker = tokio::spawn(async move {
                match work_kind {
                    OperationKind::QualifyProtocol(target) => {
                        match backend
                            .expect("validated lifecycle backend")
                            .qualify_protocol(target)
                            .await
                        {
                            Ok(result) => Outcome::Completed { result },
                            Err("E_WEB_ACTIVE") => Outcome::ActiveWork {},
                            Err(_) => Outcome::Failed {},
                        }
                    }
                    OperationKind::VerifyCompaction(target) => {
                        match backend
                            .expect("validated lifecycle backend")
                            .verify_compaction(target)
                            .await
                        {
                            Ok(result) => Outcome::Completed { result },
                            Err("E_WEB_ACTIVE") => Outcome::ActiveWork {},
                            Err(_) => Outcome::Failed {},
                        }
                    }
                    OperationKind::QualifyReasoning => {
                        match backend
                            .expect("validated lifecycle backend")
                            .qualify_reasoning()
                            .await
                        {
                            Ok(result) => Outcome::Completed { result },
                            Err("E_WEB_ACTIVE") => Outcome::ActiveWork {},
                            Err(code) => Outcome::ReasoningFailed {
                                code: ReasoningFailure::from_code(code),
                            },
                        }
                    }
                    OperationKind::WebLogin(finish) => {
                        match backend
                            .expect("validated lifecycle backend")
                            .web_login(finish)
                            .await
                        {
                            Ok(result) => Outcome::Completed { result },
                            Err("E_WEB_ACTIVE") => Outcome::ActiveWork {},
                            Err(_) => Outcome::Failed {},
                        }
                    }
                    OperationKind::RetryWeb => {
                        match backend
                            .expect("validated lifecycle backend")
                            .retry_web()
                            .await
                        {
                            Ok(result) => Outcome::Completed { result },
                            Err(_) => Outcome::Failed {},
                        }
                    }
                    action @ (OperationKind::Disconnect | OperationKind::DisconnectWhenIdle) => {
                        let backend = backend.expect("validated disconnect backend");
                        let work = if action == OperationKind::DisconnectWhenIdle {
                            backend.disconnect_when_idle()
                        } else {
                            backend.disconnect()
                        };
                        match work.await {
                            Ok(result) => Outcome::Completed { result },
                            Err("E_WEB_ACTIVE") => Outcome::ActiveWork {},
                            Err(_) => Outcome::Failed {},
                        }
                    }
                    action @ (OperationKind::Login(_)
                    | OperationKind::NativeText(_)
                    | OperationKind::Activate(_)) => {
                        let backend = login.expect("validated login backend");
                        let reset_test =
                            matches!(&action, OperationKind::Login(LoginAction::ResetTest));
                        let result = match action {
                            OperationKind::Login(action) => backend.request(action),
                            OperationKind::NativeText(target) => {
                                backend.native_text(target, cancellation)
                            }
                            OperationKind::Activate(target) => backend.activate(target),
                            OperationKind::Disconnect
                            | OperationKind::DisconnectWhenIdle
                            | OperationKind::RetryWeb
                            | OperationKind::WebLogin(_)
                            | OperationKind::QualifyReasoning
                            | OperationKind::QualifyProtocol(_)
                            | OperationKind::VerifyCompaction(_) => {
                                unreachable!()
                            }
                        }
                        .await;
                        match result {
                            Ok(result) => {
                                *status.lock().expect("login status lock poisoned") =
                                    result.clone();
                                Outcome::LoginCompleted {
                                    status: Box::new(result),
                                }
                            }
                            Err(code) => {
                                if reset_test
                                    && matches!(
                                        code,
                                        "E_BROWSER_IN_USE"
                                            | "E_BROWSER_BUSY"
                                            | "E_BROWSER_OTHER_PAGES"
                                            | "E_BROWSER_RELEASE"
                                    )
                                {
                                    // A refused retirement retains the original owner
                                    // and its qualification. No new test was sent.
                                    return Outcome::LoginFailed {
                                        code: login_error(code).to_owned(),
                                    };
                                }
                                let mut cached = status.lock().expect("login status lock poisoned");
                                cached.native_text_report = None;
                                cached.native_text_error = None;
                                cached.text_qualified_model = None;
                                cached.tool_qualified_model = None;
                                cached.qualification_evidence = None;
                                cached.qualification_diagnostic = None;
                                cached.phase = "awaiting_qualification".into();
                                Outcome::LoginFailed {
                                    code: login_error(code).to_owned(),
                                }
                            }
                        }
                    }
                }
            });
            let outcome = worker.await.unwrap_or(Outcome::Failed {});
            let mut completed = completed.lock().expect("control receipt lock poisoned");
            completed.insert(id.clone(), (kind, outcome));
            cancellations
                .lock()
                .expect("cancellation lock poisoned")
                .remove(&id);
        });
        Reply::Operation {
            operation,
            outcome: Outcome::Running {},
        }
    }

    async fn handle_checked(&self, bytes: &[u8]) -> Reply {
        // The normal decoder validates version, shape and backend availability
        // before the filesystem observation can run.
        let mut reply = self.handle(bytes);
        if let Reply::Health { health, .. } = &mut reply
            && let Some(backend) = &self.backend
        {
            **health = backend.checked_health().await;
        }
        reply
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
                let reply = self.handle_checked(&bytes).await;
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
            (
                Command::Health {},
                Reply::Health {
                    version, health, ..
                },
            ) => *version == VERSION && health.schema_version == "webbridge.health.v1",
            (Command::BrowserStatus {}, Reply::BrowserStatus { version, .. }) => {
                *version == VERSION
            }
            (Command::ReasoningStatus {}, Reply::ReasoningStatus { version, .. }) => {
                *version == VERSION
            }
            (
                Command::Disconnect { operation, .. }
                | Command::DisconnectWhenIdle { operation, .. }
                | Command::RetryWeb { operation, .. }
                | Command::WebLogin { operation, .. }
                | Command::QualifyReasoning { operation, .. }
                | Command::QualifyProtocol { operation, .. }
                | Command::VerifyCompaction { operation, .. }
                | Command::Operation { operation, .. }
                | Command::Browser { operation, .. }
                | Command::NativeText { operation, .. }
                | Command::Activate { operation, .. }
                | Command::CancelNative { operation, .. },
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

    #[tokio::test]
    async fn activation_receipt_is_bound_to_target_and_survives_a_lost_waiter() {
        struct Activator(std::sync::atomic::AtomicUsize);
        impl LoginBackend for Activator {
            fn request(&self, _: LoginAction) -> LoginWork {
                panic!("no browser action expected")
            }
            fn activate(&self, _: crate::setup_owner::ActivationTarget) -> LoginWork {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Box::pin(async {
                    Ok(ControlStatus {
                        routing_installed: true,
                        installation: Some("b".repeat(32)),
                        ..Default::default()
                    })
                })
            }
        }
        let backend = Arc::new(Activator(std::sync::atomic::AtomicUsize::new(0)));
        let service = Service::login(backend.clone());
        let target = crate::setup_owner::ActivationTarget {
            client: "C:\\fixture\\codex.exe".into(),
            home: "C:\\fixture\\home".into(),
            cwd: "C:\\fixture\\work".into(),
            route: "webbridge/fixture".into(),
        };
        let command = || Command::Activate {
            instance: service.instance.clone(),
            operation: "a".repeat(32),
            target: target.clone(),
        };
        let first = dispatch(&service, command());
        assert_eq!(dispatch(&service, command()), first);
        let mut changed = target.clone();
        changed.home = "C:\\other".into();
        assert_eq!(
            dispatch(
                &service,
                Command::Activate {
                    instance: service.instance.clone(),
                    operation: "a".repeat(32),
                    target: changed
                }
            ),
            error(ErrorCode::OperationConflict)
        );
        let terminal = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let reply = dispatch(&service, command());
                if let Reply::Operation {
                    outcome: Outcome::LoginCompleted { .. },
                    ..
                } = reply
                {
                    break reply;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(dispatch(&service, command()), terminal);
        assert_eq!(backend.0.load(std::sync::atomic::Ordering::SeqCst), 1);
        let Reply::BrowserStatus { status, .. } = dispatch(&service, Command::BrowserStatus {})
        else {
            panic!("status required")
        };
        assert!(status.routing_installed);
        assert_eq!(
            status.installation.as_deref(),
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
        );
    }
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Semaphore;
    struct Backend {
        calls: AtomicUsize,
        release: Arc<Semaphore>,
    }
    impl Lifecycle for Backend {
        fn web_login(&self, _finish: bool) -> Work {
            self.retry_web()
        }
        fn qualify_protocol(&self, _: crate::protocol_qualification::Target) -> Work {
            self.retry_web()
        }
        fn verify_compaction(
            &self,
            _target: Option<crate::native_probe::CheckpointTarget>,
        ) -> Work {
            self.retry_web()
        }
        fn reasoning_status(&self) -> Option<Vec<ReasoningFamily>> {
            Some(vec![ReasoningFamily {
                model: "webbridge/fixture".into(),
                name: "ChatGPT Web · Latest".into(),
                levels: vec![cxweb_codex_adapter::catalog_codec::ReasoningLevel {
                    effort: "low".into(),
                    description: "Instant".into(),
                }],
            }])
        }
        fn qualify_reasoning(&self) -> Work {
            self.retry_web()
        }
        fn retry_web(&self) -> Work {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let release = self.release.clone();
            Box::pin(async move {
                release.acquire().await.unwrap().forget();
                Ok(DisconnectState::Idle)
            })
        }
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
    async fn checked_dispatch_refreshes_only_valid_health_requests() {
        struct Observer(AtomicUsize);
        impl Lifecycle for Observer {
            fn state(&self) -> DisconnectState {
                DisconnectState::Idle
            }
            fn disconnect(&self) -> Work {
                Box::pin(async { Err("not used") })
            }
            fn checked_health(
                &self,
            ) -> Pin<Box<dyn Future<Output = cxweb_domain::health::Health> + Send + '_>>
            {
                Box::pin(async move {
                    self.0.fetch_add(1, Ordering::SeqCst);
                    let mut health = cxweb_domain::health::Health::default();
                    health.components.config.state = cxweb_domain::health::ComponentState::Healthy;
                    health
                })
            }
        }
        let backend = Arc::new(Observer(AtomicUsize::new(0)));
        let service = Service::new(backend.clone());
        for bytes in [
            br#"{"version":2,"command":{"type":"health"}}"#.as_slice(),
            br#"{"version":1,"command":{"type":"health","extra":true}}"#.as_slice(),
            br#"{"version":1,"command":{"type":"status"}}"#.as_slice(),
        ] {
            service.handle_checked(bytes).await;
        }
        assert_eq!(backend.0.load(Ordering::SeqCst), 0);
        let reply = service
            .handle_checked(&serde_json::to_vec(&request(Command::Health {})).unwrap())
            .await;
        let Reply::Health { health, .. } = reply else {
            panic!("expected checked health")
        };
        assert_eq!(
            health.components.config.state,
            cxweb_domain::health::ComponentState::Healthy
        );
        assert_eq!(backend.0.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn protocol_receipt_preserves_run_and_effort_and_deduplicates_lost_acknowledgements() {
        let (service, backend) = fixture();
        let target = crate::protocol_qualification::Target {
            run: "frozen".into(),
            effort: "xhigh".into(),
        };
        let command = |instance: String, operation: String, target| Command::QualifyProtocol {
            instance,
            operation,
            target,
        };
        let operation = "a".repeat(32);
        assert_eq!(
            dispatch(
                &service,
                command("stale".into(), operation.clone(), target.clone())
            ),
            error(ErrorCode::Instance)
        );
        for _ in 0..2 {
            assert!(matches!(
                dispatch(
                    &service,
                    command(service.instance.clone(), operation.clone(), target.clone())
                ),
                Reply::Operation {
                    outcome: Outcome::Running {},
                    ..
                }
            ));
        }
        let mut changed = target.clone();
        changed.effort = "high".into();
        assert_eq!(
            dispatch(
                &service,
                command(service.instance.clone(), operation.clone(), changed)
            ),
            error(ErrorCode::OperationConflict)
        );
        assert_eq!(
            dispatch(
                &service,
                command(service.instance.clone(), "b".repeat(32), target.clone())
            ),
            error(ErrorCode::Busy)
        );
        assert!(matches!(
            dispatch(
                &service,
                Command::Disconnect {
                    instance: service.instance.clone(),
                    operation: "c".repeat(32)
                }
            ),
            Reply::Operation {
                outcome: Outcome::Running {},
                ..
            }
        ));
        backend.release.add_permits(2);
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if matches!(
                    dispatch(
                        &service,
                        command(service.instance.clone(), operation.clone(), target.clone())
                    ),
                    Reply::Operation {
                        outcome: Outcome::Completed { .. },
                        ..
                    }
                ) && backend.calls.load(Ordering::SeqCst) == 2
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(backend.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn compaction_verification_is_instance_bound_deduplicated_and_disconnectable() {
        let (service, backend) = fixture();
        let command = || Command::VerifyCompaction {
            instance: service.instance.clone(),
            operation: "d".repeat(32),
            target: None,
        };
        assert_eq!(
            dispatch(
                &service,
                Command::VerifyCompaction {
                    instance: "old".into(),
                    operation: "d".repeat(32),
                    target: None,
                }
            ),
            error(ErrorCode::Instance)
        );
        for _ in 0..2 {
            assert!(matches!(
                dispatch(&service, command()),
                Reply::Operation {
                    outcome: Outcome::Running {},
                    ..
                }
            ));
        }
        assert_eq!(
            dispatch(
                &service,
                Command::VerifyCompaction {
                    instance: service.instance.clone(),
                    operation: "d".repeat(32),
                    target: Some(crate::native_probe::CheckpointTarget {
                        client: "C:/fixture/codex.exe".into(),
                        websocket: true,
                        capture_failure: false,
                        automatic: false,
                        tool_result: false,
                    }),
                }
            ),
            error(ErrorCode::OperationConflict)
        );
        assert_eq!(
            dispatch(
                &service,
                Command::VerifyCompaction {
                    instance: service.instance.clone(),
                    operation: "e".repeat(32),
                    target: None,
                }
            ),
            error(ErrorCode::Busy)
        );
        assert!(matches!(
            dispatch(
                &service,
                Command::Disconnect {
                    instance: service.instance.clone(),
                    operation: "f".repeat(32)
                }
            ),
            Reply::Operation {
                outcome: Outcome::Running {},
                ..
            }
        ));
        backend.release.add_permits(2);
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if matches!(
                    dispatch(&service, command()),
                    Reply::Operation {
                        outcome: Outcome::Completed {
                            result: DisconnectState::Idle
                        },
                        ..
                    }
                ) && backend.calls.load(Ordering::SeqCst) == 2
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn reasoning_status_is_passive_and_contains_only_published_picker_metadata() {
        let (service, backend) = fixture();
        let reply = dispatch(&service, Command::ReasoningStatus {});
        let Reply::ReasoningStatus {
            version,
            instance,
            families,
        } = reply
        else {
            panic!("expected reasoning snapshot");
        };
        assert_eq!(version, VERSION);
        assert_eq!(instance, service.instance);
        let families = families.unwrap();
        assert_eq!(families[0].model, "webbridge/fixture");
        assert_eq!(families[0].levels[0].description, "Instant");
        assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
        assert!(service.receipts.lock().unwrap().is_empty());
        let json = serde_json::to_value(&families[0]).unwrap();
        assert_eq!(json.as_object().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn reasoning_receipts_deduplicate_generation_and_allow_explicit_disconnect() {
        let (service, backend) = fixture();
        let operation = "a".repeat(32);
        let command = || Command::QualifyReasoning {
            instance: service.instance.clone(),
            operation: operation.clone(),
        };
        assert_eq!(
            dispatch(
                &service,
                Command::QualifyReasoning {
                    instance: "other".into(),
                    operation: operation.clone()
                }
            ),
            error(ErrorCode::Instance)
        );
        for _ in 0..2 {
            assert!(matches!(
                dispatch(&service, command()),
                Reply::Operation {
                    outcome: Outcome::Running {},
                    ..
                }
            ));
        }
        assert_eq!(
            dispatch(
                &service,
                Command::QualifyReasoning {
                    instance: service.instance.clone(),
                    operation: "b".repeat(32)
                }
            ),
            error(ErrorCode::Busy)
        );
        assert!(matches!(
            dispatch(
                &service,
                Command::Disconnect {
                    instance: service.instance.clone(),
                    operation: "c".repeat(32)
                }
            ),
            Reply::Operation {
                outcome: Outcome::Running {},
                ..
            }
        ));
        backend.release.add_permits(2);
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if matches!(
                    dispatch(&service, command()),
                    Reply::Operation {
                        outcome: Outcome::Completed { .. },
                        ..
                    }
                ) && backend.calls.load(Ordering::SeqCst) == 2
                {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(backend.calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            ReasoningFailure::from_code("private text").code(),
            "E_REASONING_QUALIFICATION"
        );
    }
    #[tokio::test]
    async fn retry_receipts_are_instance_bound_deduplicated_and_distinct_from_removal() {
        let (service, backend) = fixture();
        let operation = "e".repeat(32);
        let command = || Command::RetryWeb {
            instance: service.instance.clone(),
            operation: operation.clone(),
        };
        assert_eq!(
            dispatch(
                &service,
                Command::RetryWeb {
                    instance: "other".into(),
                    operation: operation.clone()
                }
            ),
            error(ErrorCode::Instance)
        );
        assert!(matches!(
            dispatch(&service, command()),
            Reply::Operation {
                outcome: Outcome::Running {},
                ..
            }
        ));
        assert!(matches!(
            dispatch(&service, command()),
            Reply::Operation {
                outcome: Outcome::Running {},
                ..
            }
        ));
        assert_eq!(
            dispatch(
                &service,
                Command::Disconnect {
                    instance: service.instance.clone(),
                    operation: operation.clone()
                }
            ),
            error(ErrorCode::OperationConflict)
        );
        backend.release.add_permits(1);
        let terminal = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let reply = dispatch(&service, command());
                if !matches!(
                    reply,
                    Reply::Operation {
                        outcome: Outcome::Running {},
                        ..
                    }
                ) {
                    break reply;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(matches!(
            terminal,
            Reply::Operation {
                outcome: Outcome::Completed {
                    result: DisconnectState::Idle
                },
                ..
            }
        ));
        assert_eq!(dispatch(&service, command()), terminal);
        assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn installed_login_receipts_distinguish_open_finish_and_replacement_instances() {
        for finish in [false, true] {
            let (service, backend) = fixture();
            let operation = "f".repeat(32);
            let command = |instance: String, finish| Command::WebLogin {
                instance,
                operation: operation.clone(),
                finish,
            };
            assert_eq!(
                dispatch(&service, command("other".into(), finish)),
                error(ErrorCode::Instance)
            );
            for _ in 0..2 {
                assert!(matches!(
                    dispatch(&service, command(service.instance.clone(), finish)),
                    Reply::Operation {
                        outcome: Outcome::Running {},
                        ..
                    }
                ));
            }
            assert_eq!(
                dispatch(&service, command(service.instance.clone(), !finish)),
                error(ErrorCode::OperationConflict)
            );
            backend.release.add_permits(1);
            let result = tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let reply = dispatch(&service, command(service.instance.clone(), finish));
                    if !matches!(
                        reply,
                        Reply::Operation {
                            outcome: Outcome::Running {},
                            ..
                        }
                    ) {
                        break reply;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert!(matches!(
                result,
                Reply::Operation {
                    outcome: Outcome::Completed {
                        result: DisconnectState::Idle
                    },
                    ..
                }
            ));
            assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
        }
    }

    #[tokio::test]
    async fn idle_removal_receipt_cannot_be_upgraded_to_force() {
        struct Active;
        impl Lifecycle for Active {
            fn state(&self) -> DisconnectState {
                DisconnectState::Idle
            }
            fn disconnect(&self) -> Work {
                panic!("force was not authorized")
            }
            fn disconnect_when_idle(&self) -> Work {
                Box::pin(async { Err("E_WEB_ACTIVE") })
            }
        }
        let service = Service::new(Arc::new(Active));
        let command = || Command::DisconnectWhenIdle {
            instance: service.instance.clone(),
            operation: "a".repeat(32),
        };
        let reply = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let reply = dispatch(&service, command());
                if !matches!(
                    reply,
                    Reply::Operation {
                        outcome: Outcome::Running {},
                        ..
                    }
                ) {
                    break reply;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(matches!(
            reply,
            Reply::Operation {
                outcome: Outcome::ActiveWork {},
                ..
            }
        ));
        assert_eq!(dispatch(&service, command()), reply);
        assert_eq!(
            dispatch(
                &service,
                Command::Disconnect {
                    instance: service.instance.clone(),
                    operation: "a".repeat(32)
                }
            ),
            error(ErrorCode::OperationConflict)
        );
    }

    #[tokio::test]
    async fn native_test_receipt_survives_requester_drop_and_binds_all_target_fields() {
        struct NativeBackend {
            calls: AtomicUsize,
            release: Arc<Semaphore>,
        }
        impl LoginBackend for NativeBackend {
            fn request(&self, _: LoginAction) -> LoginWork {
                panic!("unexpected browser observation");
            }
            fn native_text(
                &self,
                target: crate::setup_owner::NativeTarget,
                _cancellation: CancellationToken,
            ) -> LoginWork {
                assert_eq!(target.route, "webbridge/fixture");
                self.calls.fetch_add(1, Ordering::SeqCst);
                let release = self.release.clone();
                Box::pin(async move {
                    release.acquire().await.unwrap().forget();
                    Ok(ControlStatus {
                        phase: "generation_ready".into(),
                        background_session: true,
                        native_text_error: Some("E_NATIVE_PROBE_TEXT".into()),
                        ..Default::default()
                    })
                })
            }
        }
        let backend = Arc::new(NativeBackend {
            calls: AtomicUsize::new(0),
            release: Arc::new(Semaphore::new(0)),
        });
        let service = Service::login(backend.clone());
        let target = crate::setup_owner::NativeTarget {
            client: r"C:\fixture\codex.exe".into(),
            home: r"C:\fixture\home".into(),
            cwd: r"C:\fixture\workspace".into(),
            route: "webbridge/fixture".into(),
            exercise: crate::native_probe::Exercise::Text,
        };
        let command = |target| Command::NativeText {
            instance: service.instance.clone(),
            operation: "a".repeat(32),
            target,
        };
        assert!(matches!(
            dispatch(&service, command(target.clone())),
            Reply::Operation {
                outcome: Outcome::Running {},
                ..
            }
        ));
        for field in 0..7 {
            let mut changed = target.clone();
            match field {
                0 => changed.client.push("other"),
                1 => changed.home.push("other"),
                2 => changed.cwd.push("other"),
                3 => changed.route.push_str("other"),
                4 => changed.exercise = crate::native_probe::Exercise::ReadPatch,
                5 => changed.exercise = crate::native_probe::Exercise::DeniedRead,
                _ => changed.exercise = crate::native_probe::Exercise::ReadPatchTest,
            }
            assert_eq!(
                dispatch(&service, command(changed)),
                error(ErrorCode::OperationConflict)
            );
        }
        assert!(matches!(
            dispatch(&service, command(target.clone())),
            Reply::Operation {
                outcome: Outcome::Running {},
                ..
            }
        ));
        backend.release.add_permits(1);
        let terminal = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let reply = dispatch(&service, command(target.clone()));
                if !matches!(
                    reply,
                    Reply::Operation {
                        outcome: Outcome::Running {},
                        ..
                    }
                ) {
                    break reply;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(
            matches!(&terminal, Reply::Operation { outcome:Outcome::LoginCompleted {status}, .. } if status.phase == "generation_ready" && status.native_text_error.as_deref() == Some("E_NATIVE_PROBE_TEXT"))
        );
        assert_eq!(dispatch(&service, command(target)), terminal);
        assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
        assert!(
            matches!(dispatch(&service, Command::BrowserStatus {}), Reply::BrowserStatus {status, ..} if status.phase == "generation_ready" && status.native_text_error.as_deref() == Some("E_NATIVE_PROBE_TEXT"))
        );
    }

    #[tokio::test]
    async fn reset_refusals_preserve_live_owners_but_invalidate_lost_owners() {
        struct Refusing(AtomicUsize, &'static str);
        impl LoginBackend for Refusing {
            fn request(&self, action: LoginAction) -> LoginWork {
                assert_eq!(action, LoginAction::ResetTest);
                self.0.fetch_add(1, Ordering::SeqCst);
                let code = self.1;
                Box::pin(async move { Err(code) })
            }
        }
        for (error_code, preserved) in [
            ("E_BROWSER_OTHER_PAGES", true),
            ("E_BROWSER_IN_USE", true),
            ("E_ALREADY_RUNNING", false),
            ("E_CONTROL_CLOSED", false),
        ] {
            let backend = Arc::new(Refusing(AtomicUsize::new(0), error_code));
            let service = Service::login(backend.clone());
            let initial = ControlStatus {
                phase: "generation_ready".into(),
                background_session: true,
                tool_qualified_model: Some("webbridge/fixture".into()),
                qualification_evidence: Some("a".repeat(64)),
                ..Default::default()
            };
            *service.login_status.lock().unwrap() = initial.clone();
            let command = || Command::Browser {
                instance: service.instance.clone(),
                operation: "a".repeat(32),
                action: LoginAction::ResetTest,
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
                    let reply = dispatch(&service, command());
                    if !matches!(
                        reply,
                        Reply::Operation {
                            outcome: Outcome::Running {},
                            ..
                        }
                    ) {
                        break reply;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert!(
                matches!(&terminal, Reply::Operation {outcome: Outcome::LoginFailed {code}, ..} if code == error_code)
            );
            assert_eq!(dispatch(&service, command()), terminal);
            assert_eq!(backend.0.load(Ordering::SeqCst), 1);
            let status = service.login_status.lock().unwrap();
            if preserved {
                assert_eq!(*status, initial);
            } else {
                assert_eq!(status.phase, "awaiting_qualification");
                assert!(status.tool_qualified_model.is_none());
                assert!(status.qualification_evidence.is_none());
            }
        }
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
                Command::CancelNative {
                    instance: service.instance.clone(),
                    operation: "a".repeat(32),
                }
            ),
            error(ErrorCode::OperationConflict)
        );
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
    async fn client_rejects_unknown_health_schema() {
        let installation = format!("{:032x}", rand::random::<u128>());
        let mut listener = control_pipe::listen(&installation).unwrap();
        let server = tokio::spawn(async move {
            let mut pipe = listener.accept().await.unwrap();
            control_pipe::read_frame(&mut pipe).await.unwrap();
            let reply = Reply::Health {
                version: VERSION,
                instance: "a".repeat(32),
                health: Box::new(cxweb_domain::health::Health {
                    schema_version: "webbridge.health.unknown".into(),
                    ..Default::default()
                }),
            };
            control_pipe::write_frame(&mut pipe, &serde_json::to_vec(&reply).unwrap())
                .await
                .unwrap();
            let _ = control_pipe::read_frame(&mut pipe).await;
        });
        let result = exchange(&installation, &request(Command::Health {})).await;
        assert_eq!(result.unwrap_err().to_string(), "E_CONTROL_PROTOCOL");
        server.await.unwrap();
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
