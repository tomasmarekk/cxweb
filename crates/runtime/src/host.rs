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

pub struct Host {
    listener: TcpListener,
    gateway: Gateway,
    control_listener: ControlListener,
    control: Service,
    installation: String,
    serving: Arc<AtomicBool>,
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
    /// Call only after selected-target preflight and complete client/browser
    /// qualification. No desktop/remote command exposes this internal operation.
    pub async fn apply(&self) -> Result<(), &'static str> {
        self.controller.apply_prepared(self.serving.clone()).await
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
    /// Exact model IDs come from its validated journal. A recovered route stays native-only
    /// until a separate, complete activation flow qualifies browser integration.
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
        let gateway = Gateway::recover_native(port, capability, native);
        let controller =
            DisconnectController::new(gateway.clone(), journal, published, native_models)
                .map_err(io::Error::other)?;
        let control_listener = control_pipe::listen(&installation)?;
        let control = Service::new(Arc::new(controller));
        Ok(Self {
            listener,
            gateway,
            control_listener,
            control,
            installation,
            serving: Arc::new(AtomicBool::new(false)),
        })
    }

    pub(crate) fn prepared(
        listener: TcpListener,
        journal: ConfigJournal,
        gateway: Gateway,
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
        let controller = DisconnectController::new(gateway.clone(), journal, published, native)
            .map_err(io::Error::other)?;
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
        tokio::select! {
            result = axum::serve(listener, gateway.router()).into_future() => result,
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
            std::fs::remove_dir_all(&self.0).unwrap();
        }
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
