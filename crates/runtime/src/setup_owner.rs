//! Daemon-owned preparation and qualification, independent of the control window.
use crate::{
    activation::PreparedInstallation,
    control::{Control, ControlStatus, GenerationSession},
    control_protocol::{LoginAction, LoginBackend, LoginWork},
    native_probe,
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeTarget {
    pub client: PathBuf,
    pub home: PathBuf,
    pub cwd: PathBuf,
    pub route: String,
}

struct PreparedTarget {
    config: PathBuf,
    installation: PreparedInstallation,
    session: Option<Arc<GenerationSession>>,
    route: String,
}

#[derive(Default)]
struct State {
    prepared: Option<PreparedTarget>,
    report: Option<native_probe::Report>,
    error: Option<String>,
}

pub struct SetupOwner {
    control: Control,
    state: Arc<Mutex<State>>,
}
impl SetupOwner {
    pub fn new(control: Control) -> Self {
        Self {
            control,
            state: Arc::default(),
        }
    }
}

fn overlay(mut status: ControlStatus, state: &State) -> ControlStatus {
    if status.phase == "generation_ready" {
        status.native_text_report = state.report.clone();
    }
    status.native_text_error = state.error.clone();
    status
}

impl LoginBackend for SetupOwner {
    fn request(&self, action: LoginAction) -> LoginWork {
        let work = LoginBackend::request(&self.control, action);
        let state = self.state.clone();
        Box::pin(async move {
            let status = work.await?;
            let mut state = state.lock().await;
            if status.phase != "generation_ready" {
                state.report = None;
                state.error = None;
                if state
                    .prepared
                    .as_ref()
                    .and_then(|p| p.session.as_ref())
                    .is_some_and(|s| s.driver.is_closed())
                {
                    state.prepared = None;
                }
            }
            Ok(overlay(status, &state))
        })
    }

    fn native_text(&self, target: NativeTarget, cancellation: CancellationToken) -> LoginWork {
        let control = self.control.clone();
        let state = self.state.clone();
        Box::pin(async move {
            let mut state = state.lock().await;
            state.report = None;
            state.error = None;
            let result = qualify(&control, &mut state, target, cancellation).await;
            match result {
                Ok(report) => state.report = Some(report),
                Err(code) => state.error = Some(native_error(code).into()),
            }
            Ok(overlay(control.snapshot().await?, &state))
        })
    }
}

async fn qualify(
    control: &Control,
    state: &mut State,
    target: NativeTarget,
    cancellation: CancellationToken,
) -> Result<native_probe::Report, &'static str> {
    if cancellation.is_cancelled() {
        return Err("E_NATIVE_PROBE_CANCELLED");
    }
    let status = control.snapshot().await?;
    if !status.background_session
        || !matches!(
            status.phase.as_str(),
            "tool_protocol_qualified" | "generation_ready"
        )
    {
        return Err("E_NATIVE_TEST_BACKGROUND");
    }
    if status.tool_qualified_model.as_deref() != Some(target.route.as_str()) {
        return Err("E_MODEL_SELECTION");
    }
    // Reinspect at execution time; a prior desktop report is not authorization
    // to trust a changed executable or a different effective native home.
    let home_guard = cxweb_platform::target_path::TargetPathGuard::capture(&target.home, true)
        .map_err(|_| "E_PREFLIGHT_TARGET_IDENTITY")?;
    crate::native_preflight::inspect(&target.client, &target.home, &target.cwd).await?;
    home_guard
        .verify_unchanged()
        .map_err(|_| "E_PREFLIGHT_TARGET_IDENTITY")?;
    if cancellation.is_cancelled() {
        return Err("E_NATIVE_PROBE_CANCELLED");
    }
    let config = target
        .home
        .canonicalize()
        .map_err(|_| "E_PREFLIGHT_TARGET")?
        .join("config.toml");
    if let Some(prepared) = &state.prepared {
        if prepared.config != config || prepared.route != target.route {
            return Err("E_NATIVE_TEST_TARGET_CHANGED");
        }
    } else {
        let paths = cxweb_platform::state::StatePaths::open().map_err(|_| "E_STATE_PERMISSIONS")?;
        let directory = paths
            .state
            .join(format!("prepared-{:032x}", rand::random::<u128>()));
        let installation = PreparedInstallation::reserve(&directory, &config)
            .map_err(|_| "E_NATIVE_TEST_PREPARE")?;
        state.prepared = Some(PreparedTarget {
            config,
            installation,
            session: None,
            route: target.route.clone(),
        });
    }
    if cancellation.is_cancelled() {
        return Err("E_NATIVE_PROBE_CANCELLED");
    }
    let prepared = state.prepared.as_mut().expect("prepared target");
    let session = control
        .take_generation(prepared.installation.installation_id().into(), target.route)
        .await?;
    // Retain the browser receipt before any model request, including failures.
    prepared.session = Some(session.clone());
    let result = native_probe::qualify_text(&target.client, session, false, cancellation).await;
    home_guard
        .verify_unchanged()
        .map_err(|_| "E_PREFLIGHT_TARGET_IDENTITY")?;
    result
}

