//! Disconnect orchestration for a journal and its exact live gateway.
//! The daemon must retain the listener until client restart is independently
//! established. UI lifetime and operation lifetime are deliberately separate.
use crate::{
    config_journal::{ConfigJournal, Phase, Recovery},
    gateway::Gateway,
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{Mutex as AsyncMutex, watch};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisconnectState {
    Idle,
    Draining,
    Restoring,
    PendingRestart,
    DrainFailed,
    RestoreFailed,
}

#[derive(Clone)]
pub struct DisconnectController {
    gateway: Gateway,
    journal: Arc<Mutex<ConfigJournal>>,
    serial: Arc<AsyncMutex<()>>,
    state: watch::Sender<DisconnectState>,
    published: Arc<Vec<String>>,
    native: Arc<Vec<String>>,
    health: Arc<Mutex<crate::health::Tracker>>,
}

pub(crate) enum ApplySupervision {
    Registered,
    // Isolate file/ordering tests from OS task registration. Production builds
    // have no unchecked activation variant; live tests use Registered.
    #[cfg(test)]
    Fixture,
}

fn supervision_error(error: std::io::Error) -> &'static str {
    if error.to_string() == "E_SUPERVISION_PENDING" {
        "E_SUPERVISION_PENDING"
    } else {
        "E_SUPERVISION_CHANGED"
    }
}

