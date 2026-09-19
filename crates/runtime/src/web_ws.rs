//! Connection-local Responses context. Native credentials never enter this state.
use crate::{gateway::Gateway, web_provider::WebIdentity};
use axum::{
    body::{Body, to_bytes},
    http::HeaderMap,
    response::Response,
};
use cxweb_codex_adapter::strict_json;
use serde_json::{Value, json};

const INPUT_LIMIT: usize = 8 * 1024 * 1024;
const OUTPUT_LIMIT: usize = 32 * 1024 * 1024;

pub(crate) struct Connection {
    gateway: Gateway,
    correlation: HeaderMap,
    last: Option<Completed>,
}

struct Completed {
    id: String,
    identity: WebIdentity,
    payload: Value,
    output: Vec<Value>,
}

pub(crate) struct Prepared {
    payload: Value,
    identity: WebIdentity,
    warmup: bool,
}

pub(crate) struct Delivery {
    pub events: Vec<Value>,
    completed: Option<Completed>,
}

impl Connection {
    pub fn new(gateway: Gateway, headers: &HeaderMap) -> Self {
        let mut correlation = HeaderMap::new();
        for key in ["session-id", "thread-id"] {
            for value in headers.get_all(key) {
                correlation.append(key, value.clone());
            }
        }
        Self {
            gateway,
            correlation,
            last: None,
        }
    }

    pub fn clear(&mut self) {
        self.last = None;
    }

