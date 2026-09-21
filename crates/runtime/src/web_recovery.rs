//! Restore only an installed, previously qualified browser binding. Startup
//! rechecks the visible account, route and language without submitting a prompt.
use crate::{
    browser_scope::BrowserScope,
    control::GenerationSession,
    gateway::{WebFuture, WebProvider, WebRequest},
    ledger::Ledger,
    managed_driver::{Binding, ManagedDriver},
    turn::Coordinator,
    web_provider::{CoordinatorProvider, ProviderScope, web_failure},
};
use cxweb_browser_adapter::{LoginObservation, ManagedBrowser};
use cxweb_codex_adapter::catalog_codec::{CatalogCodec, CatalogRoute};
use cxweb_platform::state::{StatePaths, installed_browser};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Receipt {
    version: u32,
    binding: Binding,
    browser_version: String,
    builds: Vec<String>,
    protocol_evidence: String,
}

impl Receipt {
    pub(crate) fn validate_extension(&self, next: &Self) -> Result<(), &'static str> {
        if self.version != next.version
            || self.browser_version != next.browser_version
            || self.builds != next.builds
            || self.protocol_evidence != next.protocol_evidence
        {
            return Err("E_WEB_RECOVERY_RECEIPT");
        }
        self.binding.validate_extension(&next.binding)
    }
    pub(crate) fn new(session: &GenerationSession, codecs: &[CatalogCodec]) -> Self {
        let scope = session.scope();
        Self {
            version: 1,
            binding: Binding {
                installation: scope.installation,
                account: scope.account,
                workspace: scope.workspace,
                epoch: scope.epoch,
                routes: vec![session.route.clone()],
            },
            browser_version: session.browser_version.clone(),
            builds: codecs
                .iter()
                .map(|codec| {
                    match codec {
                        CatalogCodec::Cli01551 => "0.155.1",
                        CatalogCodec::App01550Alpha92 => "0.155.0-alpha.9.2",
                    }
                    .to_owned()
                })
                .collect(),
            protocol_evidence: session.protocol_evidence.clone(),
        }
    }

    pub(crate) fn validate(
        &self,
        installation: &str,
        published: &[String],
    ) -> Result<(), &'static str> {
        let hash = |s: &str| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit());
        if self.version != 1
            || self.binding.installation != installation
            || !hash(&self.binding.account)
            || !hash(&self.binding.workspace)
            || !hash(&self.protocol_evidence)
            || self.browser_version.is_empty()
            || self.browser_version.len() > 256
            || self.browser_version.chars().any(char::is_control)
            || self.binding.routes.len() != 1
            || self.builds.is_empty()
            || self.builds.len() > 16
            || self.builds.iter().collect::<BTreeSet<_>>().len() != self.builds.len()
        {
            return Err("E_WEB_RECOVERY_RECEIPT");
        }
        self.binding.validate()?;
        if published != [self.binding.routes[0].id.clone()] {
            return Err("E_WEB_RECOVERY_RECEIPT");
        }
        for (codec, routes) in self.catalogs()? {
            for route in routes {
                codec.encode(&route)?;
            }
        }
        Ok(())
    }

    fn catalogs(&self) -> Result<Vec<(CatalogCodec, Vec<CatalogRoute>)>, &'static str> {
        self.builds
            .iter()
            .map(|build| {
                let codec = CatalogCodec::for_build(build).ok_or("E_WEB_RECOVERY_RECEIPT")?;
                let routes = self
                    .binding
                    .routes
                    .iter()
                    .map(|route| route.catalog(true))
                    .collect::<Result<Vec<_>, &'static str>>()?;
                Ok((codec, routes))
            })
            .collect()
    }

    async fn restore(
        self,
        directory: PathBuf,
        cancellation: CancellationToken,
    ) -> Result<Restored, &'static str> {
        let catalogs = self.catalogs()?;
        let scope = ProviderScope {
            installation: self.binding.installation.clone(),
            account: self.binding.account.clone(),
            workspace: self.binding.workspace.clone(),
            epoch: self.binding.epoch,
        };
        let routes = self
            .binding
            .routes
            .iter()
            .map(|route| route.id.clone())
            .collect();
        // Browser pipe operations stay off the reactor. A busy profile is a
        // terminal recovery failure, never permission to kill its current owner.
        let browser_cancellation = cancellation.clone();
        let (driver, verified_at) = tokio::task::spawn_blocking(move || {
            let cancellation = browser_cancellation;
            if cancellation.is_cancelled() {
                return Err("E_CANCELLED");
            }
            let paths = StatePaths::open().map_err(|_| "E_STATE_PERMISSIONS")?;
            let ownership = paths.lock().map_err(|_| "E_ALREADY_RUNNING")?;
            let executable = installed_browser().map_err(|_| "E_BROWSER_RUNTIME_MISSING")?;
            let mut browser = ManagedBrowser::launch_offscreen(&executable, &paths.profile)
                .map_err(|_| "E_BROWSER_START")?;
            if browser.version().map_err(|_| "E_BROWSER_OBSERVATION")?["product"]
                != self.browser_version
            {
                return Err("E_BROWSER_VERSION_CHANGED");
            }
            if cancellation.is_cancelled() {
                return Err("E_CANCELLED");
            }
            let page = browser
                .open_background_session()
                .map_err(|_| "E_BACKGROUND_NAVIGATION")?;
            let observation = wait_for_login(
                || {
                    if cancellation.is_cancelled() {
                        return Err("E_CANCELLED");
                    }
                    browser
                        .login_observation(&page)
                        .map_err(|_| "E_BROWSER_OBSERVATION")
                },
                Duration::from_secs(45),
            )?;
            if !page.is_hidden()
                || ![observation.browser_language, observation.page_language]
                    .iter()
                    .all(|value| {
                        value
                            .as_deref()
                            .is_some_and(|v| v == "en" || v.starts_with("en-"))
                    })
            {
                return Err("E_BROWSER_LANGUAGE");
            }
            let baseline = wait_for_baseline(
                || {
                    if cancellation.is_cancelled() {
                        return Err("E_CANCELLED");
                    }
                    browser
                        .baseline(&page)
                        .map_err(|error| baseline_error(&error.to_string()))
                },
                Duration::from_secs(15),
            )?;
            if !baseline.composer_empty || baseline.generating {
                return Err("E_BROWSER_BUSY");
            }
            let surface = browser
                .account_scope(&page)
                .map_err(|error| crate::managed_driver::browser_error(&error, "E_SESSION_SCOPE"))?;
            let current = BrowserScope::from_surface(&self.binding.installation, &surface)?;
            if current.account != self.binding.account
                || current.workspace != self.binding.workspace
            {
                return Err("E_SESSION_SCOPE");
            }
            for route in &self.binding.routes {
                for variant in &route.reasoning {
                    let label = browser
                        .select_candidate(&page, &variant.identity)
                        .map_err(|_| "E_MODEL_SELECTION")?;
                    if label != variant.label {
                        return Err("E_MODEL_SELECTION");
                    }
                }
                let label = browser
                    .select_candidate(&page, &route.identity)
                    .map_err(|_| "E_MODEL_SELECTION")?;
                if label != route.label {
                    return Err("E_MODEL_SELECTION");
                }
            }
            if !browser
                .verify_temporary_chat()
                .map_err(|error| crate::managed_driver::temporary_chat_error(&error.to_string()))?
            {
                return Err("E_TEMPORARY_CHAT");
            }
            let surface = browser
                .account_scope(&page)
                .map_err(|error| crate::managed_driver::browser_error(&error, "E_SESSION_SCOPE"))?;
            let current = BrowserScope::from_surface(&self.binding.installation, &surface)?;
            if current.account != self.binding.account
                || current.workspace != self.binding.workspace
            {
                return Err("E_SESSION_SCOPE");
            }
            if cancellation.is_cancelled() {
                return Err("E_CANCELLED");
            }
            // No draft, prompt, submission or account-setting mutation above.
            browser
                .close_page_checked(&page)
                .map_err(|_| "E_BROWSER_RELEASE")?;
            browser
                .ensure_no_other_pages(None)
                .map_err(|_| "E_BROWSER_OTHER_PAGES")?;
            let verified_at = cxweb_platform::clock::utc_timestamp();
            Ok((
                ManagedDriver::start(browser, self.binding, ownership)?,
                verified_at,
            ))
        })
        .await
        .map_err(|_| "E_WEB_RECOVERY_WORKER")??;
        let result = async {
            if cancellation.is_cancelled() {
                return Err("E_CANCELLED");
            }
            let ledger = Ledger::open(&directory.join("turns.sqlite")).await?;
            // Reuse the original scope and ledger so completed deliveries replay
            // and interrupted submissions remain uncertain instead of resending.
            let coordinator = Coordinator::new(ledger, Arc::new(driver.clone()));
            let provider =
                CoordinatorProvider::new(coordinator, scope, routes)?.with_catalog(1, catalogs)?;
            Ok((
                Arc::new(ObservedProvider::new(
                    Arc::new(provider.clone()),
                    driver.clone(),
                    verified_at,
                )) as Arc<dyn WebProvider>,
                provider,
            ))
        }
        .await;
        match result {
            Ok((provider, coordinator)) => Ok(Restored {
                provider,
                driver,
                coordinator,
            }),
            Err(code) => {
                driver
                    .shutdown()
                    .await
                    .map_err(|_| "E_WEB_RECOVERY_CLEANUP")?;
                Err(code)
            }
        }
    }
}

