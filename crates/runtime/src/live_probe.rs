//! Isolated client-to-browser integration qualification. No native configuration
//! or authentication is read; native traffic terminates at a local rejection stub.
use crate::{
    browser_scope::BrowserScope,
    gateway::Gateway,
    ledger::Ledger,
    managed_driver::{Binding, ManagedDriver, Route},
    native::NativeTransport,
    turn::Coordinator,
    web_provider::{CoordinatorProvider, ProviderScope},
};
use cxweb_browser_adapter::ManagedBrowser;
use cxweb_platform::state::{StatePaths, installed_browser, protected_directory};
use serde_json::{Value, json};
use std::{
    future::Future,
    io::Write,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

const ROUTE: &str = "webbridge/live-probe";

struct ProbeProvider {
    provider: CoordinatorProvider,
    failures: Arc<Mutex<Vec<&'static str>>>,
    search_requests: Arc<AtomicUsize>,
}
impl crate::gateway::WebProvider for ProbeProvider {
    fn respond(&self, request: crate::gateway::WebRequest) -> crate::gateway::WebFuture {
        if request.payload["tools"]
            .as_array()
            .is_some_and(|tools| tools.iter().any(|tool| tool["type"] == "web_search"))
        {
            self.search_requests.fetch_add(1, Ordering::Relaxed);
        }
        let provider = self.provider.clone();
        let failures = self.failures.clone();
        Box::pin(async move {
            match provider.execute(request).await {
                Ok(response) => response,
                Err(code) => {
                    if let Ok(mut failures) = failures.lock()
                        && failures.len() < 32
                    {
                        failures.push(code);
                    }
                    crate::web_provider::web_failure(code)
                }
            }
        })
    }
}

pub async fn serve(
    output: &Path,
    stop: impl Future<Output = ()> + Send + 'static,
) -> Result<Value, &'static str> {
    if !output.is_absolute() || output.exists() {
        return Err("E_PROBE_OUTPUT");
    }
    let directory = output.parent().ok_or("E_PROBE_OUTPUT")?;
    protected_directory(directory).map_err(|_| "E_PROBE_OUTPUT")?;
    let installation = format!("cxweb-live-probe-{:032x}", rand::random::<u128>());
    let observed_installation = installation.clone();
    let (driver, scope, label, effort) = tokio::task::spawn_blocking(move || {
        let paths = StatePaths::open().map_err(|_| "E_STATE_PERMISSIONS")?;
        let ownership = paths.lock().map_err(|_| "E_ALREADY_RUNNING")?;
        let executable = installed_browser().map_err(|_| "E_BROWSER_RUNTIME_MISSING")?;
        let mut browser = ManagedBrowser::launch_offscreen(&executable, &paths.profile)
            .map_err(|_| "E_BROWSER_START")?;
        let page = browser
            .open_background_session()
            .map_err(|_| "E_BACKGROUND_NAVIGATION")?;
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            let observation = browser
                .login_observation(&page)
                .map_err(|_| "E_BROWSER_OBSERVATION")?;
            if observation.login_action || observation.verification_required {
                return Err("E_LOGIN_REQUIRED");
            }
            if observation.document_ready
                && observation.official_page
                && observation.composer
                && observation.account_surface
                && browser.baseline(&page).is_ok_and(|baseline| {
                    baseline.composer_empty
                        && !baseline.generating
                        && !baseline.selected_model.is_empty()
                })
            {
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
                break;
            }
            if Instant::now() >= deadline {
                return Err("E_BROWSER_NOT_READY");
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        let surface =
            browser
                .discover_models(&page)
                .map_err(|error| match error.to_string().as_str() {
                    "E_MODEL_OPEN" => "E_MODEL_OPEN",
                    "E_MODEL_READ" => "E_MODEL_READ",
                    "E_MODEL_SELECT" => "E_MODEL_SELECT",
                    "E_MODEL_RESTORE" => "E_MODEL_RESTORE",
                    "E_MODEL_CLOSE" => "E_MODEL_CLOSE",
                    "E_BROWSER_BUSY" => "E_BROWSER_BUSY",
                    "E_LOGIN_REQUIRED" => "E_LOGIN_REQUIRED",
                    "E_BROWSER_ADAPTER" => "E_BROWSER_ADAPTER",
                    _ => "E_MODEL_DISCOVERY",
                })?;
        let selected = surface
            .candidates
            .into_iter()
            .filter(|candidate| candidate.selected)
            .collect::<Vec<_>>();
        if selected.len() != 1 {
            return Err("E_MODEL_SELECTION");
        }
        let selected = selected.into_iter().next().unwrap();
        // Only reviewed public effort labels are exposed to this isolated client.
        let effort = match selected.label.rsplit_once(" · ").map(|(_, effort)| effort) {
            Some("Instant") => "none",
            Some("Medium") => "medium",
            Some("High") => "high",
            Some("Extra High") => "xhigh",
            _ => return Err("E_MODEL_UNAVAILABLE"),
        }
        .to_owned();
        let surface = browser
            .account_scope(&page)
            .map_err(|_| "E_SESSION_SCOPE")?;
        let scope = BrowserScope::from_surface(&observed_installation, &surface)?;
        browser.close_page(page).map_err(|_| "E_BROWSER_RELEASE")?;
        let binding = Binding {
            installation: observed_installation,
            account: scope.account.clone(),
            workspace: scope.workspace.clone(),
            epoch: 0,
            routes: vec![Route {
                id: ROUTE.into(),
                identity: selected.identity,
                label: selected.label.clone(),
                effort: Some(effort.clone()),
            }],
        };
        let driver = ManagedDriver::start(browser, binding, ownership)?;
        Ok::<_, &'static str>((driver, scope, selected.label, effort))
    })
    .await
    .map_err(|_| "E_BROWSER_WORKER")??;

    // Once transferred, every exit path requests driver shutdown and profile release.
    let result = async {
        let ledger = Ledger::open(&directory.join("turns.sqlite")).await?;
        let coordinator = Coordinator::new(ledger, Arc::new(driver.clone()));
        let provider = CoordinatorProvider::new(coordinator, ProviderScope {
            installation, account: scope.account, workspace: scope.workspace, epoch: 0,
        }, vec![ROUTE.into()])?;
        let native_listener = TcpListener::bind("127.0.0.1:0").await.map_err(|_| "E_PROBE_LISTENER")?;
        let native_address = native_listener.local_addr().map_err(|_| "E_PROBE_LISTENER")?;
        let native = NativeTransport::new(format!("http://{native_address}"))?;
        let native_stop = CancellationToken::new();
        let _native_guard = native_stop.clone().drop_guard();
        tokio::spawn(axum::serve(native_listener, axum::Router::new().route("/responses", axum::routing::get(|| async {
            // This isolated stub has no native WebSocket upstream. Ask the
            // unmodified client to negotiate HTTP rather than retrying 503s.
            axum::http::StatusCode::UPGRADE_REQUIRED
        })).fallback(|| async {
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        })).with_graceful_shutdown(native_stop.cancelled_owned()).into_future());
        let listener = TcpListener::bind("127.0.0.1:0").await.map_err(|_| "E_PROBE_LISTENER")?;
        let failures = Arc::new(Mutex::new(Vec::new()));
        let search_requests = Arc::new(AtomicUsize::new(0));
        let gateway = Gateway::new(listener.local_addr().map_err(|_| "E_PROBE_LISTENER")?.port(), native, Arc::new(ProbeProvider { provider, failures: failures.clone(), search_requests: search_requests.clone() }));
        let mut model = cxweb_codex_adapter::catalog::synthetic_model();
        model["slug"] = json!(ROUTE);
        model["display_name"] = json!(format!("ChatGPT Web · {label}"));
        model["description"] = json!(format!("ChatGPT Web · {label}. Authenticated diagnostic; hosted search unavailable"));
        model["default_reasoning_level"] = json!(effort);
        model["supported_reasoning_levels"] = json!([{"effort":effort,"description":label}]);
        let mut descriptor = std::fs::OpenOptions::new().write(true).create_new(true).open(output).map_err(|_| "E_PROBE_OUTPUT")?;
        descriptor.write_all(json!({"base_url":gateway.base_url(),"model":ROUTE,"catalog":{"models":[model]},"live":true}).to_string().as_bytes()).map_err(|_| "E_PROBE_OUTPUT")?;
        descriptor.sync_all().map_err(|_| "E_PROBE_OUTPUT")?;
        let server_stop = CancellationToken::new();
        let server = axum::serve(listener, gateway.clone().router()).with_graceful_shutdown(server_stop.clone().cancelled_owned()).into_future();
        tokio::pin!(server);
        tokio::select! {
            result = &mut server => { result.map_err(|_| "E_PROBE_SERVER")?; }
            () = stop => {
                gateway.disconnect_web(Duration::from_secs(60)).await?;
                server_stop.cancel();
                server.await.map_err(|_| "E_PROBE_SERVER")?;
            }
        }
        let diagnostic = driver.diagnostic().await?;
        Ok(json!({"live":true,"route":ROUTE,"browser_label":label,"native_upstream":"local rejection stub","routing_installed":false,"diagnostic":diagnostic,"optional_web_search_requests":search_requests.load(Ordering::Relaxed),"failures":failures.lock().map_err(|_| "E_PROBE_STATE")?.clone()}))
    }.await;
    let closed = driver.shutdown().await;
    closed?;
    result.map(|mut report| {
        report["browser_closed"] = json!(true);
        report
    })
}
