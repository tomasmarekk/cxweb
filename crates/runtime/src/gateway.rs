//! Authentication/classification boundary shared by future web providers.
use crate::native::{NativeRoute, NativeTransport, unavailable};
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State, WebSocketUpgrade},
    http::{HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::any,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use cxweb_codex_adapter::catalog_codec::CatalogCodec;
use cxweb_codex_adapter::strict_json;
use serde_json::Value;
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

/// This payload cannot carry transport headers, native authorization or the
/// installation capability. Its history is untrusted model input, never code.
pub struct WebRequest {
    pub payload: Value,
    pub identity: Option<crate::web_provider::WebIdentity>,
    pub compact: bool,
    pub cancellation: CancellationToken,
    pub transport: WebTransport,
    pub progress: Option<crate::turn::PublicProgress>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WebTransport {
    Http,
    WebSocket,
}
pub type WebFuture = Pin<Box<dyn Future<Output = Response> + Send>>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderHealth {
    Unqualified,
    Recovering,
    Verified { observed_at: Option<String> },
    Unavailable { code: &'static str },
}

#[cfg(any(windows, test))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GatewayHealth {
    pub accepting: bool,
    pub cleanup_failed: bool,
    pub active_turns: u64,
    pub provider: ProviderHealth,
    pub clients: ClientActivity,
    pub native: Option<crate::native_health::Observation>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ClientActivity {
    pub cli: Option<ClientRequest>,
    pub app: Option<ClientRequest>,
    pub cli_catalog: Option<ClientCatalog>,
    pub app_catalog: Option<ClientCatalog>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClientCatalog {
    pub observed_at: Option<String>,
    pub(crate) web_scope: String,
    pub(crate) generation: u64,
    pub current: bool,
    pub valid: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClientRequest {
    pub succeeded: bool,
    pub observed_at: Option<String>,
}

pub(crate) fn client_codec(headers: &HeaderMap) -> Option<CatalogCodec> {
    let mut values = headers.get_all("user-agent").iter();
    let agent = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        return None;
    }
    CatalogCodec::from_user_agent(agent)
}

pub trait WebProvider: Send + Sync {
    /// Cached/local observations only: never navigate, generate or probe upstream.
    fn health(&self) -> ProviderHealth {
        ProviderHealth::Unqualified
    }

    /// Keep this future alive until generation and cancellation cleanup finish.
    /// Final/tool output remains buffered. The optional progress channel can
    /// publish verified public status while this future retains its drain lease.
    fn respond(&self, request: WebRequest) -> WebFuture;
    /// Validate a no-generation WebSocket warmup without touching the browser.
    fn validate_warmup(&self, _request: &WebRequest) -> Result<(), &'static str> {
        Err("E_COMPATIBILITY_UNQUALIFIED")
    }
    /// Return only entries qualified for the reviewed client catalog codec.
    /// Native authorization is deliberately not available to the web provider.
    fn catalog(
        &self,
        _codec: cxweb_codex_adapter::catalog_codec::CatalogCodec,
    ) -> Option<crate::catalog_proxy::OwnedCatalog> {
        None
    }
}

pub struct UnqualifiedProvider;
impl WebProvider for UnqualifiedProvider {
    fn respond(&self, _: WebRequest) -> WebFuture {
        Box::pin(async { crate::web_provider::web_failure("E_COMPATIBILITY_UNQUALIFIED") })
    }
}

#[derive(Clone)]
pub struct Gateway {
    authority: String,
    capability: String,
    native: NativeTransport,
    web: Arc<dyn WebProvider>,
    admission: Arc<WebAdmission>,
    clients: Arc<Mutex<ClientActivity>>,
}

struct WebAdmission {
    state: Mutex<AdmissionState>,
    cancel: CancellationToken,
    drained: Notify,
}
struct AdmissionState {
    accepting: bool,
    maintenance: bool,
    active: usize,
    active_turns: u64,
    cleanup_failed: bool,
}
struct WebLease(Arc<WebAdmission>, bool);
#[cfg(windows)]
struct MaintenanceLease(WebLease);
#[cfg(windows)]
impl Drop for MaintenanceLease {
    fn drop(&mut self) {
        if !std::thread::panicking()
            && let Ok(mut state) = self.0.0.state.lock()
        {
            state.maintenance = false;
        }
    }
}
impl Drop for WebLease {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.state.lock() {
            if std::thread::panicking() {
                state.cleanup_failed = true;
                state.accepting = false;
                self.0.cancel.cancel();
            }
            state.active -= 1;
            if self.1 {
                state.active_turns -= 1;
            }
            if state.active == 0 {
                self.0.drained.notify_waiters();
            }
        }
    }
}
impl WebAdmission {
    fn acquire(self: &Arc<Self>, turn: bool) -> Option<WebLease> {
        let mut state = self.state.lock().ok()?;
        if !state.accepting || state.maintenance {
            return None;
        }
        state.active += 1;
        if turn {
            state.active_turns += 1;
        }
        Some(WebLease(self.clone(), turn))
    }
    async fn drain(&self) -> Result<(), &'static str> {
        loop {
            let notified = self.drained.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let state = self.state.lock().map_err(|_| "E_GATEWAY_STATE")?;
                if state.cleanup_failed {
                    return Err("E_WEB_CLEANUP_UNCONFIRMED");
                }
                if state.active == 0 {
                    return Ok(());
                }
            }
            notified.await;
        }
    }
}

