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

    /// Supply only client/route pairs with complete coding and picker evidence.
    /// A browser tool-protocol receipt alone cannot make that qualification decision.
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
        if qualified.is_empty()
            || native_models.is_empty()
            || qualified.iter().any(|(_, routes)| {
                routes.len() != 1
                    || routes.iter().any(|route| {
                        !route.coding
                            || route.id != session.route.id
                            || route.observed_label != session.route.label
                            || Some(route.effort.as_str()) != session.route.effort.as_deref()
                    })
            })
        {
            return Err("E_ACTIVATION_CATALOG");
        }
        let ledger = Ledger::open(&self.directory.join("turns.sqlite")).await?;
        let coordinator = Coordinator::new(ledger, Arc::new(session.driver.clone()));
        let provider =
            CoordinatorProvider::new(coordinator, scope, vec![session.route.id.clone()])?
                .with_catalog(1, qualified)?;
        if session.driver.is_closed() {
            return Err("E_BROWSER_CLOSED");
        }
        self.finish(
            Arc::new(provider),
            vec![session.route.id.clone()],
            native_models,
        )
        .map_err(|_| "E_ACTIVATION_PREPARE")
    }

    fn finish(
        mut self,
        provider: Arc<dyn WebProvider>,
        published: Vec<String>,
        native_models: Vec<String>,
    ) -> io::Result<(Host, ActivationHandle)> {
        self.journal.record_catalog(published, native_models)?;
        let (port, capability, _) = self.journal.runtime_route();
        let gateway = Gateway::prepared(port, capability, self.native, provider);
        Host::prepared(self.listener, self.journal, gateway)
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
            let root = std::env::temp_dir()
                .join(format!("cxweb-activation-{:032x}", rand::random::<u128>()));
            protected_directory(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    struct FixtureProvider;
    impl WebProvider for FixtureProvider {
        fn respond(&self, _: WebRequest) -> WebFuture {
            Box::pin(async { "fixture web response".into_response() })
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
                assert_eq!(activation.apply().await, Err("E_CONFIG_APPLY"));
                assert_eq!(
                    std::fs::read_to_string(&config).unwrap(),
                    "model = 'user-change'\n"
                );
            } else {
                activation.apply().await.unwrap();
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