    pub fn prepare(&self, mut payload: Value) -> Result<Prepared, &'static str> {
        if payload["type"] != "response.create"
            || !payload["model"]
                .as_str()
                .is_some_and(|m| m.starts_with(cxweb_domain::OWNED_MODEL_PREFIX))
        {
            return Err("E_UNSUPPORTED_WEBSOCKET_FRAME");
        }
        let metadata = payload["client_metadata"]
            .as_object()
            .ok_or("E_REQUEST_IDENTITY")?;
        let mut headers = self.correlation.clone();
        for (frame, header) in [("session_id", "session-id"), ("thread_id", "thread-id")] {
            let mut values = headers.get_all(header).iter();
            let expected = values
                .next()
                .and_then(|v| v.to_str().ok())
                .ok_or("E_REQUEST_IDENTITY")?;
            if values.next().is_some()
                || metadata.get(frame).and_then(Value::as_str) != Some(expected)
            {
                return Err("E_REQUEST_IDENTITY");
            }
        }
        let turn_metadata = metadata
            .get("x-codex-turn-metadata")
            .and_then(Value::as_str)
            .ok_or("E_REQUEST_IDENTITY")?;
        let parsed = strict_json::parse(turn_metadata.as_bytes(), 64 * 1024)
            .map_err(|_| "E_REQUEST_IDENTITY")?;
        if metadata.get("turn_id").and_then(Value::as_str).is_none()
            || metadata.get("turn_id") != parsed.get("turn_id")
        {
            return Err("E_REQUEST_IDENTITY");
        }
        headers.insert(
            "x-codex-turn-metadata",
            turn_metadata.parse().map_err(|_| "E_REQUEST_IDENTITY")?,
        );
        let identity = WebIdentity::from_headers(&headers).ok_or("E_REQUEST_IDENTITY")?;
        let warmup = match payload.get("generate") {
            None | Some(Value::Bool(true)) => false,
            Some(Value::Bool(false)) => true,
            _ => return Err("E_UNSUPPORTED_WEBSOCKET_FRAME"),
        };
        let previous = match payload.get("previous_response_id") {
            None | Some(Value::Null) => None,
            Some(Value::String(id)) if !id.is_empty() => Some(id.clone()),
            _ => return Err("E_NONPORTABLE_CONTEXT"),
        };
        let object = payload
            .as_object_mut()
            .ok_or("E_UNSUPPORTED_WEBSOCKET_FRAME")?;
        for key in [
            "type",
            "generate",
            "previous_response_id",
            "client_metadata",
        ] {
            object.remove(key);
        }
        object.insert("stream".into(), json!(true));
        // Reviewed native clients use arrays; a string cannot be appended safely.
        let delta = payload["input"]
            .as_array()
            .ok_or("E_UNSUPPORTED_WEBSOCKET_FRAME")?;
        if let Some(previous) = previous {
            let last = self.last.as_ref().ok_or("E_NONPORTABLE_CONTEXT")?;
            if previous != last.id
                || !identity.same_session(&last.identity)
                || properties(&payload) != properties(&last.payload)
            {
                return Err("E_NONPORTABLE_CONTEXT");
            }
            let mut input = last.payload["input"]
                .as_array()
                .ok_or("E_NONPORTABLE_CONTEXT")?
                .clone();
            input.extend(last.output.clone());
            input.extend(delta.clone());
            payload["input"] = json!(input);
        }
        if serde_json::to_vec(&payload)
            .map_err(|_| "E_INVALID_REQUEST")?
            .len()
            > INPUT_LIMIT
        {
            return Err("E_UNSUPPORTED_CONTEXT_SIZE");
        }
        Ok(Prepared {
            payload,
            identity,
            warmup,
        })
    }

    // Own all inputs so the relay can keep polling Close/Ping while this runs.
    pub fn deliver(
        &self,
        prepared: Prepared,
    ) -> impl Future<Output = Result<Delivery, Value>> + Send + 'static {
        let gateway = self.gateway.clone();
        async move {
            let response = gateway
                .dispatch_web(
                    prepared.payload.clone(),
                    Some(prepared.identity.clone()),
                    false,
                    prepared.warmup,
                    crate::gateway::WebTransport::WebSocket,
                )
                .await;
            let status = response.status();
            let bytes = to_bytes(response.into_body(), OUTPUT_LIMIT)
                .await
                .map_err(|_| error(502, "E_WEB_DELIVERY"))?;
            if !status.is_success() {
                let parsed = strict_json::parse(&bytes, OUTPUT_LIMIT).ok();
                let code = parsed
                    .as_ref()
                    .and_then(|v| v["error"]["code"].as_str())
                    .filter(|code| {
                        code.starts_with("E_")
                            && code.len() <= 128
                            && code.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')
                    })
                    .unwrap_or("E_WEB_DELIVERY");
                return Err(error(status.as_u16(), code));
            }
            let text = std::str::from_utf8(&bytes).map_err(|_| error(502, "E_WEB_DELIVERY"))?;
            let mut events = Vec::new();
            for block in text.split("\n\n").filter(|block| !block.is_empty()) {
                let data = block
                    .lines()
                    .find_map(|line| line.strip_prefix("data: "))
                    .ok_or_else(|| error(502, "E_WEB_DELIVERY"))?;
                events.push(
                    strict_json::parse(data.as_bytes(), OUTPUT_LIMIT)
                        .map_err(|_| error(502, "E_WEB_DELIVERY"))?,
                );
            }
            if crate::context_budget::is_context_failure(&events) {
                return Ok(Delivery {
                    events,
                    completed: None,
                });
            }
            let last = events
                .last()
                .filter(|v| v["type"] == "response.completed")
                .ok_or_else(|| error(502, "E_WEB_DELIVERY"))?;
            let response = &last["response"];
            let id = response["id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| error(502, "E_WEB_DELIVERY"))?
                .to_owned();
            let output = response["output"]
                .as_array()
                .ok_or_else(|| error(502, "E_WEB_DELIVERY"))?;
            let output = output
                .iter()
                .map(history_item)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|code| error(502, code))?;
            Ok(Delivery {
                events,
                completed: Some(Completed {
                    id,
                    identity: prepared.identity,
                    payload: prepared.payload,
                    output,
                }),
            })
        }
    }

    pub fn complete(&mut self, delivery: Delivery) {
        let Some(completed) = delivery.completed else {
            // A failed response is not a base for a future previous_response_id.
            self.last = None;
            return;
        };
        // Compaction replaces native history and changes context-window identity.
        // Require its next complete transcript; never append to the trigger input.
        self.last = if completed
            .output
            .iter()
            .any(|item| item["type"] == "compaction")
        {
            None
        } else {
            Some(completed)
        };
    }
}

