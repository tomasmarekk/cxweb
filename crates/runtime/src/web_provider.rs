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

/// In-process evidence only. Never serialized into HTTP headers or model data.
#[derive(Clone, Debug)]
pub(crate) enum BrowserEvidence {
    Verified,
    Failure(&'static str),
}

/// Reviewed clients recognize HTTP 400 as terminal; generic 422/409 refusals
/// were retried. Only known pre-admission queue failures permit an automatic
/// retry. Unknown web failures may follow submission and must fail closed.
pub(crate) fn web_failure(code: &'static str) -> Response {
    use axum::http::StatusCode;
    let mut response = unavailable(code);
    if code == "E_UNSUPPORTED_REASONING_SUMMARY" {
        // An already-running native client can retain fallback model metadata
        // after installation. Do not silently discard a requested capability.
        *response.body_mut() = Body::from(
            serde_json::json!({"error":{
                "code":code,
                "message":"Unsupported reasoning summary option. Use auto, concise, detailed or none. cxweb returns only public progress shown by ChatGPT."
            }})
            .to_string(),
        );
    }
    response
        .extensions_mut()
        .insert(BrowserEvidence::Failure(code));
    *response.status_mut() = match code {
        "E_QUEUE_FULL" | "E_QUEUE_TIMEOUT" => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::BAD_REQUEST,
    };
    response
}

fn digest(parts: &[&[u8]]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    format!("{:x}", hash.finalize())
}

#[derive(Clone, PartialEq, Eq)]
pub struct WebIdentity {
    native_session: String,
    task: String,
    turn: String,
}
impl WebIdentity {
    pub(crate) fn same_session(&self, other: &Self) -> bool {
        self.native_session == other.native_session
    }
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
            task: digest(&[session.as_bytes(), thread.as_bytes()]),
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
    catalog: Option<Arc<crate::catalog_snapshot::CatalogSnapshot>>,
    context_budget: Option<cxweb_codex_adapter::context_budget::LocalContextBudget>,
    #[cfg(windows)]
    native_fixture: Option<Arc<crate::native_fixture::Fixture>>,
    #[cfg(windows)]
    checkpoints: Option<(
        Arc<crate::checkpoint::Codec>,
        cxweb_codex_adapter::catalog_codec::CatalogCodec,
    )>,
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
            catalog: None,
            context_budget: None,
            #[cfg(windows)]
            native_fixture: None,
            #[cfg(windows)]
            checkpoints: None,
        })
    }

    #[cfg(windows)]
    pub(crate) fn with_native_fixture(
        mut self,
        fixture: Arc<crate::native_fixture::Fixture>,
    ) -> Self {
        self.native_fixture = Some(fixture);
        self
    }

    /// Enable only for an explicitly selected, reviewed v2 client codec.
    /// Production activation must additionally pass live compaction qualification.
    #[cfg(windows)]
    pub fn with_checkpoints(
        mut self,
        key: Arc<crate::checkpoint::Codec>,
        codec: cxweb_codex_adapter::catalog_codec::CatalogCodec,
    ) -> Result<Self, &'static str> {
        if self.checkpoints.is_some() {
            return Err("E_COMPACTION_UNQUALIFIED");
        }
        self.checkpoints = Some((key, codec));
        Ok(self)
    }

    /// Couple native recovery metadata and execution to one explicit local
    /// budget, after enabling its reviewed checkpoint codec and before publication.
    #[cfg(windows)]
    pub fn with_context_budget(
        mut self,
        budget: cxweb_codex_adapter::context_budget::LocalContextBudget,
    ) -> Result<Self, &'static str> {
        if self.checkpoints.is_none() || self.catalog.is_some() || self.context_budget.is_some() {
            return Err("E_CONTEXT_BUDGET_CONFIG");
        }
        self.context_budget = Some(budget);
        Ok(self)
    }

    fn prepare_payload(
        &self,
        payload: &mut Value,
        identity: &WebIdentity,
    ) -> Result<Option<Arc<dyn crate::turn::CheckpointEncoder>>, &'static str> {
        #[cfg(windows)]
        if let Some((key, codec)) = &self.checkpoints {
            let route = payload["model"]
                .as_str()
                .filter(|route| self.routes.contains(*route))
                .ok_or("E_MODEL_UNAVAILABLE")?;
            let bound = Arc::new(crate::compaction::BoundCheckpoint {
                codec: key.clone(),
                codec_id: format!("{}:compaction-v2-summary-v1", codec.id()),
                session: SessionKey {
                    installation: self.scope.installation.clone(),
                    native_session: identity.task.clone(),
                    account_scope: self.scope.account.clone(),
                    workspace_scope: self.scope.workspace.clone(),
                    route: route.into(),
                    epoch: self.scope.epoch,
                },
            });
            bound.expand(payload)?;
            if payload["input"].as_array().is_some_and(|items| {
                items
                    .last()
                    .is_some_and(|item| item["type"] == "compaction_trigger")
            }) {
                return Ok(Some(bound));
            }
        }
        #[cfg(not(windows))]
        let _ = (payload, identity);
        Ok(None)
    }

    /// Publish an immutable, explicitly qualified snapshot for selected clients.
    /// Callers must replace the provider on account/workspace/epoch changes.
    pub fn with_catalog(
        mut self,
        generation: u64,
        qualified: Vec<(
            cxweb_codex_adapter::catalog_codec::CatalogCodec,
            Vec<cxweb_codex_adapter::catalog_codec::CatalogRoute>,
        )>,
    ) -> Result<Self, &'static str> {
        if self.catalog.is_some() {
            return Err("E_CATALOG_SNAPSHOT");
        }
        self.catalog = Some(Arc::new(crate::catalog_snapshot::CatalogSnapshot::new(
            &self.scope,
            &self.routes,
            generation,
            qualified,
            self.context_budget,
        )?));
        Ok(self)
    }

    /// Preserve the scheduler and completed-turn replay while the installed
    /// owner replaces metadata within the same qualified scope.
    #[cfg(windows)]
    pub(crate) fn with_refreshed_catalog(
        &self,
        qualified: Vec<(
            cxweb_codex_adapter::catalog_codec::CatalogCodec,
            Vec<cxweb_codex_adapter::catalog_codec::CatalogRoute>,
        )>,
    ) -> Result<Self, &'static str> {
        if self.catalog.is_none() {
            return Err("E_CATALOG_SNAPSHOT");
        }
        let mut next = self.clone();
        next.catalog = None;
        next.with_catalog(2, qualified)
    }

    pub(crate) async fn execute(&self, request: WebRequest) -> Result<Response, &'static str> {
        if request.compact {
            return Err("E_COMPACTION_UNQUALIFIED");
        }
        let identity = request.identity.ok_or("E_REQUEST_IDENTITY")?;
        let mut payload = request.payload;
        let checkpoint = self.prepare_payload(&mut payload, &identity)?;
        let original = serde_json::to_vec(&payload).map_err(|_| "E_INVALID_REQUEST")?;
        let decoded = if checkpoint.is_some() {
            CanonicalRequest::decode_compaction(&original)?
        } else {
            CanonicalRequest::decode(&original)?
        };
        if !self.routes.contains(&decoded.model) {
            return Err("E_MODEL_UNAVAILABLE");
        }
        #[cfg(windows)]
        if let Some(budget) = self.context_budget {
            if checkpoint.is_some() {
                decoded
                    .browser_prompt("00000000000000000000000000000000", budget.summary_bytes())?;
            } else if crate::context_budget::needs_compaction(&payload, &decoded, budget)? {
                // Only the reviewed SSE contract has native recovery evidence.
                // JSON callers and unqualified codecs keep the explicit local error.
                if !decoded.stream {
                    return Err("E_CONTEXT_BUDGET");
                }
                return Ok(crate::context_budget::failure_response());
            }
        }
        let stream = decoded.stream;
        let hosted_search_unavailable = decoded.hosted_search_unavailable();
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
            .execute_with_progress(
                TurnInput {
                    request_id,
                    session,
                    bytes,
                },
                request.cancellation,
                checkpoint,
                request.progress,
            )
            .await?;
        #[cfg(windows)]
        if let Some(fixture) = &self.native_fixture {
            fixture.check_delivery(&delivery.json)?;
        }
        let mut response = Response::builder()
            .header(
                "content-type",
                if stream {
                    "text/event-stream"
                } else {
                    "application/json"
                },
            )
            .header("cache-control", "no-store")
            .header("x-cxweb-delivery", "buffered");
        if hosted_search_unavailable {
            response = response.header("x-cxweb-unavailable-tools", "web_search");
        }
        let live_verified = delivery.live_verified;
        let mut response = response
            .body(Body::from(if stream {
                delivery.sse
            } else {
                delivery.json
            }))
            .map_err(|_| "E_RESPONSE_ENCODING")?;
        if live_verified {
            response.extensions_mut().insert(BrowserEvidence::Verified);
        }
        Ok(response)
    }
}
impl WebProvider for CoordinatorProvider {
    fn catalog(
        &self,
        codec: cxweb_codex_adapter::catalog_codec::CatalogCodec,
    ) -> Option<crate::catalog_proxy::OwnedCatalog> {
        self.catalog.as_ref()?.for_client(codec)
    }
    fn validate_warmup(&self, request: &WebRequest) -> Result<(), &'static str> {
        let identity = request.identity.as_ref().ok_or("E_REQUEST_IDENTITY")?;
        let mut payload = request.payload.clone();
        if self.prepare_payload(&mut payload, identity)?.is_some() {
            return Err("E_COMPACTION_TRIGGER");
        }
        let bytes = serde_json::to_vec(&payload).map_err(|_| "E_INVALID_REQUEST")?;
        let decoded = CanonicalRequest::decode(&bytes)?;
        if !self.routes.contains(&decoded.model) {
            return Err("E_MODEL_UNAVAILABLE");
        }
        Ok(())
    }
    fn respond(&self, request: WebRequest) -> WebFuture {
        let provider = self.clone();
        Box::pin(async move { provider.execute(request).await.unwrap_or_else(web_failure) })
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
        prompt: Mutex<Value>,
        compact: std::sync::atomic::AtomicBool,
        answer_bytes: AtomicUsize,
        mismatched_effort: std::sync::atomic::AtomicBool,
        uncertain_submission: std::sync::atomic::AtomicBool,
        fixture_call: Mutex<Option<Value>>,
    }
    impl BrowserDriver for Browser {
        fn verify_completion(&self, _: String) -> BrowserFuture<()> {
            Box::pin(async { Ok(()) })
        }
        fn prepare(&self, session: SessionKey) -> BrowserFuture<Prepared> {
            self.sessions.lock().unwrap().push(session.clone());
            let effort = if self.mismatched_effort.load(Ordering::SeqCst) {
                "high"
            } else {
                "medium"
            };
            Box::pin(async move {
                Ok(Prepared {
                    handle: "fixture-target".into(),
                    verified_route: session.route.clone(),
                    verified_session: session,
                    verified_effort: Some(effort.into()),
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
            self.compact
                .store(prompt.contains("Use kind=checkpoint."), Ordering::SeqCst);
            *self.prompt.lock().unwrap() =
                serde_json::from_str(prompt.split_once("\nCLIENT_DATA_JSON\n").unwrap().1).unwrap();
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
            let uncertain = self.uncertain_submission.load(Ordering::SeqCst);
            Box::pin(async move {
                if uncertain {
                    Err("E_PIPE_TIMEOUT")
                } else {
                    Ok(())
                }
            })
        }
        fn observe(&self, _: String) -> BrowserFuture<Observation> {
            self.observing.notify_one();
            let nonce = self.nonce.lock().unwrap().clone();
            let text = if let Some(input) = self.fixture_call.lock().unwrap().clone() {
                let prompt = self.prompt.lock().unwrap();
                json!({"protocol":"webbridge.tool.v1","turn_nonce":nonce,"kind":"tool_calls","calls":[{"tool_key":prompt["tools"][0]["tool_key"],"input":input}]}).to_string()
            } else if self.compact.load(Ordering::SeqCst) {
                let prompt = self.prompt.lock().unwrap();
                let summary = json!({"goal":"Preserve fixture goal","constraints":["Read only"],"changed_files":[],"decisions":[],"outstanding_work":["Await pending result"],"test_results":["Previous read denied"],"unresolved_tool_ids":prompt["unresolved_tool_ids"]});
                json!({"protocol":"webbridge.tool.v1","turn_nonce":nonce,"kind":"checkpoint","summary":summary.to_string()}).to_string()
            } else {
                let size = self.answer_bytes.load(Ordering::SeqCst);
                let answer = if size == 0 {
                    "fixture answer".into()
                } else {
                    "fixture ".repeat(size / 8)
                };
                json!({"protocol":"webbridge.tool.v1","turn_nonce":nonce,"kind":"final","text":answer}).to_string()
            };
            let waiting = self.waiting;
            Box::pin(async move {
                Ok(Observation {
                    user_id: Some("new-user".into()),
                    user_matches: true,
                    assistant_id: Some("new-assistant".into()),
                    text,
                    summary: vec![],
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
    fn provider_fixture(waiting: bool) -> (CoordinatorProvider, Arc<Browser>) {
        let browser = Arc::new(Browser {
            sends: AtomicUsize::new(0),
            stops: AtomicUsize::new(0),
            sessions: Mutex::new(vec![]),
            nonce: Mutex::new(String::new()),
            waiting,
            observing: Notify::new(),
            prompt: Mutex::new(Value::Null),
            compact: std::sync::atomic::AtomicBool::new(false),
            answer_bytes: AtomicUsize::new(0),
            mismatched_effort: std::sync::atomic::AtomicBool::new(false),
            uncertain_submission: std::sync::atomic::AtomicBool::new(false),
            fixture_call: Mutex::new(None),
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
        (provider, browser)
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn encrypted_compaction_replays_and_restores_pending_calls_across_context_windows() {
        use crate::gateway::WebTransport;
        let (provider, browser) = provider_fixture(false);
        let directory = std::env::temp_dir().join(format!(
            "cxweb-compaction-flow-{:032x}",
            rand::random::<u128>()
        ));
        cxweb_platform::state::protected_directory(&directory).unwrap();
        let key_path = directory.join("key.dpapi");
        let key = Arc::new(
            crate::checkpoint::Codec::load_or_create(&key_path, "fixture-installation").unwrap(),
        );
        let unqualified = provider.clone();
        assert!(
            unqualified
                .clone()
                .with_context_budget(
                    cxweb_codex_adapter::context_budget::LocalContextBudget::DIAGNOSTIC
                )
                .is_err()
        );
        let provider = provider
            .with_checkpoints(
                key,
                cxweb_codex_adapter::catalog_codec::CatalogCodec::Cli01551,
            )
            .unwrap()
            .with_context_budget(
                cxweb_codex_adapter::context_budget::LocalContextBudget::DIAGNOSTIC,
            )
            .unwrap();
        let budget = cxweb_codex_adapter::context_budget::LocalContextBudget::DIAGNOSTIC;
        assert!(provider.clone().with_context_budget(budget).is_err());
        let codec = cxweb_codex_adapter::catalog_codec::CatalogCodec::Cli01551;
        let published = provider
            .clone()
            .with_catalog(
                1,
                vec![(
                    codec,
                    vec![cxweb_codex_adapter::catalog_codec::CatalogRoute {
                        id: "webbridge/test".into(),
                        observed_label: "Fixture text".into(),
                        effort: "medium".into(),
                        reasoning: vec![],
                        coding: false,
                    }],
                )],
            )
            .unwrap();
        assert_eq!(
            published.catalog(codec).unwrap().entries[0]["context_window"],
            budget.estimated_tokens()
        );
        assert!(published.with_context_budget(budget).is_err());
        let make_request = |payload, turn: &str, context: &str, task: &str| {
            let (mut parts, _) =
                request("http://127.0.0.1:12345", turn, context, false).into_parts();
            parts.headers.insert("thread-id", task.parse().unwrap());
            parts.headers.insert(
                "x-codex-turn-metadata",
                json!({"turn_id":turn,"context_window_id":context})
                    .to_string()
                    .parse()
                    .unwrap(),
            );
            WebRequest {
                payload,
                identity: WebIdentity::from_headers(&parts.headers),
                compact: false,
                cancellation: tokio_util::sync::CancellationToken::new(),
                transport: WebTransport::Http,
                progress: None,
            }
        };
        let pending = json!({"type":"custom_tool_call","name":"apply_patch","call_id":"pending","input":"literal\n🦀"});
        let large_history = json!({"model":"webbridge/test","stream":true,"input":[{"role":"user","content":"Preserve fixture task"},{"role":"assistant","content":"a".repeat(400_000)}]});
        for _ in 0..2 {
            let failure = provider
                .execute(make_request(large_history.clone(), "budget", "old", "task"))
                .await
                .unwrap();
            assert_eq!(failure.status(), StatusCode::OK);
            assert_eq!(failure.headers()["content-type"], "text/event-stream");
            let body = failure.into_body().collect().await.unwrap().to_bytes();
            let text = std::str::from_utf8(&body).unwrap();
            let event: Value = serde_json::from_str(
                text.lines()
                    .find_map(|line| line.strip_prefix("data: "))
                    .unwrap(),
            )
            .unwrap();
            assert!(crate::context_budget::is_context_failure(&[event]));
            assert!(browser.sessions.lock().unwrap().is_empty());
            assert_eq!(browser.sends.load(Ordering::SeqCst), 0);
        }
        let mut nonstream = large_history.clone();
        nonstream["stream"] = json!(false);
        assert_eq!(
            provider
                .execute(make_request(nonstream, "budget-json", "old", "task"))
                .await
                .err(),
            Some("E_CONTEXT_BUDGET")
        );
        // Unqualified providers do not gain a compaction recovery capability.
        let mut too_large = large_history.clone();
        too_large["input"][1]["content"] = json!("a".repeat(550_000));
        assert_eq!(
            unqualified
                .execute(make_request(too_large, "unqualified", "old", "task"))
                .await
                .err(),
            Some("E_CONTEXT_BUDGET")
        );
        assert!(browser.sessions.lock().unwrap().is_empty());
        let oversized = json!({"model":"webbridge/test","input":[{"role":"user","content":"*".repeat(100_000)},{"type":"compaction_trigger"}]});
        assert_eq!(
            provider
                .execute(make_request(oversized, "oversized-compact", "old", "task"))
                .await
                .err(),
            Some("E_CONTEXT_BUDGET")
        );
        assert!(browser.sessions.lock().unwrap().is_empty());
        let compact = json!({"model":"webbridge/test","input":[{"role":"user","content":"Read only"},{"role":"assistant","content":"a".repeat(400_000)},pending,{"type":"compaction_trigger"}]});
        let first = provider
            .execute(make_request(compact.clone(), "compact", "old", "task"))
            .await
            .unwrap();
        let bytes = first.into_body().collect().await.unwrap().to_bytes();
        let response: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(response["output"].as_array().unwrap().len(), 1);
        let item = response["output"][0].clone();
        assert_eq!(item["type"], "compaction");
        assert!(
            item["encrypted_content"]
                .as_str()
                .unwrap()
                .starts_with("wbr1:")
        );
        assert!(!String::from_utf8_lossy(&bytes).contains("Preserve fixture goal"));
        let replay = provider
            .execute(make_request(compact, "compact", "old", "task"))
            .await
            .unwrap();
        assert_eq!(
            replay.into_body().collect().await.unwrap().to_bytes(),
            bytes
        );
        assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
        let output =
            json!({"type":"custom_tool_call_output","call_id":"pending","output":"DENIED"});
        let continuation = json!({"model":"webbridge/test","instructions":"Current policy","input":[{"role":"user","content":"Read only"},item,output]});
        let invalid_task = provider
            .execute(make_request(
                continuation.clone(),
                "continue",
                "new",
                "different-task",
            ))
            .await;
        assert_eq!(invalid_task.err(), Some("E_NONPORTABLE_CONTEXT"));
        assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
        // Restoring an authenticated call must not make arbitrary late results
        // valid. Reject mismatched, repeated or wrong-kind results before send.
        for (case, items) in [
            (
                "unknown-result",
                json!([item, {"type":"custom_tool_call_output","call_id":"unknown","output":"SUCCESS"}]),
            ),
            ("duplicate-result", json!([item, output, output])),
            (
                "wrong-result-kind",
                json!([item, {"type":"function_call_output","call_id":"pending","output":"SUCCESS"}]),
            ),
            ("duplicate-call", json!([item, pending, output])),
            ("result-before-call", json!([output, item])),
        ] {
            let mut invalid = continuation.clone();
            invalid["input"] = items;
            assert_eq!(
                provider
                    .execute(make_request(invalid, case, "new", "task"))
                    .await
                    .err(),
                Some("E_CHECKPOINT_PENDING_TOOLS"),
                "{case}"
            );
            assert_eq!(browser.sends.load(Ordering::SeqCst), 1, "{case}");
        }
        provider
            .execute(make_request(
                continuation.clone(),
                "continue",
                "new",
                "task",
            ))
            .await
            .unwrap();
        let prompt = browser.prompt.lock().unwrap().clone();
        assert_eq!(prompt["instructions"], "Current policy");
        assert_eq!(prompt["history"][2], pending);
        assert_eq!(prompt["history"][3], output);
        assert!(
            prompt["history"][1]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("Preserve fixture goal")
        );
        // A later compaction authenticates the previous checkpoint, sees the
        // subsequent denial, and does not carry the resolved call forward again.
        let mut recompact = continuation.clone();
        recompact["input"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type":"compaction_trigger"}));
        let recompressed = provider
            .execute(make_request(recompact, "compact-again", "new", "task"))
            .await
            .unwrap();
        let recompressed: Value =
            serde_json::from_slice(&recompressed.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(recompressed["output"][0]["type"], "compaction");
        assert_ne!(
            recompressed["output"][0]["encrypted_content"],
            item["encrypted_content"]
        );
        assert_eq!(
            browser.prompt.lock().unwrap()["unresolved_tool_ids"],
            json!([])
        );
        let mut corrupted = continuation;
        corrupted["input"][1]["encrypted_content"] = json!("wbr1:corrupt");
        assert_eq!(
            provider
                .execute(make_request(corrupted, "tampered", "new", "task"))
                .await
                .err(),
            Some("E_NONPORTABLE_CONTEXT")
        );
        assert_eq!(browser.sends.load(Ordering::SeqCst), 3);
        std::fs::remove_file(key_path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
    fn fixture(waiting: bool) -> (Gateway, Arc<Browser>) {
        let (provider, browser) = provider_fixture(waiting);
        (
            Gateway::new(
                12345,
                NativeTransport::new("http://127.0.0.1:1".into()).unwrap(),
                Arc::new(provider),
            ),
            browser,
        )
    }
    #[test]
    fn catalog_requires_explicit_publication_and_does_not_mutate_existing_provider() {
        use cxweb_codex_adapter::catalog_codec::{CatalogCodec, CatalogRoute};
        let (provider, browser) = provider_fixture(false);
        assert!(provider.catalog(CatalogCodec::Cli01551).is_none());
        let published = provider
            .clone()
            .with_catalog(
                1,
                vec![(
                    CatalogCodec::Cli01551,
                    vec![CatalogRoute {
                        id: "webbridge/test".into(),
                        observed_label: "Fixture text".into(),
                        effort: "high".into(),
                        reasoning: vec![],
                        coding: false,
                    }],
                )],
            )
            .unwrap();
        assert!(provider.catalog(CatalogCodec::Cli01551).is_none());
        assert!(published.catalog(CatalogCodec::App01550Alpha92).is_none());
        assert_eq!(
            published.catalog(CatalogCodec::Cli01551).unwrap().entries[0]["slug"],
            "webbridge/test"
        );
        assert!(published.with_catalog(2, vec![]).is_err());
        assert_eq!(browser.sends.load(Ordering::SeqCst), 0);
        assert!(browser.sessions.lock().unwrap().is_empty());
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

    #[cfg(windows)]
    #[tokio::test]
    async fn fixture_policy_blocks_buffered_tool_delivery_and_its_replay() {
        let (provider, browser) = provider_fixture(false);
        *browser.fixture_call.lock().unwrap() =
            Some(json!({"cmd":"Get-Content private.txt","login":false}));
        let guarded = provider.with_native_fixture(Arc::new(crate::native_fixture::Fixture::new(
            r"C:\fixture\workspace".into(),
            "marker".into(),
        )));
        let gateway = Gateway::new(
            12345,
            NativeTransport::new("http://127.0.0.1:1".into()).unwrap(),
            Arc::new(guarded),
        );
        let base = gateway.base_url();
        let router = gateway.router();
        for stream in [false, true] {
            let (parts, _) = request(&base, "fixture-turn", "context", stream).into_parts();
            let body = json!({"model":"webbridge/test","input":"fixture task","stream":stream,
                "tools":[{"type":"function","name":"exec_command","parameters":{"type":"object","properties":{"cmd":{"type":"string"},"login":{"type":"boolean"}},"required":["cmd","login"],"additionalProperties":false}}]});
            let response = router
                .clone()
                .oneshot(Request::from_parts(parts, Body::from(body.to_string())))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let result: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(
                result,
                json!({"error":{"code":"E_NATIVE_PROBE_ACTION","message":"E_NATIVE_PROBE_ACTION"}})
            );
        }
        // The complete browser response is replayed, but remains blocked before
        // either delivery format can emit executable tool output.
        assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn public_summary_modes_are_supported_and_unknown_modes_fail_explicitly() {
        let response = web_failure("E_UNSUPPORTED_REASONING_SUMMARY");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(matches!(
            response.extensions().get::<BrowserEvidence>(),
            Some(BrowserEvidence::Failure("E_UNSUPPORTED_REASONING_SUMMARY"))
        ));
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["code"], "E_UNSUPPORTED_REASONING_SUMMARY");
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("public progress")
        );
        for summary in ["auto", "concise", "detailed"] {
            let request =
                json!({"model":"webbridge/test","input":"hello","reasoning":{"summary":summary}});
            assert!(CanonicalRequest::decode(request.to_string().as_bytes()).is_ok());
        }
    }

    #[tokio::test]
    async fn terminal_errors_preserve_codes_and_only_queue_refusals_allow_retries() {
        for code in [
            "E_BROWSER_RATE_LIMITED",
            "E_MODEL_FIDELITY",
            "E_SUBMISSION_UNCERTAIN",
            "E_CONTEXT_BUDGET",
            "E_REPLAY_UNAVAILABLE",
            "E_REQUEST_ALREADY_ADMITTED",
            "E_GENERATION_TIMEOUT",
            "E_WEB_DISCONNECTED",
            "E_FUTURE_UNKNOWN_FAILURE",
        ] {
            let response = web_failure(code);
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let parsed: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(parsed, json!({"error":{"code":code,"message":code}}));
        }
        for code in ["E_QUEUE_FULL", "E_QUEUE_TIMEOUT"] {
            assert_eq!(web_failure(code).status(), StatusCode::SERVICE_UNAVAILABLE);
        }
    }

    #[tokio::test]
    async fn oversized_full_context_is_terminal_before_browser_preparation() {
        let (gateway, browser) = fixture(false);
        let base = gateway.base_url();
        let router = gateway.router();
        for location in ["history", "instructions", "schema"] {
            // These characters expand to Unicode escapes in the literal browser
            // transport. The unescaped input is below the browser byte ceiling.
            let large = "*".repeat(100_000);
            let mut payload = json!({"model":"webbridge/test","input":"fixture"});
            match location {
                "history" => payload["input"] = json!(large),
                "instructions" => payload["instructions"] = json!(large),
                _ => {
                    payload["tools"] = json!([{"type":"function","name":"fixture","parameters":{"type":"object","description":large}}])
                }
            }
            assert!(payload.to_string().len() < 512 * 1024);
            for _ in 0..2 {
                let (parts, _) = request(&base, location, "context", false).into_parts();
                let response = router
                    .clone()
                    .oneshot(Request::from_parts(parts, Body::from(payload.to_string())))
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::BAD_REQUEST);
                let bytes = response.into_body().collect().await.unwrap().to_bytes();
                let body: Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(body["error"]["code"], "E_CONTEXT_BUDGET");
            }
        }
        assert_eq!(browser.sends.load(Ordering::SeqCst), 0);
        assert!(browser.sessions.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn optional_hosted_search_is_visible_and_forced_search_never_sends() {
        let (gateway, browser) = fixture(false);
        let base = gateway.base_url();
        let router = gateway.router();
        for (choice, expected) in [
            ("required", StatusCode::BAD_REQUEST),
            ("auto", StatusCode::OK),
        ] {
            let (parts, _) = request(&base, choice, "context", false).into_parts();
            let body = json!({"model":"webbridge/test","input":"fixture task","tools":[{"type":"web_search","external_web_access":false}],"tool_choice":choice});
            let response = router
                .clone()
                .oneshot(Request::from_parts(parts, Body::from(body.to_string())))
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            if choice == "required" {
                assert_eq!(browser.sends.load(Ordering::SeqCst), 0);
                let bytes = response.into_body().collect().await.unwrap().to_bytes();
                assert!(String::from_utf8_lossy(&bytes).contains("E_UNSUPPORTED_SERVER_TOOL"));
            } else {
                assert_eq!(
                    response.headers()["x-cxweb-unavailable-tools"],
                    "web_search"
                );
                assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
            }
        }
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
        assert!(matches!(
            first.extensions().get::<BrowserEvidence>(),
            Some(BrowserEvidence::Verified)
        ));
        let body = first.into_body().collect().await.unwrap().to_bytes();
        let original: Value = serde_json::from_slice(&body).unwrap();
        let (mut parts, body) = request(&base, "turn-one", "context-one", true).into_parts();
        let body = body.collect().await.unwrap().to_bytes();
        let compressed = zstd::stream::encode_all(body.as_ref(), 1).unwrap();
        parts
            .headers
            .insert("content-encoding", "zstd".parse().unwrap());
        let retry = router
            .clone()
            .oneshot(Request::from_parts(parts, Body::from(compressed)))
            .await
            .unwrap();
        assert_eq!(retry.headers()["content-type"], "text/event-stream");
        assert!(retry.extensions().get::<BrowserEvidence>().is_none());
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

    #[cfg(windows)]
    #[tokio::test]
    async fn refreshing_reasoning_catalog_preserves_completed_turn_replay() {
        use cxweb_codex_adapter::catalog_codec::{CatalogCodec, CatalogRoute, ReasoningLevel};
        let (provider, browser) = provider_fixture(false);
        let mut route = CatalogRoute {
            id: "webbridge/test".into(),
            observed_label: "Fixture".into(),
            effort: "high".into(),
            reasoning: vec![],
            coding: false,
        };
        let provider = provider
            .with_catalog(1, vec![(CatalogCodec::Cli01551, vec![route.clone()])])
            .unwrap();
        let gateway = Gateway::new(
            12345,
            NativeTransport::subscription().unwrap(),
            Arc::new(provider.clone()),
        );
        let base = gateway.base_url();
        let first = gateway
            .router()
            .oneshot(request(&base, "same-turn", "same-context", false))
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);
        let original = first.into_body().collect().await.unwrap().to_bytes();
        route.reasoning = vec![
            ReasoningLevel {
                effort: "medium".into(),
                description: "Medium".into(),
            },
            ReasoningLevel {
                effort: "high".into(),
                description: "High".into(),
            },
        ];
        let refreshed = provider
            .with_refreshed_catalog(vec![(CatalogCodec::Cli01551, vec![route])])
            .unwrap();
        assert_eq!(provider.catalog(CatalogCodec::Cli01551).unwrap().entries[0]["supported_reasoning_levels"].as_array().unwrap().len(), 1);
        assert_eq!(refreshed.catalog(CatalogCodec::Cli01551).unwrap().entries[0]["supported_reasoning_levels"].as_array().unwrap().len(), 2);
        let gateway = Gateway::new(
            12345,
            NativeTransport::subscription().unwrap(),
            Arc::new(refreshed),
        );
        let base = gateway.base_url();
        let replay = gateway
            .router()
            .oneshot(request(&base, "same-turn", "same-context", false))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::OK);
        assert!(replay.extensions().get::<BrowserEvidence>().is_none());
        assert_eq!(
            replay.into_body().collect().await.unwrap().to_bytes(),
            original
        );
        assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
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
                StatusCode::BAD_REQUEST
            );
        }
        assert!(browser.sessions.lock().unwrap().is_empty());
        assert_eq!(browser.sends.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn websocket_delta_then_full_http_retry_replays_the_same_durable_generation() {
        let (gateway, browser) = fixture(false);
        let base = gateway.base_url();
        let (parts, _) = request(&base, "first", "context", true).into_parts();
        let mut connection = crate::web_ws::Connection::new(gateway.clone(), &parts.headers);
        let user = json!({"type":"message","role":"user","content":[{"type":"input_text","text":"fixture task"}]});
        let frame = |turn: &str, input: Value| json!({"type":"response.create","model":"webbridge/test","stream":true,"input":input,"client_metadata":{"session_id":"RAW_SESSION","thread_id":"RAW_THREAD","turn_id":turn,"x-codex-turn-metadata":json!({"turn_id":turn,"context_window_id":"context"}).to_string()}});
        let first = connection
            .deliver(connection.prepare(frame("first", json!([user]))).unwrap())
            .await
            .unwrap();
        let completed = first.events.last().unwrap()["response"].clone();
        connection.complete(first);
        let mut second_frame = frame("second", json!([user]));
        second_frame["previous_response_id"] = completed["id"].clone();
        let second = connection
            .deliver(connection.prepare(second_frame).unwrap())
            .await
            .unwrap();
        let response_id = second.events.last().unwrap()["response"]["id"].clone();
        connection.complete(second);
        assert_eq!(browser.sends.load(Ordering::SeqCst), 2);
        // This is the reviewed native ResponseItem serialization after receiving
        // our output, independently constructed from its native wire fields.
        let assistant = &completed["output"][0];
        let history = json!([user, {"type":"message","id":assistant["id"],"role":"assistant","content":[{"type":"output_text","text":"fixture answer"}]}, user]);
        let (parts, _) = request(&base, "second", "context", false).into_parts();
        let retry = gateway
            .router()
            .oneshot(Request::from_parts(
                parts,
                Body::from(json!({"model":"webbridge/test","input":history}).to_string()),
            ))
            .await
            .unwrap();
        assert_eq!(retry.status(), StatusCode::OK);
        let response: Value =
            serde_json::from_slice(&retry.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(response["id"], response_id);
        assert_eq!(browser.sends.load(Ordering::SeqCst), 2);
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
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(browser.stops.load(Ordering::SeqCst), 1);
        assert_eq!(browser.sends.load(Ordering::SeqCst), 1);
    }

    #[cfg(windows)]
    mod native_context_probe {
        use super::*;
        include!("native_context_probe.rs");
    }
}
