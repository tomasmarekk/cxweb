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
    output_formats: Arc<Mutex<Vec<Value>>>,
    websocket_requests: Arc<AtomicUsize>,
    warmups: Arc<AtomicUsize>,
    compaction_requests: Arc<AtomicUsize>,
    checkpoint_continuations: Arc<AtomicUsize>,
    checkpoint_plaintext_history: Arc<AtomicUsize>,
}
impl crate::gateway::WebProvider for ProbeProvider {
    fn validate_warmup(&self, request: &crate::gateway::WebRequest) -> Result<(), &'static str> {
        self.warmups.fetch_add(1, Ordering::Relaxed);
        self.provider.validate_warmup(request)
    }
    fn respond(&self, request: crate::gateway::WebRequest) -> crate::gateway::WebFuture {
        if let Some(input) = request.payload["input"].as_array() {
            if input
                .last()
                .is_some_and(|item| item["type"] == "compaction_trigger")
            {
                self.compaction_requests.fetch_add(1, Ordering::Relaxed);
            }
            if input.iter().any(|item| {
                item["type"] == "compaction"
                    && item["encrypted_content"]
                        .as_str()
                        .is_some_and(|s| s.starts_with("wbr1:"))
            }) {
                self.checkpoint_continuations
                    .fetch_add(1, Ordering::Relaxed);
                if input.iter().any(|item| {
                    item["role"] == "assistant"
                        || matches!(
                            item["type"].as_str(),
                            Some(
                                "function_call"
                                    | "function_call_output"
                                    | "custom_tool_call"
                                    | "custom_tool_call_output"
                            )
                        )
                }) {
                    self.checkpoint_plaintext_history
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        if request.transport == crate::gateway::WebTransport::WebSocket {
            self.websocket_requests.fetch_add(1, Ordering::Relaxed);
        }
        if let Ok(mut formats) = self.output_formats.lock()
            && formats.len() < 32
        {
            formats.push(output_format_observation(&request.payload));
        }
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

// Retain only known format names and comparisons with a public native schema.
// Never export user-provided schemas, descriptions, property names or content.
fn output_format_observation(payload: &Value) -> Value {
    let format = payload.get("text").and_then(|text| text.get("format"));
    let kind = match format {
        None => "absent",
        Some(Value::Null) => "null",
        Some(value) => match value.get("type").and_then(Value::as_str) {
            Some("text") => "text",
            Some("json_schema") => "json_schema",
            Some("json_object") => "json_object",
            _ => "unknown",
        },
    };
    let title = json!({"type":"object","properties":{"title":{"type":"string","minLength":1,"maxLength":36}},"required":["title"],"additionalProperties":false});
    json!({"kind":kind,"matches_native_title_schema":format.is_some_and(|f| f.get("schema") == Some(&title)),"strict":format.and_then(|f| f.get("strict")).and_then(Value::as_bool)})
}

pub async fn serve(
    output: &Path,
    websocket: bool,
    codec: cxweb_codex_adapter::catalog_codec::CatalogCodec,
    coding: bool,
    compaction: bool,
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
            if observation.document_ready
                && observation.official_page
                && !observation.login_action
                && !observation.verification_required
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
                // Initial navigation can briefly show the signed-out shell or
                // an interstitial before ordinary loading completes. Observe
                // passively; never click a verification challenge or submit login.
                return Err(if observation.verification_required {
                    "E_BROWSER_VERIFICATION"
                } else if observation.login_action {
                    "E_LOGIN_REQUIRED"
                } else {
                    "E_BROWSER_NOT_READY"
                });
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
        let key = if compaction { Some(Arc::new(crate::checkpoint::Codec::load_or_create(&directory.join("checkpoint-key.dpapi"), &installation)?)) } else { None };
        let mut provider = CoordinatorProvider::new(coordinator, ProviderScope {
            installation, account: scope.account, workspace: scope.workspace, epoch: 0,
        }, vec![ROUTE.into()])?;
        let budget = cxweb_codex_adapter::context_budget::LocalContextBudget::DIAGNOSTIC;
        if let Some(key) = key { provider = provider.with_checkpoints(key, codec)?.with_context_budget(budget)?; }
        let native_listener = TcpListener::bind("127.0.0.1:0").await.map_err(|_| "E_PROBE_LISTENER")?;
        let native_address = native_listener.local_addr().map_err(|_| "E_PROBE_LISTENER")?;
        let native = NativeTransport::new(format!("http://{native_address}"))?;
        let native_stop = CancellationToken::new();
        let _native_guard = native_stop.clone().drop_guard();
        let socket_upgrades = Arc::new(AtomicUsize::new(0));
        let socket_frames = Arc::new(AtomicUsize::new(0));
        let upgrades = socket_upgrades.clone();
        let frames = socket_frames.clone();
        let peer_stop = native_stop.clone();
        tokio::spawn(axum::serve(native_listener, axum::Router::new().route("/responses", axum::routing::get(move |upgrade: axum::extract::WebSocketUpgrade| {
            let upgrades = upgrades.clone();
            let frames = frames.clone();
            let stop = peer_stop.clone();
            async move {
                use axum::response::IntoResponse;
                if !websocket { return axum::http::StatusCode::UPGRADE_REQUIRED.into_response(); }
                upgrades.fetch_add(1, Ordering::Relaxed);
                upgrade.on_upgrade(move |mut socket| async move {
                    use futures_util::StreamExt;
                    loop {
                        tokio::select! {
                            () = stop.cancelled() => break,
                            message = socket.next() => match message {
                                Some(Ok(axum::extract::ws::Message::Text(_))) => {
                                    frames.fetch_add(1, Ordering::Relaxed);
                                    let error = crate::web_ws::error(503, "E_NATIVE_REJECTED_IN_PROBE");
                                    let _ = socket.send(axum::extract::ws::Message::Text(error.to_string().into())).await;
                                }
                                Some(Ok(axum::extract::ws::Message::Ping(bytes))) => {
                                    if socket.send(axum::extract::ws::Message::Pong(bytes)).await.is_err() { break; }
                                }
                                Some(Ok(axum::extract::ws::Message::Pong(_))) => (),
                                _ => break,
                            }
                        }
                    }
                })
            }
        })).fallback(|| async {
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        })).with_graceful_shutdown(native_stop.cancelled_owned()).into_future());
        let listener = TcpListener::bind("127.0.0.1:0").await.map_err(|_| "E_PROBE_LISTENER")?;
        let failures = Arc::new(Mutex::new(Vec::new()));
        let search_requests = Arc::new(AtomicUsize::new(0));
        let output_formats = Arc::new(Mutex::new(Vec::new()));
        let websocket_requests = Arc::new(AtomicUsize::new(0));
        let warmups = Arc::new(AtomicUsize::new(0));
        let compaction_requests = Arc::new(AtomicUsize::new(0));
        let checkpoint_continuations = Arc::new(AtomicUsize::new(0));
        let checkpoint_plaintext_history = Arc::new(AtomicUsize::new(0));
        let gateway = Gateway::new(listener.local_addr().map_err(|_| "E_PROBE_LISTENER")?.port(), native, Arc::new(ProbeProvider { provider, failures: failures.clone(), search_requests: search_requests.clone(), output_formats: output_formats.clone(), websocket_requests: websocket_requests.clone(), warmups: warmups.clone(), compaction_requests: compaction_requests.clone(), checkpoint_continuations: checkpoint_continuations.clone(), checkpoint_plaintext_history: checkpoint_plaintext_history.clone() }));
        // Exercise the exact reviewed encoder in isolation; this diagnostic
        // catalog does not publish a production qualification snapshot.
        let catalog_route = cxweb_codex_adapter::catalog_codec::CatalogRoute {
            id: ROUTE.into(), observed_label: label.clone(), effort, coding,
        };
        let model = if compaction { codec.encode_with_context_budget(&catalog_route, budget)? } else { codec.encode(&catalog_route)? };
        let mut descriptor = std::fs::OpenOptions::new().write(true).create_new(true).open(output).map_err(|_| "E_PROBE_OUTPUT")?;
        descriptor.write_all(json!({"base_url":gateway.base_url(),"model":ROUTE,"catalog":{"models":[model]},"catalog_codec":codec.id(),"live":true}).to_string().as_bytes()).map_err(|_| "E_PROBE_OUTPUT")?;
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
        Ok(json!({"live":true,"route":ROUTE,"browser_label":label,"native_upstream":"local rejection stub","routing_installed":false,"diagnostic":diagnostic,"optional_web_search_requests":search_requests.load(Ordering::Relaxed),"output_formats":output_formats.lock().map_err(|_| "E_PROBE_STATE")?.clone(),"failures":failures.lock().map_err(|_| "E_PROBE_STATE")?.clone(),"websocket_enabled":websocket,"websocket_upgrades":socket_upgrades.load(Ordering::Relaxed),"native_websocket_frames":socket_frames.load(Ordering::Relaxed),"websocket_requests":websocket_requests.load(Ordering::Relaxed),"websocket_warmups":warmups.load(Ordering::Relaxed),"compaction_requests":compaction_requests.load(Ordering::Relaxed),"checkpoint_continuations":checkpoint_continuations.load(Ordering::Relaxed),"checkpoint_continuations_with_plaintext_assistant_or_tools":checkpoint_plaintext_history.load(Ordering::Relaxed)}))
    }.await;
    let closed = driver.shutdown().await;
    closed?;
    result.map(|mut report| {
        report["browser_closed"] = json!(true);
        report
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_diagnostics_never_export_arbitrary_schema_content() {
        let observed = output_format_observation(
            &json!({"text":{"format":{"type":"PRIVATE_FORMAT","name":"PRIVATE_NAME","schema":{"properties":{"PRIVATE_PROPERTY":{"description":"PRIVATE_DESCRIPTION"}}}}}}),
        );
        assert_eq!(
            observed,
            json!({"kind":"unknown","matches_native_title_schema":false,"strict":null})
        );
        assert!(!observed.to_string().contains("PRIVATE"));
        assert_eq!(
            output_format_observation(&json!({"text":{"format":null}}))["kind"],
            "null"
        );
        assert_eq!(output_format_observation(&json!({}))["kind"], "absent");
    }
}
