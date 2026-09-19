//! Isolated G0 diagnostic gateway; production browser routing is not enabled.
#[cfg(windows)]
mod browser_scope;
pub mod catalog_proxy;
mod catalog_snapshot;
#[cfg(windows)]
pub mod checkpoint;
#[cfg(windows)]
pub mod config_journal;
mod context_boundary;
#[cfg(windows)]
pub mod control;
#[cfg(windows)]
pub mod control_protocol;
pub mod gateway;
#[cfg(windows)]
pub mod host;
pub mod ledger;
#[cfg(windows)]
pub mod lifecycle;
#[cfg(windows)]
pub mod live_probe;
#[cfg(windows)]
pub mod managed_driver;
pub mod native;
#[cfg(windows)]
pub mod native_preflight;
mod native_ws;
mod probe_ws;
#[cfg(windows)]
mod qualification;
#[cfg(windows)]
pub mod remote_control;
mod request_body;
pub mod scheduler;
pub mod turn;
pub mod web_provider;
mod web_ws;
#[cfg(test)]
mod web_ws_tests;
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State, WebSocketUpgrade},
    http::{HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::any,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use cxweb_codex_adapter::catalog::{etag, synthetic_model};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct ProbeState {
    capability: String,
    authority: String,
    pub observations: Arc<Mutex<Vec<&'static str>>>,
    registry_capture: Option<std::path::PathBuf>,
    identity_capture: Option<std::path::PathBuf>,
    websocket_capture: Option<probe_ws::Capture>,
}

impl ProbeState {
    pub fn new(port: u16) -> Self {
        Self {
            capability: URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>()),
            authority: format!("127.0.0.1:{port}"),
            observations: Arc::default(),
            registry_capture: None,
            identity_capture: None,
            websocket_capture: None,
        }
    }
    pub fn base_url(&self) -> String {
        format!(
            "http://{}/wb/{}/backend-api/codex",
            self.authority, self.capability
        )
    }

    /// Synthetic development harness only. Captures tool definitions, never
    /// request history, headers, account state or authentication.
    pub fn with_registry_capture(mut self, path: std::path::PathBuf) -> Self {
        self.registry_capture = Some(path);
        self
    }
    /// Store a boolean contract result, never raw native correlation metadata.
    pub fn with_identity_capture(mut self, path: std::path::PathBuf) -> Self {
        self.identity_capture = Some(path);
        self
    }

    pub fn with_websocket_capture(mut self, path: std::path::PathBuf) -> std::io::Result<Self> {
        self.websocket_capture = Some(probe_ws::Capture::create(path)?);
        Ok(self)
    }
}

pub fn diagnostic_router(state: ProbeState) -> Router {
    Router::new()
        .fallback(any(probe))
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .with_state(state)
}

fn error(status: StatusCode, code: &str) -> Response {
    (
        status,
        axum::Json(json!({"error":{"code":code,"message":code}})),
    )
        .into_response()
}

