//! Isolated G0 diagnostic gateway; production browser routing is not enabled.
pub mod gateway;
pub mod ledger;
pub mod native;
pub mod scheduler;
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
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
}

impl ProbeState {
    pub fn new(port: u16) -> Self {
        Self {
            capability: URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>()),
            authority: format!("127.0.0.1:{port}"),
            observations: Arc::default(),
        }
    }
    pub fn base_url(&self) -> String {
        format!("http://{}/wb/{}/v1", self.authority, self.capability)
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
    let prefix = format!("/wb/{}/v1/", state.capability);
    let Some(path) = uri.path().strip_prefix(&prefix) else {
        return error(StatusCode::NOT_FOUND, "E_LOCAL_CAPABILITY");
    };
    match (method, path) {
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
                    let events = diagnostic_events();
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
    let text = "cxweb diagnostic round-trip succeeded";
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
    async fn rejects_wrong_host_origin_capability_and_admin_routes() {
        let state = ProbeState::new(12345);
        let path = format!("/wb/{}/v1/models", state.capability);
        for (uri, host, origin, expected) in [
            (path.as_str(), "evil.test", None, StatusCode::FORBIDDEN),
            (
                path.as_str(),
                "127.0.0.1:12345",
                Some("https://evil.test"),
                StatusCode::FORBIDDEN,
            ),
            (
                "/wb/wrong/v1/models",
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
                .uri(format!("/wb/{}/v1/responses", state.capability))
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
