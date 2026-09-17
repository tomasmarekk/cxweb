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
use std::{io, path::Path, sync::Arc, time::Duration};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

pub struct Host {
    listener: TcpListener,
    gateway: Gateway,
    control_listener: ControlListener,
    control: Service,
    installation: String,
}
impl Host {
    /// The selected configuration and model receipts must be independently
    /// qualified by the installation owner. A recovered route stays native-only
    /// until a separate, complete activation flow qualifies browser integration.
    pub fn recover(
        directory: &Path,
        config: &Path,
        published: Vec<String>,
        native_models: Vec<String>,
    ) -> io::Result<Self> {
        let journal = ConfigJournal::reopen(directory, config)?;
        Self::bind(
            journal,
            NativeTransport::subscription().map_err(io::Error::other)?,
            published,
            native_models,
        )
    }

    fn bind(
        journal: ConfigJournal,
        native: NativeTransport,
        published: Vec<String>,
        native_models: Vec<String>,
    ) -> io::Result<Self> {
        // Re-read current state without applying a stale plan. User edits and
        // interrupted restores remain for explicit three-way disconnect.
        journal.recovery()?;
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
        })
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
        } = self;
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
        journal.apply().unwrap();
        let installation = journal.runtime_route().2.to_owned();
        let applied = std::fs::read(&config).unwrap();
        drop(journal);
        // Occupied route fails closed: no fallback port and no config mutation.
        assert!(Host::recover(&directory, &config, vec![], vec![]).is_err());
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
            let host = Host::bind(
                journal,
                NativeTransport::new(origin.clone()).unwrap(),
                vec![],
                vec![],
            )
            .unwrap();
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