fn properties(payload: &Value) -> Value {
    let mut value = payload.clone();
    if let Some(object) = value.as_object_mut() {
        object.remove("input");
    }
    value
}

// Project only our own validated outputs into the reviewed client's ResponseItem
// representation. Do not strip fields from client-supplied history. Message and
// function status plus output_text annotations are wire-only fields in that enum.
fn history_item(item: &Value) -> Result<Value, &'static str> {
    let mut item = item.clone();
    match item["type"].as_str() {
        Some("message") => {
            item.as_object_mut()
                .ok_or("E_WEB_DELIVERY")?
                .remove("status");
            for part in item["content"].as_array_mut().ok_or("E_WEB_DELIVERY")? {
                if part["type"] != "output_text" {
                    return Err("E_WEB_DELIVERY");
                }
                part.as_object_mut()
                    .ok_or("E_WEB_DELIVERY")?
                    .remove("annotations");
            }
        }
        Some("function_call") => {
            item.as_object_mut()
                .ok_or("E_WEB_DELIVERY")?
                .remove("status");
        }
        Some("custom_tool_call") => (),
        Some("compaction")
            if item["encrypted_content"]
                .as_str()
                .is_some_and(|s| s.starts_with("wbr1:")) => {}
        _ => return Err("E_WEB_DELIVERY"),
    }
    Ok(item)
}

pub(crate) fn error(status: u16, code: &str) -> Value {
    json!({"type":"error","status":status,"error":{"code":code,"message":code}})
}