pub(crate) fn native_error(code: &str) -> &'static str {
    match code {
        "E_NATIVE_TEST_BACKGROUND" => "E_NATIVE_TEST_BACKGROUND",
        "E_NATIVE_TEST_TARGET_CHANGED" => "E_NATIVE_TEST_TARGET_CHANGED",
        "E_NATIVE_TEST_PREPARE" => "E_NATIVE_TEST_PREPARE",
        "E_PREFLIGHT_CLIENT_UNQUALIFIED" | "E_NATIVE_PROBE_CLIENT_UNQUALIFIED" => {
            "E_NATIVE_PROBE_CLIENT_UNQUALIFIED"
        }
        "E_PREFLIGHT_TARGET" | "E_PREFLIGHT_TARGET_IDENTITY" | "E_NATIVE_PROBE_TARGET" => {
            "E_NATIVE_PROBE_TARGET"
        }
        "E_NATIVE_PROBE_CANCELLED" => "E_NATIVE_PROBE_CANCELLED",
        "E_NATIVE_PROBE_TIMEOUT" => "E_NATIVE_PROBE_TIMEOUT",
        "E_NATIVE_PROBE_TEXT" => "E_NATIVE_PROBE_TEXT",
        "E_NATIVE_PROBE_GENERATION" => "E_NATIVE_PROBE_GENERATION",
        "E_NATIVE_PROBE_AUTH" | "E_NATIVE_PROBE_CONFIG" => "E_NATIVE_PROBE_CONFIG",
        "E_NATIVE_PROBE_ACTION" => "E_NATIVE_PROBE_ACTION",
        "E_BROWSER_IN_USE" => "E_BROWSER_IN_USE",
        "E_BROWSER_CLOSED" => "E_BROWSER_CLOSED",
        "E_HANDOFF_UNQUALIFIED" | "E_HANDOFF_BACKGROUND_REQUIRED" => "E_NATIVE_TEST_BACKGROUND",
        "E_MODEL_SELECTION" | "E_MODEL_FIDELITY" => "E_MODEL_SELECTION",
        "E_BROWSER_OTHER_PAGES" => "E_BROWSER_OTHER_PAGES",
        "E_BROWSER_BUSY" => "E_BROWSER_BUSY",
        _ => "E_NATIVE_TEST_FAILED",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_failures_are_fixed_codes_and_never_export_backend_text() {
        for code in [
            "E_NATIVE_PROBE_TIMEOUT",
            "E_NATIVE_PROBE_TEXT",
            "E_BROWSER_CLOSED",
            "E_NATIVE_TEST_TARGET_CHANGED",
        ] {
            assert_eq!(native_error(code), code);
        }
        assert_eq!(
            native_error("PRIVATE_ACCOUNT_AND_NATIVE_ERROR"),
            "E_NATIVE_TEST_FAILED"
        );
    }
}
