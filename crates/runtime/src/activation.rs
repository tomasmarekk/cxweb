//! Prepare the daemon's new listener before the selected configuration is changed.
//! Full native/client qualification remains required by the activation owner.
use crate::{
    config_journal::ConfigJournal,
    control::GenerationSession,
    gateway::{Gateway, WebProvider},
    host::{ActivationHandle, Host},
    ledger::Ledger,
    native::NativeTransport,
    turn::Coordinator,
    web_provider::CoordinatorProvider,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use cxweb_codex_adapter::catalog_codec::{CatalogCodec, CatalogRoute};
use cxweb_platform::loopback;
use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::net::TcpListener;

pub struct PreparedInstallation {
    listener: TcpListener,
    journal: ConfigJournal,
    directory: PathBuf,
    native: NativeTransport,
}

impl PreparedInstallation {
    /// Reserve an exclusive port and persist a reversible plan for an explicitly
    /// selected target. This does not apply configuration or launch a browser.
    pub fn reserve(directory: &Path, config: &Path) -> io::Result<Self> {
        Self::reserve_with_native(
            directory,
            config,
            NativeTransport::subscription().map_err(io::Error::other)?,
        )
    }

    fn reserve_with_native(
        directory: &Path,
        config: &Path,
        native: NativeTransport,
    ) -> io::Result<Self> {
        if !directory.is_absolute() || !config.is_absolute() {
            return Err(io::Error::other("E_ACTIVATION_TARGET"));
        }
        let listener = loopback::bind(0)?;
        let capability = URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>());
        let journal = ConfigJournal::prepare(
            directory,
            config,
            listener.local_addr()?.port(),
            &capability,
        )?;
        Ok(Self {
            listener,
            journal,
            directory: directory.into(),
            native,
        })
    }

    /// Use this persistent identity for Control::take_generation so execution,
    /// catalog and configuration share the same installation scope.
    pub fn installation_id(&self) -> &str {
        self.journal.installation_id()
    }

    /// Bind observed routes to reviewed client codecs. Actual client picker
    /// verification follows activation; it cannot precede route installation.
    /// This connects the real driver and ledger but still leaves configuration untouched.
    pub async fn bind_generation(
        self,
        session: &GenerationSession,
        qualified: Vec<(CatalogCodec, Vec<CatalogRoute>)>,
        native_models: Vec<String>,
    ) -> Result<(Host, ActivationHandle), &'static str> {
        let scope = session.scope();
        if scope.installation != self.installation_id() || session.driver.is_closed() {
            return Err("E_SESSION_SCOPE");
        }
        let expected = session.route.catalog(true)?;
        if qualified.is_empty()
            || native_models.is_empty()
            || qualified.iter().any(|(_, routes)| {
                routes.len() != 1
                    || routes.iter().any(|route| {
                        !route.coding
                            || route.id != session.route.id
                            || route.observed_label != expected.observed_label
                            || route.effort != expected.effort
                            || route.reasoning != expected.reasoning
                    })
            })
        {
            return Err("E_ACTIVATION_CATALOG");
        }
        let receipt = crate::web_recovery::Receipt::new(
            session,
            &qualified
                .iter()
                .map(|(codec, _)| *codec)
                .collect::<Vec<_>>(),
        );
        receipt.validate(
            self.installation_id(),
            std::slice::from_ref(&session.route.id),
        )?;
        let consumer = session.claim()?;
        let ledger = Ledger::open(&self.directory.join("turns.sqlite")).await?;
        let coordinator =
            Coordinator::new(ledger, Arc::new(session.driver.clone())).with_consumer(consumer);
        let provider =
            CoordinatorProvider::new(coordinator, scope, vec![session.route.id.clone()])?
                .with_catalog(1, qualified)?;
        if session.driver.is_closed() {
            return Err("E_BROWSER_CLOSED");
        }
        let active = crate::web_recovery::PendingProvider::active(
            session.driver.clone(),
            provider,
            session.verified_at.clone(),
        );
        self.finish(
            Arc::new(active.clone()),
            vec![session.route.id.clone()],
            native_models,
            Some((receipt, active)),
        )
        .map_err(|_| "E_ACTIVATION_PREPARE")
    }

    fn finish(
        mut self,
        provider: Arc<dyn WebProvider>,
        published: Vec<String>,
        native_models: Vec<String>,
        recovery: Option<(
            crate::web_recovery::Receipt,
            crate::web_recovery::PendingProvider,
        )>,
    ) -> io::Result<(Host, ActivationHandle)> {
        self.journal.record_catalog(published, native_models)?;
        let recovery = recovery
            .map(|(receipt, pending)| {
                self.journal.record_web(receipt.clone())?;
                Ok::<_, io::Error>(crate::web_recovery::RecoveryController::new(
                    pending,
                    receipt,
                    self.directory.clone(),
                ))
            })
            .transpose()?;
        let (port, capability, _) = self.journal.runtime_route();
        let gateway = Gateway::prepared(port, capability, self.native, provider);
        Host::prepared(self.listener, self.journal, gateway, recovery)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config_journal::Recovery,
        gateway::{WebFuture, WebRequest},
    };
    use axum::response::IntoResponse;
    use cxweb_platform::state::protected_directory;
    use std::time::Duration;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            Self::under(&std::env::temp_dir())
        }
        fn under(base: &Path) -> Self {
            let root = base.join(format!("cxweb-activation-{:032x}", rand::random::<u128>()));
            protected_directory(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            // A failing async test may still have a host holding the journal
            // until runtime shutdown. Preserve its private diagnostics rather
            // than panic again while unwinding and abort the entire test process.
            if std::thread::panicking() {
                return;
            }
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    struct FixtureProvider;
    impl WebProvider for FixtureProvider {
        fn respond(&self, _: WebRequest) -> WebFuture {
            Box::pin(async { "fixture web response".into_response() })
        }
    }

    struct TaskCleanup(Option<cxweb_platform::scheduled_runtime::RegisteredRuntime>);
    impl TaskCleanup {
        fn remove(&mut self) -> std::io::Result<()> {
            if let Some(task) = &self.0 {
                task.remove_stopped()?;
            }
            self.0 = None;
            Ok(())
        }
    }
    impl Drop for TaskCleanup {
        fn drop(&mut self) {
            for _ in 0..100 {
                if self.remove().is_ok() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            eprintln!("E_FIXTURE_TASK_CLEANUP: the owned registration still needs cleanup");
        }
    }

    #[tokio::test]
    #[ignore = "registers temporary owned Windows tasks; requires CXWEB_ACTIVATION_DAEMON and CXWEB_QUALIFIED_FIXTURE_ROOT with qualified ancestor permissions"]
    async fn supervised_activation_verifies_registration_and_refuses_a_removed_task() {
        use cxweb_platform::scheduled_runtime::RegistrationReceipt;
        let executable = PathBuf::from(
            std::env::var_os("CXWEB_ACTIVATION_DAEMON").expect("set CXWEB_ACTIVATION_DAEMON"),
        );
        assert!(executable.is_absolute() && executable.is_file());
        assert_eq!(executable.file_name().unwrap(), "cxweb-daemon.exe");
        let base = PathBuf::from(
            std::env::var_os("CXWEB_QUALIFIED_FIXTURE_ROOT")
                .expect("select an existing qualified test directory"),
        );
        cxweb_platform::target_path::TargetPathGuard::capture(&base, true)
            .unwrap()
            .capture_access()
            .expect("test root ancestor permissions must qualify");
        for removed_before_apply in [false, true] {
            let fixture = Fixture::under(&base);
            // Cargo's top-level binary is a hard link to its deps output.
            // Exercise an installed copy; production correctly refuses aliases.
            let installed = fixture.0.join("cxweb-daemon.exe");
            std::fs::copy(&executable, &installed).unwrap();
            let config = fixture.0.join("config.toml");
            let directory = fixture.0.join("journal");
            let reserved = PreparedInstallation::reserve(&directory, &config).unwrap();
            let installation = reserved.installation_id().to_owned();
            let (host, activation) = reserved
                .finish(
                    Arc::new(FixtureProvider),
                    vec!["webbridge/fixture".into()],
                    vec!["native-fixture".into()],
                    None,
                )
                .unwrap();
            assert_eq!(
                activation.register_supervisor(installed.clone()).await,
                Err("E_RUNTIME_NOT_READY")
            );
            let host_task = tokio::spawn(host.serve());
            tokio::time::timeout(Duration::from_secs(2), async {
                while !activation.is_serving() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(activation.apply().await, Err("E_SUPERVISION_PENDING"));
            assert!(!config.exists());
            activation
                .register_supervisor(installed.clone())
                .await
                .unwrap();
            let record: serde_json::Value =
                serde_json::from_slice(&std::fs::read(directory.join("integration.json")).unwrap())
                    .unwrap();
            let receipt: RegistrationReceipt =
                serde_json::from_value(record["scheduler"]["receipt"].clone()).unwrap();
            let mut cleanup = TaskCleanup(Some(receipt.reopen(&installation).unwrap()));
            assert!(activation.supervision_registered().await.unwrap());
            assert!(!config.exists());
            assert_eq!(
                activation.register_supervisor(installed.clone()).await,
                Err("E_SUPERVISION_PLAN")
            );
            if !removed_before_apply {
                activation.apply().await.unwrap();
                assert!(
                    std::fs::read_to_string(&config)
                        .unwrap()
                        .contains("openai_base_url")
                );
            }
            // The real recovery daemon is started by registration. The active
            // host holds the journal, so that child exits without taking its route.
            // Never terminate processes or remove a running/changed task.
            let last_result = tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let status = cleanup.0.as_ref().unwrap().status().unwrap();
                    if status.last_run > 0.0 && !status.running {
                        break status.last_result;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            })
            .await
            .unwrap();
            // The real daemon returns 3 when the existing host owns its journal.
            assert_eq!(last_result, 3);
            tokio::time::timeout(Duration::from_secs(20), async {
                while cleanup.remove().is_err() {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            })
            .await
            .unwrap();
            assert_eq!(
                activation.supervision_registered().await,
                Err("E_SUPERVISION_CHANGED")
            );
            if removed_before_apply {
                assert_eq!(activation.apply().await, Err("E_SUPERVISION_CHANGED"));
                assert!(!config.exists());
            }
            host_task.abort();
            let _ = host_task.await;
            drop(activation);
        }
    }

    #[tokio::test]
    async fn reserving_listener_never_applies_config_and_releases_port_on_drop() {
        let fixture = Fixture::new();
        let config = fixture.0.join("config.toml");
        std::fs::write(&config, "model = 'native-fixture'\n").unwrap();
        let directory = fixture.0.join("journal");
        let reservation = PreparedInstallation::reserve(&directory, &config).unwrap();
        let port = reservation.listener.local_addr().unwrap().port();
        assert!(loopback::bind(port).is_err());
        assert_eq!(
            std::fs::read_to_string(&config).unwrap(),
            "model = 'native-fixture'\n"
        );
        assert_eq!(reservation.journal.recovery().unwrap(), Recovery::Original);
        drop(reservation);
        drop(loopback::bind(port).unwrap());
        assert_eq!(
            ConfigJournal::reopen(&directory, &config)
                .unwrap()
                .recovery()
                .unwrap(),
            Recovery::Original
        );
    }

    #[tokio::test]
    async fn configuration_apply_requires_live_host_and_keeps_native_route_after_error() {
        for changed in [false, true] {
            let fixture = Fixture::new();
            let config = fixture.0.join("config.toml");
            let directory = fixture.0.join("journal");
            let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let native =
                NativeTransport::new(format!("http://{}", upstream.local_addr().unwrap())).unwrap();
            let upstream_task = tokio::spawn(async move {
                axum::serve(
                    upstream,
                    axum::Router::new().route(
                        "/responses",
                        axum::routing::post(|| async { "fixture native response" }),
                    ),
                )
                .await
                .unwrap();
            });
            let reservation =
                PreparedInstallation::reserve_with_native(&directory, &config, native).unwrap();
            let (port, capability, _) = reservation.journal.runtime_route();
            let url = format!("http://127.0.0.1:{port}/wb/{capability}/v1/responses");
            let (host, activation) = reservation
                .finish(
                    Arc::new(FixtureProvider),
                    vec!["webbridge/fixture".into()],
                    vec!["native-fixture".into()],
                    None,
                )
                .unwrap();
            assert_eq!(activation.apply().await, Err("E_RUNTIME_NOT_READY"));
            assert!(!config.exists());
            if changed {
                std::fs::write(&config, "model = 'user-change'\n").unwrap();
            }
            let host_task = tokio::spawn(host.serve());
            tokio::time::timeout(Duration::from_secs(2), async {
                while !activation.is_serving() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert!(!activation.supervision_registered().await.unwrap());
            assert_eq!(activation.apply().await, Err("E_SUPERVISION_PENDING"));
            assert_eq!(
                activation
                    .register_supervisor(fixture.0.join("missing-daemon.exe"))
                    .await,
                Err("E_SUPERVISION_EXECUTABLE")
            );
            let record: serde_json::Value =
                serde_json::from_slice(&std::fs::read(directory.join("integration.json")).unwrap())
                    .unwrap();
            assert!(record["scheduler"].is_null());
            let client = reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap();
            // Both paths already answer before the selected config is modified.
            for (model, expected) in [
                ("native-fixture", "fixture native response"),
                ("webbridge/fixture", "fixture web response"),
            ] {
                let response = client
                    .post(&url)
                    .body(serde_json::json!({"model":model}).to_string())
                    .send()
                    .await
                    .unwrap();
                assert_eq!(response.text().await.unwrap(), expected);
            }
            if changed {
                assert_eq!(activation.apply_fixture().await, Err("E_CONFIG_APPLY"));
                assert_eq!(
                    std::fs::read_to_string(&config).unwrap(),
                    "model = 'user-change'\n"
                );
            } else {
                activation.apply_fixture().await.unwrap();
                assert!(
                    std::fs::read_to_string(&config)
                        .unwrap()
                        .contains("openai_base_url")
                );
                assert_eq!(activation.apply().await, Err("E_ACTIVATION_STATE"));
            }
            let response = client
                .post(&url)
                .body(r#"{"model":"native-fixture"}"#)
                .send()
                .await
                .unwrap();
            assert_eq!(response.text().await.unwrap(), "fixture native response");
            host_task.abort();
            let _ = host_task.await;
            assert!(!activation.is_serving());
            assert_eq!(activation.apply().await, Err("E_RUNTIME_NOT_READY"));
            drop(activation);
            upstream_task.abort();
            let _ = upstream_task.await;
        }
    }
}