pub(crate) fn warmup_response(payload: &Value) -> Response {
    let id = format!("resp_web_warmup_{:032x}", rand::random::<u128>());
    let created = json!({"type":"response.created","sequence_number":0,"response":{"id":id,"object":"response","model":payload["model"],"status":"in_progress","output":[]}});
    let completed = json!({"type":"response.completed","sequence_number":1,"response":{"id":id,"object":"response","model":payload["model"],"status":"completed","output":[]}});
    Response::builder().header("content-type", "text/event-stream").header("cache-control", "no-store")
        .body(Body::from(format!("event: response.created\ndata: {created}\n\nevent: response.completed\ndata: {completed}\n\n")))
        .expect("static warmup response")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::{WebFuture, WebProvider, WebRequest};
    use crate::native::NativeTransport;
    use cxweb_codex_adapter::{envelope::ValidatedOutput, wire};
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    struct Provider {
        calls: AtomicUsize,
        inputs: Mutex<Vec<Value>>,
    }
    impl WebProvider for Provider {
        fn validate_warmup(&self, request: &WebRequest) -> Result<(), &'static str> {
            assert!(request.identity.is_some());
            Ok(())
        }
        fn respond(&self, request: WebRequest) -> WebFuture {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if request.payload["instructions"] == "fixture-context-limit" {
                return Box::pin(async { crate::context_budget::failure_response() });
            }
            let compact = request.payload["input"].as_array().is_some_and(|input| {
                input
                    .last()
                    .is_some_and(|item| item["type"] == "compaction_trigger")
            });
            self.inputs.lock().unwrap().push(request.payload);
            Box::pin(async move {
                let encoded = if compact {
                    // Public fixture only: this test exercises WS framing/cache.
                    wire::encode_checkpoint(
                        "wbr1:synthetic-ws-fixture",
                        "webbridge/test",
                        "resp_cxweb_compact",
                        1,
                    )
                } else {
                    wire::encode(
                        &ValidatedOutput::Final("fixed answer".into()),
                        "webbridge/test",
                        "resp_cxweb_fixture",
                        1,
                    )
                }
                .unwrap();
                Response::new(Body::from(encoded.sse()))
            })
        }
    }

    fn fixture() -> (Connection, Arc<Provider>) {
        let provider = Arc::new(Provider {
            calls: AtomicUsize::new(0),
            inputs: Mutex::new(Vec::new()),
        });
        let gateway = Gateway::new(
            12345,
            NativeTransport::new("http://127.0.0.1:1".into()).unwrap(),
            provider.clone(),
        );
        let mut headers = HeaderMap::new();
        headers.insert("session-id", "fixture-session".parse().unwrap());
        headers.insert("thread-id", "fixture-thread".parse().unwrap());
        headers.insert(
            "authorization",
            "Bearer PRIVATE_NATIVE_CREDENTIAL".parse().unwrap(),
        );
        (Connection::new(gateway, &headers), provider)
    }

    fn frame(turn: &str) -> Value {
        json!({"type":"response.create","model":"webbridge/test","input":[],"client_metadata":{"session_id":"fixture-session","thread_id":"fixture-thread","turn_id":turn,"x-codex-turn-metadata":json!({"turn_id":turn,"context_window_id":"fixture-context"}).to_string(),"extra":"PRIVATE_TRACE"}})
    }

    #[tokio::test]
    async fn context_failure_is_delivered_and_clears_previous_response_history() {
        let (mut connection, _) = fixture();
        let first = connection
            .deliver(connection.prepare(frame("first")).unwrap())
            .await
            .unwrap();
        let previous = first.completed.as_ref().unwrap().id.clone();
        connection.complete(first);
        let mut request = frame("too-large");
        request["instructions"] = json!("fixture-context-limit");
        let failed = connection
            .deliver(connection.prepare(request).unwrap())
            .await
            .unwrap();
        assert!(failed.completed.is_none());
        assert!(crate::context_budget::is_context_failure(&failed.events));
        let failed_id = failed.events[0]["response"]["id"].clone();
        connection.complete(failed);
        for id in [json!(previous), failed_id] {
            let mut delta = frame("next");
            delta["previous_response_id"] = id;
            assert_eq!(
                connection.prepare(delta).err(),
                Some("E_NONPORTABLE_CONTEXT")
            );
        }
        let mut compact = frame("compact");
        compact["input"] = json!([{"type":"compaction_trigger"}]);
        let compact = connection
            .deliver(connection.prepare(compact).unwrap())
            .await
            .unwrap();
        assert_eq!(
            compact.events.last().unwrap()["response"]["output"][0]["type"],
            "compaction"
        );
        connection.complete(compact);
        let continued = connection
            .deliver(connection.prepare(frame("continued")).unwrap())
            .await
            .unwrap();
        assert_eq!(
            continued.events.last().unwrap()["type"],
            "response.completed"
        );
    }

    #[tokio::test]
    async fn compaction_output_clears_delta_history_and_accepts_a_fresh_context() {
        let (mut connection, provider) = fixture();
        let mut request = frame("compact");
        request["input"] = json!([{"type":"compaction_trigger"}]);
        let delivery = connection
            .deliver(connection.prepare(request).unwrap())
            .await
            .unwrap();
        let output = delivery.completed.as_ref().unwrap().output.clone();
        assert_eq!(output.len(), 1);
        assert_eq!(output[0]["type"], "compaction");
        let response = &delivery.events.last().unwrap()["response"];
        assert!(response.get("usage").is_none());
        assert!(response["output"][0].get("status").is_none());
        connection.complete(delivery);
        let mut request = frame("next");
        request["previous_response_id"] = json!("resp_cxweb_compact");
        assert_eq!(
            connection.prepare(request.clone()).err(),
            Some("E_NONPORTABLE_CONTEXT")
        );
        request
            .as_object_mut()
            .unwrap()
            .remove("previous_response_id");
        request["client_metadata"]["x-codex-turn-metadata"] =
            json!(json!({"turn_id":"next","context_window_id":"new-context"}).to_string());
        request["input"] = json!(output);
        let prepared = connection.prepare(request).unwrap();
        assert_eq!(prepared.payload["input"], json!(output));
        connection.deliver(prepared).await.unwrap();
        assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn warmup_never_generates_and_delta_preserves_client_history_and_projected_output() {
        let (mut connection, provider) = fixture();
        assert!(!connection.correlation.contains_key("authorization"));
        let mut request = frame("warmup");
        request["generate"] = json!(false);
        let warmup = connection
            .deliver(connection.prepare(request).unwrap())
            .await
            .unwrap();
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
        let id = warmup.completed.as_ref().unwrap().id.clone();
        connection.complete(warmup);
        let user = json!({"type":"message","role":"user","content":[{"type":"input_text","text":"fixture input"}],"internal_chat_message_metadata_passthrough":{"turn_id":"preserved"}});
        let mut request = frame("turn-one");
        request["previous_response_id"] = json!(id);
        request["input"] = json!([user]);
        let prepared = connection.prepare(request).unwrap();
        assert!(!prepared.payload.to_string().contains("PRIVATE_"));
        let reply = connection.deliver(prepared).await.unwrap();
        let id = reply.completed.as_ref().unwrap().id.clone();
        connection.complete(reply);
        let mut next = frame("turn-two");
        next["previous_response_id"] = json!(id);
        next["input"] = json!([{"type":"message","role":"user","content":[{"type":"input_text","text":"next input"}]}]);
        let prepared = connection.prepare(next).unwrap();
        assert_eq!(prepared.payload["input"].as_array().unwrap().len(), 3);
        assert_eq!(prepared.payload["input"][0], user);
        assert_eq!(
            prepared.payload["input"][1],
            json!({"id":"resp_cxweb_fixture_message","type":"message","role":"assistant","content":[{"type":"output_text","text":"fixed answer"}]})
        );
        connection.deliver(prepared).await.unwrap();
        assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn continuation_rejects_foreign_ids_changed_options_and_contexts() {
        let (mut connection, provider) = fixture();
        let reply = connection
            .deliver(connection.prepare(frame("first")).unwrap())
            .await
            .unwrap();
        let id = reply.completed.as_ref().unwrap().id.clone();
        connection.complete(reply);
        for variant in 0..7 {
            let mut request = frame("second");
            request["previous_response_id"] = json!(id);
            match variant {
                0 => request["previous_response_id"] = json!("native-response"),
                1 => request["model"] = json!("webbridge/other"),
                2 => request["instructions"] = json!("changed instructions"),
                3 => {
                    request["client_metadata"]["x-codex-turn-metadata"] =
                        json!(json!({"turn_id":"second","context_window_id":"other"}).to_string())
                }
                4 => request["client_metadata"]["session_id"] = json!("other-session"),
                5 => request["client_metadata"]["turn_id"] = json!("conflict"),
                _ => request["generate"] = json!("false"),
            }
            assert!(connection.prepare(request).is_err());
        }
        connection.clear();
        let mut request = frame("second");
        request["previous_response_id"] = json!(id);
        assert!(connection.prepare(request).is_err());
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn function_and_custom_output_projection_preserves_native_call_identity() {
        for (kind, key, status) in [
            ("function_call", "arguments", false),
            ("custom_tool_call", "input", true),
        ] {
            let item = json!({"type":kind,"id":"item","call_id":"call","name":"test","namespace":"functions","status":"completed",key:"unchanged\\n\"payload"});
            let history = history_item(&item).unwrap();
            assert_eq!(history["call_id"], "call");
            assert_eq!(history["id"], "item");
            assert_eq!(history[key], item[key]);
            assert_eq!(history.get("status").is_some(), status);
            assert_eq!(history["namespace"], "functions");
        }
    }

    #[tokio::test]
    async fn closed_admission_refuses_warmups_without_generation() {
        let (connection, provider) = fixture();
        connection
            .gateway
            .disconnect_web(std::time::Duration::from_secs(1))
            .await
            .unwrap();
        let mut request = frame("warmup");
        request["generate"] = json!(false);
        let result = connection
            .deliver(connection.prepare(request).unwrap())
            .await;
        let error = result.err().unwrap();
        assert_eq!(error["error"]["code"], "E_WEB_DISCONNECTED");
        assert!(error["status"].as_u64().unwrap() >= 400);
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    }
}
