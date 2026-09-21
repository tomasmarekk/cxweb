//! Reconstitute an existing private route before accepting desktop connections.
//! Recovery never reapplies configuration or enables unqualified web generation.
use crate::{
    config_journal::ConfigJournal, control_protocol::Service, gateway::Gateway,
    lifecycle::DisconnectController, native::NativeTransport,
};
use cxweb_platform::{
    control_pipe::{self, ControlListener},
    loopback,
};
use std::{
    io,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

/// All production HTTP and WebSocket traffic passes this boundary before HTTP
/// parsing. Native configuration can be readable without granting another
/// Windows user access to the saved ChatGPT session.
struct UserListener(TcpListener);

#[cfg(test)]
#[path = "host/latency_benchmark.rs"]
mod latency_benchmark;
impl axum::serve::Listener for UserListener {
    type Io = tokio::net::TcpStream;
    type Addr = std::net::SocketAddr;
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match self.0.accept().await {
                Ok((stream, peer)) => {
                    let Ok(local) = stream.local_addr() else {
                        continue;
                    };
                    if tokio::task::spawn_blocking(move || {
                        cxweb_platform::tcp_peer::verify(peer, local)
                    })
                    .await
                    .is_ok_and(|result| result.is_ok())
                    {
                        return (stream, peer);
                    }
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
    }
    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.0.local_addr()
    }
}

pub struct Host {
    listener: TcpListener,
    gateway: Gateway,
    control_listener: ControlListener,
    control: Service,
    installation: String,
    serving: Arc<AtomicBool>,
    recovery: Option<(
        crate::web_recovery::PendingProvider,
        crate::web_recovery::Receipt,
        std::path::PathBuf,
    )>,
}

/// In-process activation receipt. The daemon retains Host independently of this
/// handle; dropping a UI waiter does not stop native forwarding or undo a write.
pub struct ActivationHandle {
    controller: DisconnectController,
    serving: Arc<AtomicBool>,
}
impl ActivationHandle {
    /// Whether the reserved host's serve future is running, not upstream health.
    pub fn is_serving(&self) -> bool {
        self.serving.load(Ordering::Acquire)
    }
    pub async fn routing_installed(&self) -> bool {
        self.is_serving() && self.controller.routing_installed().await
    }
    /// The setup owner checks the selected target and browser protocol before
    /// this transaction. Actual client picker verification follows installation.
    pub async fn apply(&self) -> Result<(), &'static str> {
        self.controller
            .apply_prepared(
                self.serving.clone(),
                crate::lifecycle::ApplySupervision::Registered,
            )
            .await
    }
    /// Register only the independently qualified installed daemon executable.
    /// The accepted operation persists its receipt even if the caller disappears.
    pub async fn register_supervisor(
        &self,
        executable: std::path::PathBuf,
    ) -> Result<(), &'static str> {
        self.controller
            .register_supervisor(self.serving.clone(), executable)
            .await
    }
    /// Verify the journal's exact task still exists; this is not current-process
    /// health or proof that the browser session has survived a runtime restart.
    pub async fn supervision_registered(&self) -> Result<bool, &'static str> {
        self.controller.supervision_registered().await
    }
    #[cfg(test)]
    pub(crate) async fn apply_fixture(&self) -> Result<(), &'static str> {
        self.controller
            .apply_prepared(
                self.serving.clone(),
                crate::lifecycle::ApplySupervision::Fixture,
            )
            .await
    }
}

struct Serving(Arc<AtomicBool>);
impl Drop for Serving {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
impl Host {
    /// The selected configuration must be independently selected by the owner.
    /// Exact bindings come from its private journal. Native forwarding starts
    /// first; web routes resume only after non-generative browser revalidation.
    pub fn recover(directory: &Path, config: &Path) -> io::Result<Self> {
        let journal = ConfigJournal::reopen(directory, config)?;
        Self::bind(
            journal,
            NativeTransport::subscription().map_err(io::Error::other)?,
        )
    }