impl DisconnectController {
    pub(crate) async fn register_supervisor(
        &self,
        serving: Arc<std::sync::atomic::AtomicBool>,
        executable: std::path::PathBuf,
    ) -> Result<(), &'static str> {
        let controller = self.clone();
        tokio::spawn(async move {
            let _operation = controller.serial.lock().await;
            if *controller.state.borrow() != DisconnectState::Idle {
                return Err("E_ACTIVATION_STATE");
            }
            let journal = controller.journal.clone();
            tokio::task::spawn_blocking(move || {
                let mut journal = journal.lock().map_err(|_| "E_INTEGRATION_STATE")?;
                if !serving.load(std::sync::atomic::Ordering::Acquire) {
                    return Err("E_RUNTIME_NOT_READY");
                }
                if journal.phase() != Phase::Prepared {
                    return Err("E_ACTIVATION_STATE");
                }
                let target =
                    cxweb_platform::target_path::TargetPathGuard::capture(&executable, false)
                        .map_err(|_| "E_SUPERVISION_EXECUTABLE")?;
                let plan = journal
                    .prepare_scheduler(&executable)
                    .map_err(|_| "E_SUPERVISION_PLAN")?;
                target
                    .verify_unchanged()
                    .map_err(|_| "E_SUPERVISION_EXECUTABLE")?;
                let task = plan.register().map_err(|_| "E_SUPERVISION_REGISTER")?;
                // An uncertain OS mutation remains pending in the journal. Never
                // adopt a task by name or roll it back without an exact receipt.
                journal
                    .record_scheduler(&task)
                    .map_err(|_| "E_SUPERVISION_RECEIPT")
            })
            .await
            .map_err(|_| "E_ACTIVATION_WORKER")?
        })
        .await
        .map_err(|_| "E_ACTIVATION_WORKER")?
    }

    pub(crate) async fn supervision_registered(&self) -> Result<bool, &'static str> {
        let journal = self.journal.clone();
        tokio::task::spawn_blocking(move || {
            let journal = journal.lock().map_err(|_| "E_INTEGRATION_STATE")?;
            match journal.registered_scheduler() {
                Ok(_) => Ok(true),
                Err(error) if error.to_string() == "E_SUPERVISION_PENDING" => Ok(false),
                Err(error) => Err(supervision_error(error)),
            }
        })
        .await
        .map_err(|_| "E_ACTIVATION_WORKER")?
    }
    /// Internal activation owner only. Native/client qualification must precede
    /// this call; the private control protocol does not expose configuration apply.
    pub(crate) async fn apply_prepared(
        &self,
        serving: Arc<std::sync::atomic::AtomicBool>,
        supervision: ApplySupervision,
    ) -> Result<(), &'static str> {
        let controller = self.clone();
        // Once accepted, UI cancellation must not interrupt a configuration write.
        tokio::spawn(async move {
            let _operation = controller.serial.lock().await;
            if *controller.state.borrow() != DisconnectState::Idle {
                return Err("E_ACTIVATION_STATE");
            }
            let journal = controller.journal.clone();
            tokio::task::spawn_blocking(move || {
                let mut journal = journal.lock().map_err(|_| "E_INTEGRATION_STATE")?;
                if !serving.load(std::sync::atomic::Ordering::Acquire) {
                    return Err("E_RUNTIME_NOT_READY");
                }
                if journal.phase() != Phase::Prepared {
                    return Err("E_ACTIVATION_STATE");
                }
                journal
                    .catalog_receipt()
                    .map_err(|_| "E_CATALOG_RECEIPT_MISSING")?;
                if matches!(supervision, ApplySupervision::Registered) {
                    journal.registered_scheduler().map_err(supervision_error)?;
                }
                journal.apply().map_err(|_| "E_CONFIG_APPLY")
            })
            .await
            .map_err(|_| "E_ACTIVATION_WORKER")?
        })
        .await
        .map_err(|_| "E_ACTIVATION_WORKER")?
    }
    /// Catalog receipts are the exact IDs published by this installation and
    /// native IDs verified for the selected clients, never guessed prefixes.
    pub fn new(
        gateway: Gateway,
        journal: ConfigJournal,
        published: Vec<String>,
        native: Vec<String>,
    ) -> Result<Self, &'static str> {
        if !journal.routes_to(&gateway.base_url()) {
            return Err("E_INTEGRATION_ROUTE_MISMATCH");
        }
        let initial = if journal.phase() == Phase::ConfigRestored {
            if journal.recovery().map_err(|_| "E_INTEGRATION_STATE")? == Recovery::Restored {
                DisconnectState::PendingRestart
            } else {
                DisconnectState::RestoreFailed
            }
        } else {
            DisconnectState::Idle
        };
        let (state, _) = watch::channel(initial);
        Ok(Self {
            gateway,
            journal: Arc::new(Mutex::new(journal)),
            serial: Arc::new(AsyncMutex::new(())),
            state,
            published: Arc::new(published),
            native: Arc::new(native),
            health: Arc::default(),
        })
    }

    pub fn health(&self) -> cxweb_domain::health::Health {
        self.health
            .lock()
            .expect("health cache lock poisoned")
            .snapshot(*self.state.borrow(), self.gateway.health())
    }

    pub fn subscribe(&self) -> watch::Receiver<DisconnectState> {
        self.state.subscribe()
    }

    /// Dropping the caller does not interrupt an accepted disconnect operation.
    /// Failure retains journal ownership and the native listener for diagnosis
    /// and explicit retry. No path automatically terminates native connections.
    pub async fn disconnect(&self, timeout: Duration) -> Result<DisconnectState, &'static str> {
        let controller = self.clone();
        tokio::spawn(async move { controller.run(timeout, false).await })
            .await
            .map_err(|_| "E_DISCONNECT_WORKER")?
    }

    pub async fn disconnect_when_idle(
        &self,
        timeout: Duration,
    ) -> Result<DisconnectState, &'static str> {
        let controller = self.clone();
        tokio::spawn(async move { controller.run(timeout, true).await })
            .await
            .map_err(|_| "E_DISCONNECT_WORKER")?
    }

    async fn run(
        self,
        timeout: Duration,
        idle_only: bool,
    ) -> Result<DisconnectState, &'static str> {
        let _operation = self.serial.lock().await;
        if idle_only {
            self.gateway.close_if_idle()?;
        }
        self.state.send_replace(DisconnectState::Draining);
        if let Err(error) = self.gateway.disconnect_web(timeout).await {
            self.state.send_replace(DisconnectState::DrainFailed);
            return Err(error);
        }
        self.state.send_replace(DisconnectState::Restoring);
        let journal = self.journal.clone();
        let published = self.published.clone();
        let native = self.native.clone();
        let result = tokio::task::spawn_blocking(move || {
            journal
                .lock()
                .map_err(|_| "E_INTEGRATION_STATE")?
                .disconnect(&published, &native)
                .map_err(|_| "E_CONFIG_RESTORE")
        })
        .await
        .unwrap_or(Err("E_DISCONNECT_WORKER"));
        match result {
            Ok(()) => {
                self.state.send_replace(DisconnectState::PendingRestart);
                Ok(DisconnectState::PendingRestart)
            }
            Err(error) => {
                self.state.send_replace(DisconnectState::RestoreFailed);
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        gateway::{UnqualifiedProvider, WebFuture, WebProvider, WebRequest},
        native::NativeTransport,
    };
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        response::IntoResponse,
    };
    use cxweb_platform::state::protected_directory;
    use std::path::PathBuf;
    use tokio::sync::{Notify, Semaphore};
    use tower::ServiceExt;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir()
                .join(format!("cxweb-lifecycle-{:032x}", rand::random::<u128>()));
            protected_directory(&path).unwrap();
            Self(path)
        }
        fn config(&self) -> PathBuf {
            self.0.join("config.toml")
        }
        fn journal(&self, gateway: &Gateway) -> ConfigJournal {
            // Test fixture extracts only its synthetic route capability.
            let base = gateway.base_url();
            let capability = base
                .split("/wb/")
                .nth(1)
                .unwrap()
                .strip_suffix("/backend-api/codex")
                .unwrap();
            let port = base
                .strip_prefix("http://127.0.0.1:")
                .unwrap()
                .split('/')
                .next()
                .unwrap()
                .parse()
                .unwrap();
            let mut journal =
                ConfigJournal::prepare(&self.0.join("state"), &self.config(), port, capability)
                    .unwrap();
            journal.apply().unwrap();
            journal
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    struct Waiting {
        started: Arc<Notify>,
        cancelled: Arc<Notify>,
        finish: Arc<Semaphore>,
    }
    impl WebProvider for Waiting {
        fn respond(&self, request: WebRequest) -> WebFuture {
            let (started, cancelled, finish) = (
                self.started.clone(),
                self.cancelled.clone(),
                self.finish.clone(),
            );
            Box::pin(async move {
                started.notify_one();
                request.cancellation.cancelled().await;
                cancelled.notify_one();
                let _permit = finish.acquire().await.unwrap();
                StatusCode::BAD_GATEWAY.into_response()
            })
        }
    }
    fn gateway(provider: Arc<dyn WebProvider>) -> Gateway {
        Gateway::new(12345, NativeTransport::subscription().unwrap(), provider)
    }
    #[tokio::test]
    async fn dropped_activation_waiter_does_not_cancel_an_accepted_write() {
        let fixture = Fixture::new();
        let gateway = gateway(Arc::new(UnqualifiedProvider));
        let base = gateway.base_url();
        let capability = base
            .split("/wb/")
            .nth(1)
            .unwrap()
            .strip_suffix("/backend-api/codex")
            .unwrap();
        let mut journal = ConfigJournal::prepare(
            &fixture.0.join("state"),
            &fixture.config(),
            12345,
            capability,
        )
        .unwrap();
        journal
            .record_catalog(vec!["webbridge/test".into()], vec!["native-fixture".into()])
            .unwrap();
        let controller = DisconnectController::new(
            gateway,
            journal,
            vec!["webbridge/test".into()],
            vec!["native-fixture".into()],
        )
        .unwrap();
        let serial = controller.serial.lock().await;
        let (started, accepted) = tokio::sync::oneshot::channel();
        let pending = controller.clone();
        let waiter = tokio::spawn(async move {
            started.send(()).unwrap();
            pending
                .apply_prepared(
                    Arc::new(std::sync::atomic::AtomicBool::new(true)),
                    ApplySupervision::Fixture,
                )
                .await
        });
        accepted.await.unwrap();
        assert!(!fixture.config().exists());
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        drop(serial);
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if controller.journal.lock().unwrap().phase() == Phase::ConfigApplied {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(
            std::fs::read_to_string(fixture.config())
                .unwrap()
                .contains("openai_base_url")
        );
        assert_eq!(
            controller.disconnect(Duration::from_secs(1)).await.unwrap(),
            DisconnectState::PendingRestart
        );
        assert!(!fixture.config().exists());
    }
    fn start_request(gateway: &Gateway) -> tokio::task::JoinHandle<()> {
        let request = Request::builder()
            .method("POST")
            .uri(format!("{}/responses", gateway.base_url()))
            .header("host", "127.0.0.1:12345")
            .body(Body::from(r#"{"model":"webbridge/test"}"#))
            .unwrap();
        let router = gateway.clone().router();
        tokio::spawn(async move {
            router.oneshot(request).await.unwrap();
        })
    }

    #[tokio::test]
    async fn drain_timeout_keeps_config_and_successful_retry_restores_it() {
        let fixture = Fixture::new();
        let started = Arc::new(Notify::new());
        let cancelled = Arc::new(Notify::new());
        let finish = Arc::new(Semaphore::new(0));
        let gateway = gateway(Arc::new(Waiting {
            started: started.clone(),
            cancelled,
            finish: finish.clone(),
        }));
        let journal = fixture.journal(&gateway);
        let installed = std::fs::read(fixture.config()).unwrap();
        let request = start_request(&gateway);
        tokio::time::timeout(Duration::from_secs(1), started.notified())
            .await
            .unwrap();
        let controller = DisconnectController::new(gateway, journal, vec![], vec![]).unwrap();
        assert_eq!(
            controller.disconnect(Duration::from_millis(20)).await,
            Err("E_WEB_DRAIN_TIMEOUT")
        );
        assert_eq!(
            *controller.subscribe().borrow(),
            DisconnectState::DrainFailed
        );
        assert_eq!(std::fs::read(fixture.config()).unwrap(), installed);
        finish.add_permits(1);
        assert_eq!(
            controller.disconnect(Duration::from_secs(1)).await.unwrap(),
            DisconnectState::PendingRestart
        );
        request.await.unwrap();
        assert!(!fixture.config().exists());
        assert_eq!(
            controller.journal.lock().unwrap().phase(),
            Phase::ConfigRestored
        );
    }

    #[tokio::test]
    async fn dropped_ui_waiter_does_not_cancel_restore_or_release_ownership_early() {
        let fixture = Fixture::new();
        let started = Arc::new(Notify::new());
        let cancelled = Arc::new(Notify::new());
        let finish = Arc::new(Semaphore::new(0));
        let gateway = gateway(Arc::new(Waiting {
            started: started.clone(),
            cancelled: cancelled.clone(),
            finish: finish.clone(),
        }));
        let journal = fixture.journal(&gateway);
        let request = start_request(&gateway);
        tokio::time::timeout(Duration::from_secs(1), started.notified())
            .await
            .unwrap();
        let controller = DisconnectController::new(gateway, journal, vec![], vec![]).unwrap();
        let mut status = controller.subscribe();
        assert_eq!(
            controller
                .disconnect_when_idle(Duration::from_secs(1))
                .await,
            Err("E_WEB_ACTIVE")
        );
        assert_eq!(*status.borrow(), DisconnectState::Idle);
        assert_eq!(controller.health().active_web_turns, 1);
        assert!(
            std::fs::read_to_string(fixture.config())
                .unwrap()
                .contains("openai_base_url")
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(20), cancelled.notified())
                .await
                .is_err()
        );
        let caller = controller.clone();
        let waiter = tokio::spawn(async move { caller.disconnect(Duration::from_secs(3)).await });
        tokio::time::timeout(Duration::from_secs(1), cancelled.notified())
            .await
            .unwrap();
        waiter.abort();
        assert!(fixture.config().exists());
        finish.add_permits(1);
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if *status.borrow_and_update() == DisconnectState::PendingRestart {
                    break;
                }
                status.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
        request.await.unwrap();
        assert!(!fixture.config().exists());
    }

    #[tokio::test]
    async fn route_mismatch_and_user_config_conflict_never_claim_removal_success() {
        let fixture = Fixture::new();
        let owned = gateway(Arc::new(UnqualifiedProvider));
        let journal = fixture.journal(&owned);
        let wrong = gateway(Arc::new(UnqualifiedProvider));
        assert!(DisconnectController::new(wrong, journal, vec![], vec![]).is_err());
        let journal = ConfigJournal::reopen(&fixture.0.join("state"), &fixture.config()).unwrap();
        let controller = DisconnectController::new(owned, journal, vec![], vec![]).unwrap();
        let edited = "openai_base_url='https://example.com'\n";
        std::fs::write(fixture.config(), edited).unwrap();
        assert_eq!(
            controller.disconnect(Duration::from_secs(1)).await,
            Err("E_CONFIG_RESTORE")
        );
        assert_eq!(
            *controller.subscribe().borrow(),
            DisconnectState::RestoreFailed
        );
        assert_eq!(std::fs::read_to_string(fixture.config()).unwrap(), edited);
    }

    #[tokio::test]
    async fn restored_config_keeps_same_live_listener_serving_native_clients() {
        use crate::control_protocol::{Command, Outcome, Reply, Request, Service, exchange};
        use cxweb_platform::control_pipe;
        use tokio_util::sync::CancellationToken;
        let fixture = Fixture::new();
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = upstream.local_addr().unwrap();
        let upstream_task = tokio::spawn(async move {
            axum::serve(
                upstream,
                axum::Router::new().route(
                    "/responses",
                    axum::routing::post(|bytes: axum::body::Bytes| async { bytes }),
                ),
            )
            .await
            .unwrap();
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gateway = Gateway::new(
            listener.local_addr().unwrap().port(),
            NativeTransport::new(format!("http://{address}")).unwrap(),
            Arc::new(UnqualifiedProvider),
        );
        let url = format!("{}/responses", gateway.base_url());
        let journal = fixture.journal(&gateway);
        let controller =
            DisconnectController::new(gateway.clone(), journal, vec![], vec![]).unwrap();
        let installation = format!("{:032x}", rand::random::<u128>());
        let control_listener = control_pipe::listen(&installation).unwrap();
        let service = Service::new(Arc::new(controller.clone()));
        let stop = CancellationToken::new();
        let control_server = tokio::spawn({
            let stop = stop.clone();
            async move { service.serve(control_listener, stop).await }
        });
        let Reply::Status { instance, .. } = exchange(
            &installation,
            &Request {
                version: 1,
                command: Command::Status {},
            },
        )
        .await
        .unwrap() else {
            panic!("expected runtime status")
        };
        let server = tokio::spawn(async move {
            axum::serve(listener, gateway.router()).await.unwrap();
        });
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let native = r#"{"model":"native","input":"unchanged"}"#;
        for after_disconnect in [false, true] {
            if after_disconnect {
                let operation = "d".repeat(32);
                let accepted = exchange(
                    &installation,
                    &Request {
                        version: 1,
                        command: Command::Disconnect {
                            instance: instance.clone(),
                            operation: operation.clone(),
                        },
                    },
                )
                .await
                .unwrap();
                assert_eq!(
                    accepted,
                    Reply::Operation {
                        operation: operation.clone(),
                        outcome: Outcome::Running {}
                    }
                );
                tokio::time::timeout(Duration::from_secs(3), async {
                    loop {
                        let reply = exchange(
                            &installation,
                            &Request {
                                version: 1,
                                command: Command::Operation {
                                    instance: instance.clone(),
                                    operation: operation.clone(),
                                },
                            },
                        )
                        .await
                        .unwrap();
                        match reply {
                            Reply::Operation {
                                outcome:
                                    Outcome::Completed {
                                        result: DisconnectState::PendingRestart,
                                    },
                                ..
                            } => break,
                            Reply::Operation {
                                outcome: Outcome::Running {},
                                ..
                            } => tokio::time::sleep(Duration::from_millis(10)).await,
                            other => panic!("unexpected disconnect result: {other:?}"),
                        }
                    }
                })
                .await
                .unwrap();
                assert!(!fixture.config().exists());
            }
            let response = client.post(&url).body(native).send().await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.text().await.unwrap(), native);
        }
        let response = client
            .post(&url)
            .body(r#"{"model":"webbridge/test"}"#)
            .send()
            .await
            .unwrap();
        assert!(
            response
                .text()
                .await
                .unwrap()
                .contains("E_WEB_DISCONNECTED")
        );
        assert_eq!(
            *controller.subscribe().borrow(),
            DisconnectState::PendingRestart
        );
        server.abort();
        upstream_task.abort();
        stop.cancel();
        control_server.await.unwrap().unwrap();
    }
}