/// A new background target may still be navigating or hydrating. Reobserve
/// that same target within a deadline; never open another tab or resubmit work.
fn wait_for_login(
    mut observe: impl FnMut() -> Result<LoginObservation, &'static str>,
    timeout: Duration,
) -> Result<LoginObservation, &'static str> {
    let deadline = Instant::now() + timeout;
    loop {
        let observation = match observe() {
            Ok(observation) => observation,
            // A navigation can replace the document between the frame/origin
            // check and a DOM read. Reobserve this same owned target within the
            // original deadline; do not navigate again or retry generation.
            Err("E_BROWSER_OBSERVATION") if Instant::now() < deadline => {
                std::thread::sleep(
                    Duration::from_millis(100)
                        .min(deadline.saturating_duration_since(Instant::now())),
                );
                continue;
            }
            Err(code) => return Err(code),
        };
        if observation.verification_required {
            return Err("E_BROWSER_VERIFICATION_REQUIRED");
        }
        if crate::control::authenticated_surface(&observation) {
            return Ok(observation);
        }
        if Instant::now() >= deadline {
            // ChatGPT can render the signed-out shell before restoring its
            // saved session. Observe the same page until the startup deadline;
            // never click login or open another page to make recovery succeed.
            return Err(if observation.login_action {
                "E_LOGIN_REQUIRED"
            } else {
                "E_BACKGROUND_NAVIGATION"
            });
        }
        std::thread::sleep(
            Duration::from_millis(100).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
}

/// Retain the actual browser-owner channel for passive liveness observation.
/// The timestamp records the last account/model verification, not a new probe.
pub(crate) struct ObservedProvider {
    provider: Arc<dyn WebProvider>,
    driver: ManagedDriver,
    observation: Arc<std::sync::Mutex<crate::gateway::ProviderHealth>>,
}
impl ObservedProvider {
    pub(crate) fn new(
        provider: Arc<dyn WebProvider>,
        driver: ManagedDriver,
        verified_at: Option<String>,
    ) -> Self {
        Self {
            provider,
            driver,
            observation: Arc::new(std::sync::Mutex::new(
                crate::gateway::ProviderHealth::Verified {
                    observed_at: verified_at,
                },
            )),
        }
    }
}
impl WebProvider for ObservedProvider {
    fn health(&self) -> crate::gateway::ProviderHealth {
        if self.driver.is_closed() {
            crate::gateway::ProviderHealth::Unavailable {
                code: "E_BROWSER_CLOSED",
            }
        } else {
            self.observation
                .lock()
                .expect("browser health lock poisoned")
                .clone()
        }
    }
    fn respond(&self, request: WebRequest) -> WebFuture {
        let work = self.provider.respond(request);
        let observation = self.observation.clone();
        Box::pin(async move {
            let response = work.await;
            observe_response(&observation, &response);
            response
        })
    }
    fn validate_warmup(&self, request: &WebRequest) -> Result<(), &'static str> {
        self.provider.validate_warmup(request)
    }
    fn catalog(&self, codec: CatalogCodec) -> Option<crate::catalog_proxy::OwnedCatalog> {
        if self.driver.is_closed() {
            None
        } else {
            self.provider.catalog(codec)
        }
    }
}

