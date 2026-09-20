//! Transfer the one browser/profile owner without enabling native configuration.
use crate::{
    browser_scope::BrowserScope,
    control::{ControlStatus, QualifiedModel},
    managed_driver::{Binding, ManagedDriver, Route},
    web_provider::ProviderScope,
};
use cxweb_browser_adapter::{ManagedBrowser, ManagedPage};
use std::fs::File;

/// Runtime-only receipt. Protocol qualification is not coding/client qualification.
/// This cannot be serialized through the desktop IPC or used to apply configuration.
pub struct GenerationSession {
    pub driver: ManagedDriver,
    pub route: Route,
    pub protocol_evidence: String,
    scope: ProviderScope,
}

impl GenerationSession {
    pub fn scope(&self) -> ProviderScope {
        ProviderScope {
            installation: self.scope.installation.clone(),
            account: self.scope.account.clone(),
            workspace: self.scope.workspace.clone(),
            epoch: self.scope.epoch,
        }
    }

    pub(crate) fn replay(
        self: &std::sync::Arc<Self>,
        installation: &str,
        route: &str,
    ) -> Result<std::sync::Arc<Self>, &'static str> {
        if self.driver.is_closed() {
            Err("E_BROWSER_CLOSED")
        } else if self.scope.installation == installation && self.route.id == route {
            Ok(self.clone())
        } else {
            Err("E_BROWSER_IN_USE")
        }
    }
}

pub(crate) struct PreparedHandoff {
    binding: Binding,
    route: Route,
    evidence: String,
}