impl Gateway {
    /// Exclude catalog and generation admission atomically while updating a
    /// qualified family. Native forwarding remains independent of this gate.
    #[cfg(windows)]
    pub(crate) async fn maintain_web<F, Fut>(&self, work: F) -> Result<(), &'static str>
    where
        F: FnOnce(CancellationToken) -> Fut,
        Fut: Future<Output = Result<(), &'static str>>,
    {
        let _lease = {
            let mut state = self.admission.state.lock().map_err(|_| "E_GATEWAY_STATE")?;
            if !state.accepting || state.cleanup_failed || state.maintenance || state.active != 0 {
                return Err("E_WEB_ACTIVE");
            }
            state.maintenance = true;
            state.active += 1;
            state.active_turns += 1;
            MaintenanceLease(WebLease(self.admission.clone(), true))
        };
        let result = work(self.admission.cancel.child_token()).await;
        if result == Err("E_WEB_CLEANUP_UNCONFIRMED") {
            let mut state = self.admission.state.lock().map_err(|_| "E_GATEWAY_STATE")?;
            state.cleanup_failed = true;
            state.accepting = false;
        }
        result
    }
    /// Atomically stop new web admission only if no generation is in flight.
    /// The normal disconnect path then drains recovery and catalog leases.
    #[cfg(windows)]
    pub(crate) fn close_if_idle(&self) -> Result<(), &'static str> {
        let mut state = self.admission.state.lock().map_err(|_| "E_WEB_STATE")?;
        if state.active_turns != 0 {
            return Err("E_WEB_ACTIVE");
        }
        state.accepting = false;
        Ok(())
    }

    #[cfg(any(windows, test))]
    pub(crate) fn health(&self) -> GatewayHealth {
        let provider = self.web.health();
        let mut clients = self
            .clients
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default();
        for (codec, observed) in [
            (CatalogCodec::Cli01551, &mut clients.cli_catalog),
            (CatalogCodec::App01550Alpha92, &mut clients.app_catalog),
        ] {
            if let Some(observed) = observed {
                observed.current = self.web.catalog(codec).is_some_and(|catalog| {
                    catalog.codec == codec.id()
                        && catalog.web_scope == observed.web_scope
                        && catalog.generation == observed.generation
                });
            }
        }
        match self.admission.state.lock() {
            Ok(state) => GatewayHealth {
                accepting: state.accepting,
                cleanup_failed: state.cleanup_failed,
                active_turns: state.active_turns,
                provider,
                clients,
                native: self.native.health.snapshot(),
            },
            Err(_) => GatewayHealth {
                accepting: false,
                cleanup_failed: true,
                active_turns: 0,
                provider,
                clients,
                native: self.native.health.snapshot(),
            },
        }
    }

    /// Recovery owns a drain lease just like a request. Disconnect waits for
    /// its non-generative browser checks and cleanup before restoring config.
    #[cfg(windows)]
    pub(crate) async fn recover_web<F, Fut>(&self, work: F) -> Result<(), &'static str>
    where
        F: FnOnce(CancellationToken) -> Fut,
        Fut: Future<Output = Result<(), &'static str>>,
    {
        let Some(_lease) = self.admission.acquire(false) else {
            return Ok(());
        };
        let result = work(self.admission.cancel.child_token()).await;
        if result.is_err() {
            self.admission
                .state
                .lock()
                .map_err(|_| "E_GATEWAY_STATE")?
                .cleanup_failed = true;
        }
        result
    }

    /// Both transports retain the drain lease until provider cleanup completes.
    /// Dropping this delivery future cancels work without dropping its worker.
    pub(crate) async fn dispatch_web(
        &self,
        payload: Value,
        identity: Option<crate::web_provider::WebIdentity>,
        compact: bool,
        warmup: bool,
        transport: WebTransport,
        client: Option<CatalogCodec>,
    ) -> Response {
        self.dispatch_web_with_progress(payload, identity, compact, warmup, transport, client, None)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn dispatch_web_with_progress(
        &self,
        payload: Value,
        identity: Option<crate::web_provider::WebIdentity>,
        compact: bool,
        warmup: bool,
        transport: WebTransport,
        client: Option<CatalogCodec>,
        progress: Option<crate::turn::PublicProgress>,
    ) -> Response {
        let Some(lease) = self.admission.acquire(!warmup) else {
            return crate::web_provider::web_failure("E_WEB_DISCONNECTED");
        };
        let cancellation = self.admission.cancel.child_token();
        let _cancel_on_disconnect = cancellation.clone().drop_guard();
        let web = self.web.clone();
        let clients = self.clients.clone();
        let worker = tokio::spawn(async move {
            let _lease = lease;
            if cancellation.is_cancelled() {
                return crate::web_provider::web_failure("E_CANCELLED");
            }
            let request = WebRequest {
                payload,
                identity,
                compact,
                cancellation: cancellation.clone(),
                transport,
                progress,
            };
            if warmup {
                return match web.validate_warmup(&request) {
                    Ok(()) => crate::web_ws::warmup_response(&request.payload),
                    Err(code) => crate::web_provider::web_failure(code),
                };
            }
            let response = web.respond(request).await;
            if !cancellation.is_cancelled()
                && let Some(client) = client
                && let Some(evidence) = response
                    .extensions()
                    .get::<crate::web_provider::BrowserEvidence>()
                && let Ok(mut clients) = clients.lock()
            {
                let observed = Some(ClientRequest {
                    succeeded: response.status().is_success()
                        && matches!(evidence, crate::web_provider::BrowserEvidence::Verified),
                    observed_at: cxweb_platform::clock::utc_timestamp(),
                });
                match client {
                    CatalogCodec::Cli01551 => clients.cli = observed,
                    CatalogCodec::App01550Alpha92 => clients.app = observed,
                }
            }
            response
        });
        worker
            .await
            .unwrap_or_else(|_| crate::web_provider::web_failure("E_WEB_WORKER"))
    }
    pub fn new(port: u16, native: NativeTransport, web: Arc<dyn WebProvider>) -> Self {
        Self {
            authority: format!("127.0.0.1:{port}"),
            capability: URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>()),
            native,
            web,
            clients: Arc::default(),
            admission: Arc::new(WebAdmission {
                state: Mutex::new(AdmissionState {
                    maintenance: false,
                    accepting: true,
                    active: 0,
                    active_turns: 0,
                    cleanup_failed: false,
                }),
                cancel: CancellationToken::new(),
                drained: Notify::new(),
            }),
        }
    }
    pub fn base_url(&self) -> String {
        format!(
            "http://{}/wb/{}/backend-api/codex",
            self.authority, self.capability
        )
    }
    /// Only a validated private journal may restore an existing capability.
    #[cfg(windows)]
    pub(crate) fn recover_native(port: u16, capability: &str, native: NativeTransport) -> Self {
        let mut gateway = Self::new(port, native, Arc::new(UnqualifiedProvider));
        gateway.capability = capability.to_owned();
        gateway
            .admission
            .state
            .lock()
            .expect("new admission lock")
            .accepting = false;
        gateway.admission.cancel.cancel();
        gateway
    }
    /// The reservation owner supplies the capability from its private journal.
    #[cfg(windows)]
    pub(crate) fn prepared(
        port: u16,
        capability: &str,
        native: NativeTransport,
        web: Arc<dyn WebProvider>,
    ) -> Self {
        let mut gateway = Self::new(port, native, web);
        gateway.capability = capability.to_owned();
        gateway
    }
    /// Irreversibly reject new web work on this listener, request cancellation,
    /// and wait for admitted providers to finish cleanup. Timeout is not a drain
    /// success: keep the listener and do not remove configuration on that result.
    /// Native HTTP/WS routes remain available to clients using the old URL.
    pub async fn disconnect_web(&self, deadline: Duration) -> Result<(), &'static str> {
        self.admission
            .state
            .lock()
            .map_err(|_| "E_GATEWAY_STATE")?
            .accepting = false;
        self.admission.cancel.cancel();
        tokio::time::timeout(deadline, self.admission.drain())
            .await
            .map_err(|_| "E_WEB_DRAIN_TIMEOUT")?
    }
    pub fn router(self) -> Router {
        Router::new()
            .fallback(any(handle))
            .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
            .with_state(self)
    }
}