async fn probe(
    State(state): State<ProbeState>,
    upgrade: Result<WebSocketUpgrade, axum::extract::ws::rejection::WebSocketUpgradeRejection>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // An origin-bearing browser request is never an authorized Codex client.
    if headers.contains_key("origin")
        || headers.get("host").and_then(|v| v.to_str().ok()) != Some(&state.authority)
    {
        return error(StatusCode::FORBIDDEN, "E_LOCAL_ORIGIN");
    }
    let prefix = format!("/wb/{}/backend-api/codex/", state.capability);
    let Some(path) = uri.path().strip_prefix(&prefix) else {
        return error(StatusCode::NOT_FOUND, "E_LOCAL_CAPABILITY");
    };
    match (method, path) {
        (Method::GET, "responses") => {
            if let Some(capture) = state.websocket_capture.clone() {
                let Ok(upgrade) = upgrade else {
                    return StatusCode::BAD_REQUEST.into_response();
                };
                let correlation = |name| {
                    headers
                        .get(name)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_owned)
                };
                let session = correlation("session-id");
                let thread = correlation("thread-id");
                drop(headers);
                record(&state, "diagnostic_websocket");
                return upgrade
                    .max_message_size(1024 * 1024)
                    .max_frame_size(1024 * 1024)
                    .on_upgrade(move |socket| probe_ws::serve(socket, capture, session, thread));
            }
            // Native Codex explicitly negotiates HTTP fallback on 426. This
            // synthetic server has no WebSocket implementation. A generic 404
            // caused five unnecessary connection retries in the actual TUI.
            record(&state, "diagnostic_http_fallback");
            error(StatusCode::UPGRADE_REQUIRED, "E_DIAGNOSTIC_HTTP_ONLY")
        }
        (Method::GET, "models") => {
            record(&state, "models");
            let payload = json!({"models":[synthetic_model()]}).to_string();
            let tag = etag(payload.as_bytes());
            if headers.get("if-none-match").and_then(|v| v.to_str().ok()) == Some(&tag) {
                return (StatusCode::NOT_MODIFIED, [("etag", tag)]).into_response();
            }
            (
                [("content-type", "application/json"), ("etag", tag.as_str())],
                payload,
            )
                .into_response()
        }
        (Method::POST, "responses") => {
            let Ok(request) = serde_json::from_slice::<Value>(&body) else {
                return error(StatusCode::BAD_REQUEST, "E_INVALID_REQUEST");
            };
            match request.get("model").and_then(Value::as_str) {
                Some("webbridge/diagnostic") => {
                    // Native authorization deliberately never enters the response adapter.
                    record(&state, "owned_response");
                    if let Some(path) = &state.identity_capture
                        && let Ok(mut file) = std::fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(path)
                    {
                        use std::io::Write;
                        let verified = web_provider::WebIdentity::from_headers(&headers).is_some();
                        let report = json!({"verified_native_identity":verified,"raw_identifiers_recorded":false});
                        let _ = file.write_all(report.to_string().as_bytes());
                    }
                    if let Some(path) = &state.registry_capture
                        && let Ok(mut file) = std::fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(path)
                    {
                        use std::io::Write;
                        let _ = file.write_all(request["tools"].to_string().as_bytes());
                    }
                    let retained = request["input"].as_array().is_some_and(|items| {
                        items.iter().any(|item| {
                            item["type"] == "compaction"
                                && item["encrypted_content"] == DIAGNOSTIC_CHECKPOINT
                        })
                    });
                    let compact = request["input"].as_array().is_some_and(|items| {
                        items
                            .last()
                            .is_some_and(|item| item == &json!({"type":"compaction_trigger"}))
                    });
                    let events = if compact {
                        if web_provider::WebIdentity::from_headers(&headers).is_none() {
                            return error(StatusCode::BAD_REQUEST, "E_REQUEST_IDENTITY");
                        }
                        record(&state, "diagnostic_compaction_v2");
                        diagnostic_compaction_events()
                    } else {
                        diagnostic_events_for(if retained {
                            "cxweb diagnostic checkpoint retained"
                        } else {
                            "cxweb diagnostic round-trip succeeded"
                        })
                    };
                    let wire: String = events
                        .iter()
                        .enumerate()
                        .map(|(i, event)| {
                            let mut event = event.clone();
                            event["sequence_number"] = json!(i);
                            format!(
                                "event: {}\ndata: {}\n\n",
                                event["type"].as_str().unwrap_or("error"),
                                event
                            )
                        })
                        .collect();
                    (
                        [
                            ("content-type", "text/event-stream"),
                            ("cache-control", "no-cache"),
                        ],
                        wire,
                    )
                        .into_response()
                }
                Some(model) if !model.starts_with(cxweb_domain::OWNED_MODEL_PREFIX) => {
                    record(&state, "native_rejected_in_probe");
                    error(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "E_DIAGNOSTIC_NO_NATIVE_UPSTREAM",
                    )
                }
                _ => error(StatusCode::BAD_REQUEST, "E_MODEL_UNAVAILABLE"),
            }
        }
        _ => error(StatusCode::NOT_FOUND, "E_UNSUPPORTED_ROUTE"),
    }
}

fn record(state: &ProbeState, event: &'static str) {
    // Bounded allowlisted metadata; never URLs, prompts, headers or credentials.
    if let Ok(mut events) = state.observations.lock()
        && events.len() < 128
    {
        events.push(event);
    }
}

pub fn diagnostic_events() -> Vec<Value> {
    diagnostic_events_for("cxweb diagnostic round-trip succeeded")
}

// Transport fixture only: no user history, encryption claim or production token.
const DIAGNOSTIC_CHECKPOINT: &str = "synthetic-cxweb-probe-only:checkpoint";