    fn bind(journal: ConfigJournal, native: NativeTransport) -> io::Result<Self> {
        // Re-read current state without applying a stale plan. User edits and
        // interrupted restores remain for explicit three-way disconnect.
        journal.recovery()?;
        let (published, native_models) = journal.catalog_receipt()?;
        let (port, capability, installation) = journal.runtime_route();
        let installation = installation.to_owned();
        let listener = loopback::bind(port)?;
        let (gateway, recovery) = if let Some(receipt) = journal.web_recovery()? {
            let pending = crate::web_recovery::PendingProvider::default();
            let gateway = Gateway::prepared(port, capability, native, Arc::new(pending.clone()));
            (
                gateway,
                Some((pending, receipt, journal.directory().to_owned())),
            )
        } else {
            (Gateway::recover_native(port, capability, native), None)
        };
        let mut controller =
            DisconnectController::new(gateway.clone(), journal, published, native_models)
                .map_err(io::Error::other)?;
        if let Some((pending, receipt, directory)) = &recovery {
            controller = controller.with_recovery(crate::web_recovery::RecoveryController::new(
                pending.clone(),
                receipt.clone(),
                directory.clone(),
            ));
        }
        let control_listener = control_pipe::listen(&installation)?;
        let control = Service::new(Arc::new(controller));
        Ok(Self {
            listener,
            gateway,
            control_listener,
            control,
            installation,
            serving: Arc::new(AtomicBool::new(false)),
            recovery,
        })
    }

    pub(crate) fn prepared(
        listener: TcpListener,
        journal: ConfigJournal,
        gateway: Gateway,
        recovery: Option<crate::web_recovery::RecoveryController>,
    ) -> io::Result<(Self, ActivationHandle)> {
        if journal.phase() != crate::config_journal::Phase::Prepared
            || listener.local_addr()?.port() != journal.runtime_route().0
            || listener.local_addr()?.ip() != std::net::Ipv4Addr::LOCALHOST
            || !journal.routes_to(&gateway.base_url())
        {
            return Err(io::Error::other("E_INTEGRATION_ROUTE_MISMATCH"));
        }
        let installation = journal.installation_id().to_owned();
        let (published, native) = journal.catalog_receipt()?;
        let control_listener = control_pipe::listen(&installation)?;
        let mut controller = DisconnectController::new(gateway.clone(), journal, published, native)
            .map_err(io::Error::other)?;
        if let Some(recovery) = recovery {
            controller = controller.with_recovery(recovery);
        }
        let serving = Arc::new(AtomicBool::new(false));
        let handle = ActivationHandle {
            controller: controller.clone(),
            serving: serving.clone(),
        };
        Ok((
            Self {
                listener,
                gateway,
                control_listener,
                control: Service::new(Arc::new(controller)),
                installation,
                serving,
                recovery: None,
            },
            handle,
        ))
    }