async fn handle(
    State(gateway): State<Gateway>,
    upgrade: Result<WebSocketUpgrade, axum::extract::ws::rejection::WebSocketUpgradeRejection>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    bytes: Bytes,
) -> Response {
    if headers.contains_key("origin")
        || headers.get_all("host").iter().count() != 1
        || headers.get("host").and_then(|h| h.to_str().ok()) != Some(&gateway.authority)
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let prefix = format!("/wb/{}/backend-api/codex/", gateway.capability);
    let legacy = format!("/wb/{}/v1/", gateway.capability);
    if method == Method::GET
        && (uri.path() == prefix.trim_end_matches('/')
            || uri.path() == legacy.trim_end_matches('/')
            || uri.path() == format!("{legacy}realtime"))
        && let Ok(upgrade) = upgrade
    {
        let route = match headers
            .get("openai-alpha")
            .and_then(|value| value.to_str().ok())
        {
            Some("quicksilver=v2") => crate::native_ws::SocketRoute::Live,
            None | Some("quicksilver=v1") => crate::native_ws::SocketRoute::Realtime,
            _ => return StatusCode::BAD_REQUEST.into_response(),
        };
        return gateway
            .native
            .upgrade(route, upgrade, uri.query(), headers, None)
            .await;
    }
    let Some(path) = uri
        .path()
        .strip_prefix(&prefix)
        .or_else(|| uri.path().strip_prefix(&legacy))
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if method == Method::GET
        && path == "responses"
        && let Ok(upgrade) = upgrade
    {
        return gateway
            .native
            .upgrade(
                crate::native_ws::SocketRoute::Responses,
                upgrade,
                uri.query(),
                headers,
                Some(gateway.clone()),
            )
            .await;
    }
    let route = match (&method, path) {
        (&Method::GET, "models") => NativeRoute::Models,
        (&Method::POST, "responses") => NativeRoute::Responses,
        (&Method::POST, "responses/compact") => NativeRoute::Compact,
        (&Method::POST, "memories/trace_summarize") => NativeRoute::MemorySummarize,
        (&Method::POST, "alpha/search") => NativeRoute::Search,
        (&Method::POST, "images/generations") => NativeRoute::ImageGeneration,
        (&Method::POST, "images/edits") => NativeRoute::ImageEdit,
        (&Method::POST, "realtime/calls") => NativeRoute::RealtimeCall,
        // Only exact reviewed routes are admitted. Realtime and other auxiliary
        // endpoints still require separate transport qualification.
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    if matches!(route, NativeRoute::Models)
        && let Some(_lease) = gateway.admission.acquire(false)
        && let Some(codec) = crate::catalog_proxy::select_codec(uri.query(), &headers)
    {
        if gateway.web.health() == ProviderHealth::Recovering {
            // A successful native-only response would overwrite Codex's merged
            // model cache for its full TTL. Recovery has no complete snapshot
            // yet. Report temporary unavailability without modifying that cache.
            // Native generation and unknown-client catalog passthrough continue.
            let mut response = unavailable("E_WEB_RECOVERING");
            *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
            response
                .headers_mut()
                .insert("cache-control", "no-store".parse().unwrap());
            response
                .headers_mut()
                .insert("retry-after", "1".parse().unwrap());
            return response;
        }
        if let Some(catalog) = gateway.web.catalog(codec) {
            let observed = (catalog.codec == codec.id()).then(|| ClientCatalog {
                observed_at: cxweb_platform::clock::utc_timestamp(),
                web_scope: catalog.web_scope.clone(),
                generation: catalog.generation,
                current: true,
                valid: false,
            });
            let response =
                crate::catalog_proxy::forward(&gateway.native, uri.query(), headers, catalog).await;
            if let Some(mut observed) = observed
                && let Ok(mut clients) = gateway.clients.lock()
            {
                observed.valid = response
                    .extensions()
                    .get::<crate::catalog_proxy::CatalogEvidence>()
                    .is_some();
                observed.observed_at = cxweb_platform::clock::utc_timestamp();
                match codec {
                    CatalogCodec::Cli01551 => clients.cli_catalog = Some(observed),
                    CatalogCodec::App01550Alpha92 => clients.app_catalog = Some(observed),
                }
            }
            return response;
        }
    }
    let inspected = if method == Method::POST {
        match crate::request_body::decode(&headers, bytes.clone()).await {
            Ok(decoded) => decoded,
            Err(status) => return status.into_response(),
        }
    } else {
        bytes.clone()
    };
    if matches!(route, NativeRoute::RealtimeCall) {
        // Subscription clients send JSON with a nested session; older call
        // creation also supports raw SDP. Neither is a Responses request.
        let media = headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .map(str::trim);
        match media {
            Some(value) if value.eq_ignore_ascii_case("application/sdp") => (),
            Some(value) if value.eq_ignore_ascii_case("application/json") => {
                let Ok(payload) = strict_json::parse(&inspected, 32 * 1024 * 1024) else {
                    return StatusCode::BAD_REQUEST.into_response();
                };
                let Some(session) = payload.get("session").and_then(Value::as_object) else {
                    return StatusCode::BAD_REQUEST.into_response();
                };
                if payload.get("sdp").and_then(Value::as_str).is_none() {
                    return StatusCode::BAD_REQUEST.into_response();
                }
                for model in [payload.get("model"), session.get("model")]
                    .into_iter()
                    .flatten()
                {
                    if model.is_null() {
                        continue;
                    }
                    match model.as_str() {
                        Some(model) if model.starts_with(cxweb_domain::OWNED_MODEL_PREFIX) => {
                            return crate::web_provider::web_failure(
                                "E_WEB_CAPABILITY_UNSUPPORTED",
                            );
                        }
                        Some(_) => (),
                        None => return StatusCode::BAD_REQUEST.into_response(),
                    }
                }
            }
            _ => return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response(),
        }
    } else if method == Method::POST {
        let payload = match strict_json::parse(&inspected, 32 * 1024 * 1024) {
            Ok(payload) => payload,
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        };
        let Some(model) = payload.get("model").and_then(Value::as_str) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        let generation = matches!(route, NativeRoute::Responses | NativeRoute::Compact);
        if model.starts_with(cxweb_domain::OWNED_MODEL_PREFIX) {
            // Native-only capabilities must never turn an owned web model into
            // an authenticated native inference request or a browser tool call.
            if !generation {
                return crate::web_provider::web_failure("E_WEB_CAPABILITY_UNSUPPORTED");
            }
            if inspected.len() > 8 * 1024 * 1024 {
                return StatusCode::PAYLOAD_TOO_LARGE.into_response();
            }
            // Drop bearer-bearing transport state before entering browser code.
            let identity = crate::web_provider::WebIdentity::from_headers(&headers);
            let client = client_codec(&headers);
            drop(headers);
            return gateway
                .dispatch_web(
                    payload,
                    identity,
                    matches!(route, NativeRoute::Compact),
                    false,
                    WebTransport::Http,
                    client,
                )
                .await;
        }
        if generation && crate::context_boundary::has_owned_reference(&payload) {
            return crate::web_provider::web_failure("E_NONPORTABLE_CONTEXT");
        }
    }
    gateway
        .native
        .forward(route, uri.query(), headers, bytes)
        .await
        .unwrap_or_else(unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[tokio::test]
    async fn catalog_handshake_requires_valid_merge_and_tracks_snapshot_changes() {
        struct CatalogOnly(Arc<std::sync::atomic::AtomicU64>);
        impl WebProvider for CatalogOnly {
            fn respond(&self, _: WebRequest) -> WebFuture {
                panic!("catalog must not generate")
            }
            fn catalog(&self, codec: CatalogCodec) -> Option<crate::catalog_proxy::OwnedCatalog> {
                Some(crate::catalog_proxy::OwnedCatalog {
                    codec: codec.id().into(),
                    web_scope: "fixture-scope".into(),
                    generation: self.0.load(std::sync::atomic::Ordering::SeqCst),
                    entries: vec![cxweb_codex_adapter::catalog::synthetic_model()],
                })
            }
        }
        let status = Arc::new(std::sync::atomic::AtomicU16::new(200));
        let upstream_status = status.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let native =
            NativeTransport::new(format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route(
                    "/models",
                    axum::routing::get(move || {
                        let status = upstream_status.load(std::sync::atomic::Ordering::SeqCst);
                        async move {
                            (
                                StatusCode::from_u16(status).unwrap(),
                                axum::Json(serde_json::json!({"models":[{"slug":"native"}]})),
                            )
                        }
                    }),
                ),
            )
            .await
            .unwrap();
        });
        let generation = Arc::new(std::sync::atomic::AtomicU64::new(1));
        let gateway = Gateway::new(12345, native, Arc::new(CatalogOnly(generation.clone())));
        for (version, agent, conditional, expected) in [
            ("unknown", "fixture/unknown", false, 200),
            ("0.155.1", "fixture/0.155.1", false, 200),
            ("0.155.0", "fixture/0.155.0-alpha.9.2", false, 200),
            ("0.155.1", "fixture/0.155.1", true, 304),
            ("0.155.1", "fixture/0.155.1", false, 401),
        ] {
            if expected == 401 {
                status.store(401, std::sync::atomic::Ordering::SeqCst);
            }
            let mut request = Request::builder()
                .uri(format!(
                    "{}/models?client_version={version}",
                    gateway.base_url()
                ))
                .header("host", "127.0.0.1:12345")
                .header("user-agent", agent);
            if conditional {
                request = request.header("if-none-match", "*");
            }
            let response = gateway
                .clone()
                .router()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), expected);
            response.into_body().collect().await.unwrap();
            let activity = gateway.health().clients;
            if version == "unknown" {
                assert_eq!(activity, ClientActivity::default());
            } else if version == "0.155.1" {
                let catalog = activity.cli_catalog.unwrap();
                assert!(catalog.current);
                assert_eq!(catalog.valid, expected != 401);
            } else {
                assert!(activity.app_catalog.unwrap().valid);
            }
            assert!(activity.cli.is_none() && activity.app.is_none());
        }
        generation.store(2, std::sync::atomic::Ordering::SeqCst);
        assert!(!gateway.health().clients.app_catalog.unwrap().current);
        server.abort();
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn reasoning_maintenance_excludes_work_reopens_after_failure_and_drains_on_disconnect() {
        let gateway = Gateway::new(
            12345,
            NativeTransport::subscription().unwrap(),
            Arc::new(UnqualifiedProvider),
        );
        let request = gateway.admission.acquire(true).unwrap();
        assert_eq!(
            gateway
                .maintain_web(|_| async { panic!("must not run while active") })
                .await,
            Err("E_WEB_ACTIVE")
        );
        drop(request);
        assert_eq!(
            gateway
                .maintain_web(|_| async { Err("E_QUALIFICATION_PROTOCOL") })
                .await,
            Err("E_QUALIFICATION_PROTOCOL")
        );
        assert!(gateway.admission.acquire(true).is_some());
        assert!(!gateway.health().cleanup_failed);
        let (entered, wait) = tokio::sync::oneshot::channel();
        let other = gateway.clone();
        let operation = tokio::spawn(async move {
            other
                .maintain_web(|cancel| async move {
                    entered.send(()).unwrap();
                    cancel.cancelled().await;
                    Err("E_CANCELLED")
                })
                .await
        });
        wait.await.unwrap();
        assert_eq!(gateway.health().active_turns, 1);
        assert!(gateway.admission.acquire(true).is_none());
        assert!(gateway.admission.acquire(false).is_none());
        gateway
            .disconnect_web(Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(operation.await.unwrap(), Err("E_CANCELLED"));
        assert_eq!(gateway.health().active_turns, 0);
        assert!(!gateway.health().accepting);
        assert!(gateway.admission.acquire(true).is_none());
        let gateway = Gateway::new(
            12345,
            NativeTransport::subscription().unwrap(),
            Arc::new(UnqualifiedProvider),
        );
        assert_eq!(
            gateway
                .maintain_web(|_| async { Err("E_WEB_CLEANUP_UNCONFIRMED") })
                .await,
            Err("E_WEB_CLEANUP_UNCONFIRMED")
        );
        assert!(gateway.health().cleanup_failed);
        assert!(gateway.admission.acquire(true).is_none());
    }
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tower::ServiceExt;

    #[tokio::test]
    async fn client_activity_requires_known_build_and_completed_browser_evidence() {
        struct Observed;
        impl WebProvider for Observed {
            fn respond(&self, request: WebRequest) -> WebFuture {
                Box::pin(async move {
                    if request.payload["instructions"] == "failed" {
                        return crate::web_provider::web_failure("E_QUALIFICATION_PROTOCOL");
                    }
                    if request.payload["instructions"] == "cancelled" {
                        request.cancellation.cancel();
                    }
                    let mut response = "fixture".into_response();
                    if request.payload["instructions"] != "plain" {
                        response
                            .extensions_mut()
                            .insert(crate::web_provider::BrowserEvidence::Verified);
                    }
                    response
                })
            }
        }
        let gateway = Gateway::new(
            12345,
            NativeTransport::subscription().unwrap(),
            Arc::new(Observed),
        );
        let send = |agent: &str, mode: &str, duplicate: bool| {
            let mut request = Request::builder()
                .method("POST")
                .uri(format!("{}/responses", gateway.base_url()))
                .header("host", "127.0.0.1:12345")
                .header("user-agent", agent);
            if duplicate {
                request = request.header("user-agent", agent);
            }
            gateway.clone().router().oneshot(
                request
                    .body(Body::from(
                        json!({"model":"webbridge/fixture","instructions":mode}).to_string(),
                    ))
                    .unwrap(),
            )
        };
        for (agent, mode, duplicate) in [
            ("codex_cli_rs/0.155.2", "verified", false),
            ("codex_cli_rs/0.155.1", "verified", true),
            ("codex_cli_rs/0.155.1", "plain", false),
            ("codex_cli_rs/0.155.1", "cancelled", false),
        ] {
            send(agent, mode, duplicate).await.unwrap();
            assert_eq!(*gateway.clients.lock().unwrap(), ClientActivity::default());
        }
        send("codex_cli_rs/0.155.1 (PRIVATE_HOST)", "verified", false)
            .await
            .unwrap();
        let passed = gateway.clients.lock().unwrap().clone();
        assert!(passed.cli.as_ref().unwrap().succeeded);
        assert!(passed.app.is_none());
        send("Codex Desktop/0.155.0-alpha.9.2", "failed", false)
            .await
            .unwrap();
        let failed = gateway.clients.lock().unwrap().clone();
        assert_eq!(failed.cli, passed.cli);
        assert!(!failed.app.as_ref().unwrap().succeeded);
        send("Codex Desktop/0.155.0-alpha.9.2", "verified", false)
            .await
            .unwrap();
        assert!(
            gateway
                .clients
                .lock()
                .unwrap()
                .app
                .as_ref()
                .unwrap()
                .succeeded
        );
    }

    struct Spy {
        calls: AtomicUsize,
    }
    impl WebProvider for Spy {
        fn respond(&self, request: WebRequest) -> WebFuture {
            self.calls.fetch_add(1, Ordering::SeqCst);
            assert!(!request.payload.to_string().contains("NATIVE_SECRET"));
            Box::pin(async { "web branch".into_response() })
        }
    }

    #[tokio::test]
    async fn native_auxiliary_routes_preserve_payload_and_never_enter_browser() {
        let received = Arc::new(AtomicUsize::new(0));
        let count = received.clone();
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let native =
            NativeTransport::new(format!("http://{}", upstream.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(async move {
            axum::serve(
                upstream,
                Router::new().fallback(any(
                    move |method: Method, uri: Uri, headers: HeaderMap, body: Bytes| {
                        let count = count.clone();
                        async move {
                            assert_eq!(method, Method::POST);
                            assert_eq!(uri.query(), Some("client_version=0.153.4"));
                            assert_eq!(headers["authorization"], "Bearer MOCK_NATIVE_SECRET");
                            assert_eq!(headers["chatgpt-account-id"], "mock-account");
                            count.fetch_add(1, Ordering::SeqCst);
                            Response::builder()
                                .status(StatusCode::TOO_MANY_REQUESTS)
                                .header("content-type", "application/json")
                                .header("retry-after", "7")
                                .header("x-reviewed-path", uri.path())
                                .body(Body::from(body))
                                .unwrap()
                        }
                    },
                )),
            )
            .await
            .unwrap();
        });
        let spy = Arc::new(Spy {
            calls: AtomicUsize::new(0),
        });
        let gateway = Gateway::new(12345, native, spy.clone());
        // Native functions remain available after web admission is disabled.
        gateway
            .disconnect_web(Duration::from_secs(1))
            .await
            .unwrap();
        let base = gateway.base_url();
        let router = gateway.router();
        for (path, body) in [
            (
                "memories/trace_summarize",
                "{ \"model\":\"native\",\"traces\":[] }\n",
            ),
            (
                "alpha/search",
                "{\"model\":\"native\",\"input\":\"webbridge/reference in ordinary data\"}\n",
            ),
            (
                "images/generations",
                "{\"model\":\"native-image\", \"prompt\":\"fixture\"}",
            ),
            (
                "images/edits",
                "{\"model\":\"native-image\",\"images\":[], \"prompt\":\"fixture\"}",
            ),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("{base}/{path}?client_version=0.153.4"))
                        .header("host", "127.0.0.1:12345")
                        .header("authorization", "Bearer MOCK_NATIVE_SECRET")
                        .header("chatgpt-account-id", "mock-account")
                        .header("content-type", "application/json")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
            assert_eq!(response.headers()["retry-after"], "7");
            assert_eq!(response.headers()["x-reviewed-path"], format!("/{path}"));
            assert_eq!(
                response.into_body().collect().await.unwrap().to_bytes(),
                body
            );
        }
        assert_eq!(received.load(Ordering::SeqCst), 4);
        assert_eq!(spy.calls.load(Ordering::SeqCst), 0);
        server.abort();
    }

    #[tokio::test]
    async fn auxiliary_owned_models_and_unreviewed_paths_never_reach_upstream() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let native =
            NativeTransport::new(format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let spy = Arc::new(Spy {
            calls: AtomicUsize::new(0),
        });
        let gateway = Gateway::new(12345, native, spy.clone());
        let base = gateway.base_url();
        let router = gateway.router();
        for path in [
            "memories/trace_summarize",
            "alpha/search",
            "images/generations",
            "images/edits",
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("{base}/{path}"))
                        .header("host", "127.0.0.1:12345")
                        .body(Body::from(r#"{"model":"webbridge/test"}"#))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let error = response.into_body().collect().await.unwrap().to_bytes();
            assert!(
                std::str::from_utf8(&error)
                    .unwrap()
                    .contains("E_WEB_CAPABILITY_UNSUPPORTED")
            );
        }
        for (method, path) in [
            ("GET", "alpha/search"),
            ("POST", "alpha/search/extra"),
            ("POST", "images%2fedits"),
            ("POST", "realtime/unreviewed"),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(format!("{base}/{path}"))
                        .header("host", "127.0.0.1:12345")
                        .body(Body::from("{}"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
        assert_eq!(spy.calls.load(Ordering::SeqCst), 0);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn realtime_call_preserves_json_sdp_and_location_without_browser_access() {
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let native =
            NativeTransport::new(format!("http://{}", upstream.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(async move {
            axum::serve(
                upstream,
                Router::new().route(
                    "/realtime/calls",
                    any(|uri: Uri, headers: HeaderMap, bytes: Bytes| async move {
                        assert_eq!(uri.query(), Some("intent=quicksilver&architecture=avas"));
                        assert_eq!(headers["authorization"], "Bearer MOCK_NATIVE_SECRET");
                        Response::builder()
                            .status(StatusCode::CREATED)
                            .header(
                                "location",
                                "https://api.openai.com/v1/realtime/calls/fixture-call",
                            )
                            .header("content-type", headers["content-type"].clone())
                            .body(Body::from(bytes))
                            .unwrap()
                    }),
                ),
            )
            .await
            .unwrap();
        });
        let spy = Arc::new(Spy {
            calls: AtomicUsize::new(0),
        });
        let gateway = Gateway::new(12345, native, spy.clone());
        let base = gateway.base_url();
        assert!(base.contains("/backend-api"));
        gateway
            .disconnect_web(Duration::from_secs(1))
            .await
            .unwrap();
        let router = gateway.router();
        for (media, body) in [
            (
                "application/json",
                "{ \"sdp\":\"v=0\\r\\n\", \"session\":{\"model\":\"native-realtime\"}}\n",
            ),
            (
                "application/json; charset=utf-8",
                "{\"sdp\":\"v=0\",\"session\":{\"model\":null}}",
            ),
            ("application/sdp", "v=0\r\no=fixture\r\n"),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!(
                            "{base}/realtime/calls?intent=quicksilver&architecture=avas"
                        ))
                        .header("host", "127.0.0.1:12345")
                        .header("authorization", "Bearer MOCK_NATIVE_SECRET")
                        .header("content-type", media)
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::CREATED);
            assert_eq!(
                response.headers()["location"],
                "https://api.openai.com/v1/realtime/calls/fixture-call"
            );
            assert_eq!(response.headers()["content-type"], media);
            assert_eq!(
                response.into_body().collect().await.unwrap().to_bytes(),
                body
            );
        }
        assert_eq!(spy.calls.load(Ordering::SeqCst), 0);
        server.abort();
    }

    #[tokio::test]
    async fn realtime_rejects_owned_nested_models_and_unqualified_media_before_connecting() {
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let native =
            NativeTransport::new(format!("http://{}", upstream.local_addr().unwrap())).unwrap();
        let gateway = Gateway::new(12345, native, Arc::new(UnqualifiedProvider));
        let base = gateway.base_url();
        let router = gateway.router();
        for (media, body, status) in [
            (
                "application/json",
                r#"{"sdp":"v=0","session":{"model":"webbridge/test"}}"#,
                StatusCode::BAD_REQUEST,
            ),
            (
                "application/json",
                r#"{"sdp":"v=0","session":{"model":"native","model":"webbridge/test"}}"#,
                StatusCode::BAD_REQUEST,
            ),
            (
                "application/json",
                r#"{"sdp":"v=0","session":[],"model":"native"}"#,
                StatusCode::BAD_REQUEST,
            ),
            (
                "multipart/form-data; boundary=fixture",
                "fixture",
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
            ),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("{base}/realtime/calls"))
                        .header("host", "127.0.0.1:12345")
                        .header("content-type", media)
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), status);
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(50), upstream.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn request_branch_never_exposes_native_authorization_to_web_provider() {
        let spy = Arc::new(Spy {
            calls: AtomicUsize::new(0),
        });
        let gateway = Gateway::new(12345, NativeTransport::subscription().unwrap(), spy.clone());
        let url = format!("{}/responses", gateway.base_url());
        let request = Request::builder()
            .method("POST")
            .uri(url)
            .header("host", "127.0.0.1:12345")
            .header("authorization", "Bearer NATIVE_SECRET")
            .body(Body::from(r#"{"model":"webbridge/test","input":"hello"}"#))
            .unwrap();
        let response = gateway.router().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.into_body().collect().await.unwrap().to_bytes(),
            "web branch"
        );
        assert_eq!(spy.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn compressed_requests_preserve_native_bytes_and_enforce_decoded_web_limits() {
        let native_body =
            zstd::stream::encode_all(&br#"{"model":"native","input":"fixture"}"#[..], 1).unwrap();
        let expected = native_body.clone();
        let received = Arc::new(AtomicUsize::new(0));
        let counter = received.clone();
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let native =
            NativeTransport::new(format!("http://{}", upstream.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(async move {
            axum::serve(
                upstream,
                Router::new().route(
                    "/responses",
                    any(move |headers: HeaderMap, bytes: Bytes| {
                        let expected = expected.clone();
                        let counter = counter.clone();
                        async move {
                            assert_eq!(bytes.as_ref(), expected.as_slice());
                            assert_eq!(headers["content-encoding"], "zstd");
                            assert_eq!(headers["authorization"], "Bearer MOCK_NATIVE_SECRET");
                            counter.fetch_add(1, Ordering::SeqCst);
                            Response::builder()
                                .header("content-encoding", "zstd")
                                .body(Body::from(bytes))
                                .unwrap()
                        }
                    }),
                ),
            )
            .await
            .unwrap();
        });
        let spy = Arc::new(Spy {
            calls: AtomicUsize::new(0),
        });
        let gateway = Gateway::new(12345, native, spy.clone());
        let base = gateway.base_url();
        let router = gateway.router();
        let request = |body: Vec<u8>| {
            Request::builder()
                .method("POST")
                .uri(format!("{base}/responses"))
                .header("host", "127.0.0.1:12345")
                .header("content-encoding", "zstd")
                .header("content-type", "application/json")
                .header("authorization", "Bearer MOCK_NATIVE_SECRET")
                .body(Body::from(body))
                .unwrap()
        };
        let response = router
            .clone()
            .oneshot(request(native_body.clone()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-encoding"], "zstd");
        assert_eq!(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .as_ref(),
            native_body.as_slice()
        );
        let web =
            zstd::stream::encode_all(&br#"{"model":"webbridge/test","input":"fixture"}"#[..], 1)
                .unwrap();
        assert_eq!(
            router.clone().oneshot(request(web)).await.unwrap().status(),
            StatusCode::OK
        );
        let duplicate =
            zstd::stream::encode_all(&br#"{"model":"native","model":"webbridge/test"}"#[..], 1)
                .unwrap();
        assert_eq!(
            router
                .clone()
                .oneshot(request(duplicate))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        let large =
            serde_json::json!({"model":"webbridge/test","input":"x".repeat(8 * 1024 * 1024)})
                .to_string();
        let large = zstd::stream::encode_all(large.as_bytes(), 1).unwrap();
        assert!(large.len() < 8192);
        assert_eq!(
            router.oneshot(request(large)).await.unwrap().status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(received.load(Ordering::SeqCst), 1);
        assert_eq!(spy.calls.load(Ordering::SeqCst), 1);
        server.abort();
    }

    #[tokio::test]
    async fn malformed_request_cannot_choose_a_different_route_via_duplicate_model() {
        let spy = Arc::new(Spy {
            calls: AtomicUsize::new(0),
        });
        let gateway = Gateway::new(12345, NativeTransport::subscription().unwrap(), spy.clone());
        let url = format!("{}/responses", gateway.base_url());
        let request = Request::builder()
            .method("POST")
            .uri(url)
            .header("host", "127.0.0.1:12345")
            .body(Body::from(r#"{"model":"native","model":"webbridge/test"}"#))
            .unwrap();
        assert_eq!(
            gateway.router().oneshot(request).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(spy.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn catalog_augmentation_requires_an_explicit_qualified_provider_snapshot() {
        struct CatalogOnly;
        impl WebProvider for CatalogOnly {
            fn respond(&self, _: WebRequest) -> WebFuture {
                panic!("catalog must not generate");
            }
            fn catalog(
                &self,
                codec: cxweb_codex_adapter::catalog_codec::CatalogCodec,
            ) -> Option<crate::catalog_proxy::OwnedCatalog> {
                (codec == cxweb_codex_adapter::catalog_codec::CatalogCodec::Cli01551).then(|| {
                    crate::catalog_proxy::OwnedCatalog {
                        codec: "test-codec".into(),
                        web_scope: "synthetic-scope".into(),
                        generation: 1,
                        entries: vec![cxweb_codex_adapter::catalog::synthetic_model()],
                    }
                })
            }
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route(
                    "/models",
                    axum::routing::get(|| async {
                        axum::Json(serde_json::json!({"models":[{"slug":"native"}]}))
                    }),
                ),
            )
            .await
            .unwrap();
        });
        let gateway = Gateway::new(
            12345,
            NativeTransport::new(format!("http://{address}")).unwrap(),
            Arc::new(CatalogOnly),
        );
        for (query, count, disconnect) in [
            ("0.155.1", 2, false),
            ("unknown-codec", 1, false),
            ("0.155.1&client_version=0.155.1", 1, false),
            ("0.155.0", 1, false),
            ("0.155.1", 1, true),
        ] {
            if disconnect {
                gateway
                    .disconnect_web(Duration::from_secs(1))
                    .await
                    .unwrap();
            }
            let response = gateway
                .clone()
                .router()
                .oneshot(
                    Request::builder()
                        .uri(format!(
                            "{}/models?client_version={query}",
                            gateway.base_url()
                        ))
                        .header("host", "127.0.0.1:12345")
                        .header("user-agent", "codex_cli_rs/0.155.1 (Windows 11)")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let catalog: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(catalog["models"].as_array().unwrap().len(), count);
        }
        server.abort();
    }

    struct WaitingProvider {
        started: Arc<Notify>,
        cancelled: Arc<Notify>,
        cleanup: Arc<tokio::sync::Semaphore>,
    }
    #[tokio::test]
    async fn recovering_catalog_cannot_overwrite_a_complete_native_client_cache() {
        struct RecoveringCatalog(AtomicUsize);
        impl WebProvider for RecoveringCatalog {
            fn respond(&self, _: WebRequest) -> WebFuture {
                panic!("catalog and native requests must not generate in the browser");
            }
            fn health(&self) -> ProviderHealth {
                match self.0.load(Ordering::SeqCst) {
                    0 => ProviderHealth::Recovering,
                    1 => ProviderHealth::Verified { observed_at: None },
                    _ => ProviderHealth::Unavailable {
                        code: "E_LOGIN_REQUIRED",
                    },
                }
            }
            fn catalog(
                &self,
                _: cxweb_codex_adapter::catalog_codec::CatalogCodec,
            ) -> Option<crate::catalog_proxy::OwnedCatalog> {
                (self.0.load(Ordering::SeqCst) == 1).then(|| crate::catalog_proxy::OwnedCatalog {
                    codec: "test-codec".into(),
                    web_scope: "fixture".into(),
                    generation: 1,
                    entries: vec![cxweb_codex_adapter::catalog::synthetic_model()],
                })
            }
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let native =
            NativeTransport::new(format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, Router::new()
                .route("/models", axum::routing::get(|| async { axum::Json(serde_json::json!({"models":[{"slug":"native","future_field":"preserved"}]})) }))
                .route("/responses", axum::routing::post(|body: Bytes| async { body })))
                .await.unwrap();
        });
        let provider = Arc::new(RecoveringCatalog(AtomicUsize::new(1)));
        let gateway = Gateway::new(12345, native, provider.clone());
        let request = |build: &str, etag: Option<&str>| {
            let agent = if build == "0.155.0" {
                "Codex Desktop/0.155.0-alpha.9.2 (Windows 11)".to_owned()
            } else {
                format!("codex_cli_rs/{build} (Windows 11)")
            };
            let mut request = Request::builder()
                .uri(format!(
                    "{}/models?client_version={build}",
                    gateway.base_url()
                ))
                .header("host", "127.0.0.1:12345")
                .header("user-agent", agent);
            if let Some(etag) = etag {
                request = request.header("if-none-match", etag);
            }
            request.body(Body::empty()).unwrap()
        };
        let response = gateway
            .clone()
            .router()
            .oneshot(request("0.155.1", None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let etag = response.headers()["etag"].to_str().unwrap().to_owned();
        let cached = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            serde_json::from_slice::<Value>(&cached).unwrap()["models"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        provider.0.store(0, Ordering::SeqCst);
        for (build, validator) in [
            ("0.155.1", None),
            ("0.155.1", Some(etag.as_str())),
            ("0.155.0", None),
            ("0.155.0", Some(etag.as_str())),
        ] {
            let response = gateway
                .clone()
                .router()
                .oneshot(request(build, validator))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            assert_eq!(response.headers()["cache-control"], "no-store");
            assert_eq!(response.headers()["retry-after"], "1");
            assert!(!response.headers().contains_key("etag"));
        }
        // Unknown versions retain native passthrough, and native generation is
        // independent of browser recovery and does not wait for it.
        let response = gateway
            .clone()
            .router()
            .oneshot(request("unknown", None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let native_body = r#"{"model":"native","input":"fixture"}"#;
        let response = gateway
            .clone()
            .router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("{}/responses", gateway.base_url()))
                    .header("host", "127.0.0.1:12345")
                    .body(Body::from(native_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.into_body().collect().await.unwrap().to_bytes(),
            native_body
        );
        provider.0.store(1, Ordering::SeqCst);
        let response = gateway
            .clone()
            .router()
            .oneshot(request("0.155.1", Some(&etag)))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        provider.0.store(2, Ordering::SeqCst);
        let response = gateway
            .clone()
            .router()
            .oneshot(request("0.155.1", None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            serde_json::from_slice::<Value>(
                &response.into_body().collect().await.unwrap().to_bytes()
            )
            .unwrap(),
            serde_json::json!({"models":[{"slug":"native","future_field":"preserved"}]})
        );
        server.abort();
    }
    #[tokio::test]
    async fn provider_panic_does_not_certify_successful_cleanup() {
        struct Panicking;
        impl WebProvider for Panicking {
            fn respond(&self, _: WebRequest) -> WebFuture {
                Box::pin(async { panic!("synthetic worker failure") })
            }
        }
        let gateway = Gateway::new(
            12345,
            NativeTransport::subscription().unwrap(),
            Arc::new(Panicking),
        );
        let response = gateway
            .clone()
            .router()
            .oneshot(owned_request(&gateway.base_url()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            gateway.disconnect_web(Duration::from_secs(1)).await,
            Err("E_WEB_CLEANUP_UNCONFIRMED")
        );
        let response = gateway
            .clone()
            .router()
            .oneshot(owned_request(&gateway.base_url()))
            .await
            .unwrap();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert!(
            std::str::from_utf8(&bytes)
                .unwrap()
                .contains("E_WEB_DISCONNECTED")
        );
    }
    impl WebProvider for WaitingProvider {
        fn respond(&self, request: WebRequest) -> WebFuture {
            let (started, cancelled, cleanup) = (
                self.started.clone(),
                self.cancelled.clone(),
                self.cleanup.clone(),
            );
            Box::pin(async move {
                started.notify_one();
                request.cancellation.cancelled().await;
                cancelled.notify_one();
                let _permit = cleanup.acquire().await.unwrap();
                unavailable("E_CANCELLED")
            })
        }
    }
    fn owned_request(base: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(format!("{base}/responses"))
            .header("host", "127.0.0.1:12345")
            .body(Body::from(r#"{"model":"webbridge/test","input":"hello"}"#))
            .unwrap()
    }

    #[tokio::test]
    async fn disconnect_and_client_drop_wait_for_actual_provider_cleanup() {
        for client_drop in [false, true] {
            let started = Arc::new(Notify::new());
            let cancelled = Arc::new(Notify::new());
            let cleanup = Arc::new(tokio::sync::Semaphore::new(0));
            let gateway = Gateway::new(
                12345,
                NativeTransport::subscription().unwrap(),
                Arc::new(WaitingProvider {
                    started: started.clone(),
                    cancelled: cancelled.clone(),
                    cleanup: cleanup.clone(),
                }),
            );
            let request = owned_request(&gateway.base_url());
            let router = gateway.clone().router();
            let pending = tokio::spawn(async move { router.oneshot(request).await.unwrap() });
            tokio::time::timeout(Duration::from_secs(1), started.notified())
                .await
                .unwrap();
            #[cfg(windows)]
            assert_eq!(gateway.health().active_turns, 1);
            if client_drop {
                pending.abort();
                tokio::time::timeout(Duration::from_secs(1), cancelled.notified())
                    .await
                    .unwrap();
            }
            assert_eq!(
                gateway.disconnect_web(Duration::from_millis(20)).await,
                Err("E_WEB_DRAIN_TIMEOUT")
            );
            if !client_drop {
                tokio::time::timeout(Duration::from_secs(1), cancelled.notified())
                    .await
                    .unwrap();
            }
            let rejected = gateway
                .clone()
                .router()
                .oneshot(owned_request(&gateway.base_url()))
                .await
                .unwrap();
            let body = rejected.into_body().collect().await.unwrap().to_bytes();
            assert!(
                std::str::from_utf8(&body)
                    .unwrap()
                    .contains("E_WEB_DISCONNECTED")
            );
            #[cfg(windows)]
            assert_eq!(gateway.health().active_turns, 1);
            cleanup.add_permits(1);
            gateway
                .disconnect_web(Duration::from_secs(1))
                .await
                .unwrap();
            #[cfg(windows)]
            assert_eq!(gateway.health().active_turns, 0);
            if !client_drop {
                assert_eq!(pending.await.unwrap().status(), StatusCode::BAD_GATEWAY);
            }
            gateway
                .disconnect_web(Duration::from_secs(1))
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn owned_context_never_crosses_to_native_after_model_switch_or_disconnect() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let spy = Arc::new(Spy {
            calls: AtomicUsize::new(0),
        });
        let gateway = Gateway::new(
            12345,
            NativeTransport::new(format!("http://{}", listener.local_addr().unwrap())).unwrap(),
            spy.clone(),
        );
        for disconnected in [false, true] {
            if disconnected {
                gateway
                    .disconnect_web(Duration::from_secs(1))
                    .await
                    .unwrap();
            }
            for path in ["responses", "responses/compact"] {
                for payload in [
                    json!({"model":"native","previous_response_id":"resp_cxweb_fixture"}),
                    json!({"model":"native","input":[{"type":"compaction","encrypted_content":"wbr1:fixture"}]}),
                    json!({"model":"native","input":[{"type":"item_reference","id":"cmp_cxweb_fixture"}]}),
                ] {
                    for compressed in [false, true] {
                        let text = payload.to_string();
                        let bytes = if compressed {
                            zstd::stream::encode_all(text.as_bytes(), 1).unwrap()
                        } else {
                            text.into_bytes()
                        };
                        let mut request = Request::builder()
                            .method("POST")
                            .uri(format!("{}/{path}", gateway.base_url()))
                            .header("host", "127.0.0.1:12345");
                        if compressed {
                            request = request.header("content-encoding", "zstd");
                        }
                        let response = gateway
                            .clone()
                            .router()
                            .oneshot(request.body(Body::from(bytes)).unwrap())
                            .await
                            .unwrap();
                        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
                        let bytes = response.into_body().collect().await.unwrap().to_bytes();
                        assert!(
                            std::str::from_utf8(&bytes)
                                .unwrap()
                                .contains("E_NONPORTABLE_CONTEXT")
                        );
                    }
                }
            }
        }
        assert_eq!(spy.calls.load(Ordering::SeqCst), 0);
        assert!(
            tokio::time::timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn disconnected_listener_forwards_native_http_but_rejects_owned_responses_and_compaction()
    {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let native_calls = Arc::new(AtomicUsize::new(0));
        let calls = native_calls.clone();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().fallback(any(move |headers: HeaderMap, bytes: Bytes| {
                    let calls = calls.clone();
                    async move {
                        assert_eq!(headers["authorization"], "Bearer SYNTHETIC_NATIVE");
                        calls.fetch_add(1, Ordering::SeqCst);
                        (StatusCode::ACCEPTED, bytes)
                    }
                })),
            )
            .await
            .unwrap();
        });
        let spy = Arc::new(Spy {
            calls: AtomicUsize::new(0),
        });
        let gateway = Gateway::new(
            12345,
            NativeTransport::new(format!("http://{address}")).unwrap(),
            spy.clone(),
        );
        gateway
            .disconnect_web(Duration::from_secs(1))
            .await
            .unwrap();
        for path in ["responses", "responses/compact"] {
            for model in ["native", "webbridge/test"] {
                let body = format!("{{\"model\":\"{model}\",\"input\":\"literal input\"}}");
                let response = gateway
                    .clone()
                    .router()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri(format!("{}/{path}", gateway.base_url()))
                            .header("host", "127.0.0.1:12345")
                            .header("authorization", "Bearer SYNTHETIC_NATIVE")
                            .body(Body::from(body.clone()))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                if model == "native" {
                    assert_eq!(response.status(), StatusCode::ACCEPTED);
                    assert_eq!(
                        response.into_body().collect().await.unwrap().to_bytes(),
                        body
                    );
                } else {
                    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
                    let bytes = response.into_body().collect().await.unwrap().to_bytes();
                    assert!(
                        std::str::from_utf8(&bytes)
                            .unwrap()
                            .contains("E_WEB_DISCONNECTED")
                    );
                }
            }
        }
        assert_eq!(native_calls.load(Ordering::SeqCst), 2);
        assert_eq!(spy.calls.load(Ordering::SeqCst), 0);
        server.abort();
    }
}
