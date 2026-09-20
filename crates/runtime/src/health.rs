//! Passive private-control health. No browser, filesystem or upstream probes.
use crate::{
    gateway::{GatewayHealth, ProviderHealth},
    lifecycle::DisconnectState,
};
use cxweb_domain::health::{
    Action, Component, ComponentState as State, Components, Evidence, Health, Overall,
};

#[derive(Default)]
pub(crate) struct Tracker {
    previous: Option<(DisconnectState, GatewayHealth, Health)>,
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
    match code {
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
    pub(crate) fn snapshot(&mut self, state: DisconnectState, gateway: GatewayHealth) -> Health {
        if let Some((previous_state, previous_gateway, health)) = &self.previous
            && *previous_state == state
            && *previous_gateway == gateway
        {
            return health.clone();
        }
        let now = cxweb_platform::clock::utc_timestamp();
        let mut health = Health {
            revision: self
                .previous
                .as_ref()
                .map_or(1, |(_, _, h)| h.revision.saturating_add(1)),
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
        if !gateway.accepting {
            health.overall = Overall::Disconnected;
            health.suggested_action = Action::Connect;
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
        self.previous = Some((state, gateway, health.clone()));
        health
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn gateway(provider: ProviderHealth) -> GatewayHealth {
        GatewayHealth {
            accepting: true,
            cleanup_failed: false,
            active_turns: 0,
            provider,
        }
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
        for (code, overall, action) in [
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
