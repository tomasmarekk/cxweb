//! Private-control health from sanitized observations; this module performs no IO.
use crate::{
    gateway::{GatewayHealth, ProviderHealth},
    lifecycle::DisconnectState,
};
use cxweb_domain::health::{
    Action, Component, ComponentState as State, Components, Evidence, Health, Overall,
};

#[derive(Default)]
pub(crate) struct Tracker {
    previous: Option<(
        DisconnectState,
        GatewayHealth,
        ConfigurationObservation,
        Health,
    )>,
    configuration: ConfigurationObservation,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ConfigurationObservation {
    state: Configuration,
    observed_at: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Configuration {
    #[default]
    Unknown,
    Installed,
    NotInstalled,
    Conflict,
    Unavailable,
}

fn component(
    state: State,
    evidence: Evidence,
    observed_at: Option<String>,
    code: Option<&str>,
) -> Component {
    Component {
        state,
        evidence,
        observed_at,
        code: code.map(str::to_owned),
    }
}

/// Never pass arbitrary backend text through the control protocol. Codes below
/// are fixed local observations and carry no paths, URLs, IDs or account data.
fn failure(code: &str) -> (Overall, State, Action, &'static str) {
    if matches!(
        code,
        "E_BROWSER_BASELINE" | "E_BROWSER_BASELINE_COMPOSER" | "E_BROWSER_BASELINE_MODEL"
    ) {
        return (
            Overall::Unavailable,
            State::Unavailable,
            Action::Details,
            crate::web_recovery::baseline_error(code),
        );
    }
    let preparation = crate::managed_driver::temporary_chat_error(code);
    if preparation.starts_with("E_BROWSER_TEMPORARY_") {
        return (
            Overall::Unavailable,
            State::Unavailable,
            Action::Details,
            preparation,
        );
    }
    match code {
        "E_BROWSER_RATE_LIMITED" => (
            Overall::RateLimited,
            State::Unavailable,
            Action::Details,
            "E_BROWSER_RATE_LIMITED",
        ),
        "E_LOGIN_REQUIRED" => (
            Overall::AuthRequired,
            State::AuthRequired,
            Action::OpenLogin,
            "E_LOGIN_REQUIRED",
        ),
        "E_BROWSER_VERIFICATION_REQUIRED" => (
            Overall::AuthRequired,
            State::AuthRequired,
            Action::OpenLogin,
            "E_BROWSER_VERIFICATION_REQUIRED",
        ),
        "E_SESSION_SCOPE" => (
            Overall::AuthRequired,
            State::AuthRequired,
            Action::Check,
            "E_SESSION_SCOPE",
        ),
        "E_BROWSER_VERSION_CHANGED" => (
            Overall::Incompatible,
            State::Incompatible,
            Action::Check,
            "E_BROWSER_VERSION_CHANGED",
        ),
        "E_MODEL_SELECTION" => (
            Overall::Incompatible,
            State::Incompatible,
            Action::Check,
            "E_MODEL_SELECTION",
        ),
        "E_BROWSER_LANGUAGE" => (
            Overall::Incompatible,
            State::Incompatible,
            Action::Check,
            "E_BROWSER_LANGUAGE",
        ),
        "E_ALREADY_RUNNING" => (
            Overall::Unavailable,
            State::Conflict,
            Action::Details,
            "E_ALREADY_RUNNING",
        ),
        "E_BROWSER_CLOSED" => (
            Overall::Unavailable,
            State::Unavailable,
            Action::Check,
            "E_BROWSER_CLOSED",
        ),
        "E_BACKGROUND_NAVIGATION" => (
            Overall::Unavailable,
            State::Unavailable,
            Action::Check,
            "E_BACKGROUND_NAVIGATION",
        ),
        "E_BROWSER_START" => (
            Overall::Unavailable,
            State::Unavailable,
            Action::Check,
            "E_BROWSER_START",
        ),
        "E_BROWSER_RUNTIME_MISSING" => (
            Overall::Unavailable,
            State::Unavailable,
            Action::Check,
            "E_BROWSER_RUNTIME_MISSING",
        ),
        "E_BROWSER_OBSERVATION" => (
            Overall::Unavailable,
            State::Unavailable,
            Action::Check,
            "E_BROWSER_OBSERVATION",
        ),
        "E_BROWSER_BUSY" | "E_BROWSER_OTHER_PAGES" => (
            Overall::Unavailable,
            State::Conflict,
            Action::Details,
            "E_BROWSER_BUSY",
        ),
        "E_STATE_PERMISSIONS" => (
            Overall::Unavailable,
            State::Conflict,
            Action::Details,
            "E_STATE_PERMISSIONS",
        ),
        "E_CANCELLED" => (
            Overall::Unavailable,
            State::Unavailable,
            Action::Check,
            "E_CANCELLED",
        ),
        _ => (
            Overall::Unavailable,
            State::Unavailable,
            Action::Details,
            "E_WEB_UNAVAILABLE",
        ),
    }
}

impl Tracker {
    pub(crate) fn observe_configuration(&mut self, configuration: Configuration) {
        self.configuration = ConfigurationObservation {
            state: configuration,
            observed_at: cxweb_platform::clock::utc_timestamp(),
        };
    }

    pub(crate) fn snapshot(&mut self, state: DisconnectState, gateway: GatewayHealth) -> Health {
        if let Some((previous_state, previous_gateway, previous_config, health)) = &self.previous
            && *previous_state == state
            && *previous_gateway == gateway
            && *previous_config == self.configuration
        {
            return health.clone();
        }
        let now = cxweb_platform::clock::utc_timestamp();
        let mut health = Health {
            revision: self
                .previous
                .as_ref()
                .map_or(1, |(_, _, _, h)| h.revision.saturating_add(1)),
            active_web_turns: gateway.active_turns,
            components: Components {
                runtime: component(State::Healthy, Evidence::LocalProbe, now.clone(), None),
                ..Default::default()
            },
            ..Default::default()
        };
        match &gateway.provider {
            ProviderHealth::Unqualified => {
                health.overall = Overall::Incompatible;
                health.components.web_models = component(
                    State::Incompatible,
                    Evidence::LocalProbe,
                    now.clone(),
                    Some("E_COMPATIBILITY_UNQUALIFIED"),
                );
            }
            ProviderHealth::Recovering => {
                health.overall = Overall::Preflight;
                health.components.browser.code = Some("E_WEB_RECOVERING".into());
            }
            ProviderHealth::Verified { observed_at } => {
                let verified = component(
                    State::Healthy,
                    Evidence::PassiveBrowser,
                    observed_at.clone(),
                    None,
                );
                health.components.browser = verified.clone();
                health.components.web_auth = verified.clone();
                health.components.web_models = verified;
                // Passive browser verification does not prove the native
                // subscription, actual App/CLI picker or current config state.
                health.overall = if gateway.active_turns > 0 {
                    Overall::Busy
                } else {
                    Overall::Preflight
                };
            }
            ProviderHealth::Unavailable { code } => {
                let (overall, component_state, action, fixed) = failure(code);
                health.overall = overall;
                health.suggested_action = action;
                let failed = component(
                    component_state,
                    Evidence::LocalProbe,
                    now.clone(),
                    Some(fixed),
                );
                match fixed {
                    "E_LOGIN_REQUIRED" | "E_BROWSER_VERIFICATION_REQUIRED" | "E_SESSION_SCOPE" => {
                        health.components.web_auth = failed
                    }
                    "E_MODEL_SELECTION" => health.components.web_models = failed,
                    _ => health.components.browser = failed,
                }
            }
        }
        for (observed, catalog, dimension) in [
            (
                &gateway.clients.cli,
                &gateway.clients.cli_catalog,
                &mut health.components.codex_cli,
            ),
            (
                &gateway.clients.app,
                &gateway.clients.app_catalog,
                &mut health.components.codex_app,
            ),
        ] {
            if gateway.accepting
                && state == DisconnectState::Idle
                && let Some(catalog) = catalog
            {
                *dimension = component(
                    if !catalog.valid {
                        State::Degraded
                    } else if catalog.current {
                        State::Healthy
                    } else {
                        State::RestartRequired
                    },
                    Evidence::ClientHandshake,
                    catalog.observed_at.clone(),
                    if !catalog.valid {
                        Some("E_CLIENT_CATALOG_RESPONSE")
                    } else if catalog.current {
                        None
                    } else {
                        Some("E_CLIENT_CATALOG_CHANGED")
                    },
                );
            }
            if gateway.accepting
                && state == DisconnectState::Idle
                && let Some(observed) = observed
                && catalog
                    .as_ref()
                    .is_none_or(|catalog| catalog.current && catalog.valid)
            {
                *dimension = component(
                    if observed.succeeded {
                        State::Healthy
                    } else {
                        State::Degraded
                    },
                    if observed.succeeded {
                        Evidence::RequestSuccess
                    } else {
                        Evidence::LocalProbe
                    },
                    observed.observed_at.clone(),
                    if observed.succeeded {
                        None
                    } else {
                        Some("E_CLIENT_WEB_REQUEST")
                    },
                );
            }
        }
        if let Some(observed) = &gateway.native {
            use crate::native_health::Outcome;
            let (state, evidence, code) = match observed.outcome {
                Outcome::Received => (State::Healthy, Evidence::LocalProbe, None),
                Outcome::Connected => (State::Healthy, Evidence::ClientHandshake, None),
                Outcome::AuthRequired => (
                    State::AuthRequired,
                    Evidence::LocalProbe,
                    Some("E_NATIVE_AUTH_REQUIRED"),
                ),
                Outcome::Forbidden => (
                    State::Degraded,
                    Evidence::LocalProbe,
                    Some("E_NATIVE_FORBIDDEN"),
                ),
                Outcome::RateLimited => (
                    State::Degraded,
                    Evidence::LocalProbe,
                    Some("E_NATIVE_RATE_LIMITED"),
                ),
                Outcome::ServerError => (
                    State::Unavailable,
                    Evidence::LocalProbe,
                    Some("E_NATIVE_SERVER"),
                ),
                Outcome::RequestRejected => (
                    State::Degraded,
                    Evidence::LocalProbe,
                    Some("E_NATIVE_REQUEST_REJECTED"),
                ),
                Outcome::TransportError => (
                    State::Unavailable,
                    Evidence::LocalProbe,
                    Some("E_NATIVE_UNAVAILABLE"),
                ),
                Outcome::Redirect => (
                    State::Unavailable,
                    Evidence::LocalProbe,
                    Some("E_NATIVE_REDIRECT"),
                ),
                Outcome::StreamError => (
                    State::Unavailable,
                    Evidence::LocalProbe,
                    Some("E_NATIVE_STREAM"),
                ),
            };
            health.components.native_upstream =
                component(state, evidence, observed.observed_at.clone(), code);
        }
        if state == DisconnectState::Idle {
            let (config_state, code) = match self.configuration.state {
                Configuration::Unknown => (State::Unknown, None),
                Configuration::Installed => (State::Healthy, None),
                Configuration::NotInstalled => (State::NotInstalled, None),
                Configuration::Conflict => (State::Conflict, Some("E_CONFIG_CHANGED")),
                Configuration::Unavailable => (State::Unavailable, Some("E_CONFIG_OBSERVATION")),
            };
            if self.configuration.state != Configuration::Unknown {
                health.components.config = component(
                    config_state,
                    Evidence::LocalProbe,
                    self.configuration.observed_at.clone(),
                    code,
                );
            }
            if self.configuration.state == Configuration::Conflict {
                health.overall = Overall::ConfigConflict;
                health.suggested_action = Action::Details;
            } else if self.configuration.state == Configuration::Unavailable {
                health.overall = Overall::Unavailable;
                health.suggested_action = Action::Details;
            }
        }
        if !gateway.accepting {
            health.overall = Overall::Disconnected;
            health.suggested_action = Action::Connect;
        }
        if state == DisconnectState::Idle
            && gateway.accepting
            && !gateway.cleanup_failed
            && gateway.active_turns == 0
            && health.overall == Overall::Preflight
            && [
                &health.components.runtime,
                &health.components.browser,
                &health.components.web_auth,
                &health.components.web_models,
                &health.components.native_upstream,
                &health.components.config,
            ]
            .iter()
            .all(|component| component.state == State::Healthy)
        {
            let clients = [&health.components.codex_cli, &health.components.codex_app];
            if clients
                .iter()
                .all(|component| component.state == State::Healthy)
            {
                health.overall = Overall::Ready;
                health.suggested_action = Action::None;
            } else if clients
                .iter()
                .all(|component| matches!(component.state, State::Healthy | State::RestartRequired))
            {
                health.overall = Overall::RestartRequired;
                health.suggested_action = Action::Details;
            }
        }
        if gateway.cleanup_failed {
            health.overall = Overall::Unavailable;
            health.suggested_action = Action::Details;
            health.components.runtime = component(
                State::Degraded,
                Evidence::LocalProbe,
                now.clone(),
                Some("E_WEB_CLEANUP_UNCONFIRMED"),
            );
        }
        match state {
            DisconnectState::Idle => (),
            DisconnectState::Draining | DisconnectState::Restoring => {
                health.overall = Overall::Disconnecting;
                health.suggested_action = Action::None;
            }
            DisconnectState::PendingRestart => {
                health.overall = Overall::RemovalPendingRestart;
                health.suggested_action = Action::Details;
                health.components.config =
                    component(State::Healthy, Evidence::LocalProbe, now, None);
            }
            DisconnectState::RestoreFailed => {
                health.overall = Overall::ConfigConflict;
                health.suggested_action = Action::Details;
                health.components.config = component(
                    State::Conflict,
                    Evidence::LocalProbe,
                    now,
                    Some("E_CONFIG_RESTORE"),
                );
            }
            DisconnectState::DrainFailed => {
                health.overall = Overall::Unavailable;
                health.suggested_action = Action::Details;
                health.components.runtime = component(
                    State::Degraded,
                    Evidence::LocalProbe,
                    now,
                    Some("E_WEB_CLEANUP_UNCONFIRMED"),
                );
            }
        }
        self.previous = Some((state, gateway, self.configuration.clone(), health.clone()));
        health
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ready_requires_every_dimension_and_catalog_cannot_erase_failed_generation() {
        let mut source = gateway(ProviderHealth::Verified { observed_at: None });
        let catalog = crate::gateway::ClientCatalog {
            observed_at: Some("2026-09-20T00:00:00.000Z".into()),
            web_scope: "scope".into(),
            generation: 1,
            current: true,
            valid: true,
        };
        source.clients.cli_catalog = Some(catalog.clone());
        source.clients.app_catalog = Some(catalog);
        source.native = Some(crate::native_health::Observation {
            outcome: crate::native_health::Outcome::Received,
            observed_at: None,
        });
        let mut tracker = Tracker::default();
        assert_ne!(
            tracker
                .snapshot(DisconnectState::Idle, source.clone())
                .overall,
            Overall::Ready
        );
        tracker.observe_configuration(Configuration::Installed);
        let ready = tracker.snapshot(DisconnectState::Idle, source.clone());
        assert_eq!(ready.overall, Overall::Ready);
        assert_eq!(
            ready.components.codex_cli.evidence,
            Evidence::ClientHandshake
        );
        assert_eq!(
            ready.components.codex_app.evidence,
            Evidence::ClientHandshake
        );
        let mut changed = source.clone();
        changed.clients.cli_catalog = None;
        assert_eq!(
            tracker.snapshot(DisconnectState::Idle, changed).overall,
            Overall::Preflight
        );
        let mut changed = source.clone();
        changed.clients.cli_catalog.as_mut().unwrap().current = false;
        assert_eq!(
            tracker.snapshot(DisconnectState::Idle, changed).overall,
            Overall::RestartRequired
        );
        let mut changed = source.clone();
        changed.clients.cli_catalog.as_mut().unwrap().valid = false;
        assert_eq!(
            tracker
                .snapshot(DisconnectState::Idle, changed)
                .components
                .codex_cli
                .state,
            State::Degraded
        );
        let mut changed = source.clone();
        changed.clients.cli = Some(crate::gateway::ClientRequest {
            succeeded: false,
            observed_at: None,
        });
        assert_eq!(
            tracker
                .snapshot(DisconnectState::Idle, changed)
                .components
                .codex_cli
                .state,
            State::Degraded
        );
        let mut changed = source.clone();
        changed.native = None;
        assert_eq!(
            tracker.snapshot(DisconnectState::Idle, changed).overall,
            Overall::Preflight
        );
        let mut changed = source.clone();
        changed.provider = ProviderHealth::Recovering;
        assert_eq!(
            tracker.snapshot(DisconnectState::Idle, changed).overall,
            Overall::Preflight
        );
        let mut changed = source.clone();
        changed.active_turns = 1;
        assert_eq!(
            tracker.snapshot(DisconnectState::Idle, changed).overall,
            Overall::Busy
        );
        let mut changed = source.clone();
        changed.cleanup_failed = true;
        assert_eq!(
            tracker.snapshot(DisconnectState::Idle, changed).overall,
            Overall::Unavailable
        );
        tracker.observe_configuration(Configuration::Conflict);
        assert_eq!(
            tracker.snapshot(DisconnectState::Idle, source).overall,
            Overall::ConfigConflict
        );
    }
    #[test]
    fn native_transport_evidence_is_independent_of_browser_and_task_qualification() {
        use crate::native_health::{Observation, Outcome};
        let mut source = gateway(ProviderHealth::Recovering);
        let mut tracker = Tracker::default();
        for (outcome, expected) in [
            (Outcome::Received, State::Healthy),
            (Outcome::Connected, State::Healthy),
            (Outcome::AuthRequired, State::AuthRequired),
            (Outcome::Forbidden, State::Degraded),
            (Outcome::RateLimited, State::Degraded),
            (Outcome::ServerError, State::Unavailable),
            (Outcome::TransportError, State::Unavailable),
            (Outcome::Redirect, State::Unavailable),
            (Outcome::StreamError, State::Unavailable),
            (Outcome::RequestRejected, State::Degraded),
        ] {
            source.native = Some(Observation {
                outcome,
                observed_at: Some("2026-09-20T00:00:00.000Z".into()),
            });
            let health = tracker.snapshot(DisconnectState::Idle, source.clone());
            assert_eq!(health.components.native_upstream.state, expected);
            assert_eq!(
                health.components.native_upstream.observed_at,
                source.native.as_ref().unwrap().observed_at
            );
            assert_eq!(health.components.web_auth.state, State::Unknown);
            assert_eq!(health.components.codex_app.state, State::Unknown);
            assert_eq!(health.overall, Overall::Preflight);
            assert_ne!(
                health.components.native_upstream.evidence,
                Evidence::RequestSuccess
            );
        }
    }
    #[test]
    fn configuration_observation_changes_revision_without_certifying_other_dimensions() {
        let mut tracker = Tracker::default();
        let source = gateway(ProviderHealth::Verified { observed_at: None });
        let unknown = tracker.snapshot(DisconnectState::Idle, source.clone());
        tracker.observe_configuration(Configuration::Installed);
        tracker.configuration.observed_at = Some("2026-09-20T00:00:00.000Z".into());
        let installed = tracker.snapshot(DisconnectState::Idle, source.clone());
        assert!(installed.revision > unknown.revision);
        assert_eq!(installed.components.config.state, State::Healthy);
        assert_eq!(installed.components.native_upstream.state, State::Unknown);
        assert_eq!(installed.components.codex_app.state, State::Unknown);
        assert_eq!(installed.overall, Overall::Preflight);
        assert_eq!(
            tracker.snapshot(DisconnectState::Idle, source.clone()),
            installed
        );
        let mut busy_source = source.clone();
        busy_source.active_turns = 1;
        let busy = tracker.snapshot(DisconnectState::Idle, busy_source);
        assert_eq!(
            busy.components.config.observed_at,
            installed.components.config.observed_at
        );
        tracker.observe_configuration(Configuration::Conflict);
        let conflict = tracker.snapshot(DisconnectState::Idle, source.clone());
        assert!(conflict.revision > installed.revision);
        assert_eq!(conflict.overall, Overall::ConfigConflict);
        assert_eq!(
            conflict.components.config.code.as_deref(),
            Some("E_CONFIG_CHANGED")
        );
        tracker.observe_configuration(Configuration::Unavailable);
        let unavailable = tracker.snapshot(DisconnectState::Idle, source.clone());
        assert_eq!(unavailable.overall, Overall::Unavailable);
        assert_eq!(
            unavailable.components.config.code.as_deref(),
            Some("E_CONFIG_OBSERVATION")
        );
        let removed = tracker.snapshot(DisconnectState::PendingRestart, source);
        assert_eq!(removed.overall, Overall::RemovalPendingRestart);
        assert_eq!(removed.components.config.state, State::Healthy);
    }
    fn gateway(provider: ProviderHealth) -> GatewayHealth {
        GatewayHealth {
            accepting: true,
            cleanup_failed: false,
            active_turns: 0,
            provider,
            clients: Default::default(),
            native: None,
        }
    }
    #[test]
    fn client_request_evidence_is_independent_and_does_not_certify_picker_or_upstream() {
        let mut tracker = Tracker::default();
        let mut source = gateway(ProviderHealth::Verified {
            observed_at: Some("2026-09-20T00:00:00.000Z".into()),
        });
        source.clients.cli = Some(crate::gateway::ClientRequest {
            succeeded: true,
            observed_at: Some("2026-09-20T00:01:00.000Z".into()),
        });
        let first = tracker.snapshot(DisconnectState::Idle, source.clone());
        assert_eq!(first.components.codex_cli.state, State::Healthy);
        assert_eq!(
            first.components.codex_cli.evidence,
            Evidence::RequestSuccess
        );
        assert_eq!(first.components.codex_app.state, State::Unknown);
        assert_eq!(first.components.native_upstream.state, State::Unknown);
        assert_eq!(first.components.config.state, State::Unknown);
        assert_eq!(first.overall, Overall::Preflight);
        assert_eq!(
            tracker.snapshot(DisconnectState::Idle, source.clone()),
            first
        );
        source.clients.app = Some(crate::gateway::ClientRequest {
            succeeded: false,
            observed_at: Some("2026-09-20T00:02:00.000Z".into()),
        });
        let failed = tracker.snapshot(DisconnectState::Idle, source.clone());
        assert!(failed.revision > first.revision);
        assert_eq!(failed.components.codex_cli, first.components.codex_cli);
        assert_eq!(failed.components.codex_app.state, State::Degraded);
        assert_eq!(
            failed.components.codex_app.code.as_deref(),
            Some("E_CLIENT_WEB_REQUEST")
        );
        source.accepting = false;
        let disconnected = tracker.snapshot(DisconnectState::PendingRestart, source);
        assert_eq!(disconnected.components.codex_cli.state, State::Unknown);
        assert_eq!(disconnected.components.codex_app.state, State::Unknown);
    }
    #[test]
    fn revisions_follow_observed_changes_and_browser_proof_never_certifies_clients() {
        let mut tracker = Tracker::default();
        let pending = gateway(ProviderHealth::Recovering);
        let first = tracker.snapshot(DisconnectState::Idle, pending.clone());
        for _ in 0..20 {
            assert_eq!(
                tracker.snapshot(DisconnectState::Idle, pending.clone()),
                first
            );
        }
        assert_eq!(first.overall, Overall::Preflight);
        assert_eq!(first.components.runtime.state, State::Healthy);
        assert_eq!(first.components.browser.state, State::Unknown);
        let observed = Some("2026-09-20T00:00:00.000Z".into());
        let mut ready = gateway(ProviderHealth::Verified {
            observed_at: observed.clone(),
        });
        let second = tracker.snapshot(DisconnectState::Idle, ready.clone());
        assert_eq!(second.revision, first.revision + 1);
        assert_eq!(second.overall, Overall::Preflight);
        assert_eq!(second.components.web_auth.observed_at, observed);
        assert_eq!(
            second.components.web_auth.evidence,
            Evidence::PassiveBrowser
        );
        for unproven in [
            second.components.native_upstream,
            second.components.codex_app,
            second.components.codex_cli,
            second.components.config,
        ] {
            assert_eq!(unproven.state, State::Unknown);
            assert_eq!(unproven.evidence, Evidence::None);
        }
        ready.active_turns = 2;
        let busy = tracker.snapshot(DisconnectState::Idle, ready);
        assert_eq!(busy.active_web_turns, 2);
        assert_eq!(busy.overall, Overall::Busy);
        assert_eq!(busy.revision, second.revision + 1);
    }
    #[test]
    fn failures_are_sanitized_and_disconnect_states_override_cached_browser_health() {
        for code in [
            "E_BROWSER_BASELINE",
            "E_BROWSER_BASELINE_COMPOSER",
            "E_BROWSER_BASELINE_MODEL",
        ] {
            assert_eq!(failure(code).3, code);
        }
        assert_eq!(
            failure("E_BROWSER_BASELINE_PRIVATE_CONTENT").3,
            "E_WEB_UNAVAILABLE"
        );
        assert_eq!(
            failure("E_BROWSER_TEMPORARY_LOADING").3,
            "E_BROWSER_TEMPORARY_LOADING"
        );
        assert_eq!(
            failure("E_BROWSER_TEMPORARY_PRIVATE_CONTENT").3,
            "E_WEB_UNAVAILABLE"
        );
        for (code, overall, action) in [
            (
                "E_BROWSER_RATE_LIMITED",
                Overall::RateLimited,
                Action::Details,
            ),
            ("E_LOGIN_REQUIRED", Overall::AuthRequired, Action::OpenLogin),
            (
                "E_BROWSER_VERIFICATION_REQUIRED",
                Overall::AuthRequired,
                Action::OpenLogin,
            ),
            (
                "E_BROWSER_VERSION_CHANGED",
                Overall::Incompatible,
                Action::Check,
            ),
            ("E_BROWSER_CLOSED", Overall::Unavailable, Action::Check),
            (
                "PRIVATE_ACCOUNT_TOKEN_URL_PATH",
                Overall::Unavailable,
                Action::Details,
            ),
        ] {
            let mut tracker = Tracker::default();
            let health = tracker.snapshot(
                DisconnectState::Idle,
                gateway(ProviderHealth::Unavailable { code }),
            );
            assert_eq!(health.overall, overall);
            assert_eq!(health.suggested_action, action);
            assert!(
                !serde_json::to_string(&health)
                    .unwrap()
                    .contains("PRIVATE_ACCOUNT_TOKEN_URL_PATH")
            );
        }
        let mut tracker = Tracker::default();
        let mut ready = gateway(ProviderHealth::Verified { observed_at: None });
        ready.accepting = false;
        for (state, overall) in [
            (DisconnectState::Idle, Overall::Disconnected),
            (DisconnectState::Draining, Overall::Disconnecting),
            (DisconnectState::Restoring, Overall::Disconnecting),
            (
                DisconnectState::PendingRestart,
                Overall::RemovalPendingRestart,
            ),
            (DisconnectState::RestoreFailed, Overall::ConfigConflict),
            (DisconnectState::DrainFailed, Overall::Unavailable),
        ] {
            assert_eq!(tracker.snapshot(state, ready.clone()).overall, overall);
        }
        ready.cleanup_failed = true;
        assert_eq!(
            tracker.snapshot(DisconnectState::Idle, ready).overall,
            Overall::Unavailable
        );
    }
}
