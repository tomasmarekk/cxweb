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
    #[serde(default)]
    pub exercise: native_probe::Exercise,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationTarget {
    pub client: PathBuf,
    pub home: PathBuf,
    pub cwd: PathBuf,
    pub route: String,
}

struct PreparedTarget {
    directory: PathBuf,
    config: PathBuf,
    installation: PreparedInstallation,
    session: Option<Arc<GenerationSession>>,
    route: String,
}

struct ActiveTarget {
    target: ActivationTarget,
    installation: String,
    executable: PathBuf,
    activation: crate::host::ActivationHandle,
    applied: bool,
}

#[derive(Default)]
struct State {
    active: Option<ActiveTarget>,
    activation_error: Option<String>,
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

async fn overlay(mut status: ControlStatus, state: &State) -> ControlStatus {
    if let Some(active) = &state.active {
        status.installation = Some(active.installation.clone());
        status.routing_installed = active.applied && active.activation.routing_installed().await;
    }
    status.activation_error = state.activation_error.clone();
    if status.phase == "generation_ready" {
        status.native_text_report = state.report.clone();
    }
    status.native_text_error = state.error.clone();
    status
}

impl LoginBackend for SetupOwner {
    fn activate(&self, target: ActivationTarget) -> LoginWork {
        let control = self.control.clone();
        let state = self.state.clone();
        Box::pin(async move {
            let mut state = state.lock().await;
            state.activation_error = activate(&control, &mut state, target)
                .await
                .err()
                .map(|code| activation_error(code).to_owned());
            Ok(overlay(control.snapshot().await?, &state).await)
        })
    }
    fn request(&self, action: LoginAction) -> LoginWork {
        let work = LoginBackend::request(&self.control, action);
        let state = self.state.clone();
        Box::pin(async move {
            let status = work.await?;
            let mut state = state.lock().await;
            if status.phase != "generation_ready" {
                state.report = None;
                state.error = None;
                if action == LoginAction::ResetTest
                    || state
                        .prepared
                        .as_ref()
                        .and_then(|p| p.session.as_ref())
                        .is_some_and(|s| s.driver.is_closed())
                {
                    state.prepared = None;
                }
            }
            Ok(overlay(status, &state).await)
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
            Ok(overlay(control.snapshot().await?, &state).await)
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
        let root = cxweb_platform::state::StatePaths::installations()
            .map_err(|_| "E_STATE_PERMISSIONS")?;
        let directory = root.join(format!("prepared-{:032x}", rand::random::<u128>()));
        let installation = PreparedInstallation::reserve_with_web_tools(&directory, &config)
            .map_err(|_| "E_NATIVE_TEST_PREPARE")?;
        state.prepared = Some(PreparedTarget {
            directory,
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
    let result = if target.exercise != native_probe::Exercise::Text {
        native_probe::qualify_tools(&target.client, session, target.exercise, cancellation).await
    } else {
        native_probe::qualify_text(&target.client, session, false, cancellation).await
    };
    home_guard
        .verify_unchanged()
        .map_err(|_| "E_PREFLIGHT_TARGET_IDENTITY")?;
    result
}

async fn activate(
    control: &Control,
    state: &mut State,
    target: ActivationTarget,
) -> Result<(), &'static str> {
    if let Some(active) = &state.active {
        if active.target != target {
            return Err("E_ACTIVATION_TARGET_CHANGED");
        }
        if active.applied {
            return Ok(());
        }
    } else {
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
        let report =
            crate::native_preflight::inspect(&target.client, &target.home, &target.cwd).await?;
        if !report.assessment.configuration_compatible {
            return Err("E_ACTIVATION_CONFIG_CONFLICT");
        }
        if report.native_models.is_empty() {
            return Err("E_PREFLIGHT_MODELS");
        }
        let config = target
            .home
            .canonicalize()
            .map_err(|_| "E_PREFLIGHT_TARGET")?
            .join("config.toml");
        if let Some(prepared) = &state.prepared {
            if prepared.config != config || prepared.route != target.route {
                return Err("E_ACTIVATION_TARGET_CHANGED");
            }
        } else {
            let root = cxweb_platform::state::StatePaths::installations()
                .map_err(|_| "E_STATE_PERMISSIONS")?;
            let directory = root.join(format!("prepared-{:032x}", rand::random::<u128>()));
            let installation = PreparedInstallation::reserve_with_web_tools(&directory, &config)
                .map_err(|_| "E_NATIVE_TEST_PREPARE")?;
            state.prepared = Some(PreparedTarget {
                directory,
                config,
                installation,
                session: None,
                route: target.route.clone(),
            });
        }
        let prepared = state.prepared.as_mut().expect("prepared installation");
        let session = control
            .take_generation(
                prepared.installation.installation_id().into(),
                target.route.clone(),
            )
            .await?;
        prepared.session = Some(session.clone());
        let executable = prepared.directory.join("cxweb-daemon.exe");
        // Install a private independent executable, not Cargo's hard-linked build
        // output. The scheduler must keep working after the build tree changes.
        let source = std::env::current_exe().map_err(|_| "E_RUNTIME_PATH")?;
        if source.file_name().and_then(|name| name.to_str()) != Some("cxweb-daemon.exe") {
            return Err("E_RUNTIME_PATH");
        }
        if !executable.exists() {
            std::fs::copy(&source, &executable).map_err(|_| "E_ACTIVATION_INSTALL")?;
        }
        let source_hash = crate::native_preflight::fingerprint(&source).await?;
        if crate::native_preflight::fingerprint(&executable).await? != source_hash {
            return Err("E_ACTIVATION_INSTALL");
        }
        use cxweb_codex_adapter::catalog_codec::CatalogCodec;
        let route = session.route.catalog(true)?;
        // Both reviewed wire formats are served from the same selected home.
        // GUI names and launcher filenames do not determine catalog support.
        let codecs = [CatalogCodec::CliModelInfoV1, CatalogCodec::AppModelInfoV1]
            .into_iter()
            .map(|codec| (codec, vec![route.clone()]))
            .collect();
        let prepared = state.prepared.take().expect("prepared installation");
        let installation = prepared.installation.installation_id().to_owned();
        let (host, activation) = prepared
            .installation
            .bind_generation(&session, codecs, report.native_models)
            .await?;
        tokio::spawn(host.serve());
        state.active = Some(ActiveTarget {
            target,
            installation,
            executable,
            activation,
            applied: false,
        });
    }
    let active = state.active.as_mut().expect("active installation");
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !active.activation.is_serving() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "E_RUNTIME_NOT_READY")?;
    if !active.activation.supervision_registered().await? {
        active
            .activation
            .register_supervisor(active.executable.clone())
            .await?;
    }
    active.activation.apply().await?;
    active.applied = true;
    Ok(())
}

fn activation_error(code: &str) -> &'static str {
    match code {
        "E_ACTIVATION_TARGET_CHANGED" => "E_ACTIVATION_TARGET_CHANGED",
        "E_ACTIVATION_CONFIG_CONFLICT" => "E_ACTIVATION_CONFIG_CONFLICT",
        "E_ACTIVATION_TARGET_PERMISSIONS" => "E_ACTIVATION_TARGET_PERMISSIONS",
        "E_PREFLIGHT_CONFIG_PERMISSIONS" => "E_ACTIVATION_TARGET_PERMISSIONS",
        "E_PREFLIGHT_MODELS" => "E_PREFLIGHT_MODELS",
        "E_CATALOG_CACHE" => "E_CATALOG_CACHE",
        "E_ACTIVATION_INSTALL" => "E_ACTIVATION_INSTALL",
        "E_CONFIG_APPLY" => "E_CONFIG_APPLY",
        "E_SUPERVISION_REGISTER" | "E_SUPERVISION_PLAN" | "E_SUPERVISION_VERIFY" => {
            "E_ACTIVATION_SUPERVISION"
        }
        _ => native_error(code),
    }
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
        "E_NATIVE_PROBE_DENIAL" => "E_NATIVE_PROBE_DENIAL",
        "E_NATIVE_PROBE_TEST" => "E_NATIVE_PROBE_TEST",
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
            "E_NATIVE_PROBE_DENIAL",
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