fn observe_response(
    observation: &std::sync::Mutex<crate::gateway::ProviderHealth>,
    response: &axum::response::Response,
) {
    use crate::{gateway::ProviderHealth, web_provider::BrowserEvidence};
    let changed = match response.extensions().get::<BrowserEvidence>() {
        Some(BrowserEvidence::Verified) => Some(ProviderHealth::Verified {
            observed_at: cxweb_platform::clock::utc_timestamp(),
        }),
        Some(BrowserEvidence::Failure(code))
            if code.starts_with("E_BROWSER_")
                || matches!(
                    *code,
                    "E_SESSION_SCOPE"
                        | "E_LOGIN_REQUIRED"
                        | "E_MODEL_SELECTION"
                        | "E_MODEL_FIDELITY"
                        | "E_TEMPORARY_CHAT"
                ) =>
        {
            Some(ProviderHealth::Unavailable { code })
        }
        _ => None,
    };
    if let Some(changed) = changed {
        *observation.lock().expect("browser health lock poisoned") = changed;
    }
}

struct Restored {
    provider: Arc<dyn WebProvider>,
    driver: ManagedDriver,
    coordinator: CoordinatorProvider,
}

type RecoveryResult = Result<Arc<dyn WebProvider>, &'static str>;
/// Native forwarding starts immediately. Web work and publication remain
/// unavailable until a non-generative recovery attempt finishes. Only explicit
/// retries of a completed transient startup failure may replace that result.
#[derive(Clone, Default)]
pub(crate) struct PendingProvider(
    Arc<Mutex<Option<RecoveryResult>>>,
    Arc<Mutex<Option<(ManagedDriver, CoordinatorProvider)>>>,
);
impl PendingProvider {
    fn begin_retry(&self) -> Result<(), &'static str> {
        let mut state = self.0.lock().map_err(|_| "E_WEB_RECOVERY_STATE")?;
        if !matches!(state.as_ref(), Some(Err(code)) if retryable(code)) {
            return Err("E_WEB_RECOVERY_NOT_RETRYABLE");
        }
        *state = None;
        Ok(())
    }
    pub(crate) async fn restore(
        &self,
        receipt: Receipt,
        directory: PathBuf,
        cancellation: CancellationToken,
    ) -> Result<(), &'static str> {
        match receipt.restore(directory, cancellation.clone()).await {
            Ok(restored) => {
                *self.1.lock().map_err(|_| "E_WEB_RECOVERY_STATE")? =
                    Some((restored.driver.clone(), restored.coordinator));
                if !self.finish(Ok(restored.provider), &cancellation) {
                    // Retain the recovery drain lease until the browser and its
                    // profile lock are actually released, even if the UI left.
                    restored
                        .driver
                        .shutdown()
                        .await
                        .map_err(|_| "E_WEB_RECOVERY_CLEANUP")?;
                }
            }
            Err(code) => {
                self.finish(Err(code), &cancellation);
                if code == "E_WEB_RECOVERY_CLEANUP" {
                    return Err(code);
                }
            }
        }
        Ok(())
    }
    fn finish(&self, result: RecoveryResult, cancellation: &CancellationToken) -> bool {
        let result = if cancellation.is_cancelled() {
            Err("E_CANCELLED")
        } else {
            result
        };
        let ready = result.is_ok();
        let mut state = self.0.lock().expect("recovery state lock poisoned");
        if state.is_some() {
            return false;
        }
        *state = Some(result);
        ready
    }
    fn ready(&self) -> RecoveryResult {
        self.0
            .lock()
            .expect("recovery state lock poisoned")
            .clone()
            .unwrap_or(Err("E_WEB_RECOVERING"))
    }
}
impl WebProvider for PendingProvider {
    fn health(&self) -> crate::gateway::ProviderHealth {
        match self
            .0
            .lock()
            .expect("recovery state lock poisoned")
            .as_ref()
        {
            None => crate::gateway::ProviderHealth::Recovering,
            Some(Ok(provider)) => provider.health(),
            Some(Err(code)) => crate::gateway::ProviderHealth::Unavailable { code },
        }
    }