fn selected_route<'a>(
    status: &ControlStatus,
    routes: &'a [(QualifiedModel, String)],
    installation: &str,
    requested: &str,
) -> Result<&'a (QualifiedModel, String), &'static str> {
    if installation.is_empty()
        || installation.len() > 512
        || installation.chars().any(char::is_control)
    {
        return Err("E_SESSION_SCOPE");
    }
    if status.phase != "tool_protocol_qualified"
        || status.tool_qualified_model.as_deref() != Some(requested)
        || status.temporary_chat_available != Some(true)
        || !status
            .qualification_evidence
            .as_deref()
            .is_some_and(|value| {
                value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
    {
        return Err("E_HANDOFF_UNQUALIFIED");
    }
    let mut selected = routes.iter().filter(|(route, _)| route.selected);
    let route = selected.next().ok_or("E_MODEL_SELECTION")?;
    if selected.next().is_some() || route.0.id != requested || route.1.is_empty() {
        return Err("E_MODEL_SELECTION");
    }
    Ok(route)
}

impl PreparedHandoff {
    /// All fallible observations occur while Control still owns browser and lock.
    pub(crate) fn prepare(
        browser: &mut ManagedBrowser,
        page: &ManagedPage,
        status: &ControlStatus,
        routes: &[(QualifiedModel, String)],
        observed: (&str, &BrowserScope),
        installation: &str,
        requested: &str,
    ) -> Result<Self, &'static str> {
        let (model, identity) = selected_route(status, routes, installation, requested)?;
        if !page.is_hidden() {
            return Err("E_HANDOFF_BACKGROUND_REQUIRED");
        }
        browser.ensure_no_other_pages(Some(page)).map_err(|error| {
            match error.to_string().as_str() {
                "E_BROWSER_OTHER_PAGES" => "E_BROWSER_OTHER_PAGES",
                _ => "E_BROWSER_RELEASE",
            }
        })?;
        let observation = browser
            .login_observation(page)
            .map_err(|_| "E_BROWSER_OBSERVATION")?;
        if !super::control::authenticated_surface(&observation) {
            return Err("E_LOGIN_REQUIRED");
        }
        if ![observation.browser_language, observation.page_language]
            .iter()
            .all(|value| {
                value
                    .as_deref()
                    .is_some_and(|value| value == "en" || value.starts_with("en-"))
            })
        {
            return Err("E_BROWSER_LANGUAGE");
        }
        let baseline = browser
            .baseline(page)
            .map_err(|_| "E_BROWSER_OBSERVATION")?;
        if !baseline.composer_empty || baseline.generating {
            return Err("E_BROWSER_BUSY");
        }
        browser
            .verify_candidate(page, identity, &model.label)
            .map_err(|_| "E_MODEL_SELECTION")?;
        let surface = browser.account_scope(page).map_err(|_| "E_SESSION_SCOPE")?;
        if &BrowserScope::from_surface(observed.0, &surface)? != observed.1 {
            return Err("E_SESSION_SCOPE");
        }
        let scope = BrowserScope::from_surface(installation, &surface)?;
        let effort = match model.label.rsplit_once(" · ").map(|(_, effort)| effort) {
            Some("Instant") => "none",
            Some("Medium") => "medium",
            Some("High") => "high",
            Some("Extra High") => "xhigh",
            _ => return Err("E_MODEL_UNAVAILABLE"),
        };
        let route = Route {
            id: model.id.clone(),
            identity: identity.clone(),
            label: model.label.clone(),
            effort: Some(effort.into()),
        };
        let prepared = Self {
            binding: Binding {
                installation: installation.into(),
                account: scope.account,
                workspace: scope.workspace,
                epoch: 0,
                routes: vec![route.clone()],
            },
            route,
            evidence: status
                .qualification_evidence
                .clone()
                .ok_or("E_HANDOFF_UNQUALIFIED")?,
        };
        prepared.binding.validate()?;
        let baseline = browser
            .baseline(page)
            .map_err(|_| "E_BROWSER_OBSERVATION")?;
        if !baseline.composer_empty || baseline.generating {
            return Err("E_BROWSER_BUSY");
        }
        // Retained failed tests were checked above. Do not close any other target.
        browser
            .close_page_checked(page)
            .map_err(|_| "E_BROWSER_RELEASE")?;
        browser
            .ensure_no_other_pages(None)
            .map_err(|error| match error.to_string().as_str() {
                "E_BROWSER_OTHER_PAGES" => "E_BROWSER_OTHER_PAGES",
                _ => "E_BROWSER_RELEASE",
            })?;
        Ok(prepared)
    }

    pub(crate) fn start(
        self,
        browser: ManagedBrowser,
        ownership: File,
    ) -> Result<GenerationSession, &'static str> {
        let scope = ProviderScope {
            installation: self.binding.installation.clone(),
            account: self.binding.account.clone(),
            workspace: self.binding.workspace.clone(),
            epoch: self.binding.epoch,
        };
        Ok(GenerationSession {
            driver: ManagedDriver::start(browser, self.binding, ownership)?,
            route: self.route,
            protocol_evidence: self.evidence,
            scope,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_status() -> ControlStatus {
        ControlStatus {
            phase: "tool_protocol_qualified".into(),
            tool_qualified_model: Some("webbridge/fixture".into()),
            temporary_chat_available: Some(true),
            qualification_evidence: Some("a".repeat(64)),
            ..Default::default()
        }
    }

    fn fixture_routes() -> Vec<(QualifiedModel, String)> {
        vec![(
            QualifiedModel {
                id: "webbridge/fixture".into(),
                label: "Fixture · High".into(),
                selected: true,
            },
            "fixture-identity".into(),
        )]
    }

    #[test]
    fn handoff_requires_current_protocol_evidence_and_exact_selected_route() {
        let status = fixture_status();
        let routes = fixture_routes();
        assert!(selected_route(&status, &routes, "installation", "webbridge/fixture").is_ok());
        let mutations: &[fn(&mut ControlStatus)] = &[
            |s| s.phase = "candidates_observed".into(),
            |s| s.phase = "text_qualified".into(),
            |s| s.tool_qualified_model = None,
            |s| s.tool_qualified_model = Some("webbridge/other".into()),
            |s| s.temporary_chat_available = Some(false),
            |s| s.qualification_evidence = None,
            |s| s.qualification_evidence = Some("not-an-evidence-hash".into()),
        ];
        for mutate in mutations {
            let mut changed = status.clone();
            mutate(&mut changed);
            assert!(matches!(
                selected_route(&changed, &routes, "installation", "webbridge/fixture"),
                Err("E_HANDOFF_UNQUALIFIED")
            ));
        }
        for installation in [
            "".to_owned(),
            "x".repeat(513),
            "invalid\ninstallation".into(),
        ] {
            assert!(matches!(
                selected_route(&status, &routes, &installation, "webbridge/fixture"),
                Err("E_SESSION_SCOPE")
            ));
        }
        assert!(selected_route(&status, &routes, "installation", "webbridge/stale").is_err());
        let mut changed = routes.clone();
        changed.push(changed[0].clone());
        assert!(matches!(
            selected_route(&status, &changed, "installation", "webbridge/fixture"),
            Err("E_MODEL_SELECTION")
        ));
        changed = routes.clone();
        changed[0].0.selected = false;
        assert!(selected_route(&status, &changed, "installation", "webbridge/fixture").is_err());
        changed = routes.clone();
        changed[0].1.clear();
        assert!(selected_route(&status, &changed, "installation", "webbridge/fixture").is_err());
    }

    #[tokio::test]
    #[ignore = "requires installed Chrome; transfers a fresh profile without account access"]
    async fn generation_receipt_keeps_profile_owned_until_driver_shutdown() {
        use cxweb_platform::state::{StatePaths, installed_browser, protected_directory};
        let root =
            std::env::temp_dir().join(format!("cxweb-handoff-{:032x}", rand::random::<u128>()));
        protected_directory(&root).unwrap();
        let paths = StatePaths {
            root: root.clone(),
            profile: root.join("profile"),
            state: root.join("state"),
        };
        protected_directory(&paths.profile).unwrap();
        protected_directory(&paths.state).unwrap();
        let ownership = paths.lock().unwrap();
        let browser =
            ManagedBrowser::launch(&installed_browser().unwrap(), &paths.profile, false).unwrap();
        let route = Route {
            id: "webbridge/fixture".into(),
            identity: "fixture".into(),
            label: "Fixture · High".into(),
            effort: Some("high".into()),
        };
        let prepared = PreparedHandoff {
            binding: Binding {
                installation: "fixture-installation".into(),
                account: "fixture-account".into(),
                workspace: "fixture-workspace".into(),
                epoch: 0,
                routes: vec![route.clone()],
            },
            route,
            evidence: "a".repeat(64),
        };
        let receipt = std::sync::Arc::new(prepared.start(browser, ownership).unwrap());
        assert!(paths.lock().is_err());
        let replay = receipt
            .replay("fixture-installation", "webbridge/fixture")
            .unwrap();
        assert!(std::sync::Arc::ptr_eq(&receipt, &replay));
        drop(replay);
        assert!(matches!(
            receipt.replay("another-installation", "webbridge/fixture"),
            Err("E_BROWSER_IN_USE")
        ));
        assert!(matches!(
            receipt.replay("fixture-installation", "webbridge/another"),
            Err("E_BROWSER_IN_USE")
        ));
        assert_eq!(receipt.scope().account, "fixture-account");
        // The runtime receipt retains the owner after a requesting UI drops it.
        let runtime_receipt = receipt.clone();
        drop(receipt);
        assert!(!runtime_receipt.driver.is_closed());
        assert!(runtime_receipt.driver.diagnostic().await.is_ok());
        assert!(paths.lock().is_err());
        // A host driver handle likewise outlives the login control receipt.
        let driver = runtime_receipt.driver.clone();
        drop(runtime_receipt);
        assert!(paths.lock().is_err());
        driver.shutdown().await.unwrap();
        assert!(driver.is_closed());
        drop(paths.lock().unwrap());
    }

    #[tokio::test]
    #[ignore = "requires installed Chrome; binds a synthetic generation session to a disposable installation"]
    async fn generation_session_binds_to_reserved_installation_without_another_browser() {
        use crate::activation::PreparedInstallation;
        use cxweb_codex_adapter::catalog_codec::{CatalogCodec, CatalogRoute};
        use cxweb_platform::state::{StatePaths, installed_browser, protected_directory};
        let root = std::env::temp_dir().join(format!(
            "cxweb-activation-browser-{:032x}",
            rand::random::<u128>()
        ));
        protected_directory(&root).unwrap();
        let paths = StatePaths {
            root: root.clone(),
            profile: root.join("profile"),
            state: root.join("browser-state"),
        };
        protected_directory(&paths.profile).unwrap();
        protected_directory(&paths.state).unwrap();
        let config = root.join("config.toml");
        let reserved = PreparedInstallation::reserve(&root.join("installation"), &config).unwrap();
        let installation = reserved.installation_id().to_owned();
        let route = Route {
            id: "webbridge/fixture".into(),
            identity: "fixture".into(),
            label: "Fixture · High".into(),
            effort: Some("high".into()),
        };
        let browser =
            ManagedBrowser::launch(&installed_browser().unwrap(), &paths.profile, false).unwrap();
        let session = PreparedHandoff {
            binding: Binding {
                installation,
                account: "fixture-account".into(),
                workspace: "fixture-workspace".into(),
                epoch: 0,
                routes: vec![route.clone()],
            },
            route,
            evidence: "a".repeat(64),
        }
        .start(browser, paths.lock().unwrap())
        .unwrap();
        let (host, activation) = reserved
            .bind_generation(
                &session,
                vec![(
                    CatalogCodec::Cli01551,
                    vec![CatalogRoute {
                        id: session.route.id.clone(),
                        observed_label: session.route.label.clone(),
                        effort: "high".into(),
                        coding: true,
                    }],
                )],
                vec!["native-fixture".into()],
            )
            .await
            .unwrap();
        assert!(!config.exists());
        assert!(paths.lock().is_err());
        assert_eq!(activation.apply().await, Err("E_RUNTIME_NOT_READY"));
        let serving = tokio::spawn(host.serve());
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !activation.is_serving() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(activation.apply().await, Err("E_SUPERVISION_PENDING"));
        activation.apply_fixture().await.unwrap();
        assert!(
            std::fs::read_to_string(&config)
                .unwrap()
                .contains("openai_base_url")
        );
        assert!(session.driver.diagnostic().await.is_ok());
        assert!(paths.lock().is_err());
        serving.abort();
        let _ = serving.await;
        session.driver.shutdown().await.unwrap();
        drop(paths.lock().unwrap());
    }
}
