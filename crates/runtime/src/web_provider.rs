//! Bind classified web HTTP requests to durable turn orchestration.
use crate::{
    gateway::{WebFuture, WebProvider, WebRequest},
    native::unavailable,
    turn::{Coordinator, TurnInput},
};
use axum::{body::Body, http::HeaderMap, response::Response};
use cxweb_codex_adapter::{request::CanonicalRequest, strict_json};
use cxweb_domain::SessionKey;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, sync::Arc};

fn digest(parts: &[&[u8]]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    format!("{:x}", hash.finalize())
}

#[derive(Clone)]
pub struct WebIdentity {
    native_session: String,
    turn: String,
}
impl WebIdentity {
    /// Read only reviewed correlation fields. Native authorization, account
    /// headers, routing tokens and the rest of turn metadata never leave here.
    pub(crate) fn from_headers(headers: &HeaderMap) -> Option<Self> {
        let one = |name| {
            let mut values = headers.get_all(name).iter();
            let value = values.next()?.to_str().ok()?;
            values.next().is_none().then_some(value)
        };
        let valid = |value: &str| {
            !value.is_empty() && value.len() <= 256 && value.bytes().all(|b| b.is_ascii_graphic())
        };
        let session = one("session-id").filter(|v| valid(v))?;
        let thread = one("thread-id").filter(|v| valid(v))?;
        let metadata =
            strict_json::parse(one("x-codex-turn-metadata")?.as_bytes(), 64 * 1024).ok()?;
        let turn = metadata.get("turn_id")?.as_str().filter(|v| valid(v))?;
        for (name, expected) in [("session_id", session), ("thread_id", thread)] {
            if metadata
                .get(name)
                .is_some_and(|v| v.as_str() != Some(expected))
            {
                return None;
            }
        }
        let context = match metadata.get("context_window_id") {
            None | Some(Value::Null) => "",
            Some(Value::String(value)) if valid(value) => value,
            _ => return None,
        };
        Some(Self {
            native_session: digest(&[session.as_bytes(), thread.as_bytes(), context.as_bytes()]),
            turn: digest(&[turn.as_bytes()]),
        })
    }
}

/// Runtime-owned scope from account/workspace qualification, never request data.
pub struct ProviderScope {
    pub installation: String,
    pub account: String,
    pub workspace: String,
    pub epoch: u64,
}