    fn respond(&self, request: WebRequest) -> WebFuture {
        match self.ready() {
            Ok(provider) => provider.respond(request),
            Err(code) => Box::pin(async move { web_failure(code) }),
        }
    }
    fn validate_warmup(&self, request: &WebRequest) -> Result<(), &'static str> {
        self.ready()?.validate_warmup(request)
    }
    fn catalog(&self, codec: CatalogCodec) -> Option<crate::catalog_proxy::OwnedCatalog> {
        self.ready().ok()?.catalog(codec)
    }
}

fn wait_for_baseline<T>(
    mut observe: impl FnMut() -> Result<T, &'static str>,
    timeout: Duration,
) -> Result<T, &'static str> {
    let deadline = Instant::now() + timeout;
    loop {
        match observe() {
            Err("E_BROWSER_BASELINE_MODEL" | "E_BROWSER_BASELINE_COMPOSER")
                if Instant::now() < deadline =>
            {
                std::thread::sleep(
                    Duration::from_millis(100)
                        .min(deadline.saturating_duration_since(Instant::now())),
                );
            }
            result => return result,
        }
    }
}

pub(crate) fn baseline_error(code: &str) -> &'static str {
    match code {
        "E_BROWSER_BASELINE_COMPOSER" => "E_BROWSER_BASELINE_COMPOSER",
        "E_BROWSER_BASELINE_MODEL" => "E_BROWSER_BASELINE_MODEL",
        _ => "E_BROWSER_BASELINE",
    }
}

fn retryable(code: &str) -> bool {
    matches!(
        code,
        "E_ALREADY_RUNNING"
            | "E_BROWSER_RUNTIME_MISSING"
            | "E_BROWSER_START"
            | "E_BACKGROUND_NAVIGATION"
            | "E_BROWSER_OBSERVATION"
    )
}