    /// Process owner, not a window, owns this future. There is deliberately no
    /// remote shutdown or UI-lifetime cancellation. Client-exit qualification is
    /// required before the supervisor may terminate a retained listener.
    pub async fn serve(self) -> io::Result<()> {
        let Self {
            listener,
            gateway,
            control_listener,
            control,
            installation,
            serving,
            recovery,
        } = self;
        let _serving = Serving(serving.clone());
        serving.store(true, Ordering::Release);
        let control_loop = async {
            let mut pending = Some(control_listener);
            loop {
                if let Some(listener) = pending.take() {
                    // A control failure must not drop the native listener or its
                    // journal lock. Retry private admission without moving ports.
                    let _ = control.serve(listener, CancellationToken::new()).await;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
                pending = control_pipe::listen(&installation).ok();
            }
        };
        let recovery_gateway = gateway.clone();
        let recover = async {
            if let Some((pending, receipt, directory)) = recovery {
                let _ = recovery_gateway
                    .recover_web(|cancel| pending.restore(receipt, directory, cancel))
                    .await;
            }
            std::future::pending::<()>().await;
        };
        tokio::select! {
            () = recover => unreachable!("recovery remains owned by the host"),
            result = axum::serve(UserListener(listener), gateway.router()).into_future() => result,
            () = control_loop => unreachable!("control admission loop never returns"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_protocol::{Command, Outcome, Reply, Request, exchange};
    use crate::lifecycle::DisconnectState;
    use cxweb_platform::state::protected_directory;
    use std::path::PathBuf;

    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let result = std::fs::remove_dir_all(&self.0);
            if !std::thread::panicking() {
                result.unwrap();
            }
        }
    }
    #[tokio::test]
    async fn prepared_host_exposes_maintenance_without_restart_or_background_restore() {
        use crate::web_recovery::{PendingProvider, Receipt, RecoveryController};
        let fixture = Fixture(std::env::temp_dir().join(format!(
            "cxweb-prepared-maintenance-{:032x}",
            rand::random::<u128>()
        )));
        protected_directory(&fixture.0).unwrap();
        let config = fixture.0.join("config.toml");
        let directory = fixture.0.join("journal");
        let listener = loopback::bind(0).unwrap();
        let port = listener.local_addr().unwrap().port();
        let capability = "a".repeat(43);
        let mut journal = ConfigJournal::prepare(&directory, &config, port, &capability).unwrap();
        journal
            .record_catalog(vec!["webbridge/fixture".into()], vec!["native".into()])
            .unwrap();
        let receipt = Receipt::fixture(journal.installation_id());
        journal.record_web(receipt.clone()).unwrap();
        let pending = PendingProvider::default();
        let gateway = Gateway::prepared(
            port,
            &capability,
            NativeTransport::new("http://127.0.0.1:1".into()).unwrap(),
            Arc::new(pending.clone()),
        );
        let recovery = RecoveryController::new(pending, receipt, directory);
        let (host, activation) =
            Host::prepared(listener, journal, gateway, Some(recovery)).unwrap();
        // The actual maintenance call reaches the attached provider. Before the
        // fix it failed with E_WEB_RECOVERY_STATE because no controller existed.
        assert_eq!(
            activation.controller.qualify_reasoning().await,
            Err("E_WEB_RECOVERING")
        );
        assert!(
            host.recovery.is_none(),
            "an attached owner must never be restored a second time"
        );
        assert!(
            !config.exists(),
            "maintenance wiring must not apply configuration"
        );
        drop(activation);
        drop(host);
    }

    #[tokio::test]
    #[ignore = "requires CXWEB_HEALTH_CLI pointing to built CLI; synthetic runtime only, no browser or account"]
    async fn actual_cli_reads_sanitized_health_without_launching_a_runtime() {
        let executable = PathBuf::from(
            std::env::var_os("CXWEB_HEALTH_CLI").expect("select the built cxweb CLI"),
        );
        assert!(executable.is_absolute() && executable.is_file());
        let fixture = Fixture(
            std::env::temp_dir().join(format!("cxweb-health-cli-{:032x}", rand::random::<u128>())),
        );
        protected_directory(&fixture.0).unwrap();
        let config = fixture.0.join("config.toml");
        let directory = fixture.0.join("journal");
        let reservation = loopback::bind(0).unwrap();
        let mut journal = ConfigJournal::prepare(
            &directory,
            &config,
            reservation.local_addr().unwrap().port(),
            &"a".repeat(43),
        )
        .unwrap();
        journal.record_catalog(vec![], vec![]).unwrap();
        journal.apply().unwrap();
        let installation = journal.installation_id().to_owned();
        drop(journal);
        drop(reservation);
        let host = Host::recover(&directory, &config).unwrap();
        let server = tokio::spawn(host.serve());
        let mut child = tokio::process::Command::new(executable);
        child
            .arg("runtime-health")
            .arg("--installation")
            .arg(&installation)
            .creation_flags(0x08000000)
            .kill_on_drop(true)
            .stdin(std::process::Stdio::null());
        let result = tokio::time::timeout(Duration::from_secs(15), child.output())
            .await
            .unwrap()
            .unwrap();
        server.abort();
        let _ = server.await;
        assert!(result.status.success());
        let health: cxweb_domain::health::Health = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(health.overall, cxweb_domain::health::Overall::Disconnected);
        assert_eq!(
            health.components.runtime.state,
            cxweb_domain::health::ComponentState::Healthy
        );
        assert_eq!(
            health.components.codex_cli.state,
            cxweb_domain::health::ComponentState::Unknown
        );
        let text = std::str::from_utf8(&result.stdout).unwrap();
        for forbidden in [&installation, &"a".repeat(43), "127.0.0.1", "auth.json"] {
            assert!(!text.contains(forbidden));
        }
        if let Some(output) = std::env::var_os("CXWEB_HEALTH_REPORT") {
            use std::io::Write;
            let output = PathBuf::from(output);
            assert!(output.is_absolute());
            std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(output)
                .unwrap()
                .write_all(&result.stdout)
                .unwrap();
        }
    }

    #[tokio::test]
    async fn installed_web_receipt_reconstitutes_pending_provider_without_opening_browser_in_bind()
    {
        let fixture = Fixture(
            std::env::temp_dir().join(format!("cxweb-host-web-{:032x}", rand::random::<u128>())),
        );
        protected_directory(&fixture.0).unwrap();
        let config = fixture.0.join("config.toml");
        let directory = fixture.0.join("journal");
        let reservation = loopback::bind(0).unwrap();
        let port = reservation.local_addr().unwrap().port();
        let mut journal =
            ConfigJournal::prepare(&directory, &config, port, &"a".repeat(43)).unwrap();
        journal
            .record_catalog(vec!["webbridge/fixture".into()], vec!["native".into()])
            .unwrap();
        journal
            .record_web(crate::web_recovery::Receipt::fixture(
                journal.installation_id(),
            ))
            .unwrap();
        journal.apply().unwrap();
        let applied = std::fs::read(&config).unwrap();
        drop(journal);
        drop(reservation);
        let host = Host::bind(
            ConfigJournal::reopen(&directory, &config).unwrap(),
            NativeTransport::new("http://127.0.0.1:1".into()).unwrap(),
        )
        .unwrap();
        assert_eq!(host.listener.local_addr().unwrap().port(), port);
        assert!(host.recovery.is_some());
        let response = host
            .gateway
            .dispatch_web(
                serde_json::json!({"model":"webbridge/fixture"}),
                None,
                false,
                false,
                crate::gateway::WebTransport::Http,
                None,
            )
            .await;
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        assert!(
            std::str::from_utf8(&bytes)
                .unwrap()
                .contains("E_WEB_RECOVERING")
        );
        assert_eq!(std::fs::read(&config).unwrap(), applied);
        // serve owns the later browser launch; binding alone does not run it.
        drop(host);
    }

    #[tokio::test]
    async fn restarted_host_keeps_original_route_and_restores_over_private_control() {
        let fixture = Fixture(
            std::env::temp_dir().join(format!("cxweb-host-{:032x}", rand::random::<u128>())),
        );
        protected_directory(&fixture.0).unwrap();
        let config = fixture.0.join("config.toml");
        let directory = fixture.0.join("journal");
        let reservation = loopback::bind(0).unwrap();
        let port = reservation.local_addr().unwrap().port();
        let capability = "a".repeat(43);
        let mut journal = ConfigJournal::prepare(&directory, &config, port, &capability).unwrap();
        journal.record_catalog(vec![], vec![]).unwrap();
        journal.apply().unwrap();
        let installation = journal.runtime_route().2.to_owned();
        let applied = std::fs::read(&config).unwrap();
        drop(journal);
        // Occupied route fails closed: no fallback port and no config mutation.
        assert!(Host::recover(&directory, &config).is_err());
        assert_eq!(std::fs::read(&config).unwrap(), applied);
        drop(reservation);
        let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", upstream.local_addr().unwrap());
        let upstream_task = tokio::spawn(async move {
            axum::serve(
                upstream,
                axum::Router::new().route(
                    "/responses",
                    axum::routing::post(|| async { "native response" }),
                ),
            )
            .await
            .unwrap();
        });
        let client = reqwest::Client::builder()
            .no_proxy()
            .pool_max_idle_per_host(0)
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let url = format!("http://127.0.0.1:{port}/wb/{capability}/v1/responses");
        for expected in [
            DisconnectState::Idle,
            DisconnectState::PendingRestart,
            DisconnectState::RestoreFailed,
        ] {
            if expected == DisconnectState::RestoreFailed {
                std::fs::write(&config, "model = 'user-selected'\n").unwrap();
            }
            let journal = ConfigJournal::reopen(&directory, &config).unwrap();
            let host = Host::bind(journal, NativeTransport::new(origin.clone()).unwrap()).unwrap();
            let task = tokio::spawn(host.serve());
            let Reply::Status {
                instance, state, ..
            } = exchange(
                &installation,
                &Request {
                    version: 1,
                    command: Command::Status {},
                },
            )
            .await
            .unwrap()
            else {
                panic!("missing status")
            };
            assert_eq!(state, expected);
            let health_request = Request {
                version: 1,
                command: Command::Health {},
            };
            let Reply::Health {
                instance: health_instance,
                health,
                ..
            } = exchange(&installation, &health_request).await.unwrap()
            else {
                panic!("missing health");
            };
            assert_eq!(health_instance, instance);
            assert_eq!(
                health.components.runtime.state,
                cxweb_domain::health::ComponentState::Healthy
            );
            assert_eq!(
                health.components.codex_app.state,
                cxweb_domain::health::ComponentState::Unknown
            );
            let expected_health = match expected {
                DisconnectState::Idle => cxweb_domain::health::Overall::Disconnected,
                DisconnectState::PendingRestart => {
                    cxweb_domain::health::Overall::RemovalPendingRestart
                }
                DisconnectState::RestoreFailed => cxweb_domain::health::Overall::ConfigConflict,
                _ => unreachable!(),
            };
            assert_eq!(health.overall, expected_health);
            let Reply::Health {
                health: repeated, ..
            } = exchange(&installation, &health_request).await.unwrap()
            else {
                panic!("missing health");
            };
            // Health now performs a real config read. Evidence remains the
            // same, but a subsequent observation may advance time/revision.
            assert!(repeated.revision >= health.revision);
            assert_eq!(repeated.overall, health.overall);
            assert_eq!(repeated.active_web_turns, health.active_web_turns);
            assert_eq!(repeated.suggested_action, health.suggested_action);
            let before = serde_json::to_value(&health.components).unwrap();
            let after = serde_json::to_value(&repeated.components).unwrap();
            for (name, component) in before.as_object().unwrap() {
                for key in ["state", "evidence", "code"] {
                    assert_eq!(component[key], after[name][key]);
                }
                assert!(component["observed_at"].as_str() <= after[name]["observed_at"].as_str());
            }
            let serialized = serde_json::to_string(&health).unwrap();
            assert!(!serialized.contains(&capability));
            assert!(!serialized.contains(&installation));
            assert!(!serialized.contains("127.0.0.1"));

            let response = client
                .post(&url)
                .body(r#"{"model":"native"}"#)
                .send()
                .await
                .unwrap();
            assert_eq!(response.text().await.unwrap(), "native response");
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
            if expected == DisconnectState::Idle {
                let operation = "b".repeat(32);
                exchange(
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
                            other => panic!("unexpected result {other:?}"),
                        }
                    }
                })
                .await
                .unwrap();
                assert!(!config.exists());
            }
            if expected == DisconnectState::RestoreFailed {
                assert_eq!(
                    std::fs::read_to_string(&config).unwrap(),
                    "model = 'user-selected'\n"
                );
            }
            // Synthetic test host only; real shutdown requires client-exit proof.
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
        }
        upstream_task.abort();
        let _ = upstream_task.await;
    }
}