#[derive(Clone)]
pub struct CoordinatorProvider {
    coordinator: Coordinator,
    scope: Arc<ProviderScope>,
    routes: Arc<BTreeSet<String>>,
}
impl CoordinatorProvider {
    /// The activation owner supplies only routes whose codec and browser driver
    /// passed qualification. This constructor does not discover or advertise models.
    pub fn new(
        coordinator: Coordinator,
        scope: ProviderScope,
        routes: Vec<String>,
    ) -> Result<Self, &'static str> {
        if [&scope.installation, &scope.account, &scope.workspace]
            .iter()
            .any(|v| v.is_empty() || v.len() > 512 || v.chars().any(char::is_control))
            || routes.is_empty()
            || routes.len() > 256
            || routes.iter().any(|route| {
                !route.starts_with(cxweb_domain::OWNED_MODEL_PREFIX)
                    || route.len() <= cxweb_domain::OWNED_MODEL_PREFIX.len()
                    || route.len() > 512
                    || route.chars().any(char::is_control)
            })
        {
            return Err("E_PROVIDER_SCOPE");
        }
        let count = routes.len();
        let routes: BTreeSet<_> = routes.into_iter().collect();
        if routes.len() != count {
            return Err("E_PROVIDER_SCOPE");
        }
        Ok(Self {
            coordinator,
            scope: Arc::new(scope),
            routes: Arc::new(routes),
        })
    }

    async fn execute(&self, request: WebRequest) -> Result<Response, &'static str> {
        if request.compact {
            return Err("E_COMPACTION_UNQUALIFIED");
        }
        let identity = request.identity.ok_or("E_REQUEST_IDENTITY")?;
        let mut payload = request.payload;
        let original = serde_json::to_vec(&payload).map_err(|_| "E_INVALID_REQUEST")?;
        let decoded = CanonicalRequest::decode(&original)?;
        if !self.routes.contains(&decoded.model) {
            return Err("E_MODEL_UNAVAILABLE");
        }
        let stream = decoded.stream;
        // Delivery format and tracing metadata cannot turn a retry into a second
        // generation. All generation/tool/history fields remain in the digest.
        let object = payload.as_object_mut().ok_or("E_INVALID_REQUEST")?;
        object.remove("stream");
        object.remove("client_metadata");
        let bytes = serde_json::to_vec(&payload).map_err(|_| "E_INVALID_REQUEST")?;
        let session = SessionKey {
            installation: self.scope.installation.clone(),
            native_session: identity.native_session,
            account_scope: self.scope.account.clone(),
            workspace_scope: self.scope.workspace.clone(),
            route: decoded.model,
            epoch: self.scope.epoch,
        };
        let scope = serde_json::to_vec(&session).map_err(|_| "E_PROVIDER_SCOPE")?;
        let request_id = digest(&[&scope, identity.turn.as_bytes(), &bytes]);
        let delivery = self
            .coordinator
            .execute(
                TurnInput {
                    request_id,
                    session,
                    bytes,
                },
                request.cancellation,
            )
            .await?;
        Response::builder()
            .header(
                "content-type",
                if stream {
                    "text/event-stream"
                } else {
                    "application/json"
                },
            )
            .header("cache-control", "no-store")
            .header("x-cxweb-delivery", "buffered")
            .body(Body::from(if stream {
                delivery.sse
            } else {
                delivery.json
            }))
            .map_err(|_| "E_RESPONSE_ENCODING")
    }
}
impl WebProvider for CoordinatorProvider {
    fn respond(&self, request: WebRequest) -> WebFuture {
        let provider = self.clone();
        Box::pin(async move { provider.execute(request).await.unwrap_or_else(unavailable) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        gateway::Gateway,
        ledger::Ledger,
        native::NativeTransport,
        turn::{BrowserDriver, BrowserFuture, Prepared},
    };
    use axum::http::{Request, StatusCode};
    use cxweb_browser_adapter::turn::{Baseline, Observation};
    use http_body_util::BodyExt;
    use serde_json::json;
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::sync::Notify;
    use tower::ServiceExt;

    struct Browser {
        sends: AtomicUsize,
        stops: AtomicUsize,
        sessions: Mutex<Vec<SessionKey>>,
        nonce: Mutex<String>,
        waiting: bool,
        observing: Notify,
    }
    impl BrowserDriver for Browser {
        fn prepare(&self, session: SessionKey) -> BrowserFuture<Prepared> {
            self.sessions.lock().unwrap().push(session.clone());
            Box::pin(async move {
                Ok(Prepared {
                    handle: "fixture-target".into(),
                    verified_route: session.route.clone(),
                    verified_session: session,
                    verified_effort: None,
                    baseline: Baseline {
                        ids: vec![],
                        selected_model: "Fixture text".into(),
                        composer_empty: true,
                        generating: false,
                    },
                })
            })
        }
        fn submit(&self, _: String, prompt: String, _: String) -> BrowserFuture<()> {
            self.sends.fetch_add(1, Ordering::SeqCst);
            for forbidden in [
                "NATIVE_SECRET",
                "RAW_SESSION",
                "RAW_THREAD",
                "PRIVATE_METADATA",
            ] {
                assert!(!prompt.contains(forbidden));
            }
            *self.nonce.lock().unwrap() = prompt
                .split("turn_nonce=")
                .nth(1)
                .unwrap()
                .split('.')
                .next()
                .unwrap()
                .to_owned();
            Box::pin(async { Ok(()) })
        }
        fn observe(&self, _: String) -> BrowserFuture<Observation> {
            self.observing.notify_one();
            let text = json!({"protocol":"webbridge.tool.v1","turn_nonce":self.nonce.lock().unwrap().clone(),"kind":"final","text":"fixture answer"}).to_string();
            let waiting = self.waiting;
            Box::pin(async move {
                Ok(Observation {
                    user_id: Some("new-user".into()),
                    user_matches: true,
                    assistant_id: Some("new-assistant".into()),
                    text,
                    generating: waiting,
                    completion_control: !waiting,
                    fenced_output: false,
                    selected_model: "Fixture text".into(),
                    ambiguous: false,
                })
            })
        }
        fn stop(&self, _: String) -> BrowserFuture<bool> {
            self.stops.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(true) })
        }
        fn release(&self, _: String) -> BrowserFuture<()> {
            Box::pin(async { Ok(()) })
        }
    }
    fn fixture(waiting: bool) -> (Gateway, Arc<Browser>) {
        let browser = Arc::new(Browser {
            sends: AtomicUsize::new(0),
            stops: AtomicUsize::new(0),
            sessions: Mutex::new(vec![]),
            nonce: Mutex::new(String::new()),
            waiting,
            observing: Notify::new(),
        });
        let coordinator = Coordinator::new(Ledger::in_memory(), browser.clone());
        let provider = CoordinatorProvider::new(
            coordinator,
            ProviderScope {
                installation: "fixture-installation".into(),
                account: "qualified-account".into(),
                workspace: "qualified-workspace".into(),
                epoch: 4,
            },
            vec!["webbridge/test".into()],
        )
        .unwrap();
        (
            Gateway::new(
                12345,
                NativeTransport::new("http://127.0.0.1:1".into()).unwrap(),
                Arc::new(provider),
            ),
            browser,
        )
    }
    fn request(base: &str, turn: &str, context: &str, stream: bool) -> Request<Body> {
        Request::builder().method("POST").uri(format!("{base}/responses"))
            .header("host", "127.0.0.1:12345")
            .header("authorization", "Bearer NATIVE_SECRET")
            .header("session-id", "RAW_SESSION")
            .header("thread-id", "RAW_THREAD")
            .header("x-client-request-id", "RAW_THREAD")
            .header("x-codex-turn-metadata", json!({"turn_id":turn,"context_window_id":context,"session_id":"RAW_SESSION","thread_id":"RAW_THREAD","extra":"PRIVATE_METADATA"}).to_string())
            .body(Body::from(json!({"model":"webbridge/test","input":"fixture task","stream":stream,"client_metadata":{"trace":"PRIVATE_METADATA"}}).to_string())).unwrap()
    }
    #[tokio::test]
    async fn gateway_coordinator_replays_retries_but_distinguishes_new_turns_and_contexts() {
        let (gateway, browser) = fixture(false);
        let base = gateway.base_url();
        let router = gateway.router();
        let first = router
            .clone()
            .oneshot(request(&base, "turn-one", "context-one", false))
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);
        assert_eq!(first.headers()["x-cxweb-delivery"], "buffered");
        let body = first.into_body().collect().await.unwrap().to_bytes();
        let original: Value = serde_json::from_slice(&body).unwrap();
        let retry = router
            .clone()
            .oneshot(request(&base, "turn-one", "context-one", true))
            .await
            .unwrap();
        assert_eq!(retry.headers()["content-type"], "text/event-stream");
        let sse = String::from_utf8(
            retry
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap();
        assert!(sse.contains(original["id"].as_str().unwrap()));
        assert!(sse.contains("fixture answer"));
        assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
        for (turn, context) in [("turn-two", "context-one"), ("turn-two", "context-two")] {
            let result = router
                .clone()
                .oneshot(request(&base, turn, context, false))
                .await
                .unwrap();
            assert_eq!(result.status(), StatusCode::OK);
        }
        assert_eq!(browser.sends.load(Ordering::SeqCst), 3);
        let sessions = browser.sessions.lock().unwrap();
        assert_eq!(sessions[0].native_session, sessions[1].native_session);
        assert_ne!(sessions[1].native_session, sessions[2].native_session);
        assert_eq!(sessions[0].account_scope, "qualified-account");
        assert_eq!(sessions[0].workspace_scope, "qualified-workspace");
        assert_eq!(sessions[0].epoch, 4);
    }
    #[tokio::test]
    async fn missing_conflicting_identity_and_unknown_routes_never_prepare_browser() {
        let (gateway, browser) = fixture(false);
        let base = gateway.base_url();
        let router = gateway.router();
        for variant in 0..4 {
            let mut req = request(&base, "turn", "context", false);
            match variant {
                0 => {
                    req.headers_mut().remove("x-codex-turn-metadata");
                }
                1 => {
                    req.headers_mut()
                        .append("thread-id", "OTHER_THREAD".parse().unwrap());
                }
                2 => {
                    req.headers_mut()
                        .insert("thread-id", "OTHER_THREAD".parse().unwrap());
                }
                _ => {
                    *req.body_mut() =
                        Body::from(r#"{"model":"webbridge/unpublished","input":"fixture"}"#);
                }
            }
            assert_eq!(
                router.clone().oneshot(req).await.unwrap().status(),
                StatusCode::BAD_GATEWAY
            );
        }
        assert!(browser.sessions.lock().unwrap().is_empty());
        assert_eq!(browser.sends.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn disconnect_reaches_the_bound_coordinator_and_waits_for_browser_stop() {
        let (gateway, browser) = fixture(true);
        let req = request(&gateway.base_url(), "turn", "context", false);
        let router = gateway.clone().router();
        let task = tokio::spawn(async move { router.oneshot(req).await.unwrap() });
        browser.observing.notified().await;
        gateway
            .disconnect_web(std::time::Duration::from_secs(3))
            .await
            .unwrap();
        let response = task.await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(browser.stops.load(Ordering::SeqCst), 1);
        assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
    }
}