/// The installed host owns the original binding and retry policy. A desktop
/// request cannot supply a replacement account, route, profile or browser path.
#[derive(Clone)]
pub(crate) struct RecoveryController {
    pending: PendingProvider,
    receipt: Arc<Mutex<Receipt>>,
    directory: PathBuf,
    login: Arc<Mutex<Option<InstalledLogin>>>,
}
struct InstalledLogin {
    browser: ManagedBrowser,
    page: Option<cxweb_browser_adapter::ManagedPage>,
    _ownership: std::fs::File,
}
impl RecoveryController {
    pub(crate) async fn web_login(
        &self,
        finish: bool,
        cancel: CancellationToken,
    ) -> Result<(), &'static str> {
        if cancel.is_cancelled() {
            return Err("E_CANCELLED");
        }
        if finish {
            if !matches!(
                self.pending.health(),
                crate::gateway::ProviderHealth::Unavailable {
                    code: "E_LOGIN_WINDOW_OPEN"
                }
            ) {
                return Err("E_WEB_RECOVERY_STATE");
            }
            self.release_login().await?;
            *self.pending.0.lock().map_err(|_| "E_WEB_RECOVERY_STATE")? = None;
            let receipt = self
                .receipt
                .lock()
                .map_err(|_| "E_WEB_RECOVERY_STATE")?
                .clone();
            return self
                .pending
                .restore(receipt, self.directory.clone(), cancel)
                .await;
        }
        let code = match self.pending.health() {
            crate::gateway::ProviderHealth::Unavailable { code } => code,
            _ => return Err("E_WEB_RECOVERY_STATE"),
        };
        if !matches!(
            code,
            "E_LOGIN_REQUIRED"
                | "E_BROWSER_VERIFICATION_REQUIRED"
                | "E_SESSION_SCOPE"
                | "E_LOGIN_WINDOW_OPEN"
        ) {
            return Err("E_WEB_RECOVERY_STATE");
        }
        let driver = self
            .pending
            .1
            .lock()
            .map_err(|_| "E_WEB_RECOVERY_STATE")?
            .clone();
        if let Some((driver, _)) = driver {
            driver.verify_idle().await?;
            driver.shutdown().await?;
            *self.pending.1.lock().map_err(|_| "E_WEB_RECOVERY_STATE")? = None;
        }
        let login = self.login.clone();
        tokio::task::spawn_blocking(move || {
            let mut session = login.lock().map_err(|_| "E_WEB_RECOVERY_STATE")?;
            if session.is_some() {
                return Ok(());
            }
            if cancel.is_cancelled() {
                return Err("E_CANCELLED");
            }
            let paths = StatePaths::open().map_err(|_| "E_STATE_PERMISSIONS")?;
            let ownership = paths.lock().map_err(|_| "E_ALREADY_RUNNING")?;
            let executable = installed_browser().map_err(|_| "E_BROWSER_RUNTIME_MISSING")?;
            let browser = ManagedBrowser::launch(&executable, &paths.profile, true)
                .map_err(|_| "E_BROWSER_START")?;
            // Retain ownership even if opening the page has an uncertain outcome.
            *session = Some(InstalledLogin {
                browser,
                page: None,
                _ownership: ownership,
            });
            let session = session.as_mut().ok_or("E_WEB_RECOVERY_STATE")?;
            session.page = Some(
                session
                    .browser
                    .open_login()
                    .map_err(|_| "E_BROWSER_RELEASE")?,
            );
            Ok(())
        })
        .await
        .map_err(|_| "E_WEB_RECOVERY_WORKER")??;
        *self.pending.0.lock().map_err(|_| "E_WEB_RECOVERY_STATE")? =
            Some(Err("E_LOGIN_WINDOW_OPEN"));
        Ok(())
    }

    pub(crate) async fn release_login(&self) -> Result<(), &'static str> {
        let login = self.login.clone();
        tokio::task::spawn_blocking(move || {
            let mut holder = login.lock().map_err(|_| "E_WEB_RECOVERY_STATE")?;
            if let Some(session) = holder.as_mut() {
                if !session
                    .browser
                    .has_exited()
                    .map_err(|_| "E_BROWSER_RELEASE")?
                {
                    if let Some(page) = session.page.as_ref()
                        && session
                            .browser
                            .page_exists(page)
                            .map_err(|_| "E_BROWSER_RELEASE")?
                    {
                        return Err("E_LOGIN_WINDOW_OPEN");
                    }
                    // Never close another user-opened tab to obtain the profile.
                    session
                        .browser
                        .close_for_replacement(None)
                        .map_err(|_| "E_BROWSER_RELEASE")?;
                }
                *holder = None;
            }
            Ok(())
        })
        .await
        .map_err(|_| "E_WEB_RECOVERY_WORKER")?
    }
    pub(crate) async fn qualify_protocol(
        &self,
        target: crate::protocol_qualification::Target,
        cancel: CancellationToken,
    ) -> Result<(), &'static str> {
        self.pending.ready()?;
        let (driver, _) = self
            .pending
            .1
            .lock()
            .map_err(|_| "E_WEB_RECOVERY_STATE")?
            .clone()
            .ok_or("E_WEB_RECOVERY_STATE")?;
        driver.verify_idle().await?;
        let result = driver
            .qualify_protocol(self.directory.clone(), target, cancel)
            .await;
        driver.verify_idle().await?;
        result
    }
    pub(crate) async fn verify_compaction(
        &self,
        target: Option<crate::native_probe::CheckpointTarget>,
        cancel: CancellationToken,
    ) -> Result<(), &'static str> {
        self.pending.ready()?;
        let (driver, coordinator) = self
            .pending
            .1
            .lock()
            .map_err(|_| "E_WEB_RECOVERY_STATE")?
            .clone()
            .ok_or("E_WEB_RECOVERY_STATE")?;
        let receipt = self
            .receipt
            .lock()
            .map_err(|_| "E_WEB_RECOVERY_STATE")?
            .clone();
        driver.verify_idle().await?;
        if let Some(target) = target {
            let result = crate::native_probe::qualify_installed_checkpoint(
                &target,
                &driver,
                &receipt.binding,
                &self.directory,
                cancel,
            )
            .await;
            // Any uncertain cleanup must keep the production admission gate closed.
            driver.verify_idle().await?;
            return result;
        }
        // The probe shares the browser owner but never publishes its checkpoint codec.
        // It verifies cleanup before allowing normal web admission to resume.
        crate::compaction_probe::run(
            coordinator,
            &driver,
            &receipt.binding,
            &self.directory,
            cancel,
        )
        .await
    }
    pub(crate) fn reasoning_status(&self) -> Option<Vec<crate::control_protocol::ReasoningFamily>> {
        // Read the actually published catalog. Discovery and generation remain
        // explicit operations; status never touches the browser or journal.
        let provider = self.pending.ready().ok()?;
        let catalog = provider
            .catalog(CatalogCodec::Cli01551)
            .or_else(|| provider.catalog(CatalogCodec::App01550Alpha92))?;
        catalog
            .entries
            .iter()
            .map(|entry| {
                Some(crate::control_protocol::ReasoningFamily {
                    model: entry.get("slug")?.as_str()?.into(),
                    name: entry.get("display_name")?.as_str()?.into(),
                    levels: serde_json::from_value(
                        entry.get("supported_reasoning_levels")?.clone(),
                    )
                    .ok()?,
                })
            })
            .collect()
    }
    pub(crate) fn new(pending: PendingProvider, receipt: Receipt, directory: PathBuf) -> Self {
        Self {
            pending,
            receipt: Arc::new(Mutex::new(receipt)),
            directory,
            login: Arc::default(),
        }
    }
    pub(crate) async fn retry(
        &self,
        gateway: &crate::gateway::Gateway,
    ) -> Result<(), &'static str> {
        self.pending.begin_retry()?;
        let receipt = self
            .receipt
            .lock()
            .map_err(|_| "E_WEB_RECOVERY_STATE")?
            .clone();
        let mut admitted = false;
        let result = gateway
            .recover_web(|cancel| {
                admitted = true;
                self.pending
                    .restore(receipt, self.directory.clone(), cancel)
            })
            .await;
        if !admitted {
            self.pending
                .finish(Err("E_WEB_DISCONNECTED"), &CancellationToken::new());
            return Err("E_WEB_DISCONNECTED");
        }
        result
    }

    pub(crate) async fn qualify_reasoning(
        &self,
        journal: Arc<Mutex<crate::config_journal::ConfigJournal>>,
        cancel: CancellationToken,
    ) -> Result<(), &'static str> {
        self.pending.ready()?;
        let (driver, coordinator) = self
            .pending
            .1
            .lock()
            .map_err(|_| "E_WEB_RECOVERY_STATE")?
            .clone()
            .ok_or("E_WEB_RECOVERY_STATE")?;
        let previous = self
            .receipt
            .lock()
            .map_err(|_| "E_WEB_RECOVERY_STATE")?
            .clone();
        let binding = driver
            .qualify_reasoning(self.directory.clone(), cancel.clone())
            .await?;
        if cancel.is_cancelled() {
            return Err("E_CANCELLED");
        }
        let mut next = previous.clone();
        next.binding = binding;
        previous.validate_extension(&next)?;
        let coordinator = coordinator.with_refreshed_catalog(next.catalogs()?)?;
        let provider = Arc::new(ObservedProvider::new(
            Arc::new(coordinator.clone()),
            driver.clone(),
            cxweb_platform::clock::utc_timestamp(),
        )) as Arc<dyn WebProvider>;
        let saved = next.clone();
        tokio::task::spawn_blocking(move || {
            journal
                .lock()
                .map_err(|_| "E_INTEGRATION_STATE")?
                .extend_web(&previous, &saved)
                .map_err(|_| "E_WEB_RECOVERY_RECEIPT")
        })
        .await
        .map_err(|_| "E_REASONING_WORKER")??;
        *self.receipt.lock().map_err(|_| "E_WEB_RECOVERY_STATE")? = next.clone();
        if let Err(code) = driver.adopt_binding(next.binding).await {
            *self.pending.0.lock().map_err(|_| "E_WEB_RECOVERY_STATE")? = Some(Err(code));
            return Err(code);
        }
        *self.pending.1.lock().map_err(|_| "E_WEB_RECOVERY_STATE")? = Some((driver, coordinator));
        *self.pending.0.lock().map_err(|_| "E_WEB_RECOVERY_STATE")? = Some(Ok(provider));
        Ok(())
    }
}