fn diagnostic_compaction_events() -> Vec<Value> {
    let item = json!({"type":"compaction","id":"cmp_cxweb_diagnostic","encrypted_content":DIAGNOSTIC_CHECKPOINT});
    vec![
        json!({"type":"response.created","response":{"id":"resp_cxweb_diagnostic_compact","object":"response","status":"in_progress","output":[]}}),
        json!({"type":"response.output_item.added","output_index":0,"item":{"type":"compaction","id":"cmp_cxweb_diagnostic","encrypted_content":""}}),
        json!({"type":"response.output_item.done","output_index":0,"item":item}),
        json!({"type":"response.completed","response":{"id":"resp_cxweb_diagnostic_compact","object":"response","status":"completed","output":[item]}}),
    ]
}

fn diagnostic_events_for(text: &str) -> Vec<Value> {
    let item = json!({"id":"msg_diagnostic", "type":"message", "role":"assistant", "status":"completed", "content":[{"type":"output_text","text":text,"annotations":[]}]});
    vec![
        json!({"type":"response.created", "response":{"id":"resp_diagnostic","object":"response","status":"in_progress","output":[]}}),
        json!({"type":"response.output_item.added", "output_index":0, "item":{"id":"msg_diagnostic","type":"message","role":"assistant","status":"in_progress","content":[]}}),
        json!({"type":"response.content_part.added", "item_id":"msg_diagnostic", "output_index":0,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}),
        json!({"type":"response.output_text.delta", "item_id":"msg_diagnostic", "output_index":0,"content_index":0,"delta":text}),
        json!({"type":"response.output_text.done", "item_id":"msg_diagnostic", "output_index":0,"content_index":0,"text":text}),
        json!({"type":"response.content_part.done", "item_id":"msg_diagnostic", "output_index":0,"content_index":0,"part":item["content"][0]}),
        json!({"type":"response.output_item.done", "output_index":0,"item":item}),
        json!({"type":"response.completed", "response":{"id":"resp_diagnostic","object":"response","status":"completed","output":[item],"usage":{"input_tokens":0,"output_tokens":0,"total_tokens":0}}}),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn synthetic_websocket_negotiation_uses_explicit_http_fallback() {
        let state = ProbeState::new(12345);
        let response = diagnostic_router(state.clone())
            .oneshot(
                Request::builder()
                    .uri(format!("{}/responses", state.base_url()))
                    .header("host", "127.0.0.1:12345")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UPGRADE_REQUIRED);
        assert_eq!(
            *state.observations.lock().unwrap(),
            vec!["diagnostic_http_fallback"]
        );
    }

    #[tokio::test]
    async fn rejects_wrong_host_origin_capability_and_admin_routes() {
        let state = ProbeState::new(12345);
        let path = format!("/wb/{}/backend-api/codex/models", state.capability);
        for (uri, host, origin, expected) in [
            (path.as_str(), "evil.test", None, StatusCode::FORBIDDEN),
            (
                path.as_str(),
                "127.0.0.1:12345",
                Some("https://evil.test"),
                StatusCode::FORBIDDEN,
            ),
            (
                "/wb/wrong/backend-api/codex/models",
                "127.0.0.1:12345",
                None,
                StatusCode::NOT_FOUND,
            ),
            (
                "/admin/disconnect",
                "127.0.0.1:12345",
                None,
                StatusCode::NOT_FOUND,
            ),
        ] {
            let mut request = Request::builder().uri(uri).header("host", host);
            if let Some(origin) = origin {
                request = request.header("origin", origin);
            }
            let response = diagnostic_router(state.clone())
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
        }
        assert!(state.observations.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn owned_response_does_not_echo_secrets_and_native_never_becomes_web() {
        let state = ProbeState::new(12345);
        for (model, expected) in [
            ("webbridge/diagnostic", StatusCode::OK),
            ("native", StatusCode::SERVICE_UNAVAILABLE),
        ] {
            let request = Request::builder()
                .method("POST")
                .uri(format!(
                    "/wb/{}/backend-api/codex/responses",
                    state.capability
                ))
                .header("host", "127.0.0.1:12345")
                .header("authorization", "Bearer SEEDED_SECRET")
                .body(Body::from(
                    json!({"model":model,"input":"SEEDED_PROMPT"}).to_string(),
                ))
                .unwrap();
            let response = diagnostic_router(state.clone())
                .oneshot(request)
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let text = String::from_utf8(bytes.to_vec()).unwrap();
            assert!(!text.contains("SEEDED_"));
            if model.starts_with("webbridge/") {
                assert!(text.contains("response.completed"));
            }
        }
        assert_eq!(
            *state.observations.lock().unwrap(),
            ["owned_response", "native_rejected_in_probe"]
        );
    }
}