#[cfg(test)]
impl Receipt {
    pub(crate) fn fixture(installation: &str) -> Self {
        Self {
            version: 1,
            binding: Binding {
                installation: installation.into(),
                account: "a".repeat(64),
                workspace: "b".repeat(64),
                epoch: 0,
                routes: vec![crate::managed_driver::Route {
                    id: "webbridge/fixture".into(),
                    identity: "fixture".into(),
                    label: "Fixture High".into(),
                    reasoning: vec![],
                    effort: Some("high".into()),
                }],
            },
            browser_version: "Chrome/fixture".into(),
            builds: vec!["0.155.1".into(), "0.155.0-alpha.9.2".into()],
            protocol_evidence: "c".repeat(64),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{gateway::Gateway, native::NativeTransport};
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
        response::IntoResponse,
    };
    use serde_json::{Value, json};
    use std::time::Duration;
    use tower::ServiceExt;

    #[test]
    fn recovery_receipt_binds_scope_version_route_and_reviewed_catalogs() {
        let receipt = Receipt::fixture("installation");
        let published = vec!["webbridge/fixture".into()];
        receipt.validate("installation", &published).unwrap();
        assert!(receipt.validate("other", &published).is_err());
        assert!(
            receipt
                .validate("installation", &["webbridge/other".into()])
                .is_err()
        );
        let original = serde_json::to_value(receipt).unwrap();
        for (pointer, value) in [
            ("/version", json!(2)),
            ("/binding/account", json!("raw-account")),
            ("/binding/workspace", json!("")),
            ("/browser_version", json!("")),
            ("/protocol_evidence", Value::Null),
            ("/builds", json!(["unreviewed"])),
            ("/builds", json!(["0.155.1", "0.155.1"])),
            ("/binding/routes/0/effort", json!("unknown")),
            ("/binding/routes/0/id", json!("native")),
        ] {
            let mut changed = original.clone();
            *changed.pointer_mut(pointer).unwrap() = value;
            assert!(
                serde_json::from_value::<Receipt>(changed)
                    .map_or(true, |r| r.validate("installation", &published).is_err())
            );
        }
        let mut changed = original;
        changed["profile_path"] = json!("C:/another-browser");
        assert!(serde_json::from_value::<Receipt>(changed).is_err());
    }

    #[test]
    fn baseline_hydration_is_bounded_and_cancellation_is_not_retried() {
        let mut observations = [
            Err("E_BROWSER_BASELINE_COMPOSER"),
            Err("E_BROWSER_BASELINE_MODEL"),
            Ok(7),
        ]
        .into_iter();
        assert_eq!(
            wait_for_baseline(|| observations.next().unwrap(), Duration::from_secs(1)),
            Ok(7)
        );
        for code in ["E_CANCELLED", "E_BROWSER_BASELINE", "E_BROWSER_BUSY"] {
            let mut calls = 0;
            assert_eq!(
                wait_for_baseline::<()>(
                    || {
                        calls += 1;
                        Err(code)
                    },
                    Duration::from_secs(1)
                ),
                Err(code)
            );
            assert_eq!(calls, 1);
        }
        assert_eq!(
            wait_for_baseline::<()>(|| Err("E_BROWSER_BASELINE_MODEL"), Duration::ZERO),
            Err("E_BROWSER_BASELINE_MODEL")
        );
        assert_eq!(baseline_error("private page content"), "E_BROWSER_BASELINE");
    }

    #[test]
    fn recovery_waits_for_session_hydration_without_interacting_with_login_or_challenges() {
        let loaded = LoginObservation {
            verification_required: false,
            document_ready: true,
            browser_language: Some("en-US".into()),
            page_language: Some("en".into()),
            official_page: true,
            composer: true,
            account_surface: true,
            login_action: false,
            selected_label: Some("Fixture".into()),
        };
        let mut loading = loaded.clone();
        loading.document_ready = false;
        loading.composer = false;
        loading.account_surface = false;
        let mut calls = 0;
        let ready = wait_for_login(
            || {
                calls += 1;
                Ok(if calls == 1 {
                    loading.clone()
                } else {
                    loaded.clone()
                })
            },
            Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(calls, 2);
        assert_eq!(ready, loaded);
        let mut signed_out_shell = loading.clone();
        signed_out_shell.document_ready = true;
        signed_out_shell.login_action = true;
        let mut calls = 0;
        assert_eq!(
            wait_for_login(
                || {
                    calls += 1;
                    Ok(if calls == 1 {
                        signed_out_shell.clone()
                    } else {
                        loaded.clone()
                    })
                },
                Duration::from_secs(1)
            )
            .unwrap(),
            loaded
        );
        assert_eq!(calls, 2);
        let mut reads = 0;
        assert_eq!(
            wait_for_login(
                || {
                    reads += 1;
                    if reads == 1 {
                        Err("E_BROWSER_OBSERVATION")
                    } else {
                        Ok(loaded.clone())
                    }
                },
                Duration::from_secs(1)
            )
            .unwrap(),
            loaded
        );
        assert_eq!(reads, 2);
        assert_eq!(
            wait_for_login(|| Err("E_BROWSER_OBSERVATION"), Duration::ZERO),
            Err("E_BROWSER_OBSERVATION")
        );
        assert_eq!(
            wait_for_login(|| Ok(loading.clone()), Duration::ZERO),
            Err("E_BACKGROUND_NAVIGATION")
        );
        for (challenge, code) in [
            (false, "E_LOGIN_REQUIRED"),
            (true, "E_BROWSER_VERIFICATION_REQUIRED"),
        ] {
            let mut state = loading.clone();
            state.verification_required = challenge;
            state.login_action = !challenge;
            let mut calls = 0;
            assert_eq!(
                wait_for_login(
                    || {
                        calls += 1;
                        Ok(state.clone())
                    },
                    Duration::ZERO
                ),
                Err(code)
            );
            assert_eq!(calls, 1);
        }
        assert_eq!(
            wait_for_login(|| Err("E_CANCELLED"), Duration::from_secs(1)),
            Err("E_CANCELLED")
        );
    }

    struct Ready;
    #[test]
    fn only_fresh_browser_evidence_changes_cached_health() {
        use crate::{gateway::ProviderHealth, web_provider::BrowserEvidence};
        let observation = std::sync::Mutex::new(ProviderHealth::Verified {
            observed_at: Some("2026-09-20T00:00:00.000Z".into()),
        });
        observe_response(&observation, &web_failure("E_SESSION_SCOPE"));
        let failed = ProviderHealth::Unavailable {
            code: "E_SESSION_SCOPE",
        };
        assert_eq!(*observation.lock().unwrap(), failed);
        // Cached delivery and request-validation failures do not reverify the
        // browser or erase a previously observed account-scope failure.
        observe_response(&observation, &StatusCode::OK.into_response());
        observe_response(&observation, &web_failure("E_REQUEST_IDENTITY"));
        assert_eq!(*observation.lock().unwrap(), failed);
        let mut verified = StatusCode::OK.into_response();
        verified.extensions_mut().insert(BrowserEvidence::Verified);
        observe_response(&observation, &verified);
        assert!(matches!(
            &*observation.lock().unwrap(),
            ProviderHealth::Verified {
                observed_at: Some(_)
            }
        ));
        observe_response(&observation, &web_failure("E_BROWSER_CLOSED"));
        assert_eq!(
            *observation.lock().unwrap(),
            ProviderHealth::Unavailable {
                code: "E_BROWSER_CLOSED"
            }
        );
    }

    impl WebProvider for Ready {
        fn respond(&self, _: WebRequest) -> WebFuture {
            Box::pin(async { "recovered web".into_response() })
        }
    }
    async fn body(gateway: &Gateway, model: &str) -> String {
        let request = Request::builder()
            .method("POST")
            .uri(format!("{}/responses", gateway.base_url()))
            .header("host", "127.0.0.1:43127")
            .body(Body::from(json!({"model":model}).to_string()))
            .unwrap();
        let response = gateway.clone().router().oneshot(request).await.unwrap();
        assert_ne!(response.status(), StatusCode::NOT_FOUND);
        String::from_utf8(to_bytes(response.into_body(), 4096).await.unwrap().to_vec()).unwrap()
    }

    #[tokio::test]
    async fn native_forwarding_survives_recovery_and_disconnect_waits_for_its_cleanup() {
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let native =
            NativeTransport::new(format!("http://{}", upstream.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(
            axum::serve(
                upstream,
                axum::Router::new().route(
                    "/responses",
                    axum::routing::post(|| async { "native response" }),
                ),
            )
            .into_future(),
        );
        for (cancelled, retried) in [(false, false), (true, false), (false, true), (true, true)] {
            let pending = PendingProvider::default();
            if retried {
                pending.finish(Err("E_ALREADY_RUNNING"), &CancellationToken::new());
                pending.begin_retry().unwrap();
            }
            let gateway = Gateway::prepared(
                43127,
                &"a".repeat(43),
                native.clone(),
                Arc::new(pending.clone()),
            );
            let entered = Arc::new(tokio::sync::Notify::new());
            let release = Arc::new(tokio::sync::Notify::new());
            let recovery = tokio::spawn({
                let gateway = gateway.clone();
                let pending = pending.clone();
                let entered = entered.clone();
                let release = release.clone();
                async move {
                    gateway
                        .recover_web(|cancel| async move {
                            entered.notify_one();
                            release.notified().await;
                            pending.finish(Ok(Arc::new(Ready)), &cancel);
                            Ok(())
                        })
                        .await
                }
            });
            entered.notified().await;
            assert_eq!(gateway.health().active_turns, 0);
            assert_eq!(pending.health(), crate::gateway::ProviderHealth::Recovering);
            assert_eq!(body(&gateway, "native").await, "native response");
            assert!(
                body(&gateway, "webbridge/fixture")
                    .await
                    .contains("E_WEB_RECOVERING")
            );
            assert!(pending.catalog(CatalogCodec::Cli01551).is_none());
            if cancelled {
                assert_eq!(
                    gateway.disconnect_web(Duration::from_millis(20)).await,
                    Err("E_WEB_DRAIN_TIMEOUT")
                );
                assert!(!recovery.is_finished());
            }
            release.notify_one();
            recovery.await.unwrap().unwrap();
            if cancelled {
                gateway
                    .disconnect_web(Duration::from_secs(1))
                    .await
                    .unwrap();
                assert!(
                    body(&gateway, "webbridge/fixture")
                        .await
                        .contains("E_WEB_DISCONNECTED")
                );
                assert!(matches!(pending.ready(), Err("E_CANCELLED")));
                gateway
                    .recover_web(|_| async { panic!("disconnected recovery must not start") })
                    .await
                    .unwrap();
            } else {
                assert_eq!(body(&gateway, "webbridge/fixture").await, "recovered web");
            }
            assert_eq!(body(&gateway, "native").await, "native response");
        }
        server.abort();
        let _ = server.await;
    }

    #[tokio::test]
    async fn unconfirmed_recovery_cleanup_prevents_successful_disconnect() {
        let gateway = Gateway::prepared(
            43127,
            &"a".repeat(43),
            NativeTransport::new("http://127.0.0.1:1".into()).unwrap(),
            Arc::new(PendingProvider::default()),
        );
        assert_eq!(
            gateway
                .recover_web(|_| async { Err("E_WEB_RECOVERY_CLEANUP") })
                .await,
            Err("E_WEB_RECOVERY_CLEANUP")
        );
        assert_eq!(
            gateway.disconnect_web(Duration::from_secs(1)).await,
            Err("E_WEB_CLEANUP_UNCONFIRMED")
        );
    }

    #[tokio::test]
    async fn disconnect_winning_retry_admission_never_starts_a_browser() {
        let pending = PendingProvider::default();
        pending.finish(Err("E_ALREADY_RUNNING"), &CancellationToken::new());
        let gateway = Gateway::prepared(
            43127,
            &"a".repeat(43),
            NativeTransport::new("http://127.0.0.1:1".into()).unwrap(),
            Arc::new(pending.clone()),
        );
        gateway
            .disconnect_web(Duration::from_secs(1))
            .await
            .unwrap();
        let recovery = RecoveryController::new(
            pending.clone(),
            Receipt::fixture("installation"),
            PathBuf::new(),
        );
        assert_eq!(recovery.retry(&gateway).await, Err("E_WEB_DISCONNECTED"));
        assert!(matches!(pending.ready(), Err("E_WEB_DISCONNECTED")));
        assert!(!gateway.health().cleanup_failed);
    }

    #[tokio::test]
    async fn cancelled_recovery_does_not_open_a_browser_or_profile() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let pending = PendingProvider::default();
        pending
            .restore(Receipt::fixture("installation"), PathBuf::new(), cancel)
            .await
            .unwrap();
        assert!(matches!(pending.ready(), Err("E_CANCELLED")));
    }
    #[tokio::test]
    async fn installed_login_refuses_non_auth_states_and_cancelled_operations() {
        for code in [
            "E_BROWSER_RATE_LIMITED",
            "E_MODEL_SELECTION",
            "E_BROWSER_START",
            "E_BROWSER_CLOSED",
        ] {
            let pending = PendingProvider::default();
            pending.finish(Err(code), &CancellationToken::new());
            let recovery = RecoveryController::new(
                pending.clone(),
                Receipt::fixture("installation"),
                PathBuf::new(),
            );
            for finish in [false, true] {
                assert_eq!(
                    recovery.web_login(finish, CancellationToken::new()).await,
                    Err("E_WEB_RECOVERY_STATE")
                );
            }
            assert!(recovery.login.lock().unwrap().is_none());
            assert!(matches!(pending.ready(), Err(observed) if observed == code));
        }
        let pending = PendingProvider::default();
        pending.finish(Err("E_LOGIN_REQUIRED"), &CancellationToken::new());
        let recovery =
            RecoveryController::new(pending, Receipt::fixture("installation"), PathBuf::new());
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert_eq!(recovery.web_login(false, cancel).await, Err("E_CANCELLED"));
        assert!(recovery.login.lock().unwrap().is_none());
    }
    #[test]
    fn only_completed_transient_failures_can_be_explicitly_retried() {
        for code in [
            "E_ALREADY_RUNNING",
            "E_BROWSER_START",
            "E_BROWSER_RUNTIME_MISSING",
            "E_BACKGROUND_NAVIGATION",
            "E_BROWSER_OBSERVATION",
        ] {
            let pending = PendingProvider::default();
            let cancel = CancellationToken::new();
            assert!(pending.begin_retry().is_err());
            assert!(!pending.finish(Err(code), &cancel));
            pending.begin_retry().unwrap();
            assert!(pending.begin_retry().is_err());
            assert!(matches!(
                pending.health(),
                crate::gateway::ProviderHealth::Recovering
            ));
            assert!(pending.finish(Ok(Arc::new(Ready)), &cancel));
            assert!(pending.begin_retry().is_err());
            assert!(!pending.finish(Err("E_BROWSER_START"), &cancel));
            assert!(pending.ready().is_ok());
        }
        for code in [
            "E_LOGIN_REQUIRED",
            "E_BROWSER_VERIFICATION_REQUIRED",
            "E_SESSION_SCOPE",
            "E_BROWSER_VERSION_CHANGED",
            "E_MODEL_SELECTION",
            "E_BROWSER_LANGUAGE",
            "E_WEB_RECOVERY_CLEANUP",
            "E_CANCELLED",
        ] {
            let pending = PendingProvider::default();
            pending.finish(Err(code), &CancellationToken::new());
            assert!(pending.begin_retry().is_err(), "{code}");
            assert!(matches!(pending.ready(), Err(actual) if actual == code));
        }
    }
}
