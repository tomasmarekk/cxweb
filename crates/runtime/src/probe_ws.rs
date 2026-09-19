//! Synthetic WebSocket contract capture. Never routes to a browser or native API.
use axum::extract::ws::{Message, WebSocket};
use cxweb_codex_adapter::strict_json;
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

const LIMIT: usize = 1024 * 1024;

#[derive(Clone)]
pub(crate) struct Capture {
    path: PathBuf,
    frames: Arc<Mutex<Vec<Value>>>,
}
impl Capture {
    pub fn create(path: PathBuf) -> std::io::Result<Self> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.write_all(b"{\"schema\":\"cxweb.synthetic-websocket.v1\",\"frames\":[]}")?;
        Ok(Self {
            path,
            frames: Arc::default(),
        })
    }
    fn record(&self, value: Value) -> Result<(), ()> {
        let mut frames = self.frames.lock().map_err(|_| ())?;
        if frames.len() >= 32 {
            return Err(());
        }
        frames.push(value);
        std::fs::write(
            &self.path,
            json!({"schema":"cxweb.synthetic-websocket.v1","frames":*frames}).to_string(),
        )
        .map_err(|_| ())
    }
}

// Only correlation fields cross this boundary; native authorization is dropped.
pub(crate) async fn serve(
    mut socket: WebSocket,
    capture: Capture,
    session: Option<String>,
    thread: Option<String>,
) {
    let mut prior = None::<String>;
    while let Ok(Some(Ok(message))) =
        tokio::time::timeout(Duration::from_secs(300), socket.next()).await
    {
        let text = match message {
            Message::Text(text) => text,
            Message::Ping(bytes) => {
                if socket.send(Message::Pong(bytes)).await.is_err() {
                    break;
                }
                continue;
            }
            Message::Pong(_) => continue,
            _ => break,
        };
        let Ok(value) = strict_json::parse(text.as_bytes(), LIMIT) else {
            break;
        };
        if value["type"] != "response.create" || value["model"] != "webbridge/diagnostic" {
            let _ = socket.send(Message::Text(json!({"type":"error","error":{"code":"E_DIAGNOSTIC_MODEL","message":"Synthetic route only"}}).to_string().into())).await;
            break;
        }
        if capture
            .record(observe(
                &value,
                session.as_deref(),
                thread.as_deref(),
                prior.as_deref(),
            ))
            .is_err()
        {
            break;
        }
        let response_id = format!("resp_diagnostic_{:032x}", rand::random::<u128>());
        let message_id = format!("msg_diagnostic_{:032x}", rand::random::<u128>());
        let warmup = value["generate"] == false;
        let mut events = if warmup {
            vec![
                json!({"type":"response.created","response":{"id":response_id,"object":"response","status":"in_progress","output":[]}}),
                json!({"type":"response.completed","response":{"id":response_id,"object":"response","status":"completed","output":[],"usage":{"input_tokens":0,"output_tokens":0,"total_tokens":0}}}),
            ]
        } else {
            crate::diagnostic_events()
        };
        // A synthetic structured response exercises the native auxiliary path.
        // It is never evidence of browser/schema support by itself.
        let structured = value["text"]["format"]["type"] == "json_schema";
        for (index, event) in events.iter_mut().enumerate() {
            event["sequence_number"] = json!(index);
            if let Some(response) = event.get_mut("response") {
                response["id"] = json!(response_id);
                if let Some(items) = response["output"].as_array_mut() {
                    for item in items {
                        item["id"] = json!(message_id);
                    }
                }
            }
            if let Some(item) = event.get_mut("item") {
                item["id"] = json!(message_id);
            }
            if event.get("item_id").is_some() {
                event["item_id"] = json!(message_id);
            }
            if structured {
                replace_fixed_text(event);
            }
            if socket
                .send(Message::Text(event.to_string().into()))
                .await
                .is_err()
            {
                return;
            }
        }
        prior = Some(response_id);
    }
}

fn replace_fixed_text(value: &mut Value) {
    match value {
        Value::String(text) if text == "cxweb diagnostic round-trip succeeded" => {
            *text = r#"{"title":"Verify diagnostic response"}"#.into()
        }
        Value::Array(items) => items.iter_mut().for_each(replace_fixed_text),
        Value::Object(object) => object.values_mut().for_each(replace_fixed_text),
        _ => (),
    }
}

fn observe(
    value: &Value,
    session: Option<&str>,
    thread: Option<&str>,
    prior: Option<&str>,
) -> Value {
    let metadata = &value["client_metadata"];
    let turn = metadata["x-codex-turn-metadata"]
        .as_str()
        .and_then(|text| strict_json::parse(text.as_bytes(), 64 * 1024).ok());
    let input = value["input"].as_array();
    json!({
        "warmup":value["generate"] == false,
        "input_items":input.map(Vec::len),
        "has_previous_response":value["previous_response_id"].is_string(),
        "previous_matches_local":prior.is_some() && value["previous_response_id"].as_str() == prior,
        "client_metadata_object":metadata.is_object(),
        "session_matches_handshake":session.is_some() && metadata["session_id"].as_str() == session,
        "thread_matches_handshake":thread.is_some() && metadata["thread_id"].as_str() == thread,
        "turn_metadata_valid":turn.is_some(),
        "turn_id_matches_metadata":turn.as_ref().and_then(|t| t["turn_id"].as_str()).is_some_and(|id| metadata["turn_id"].as_str() == Some(id)),
        "context_window_present":turn.as_ref().is_some_and(|t| t["context_window_id"].is_string()),
        "structured_output":value["text"]["format"]["type"] == "json_schema",
        "stream_field_present":value.get("stream").is_some(),
        "authorization_recorded":false,
        "content_recorded":false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::{Message as ClientMessage, client::IntoClientRequest};

    #[tokio::test]
    async fn synthetic_socket_captures_warmup_continuation_and_never_echoes_private_input() {
        let path = std::env::temp_dir().join(format!(
            "cxweb-ws-fixture-{:032x}.json",
            rand::random::<u128>()
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let state = crate::ProbeState::new(listener.local_addr().unwrap().port())
            .with_websocket_capture(path.clone())
            .unwrap();
        let url = format!("{}/responses", state.base_url()).replacen("http:", "ws:", 1);
        let server = tokio::spawn(async move {
            axum::serve(listener, crate::diagnostic_router(state))
                .await
                .unwrap();
        });
        let mut request = url.into_client_request().unwrap();
        for (key, value) in [
            ("authorization", "Bearer PRIVATE_SECRET"),
            ("session-id", "PRIVATE_SESSION"),
            ("thread-id", "PRIVATE_THREAD"),
        ] {
            request.headers_mut().insert(key, value.parse().unwrap());
        }
        let (mut socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
        let mut prior = Value::Null;
        for warmup in [true, false] {
            let frame = json!({"type":"response.create","model":"webbridge/diagnostic","generate":!warmup,"previous_response_id":prior,"input":[{"role":"user","content":"PRIVATE_PROMPT"}],"client_metadata":{"session_id":"PRIVATE_SESSION","thread_id":"PRIVATE_THREAD","turn_id":"PRIVATE_TURN","x-codex-turn-metadata":json!({"turn_id":"PRIVATE_TURN"}).to_string()}});
            socket
                .send(ClientMessage::Text(frame.to_string().into()))
                .await
                .unwrap();
            loop {
                let incoming = tokio::time::timeout(Duration::from_secs(2), socket.next())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap()
                    .into_text()
                    .unwrap();
                assert!(!incoming.contains("PRIVATE"));
                let event: Value = serde_json::from_str(&incoming).unwrap();
                if event["type"] == "response.completed" {
                    assert_eq!(
                        event["response"]["output"].as_array().unwrap().is_empty(),
                        warmup
                    );
                    if !warmup {
                        assert_eq!(
                            event["response"]["output"][0]["content"][0]["text"],
                            "cxweb diagnostic round-trip succeeded"
                        );
                    }
                    prior = event["response"]["id"].clone();
                    break;
                }
            }
        }
        socket
            .send(ClientMessage::Text(
                r#"{"type":"response.create","model":"native"}"#.into(),
            ))
            .await
            .unwrap();
        assert!(
            socket
                .next()
                .await
                .unwrap()
                .unwrap()
                .into_text()
                .unwrap()
                .contains("E_DIAGNOSTIC_MODEL")
        );
        let capture = std::fs::read_to_string(&path).unwrap();
        assert!(!capture.contains("PRIVATE"));
        let capture: Value = serde_json::from_str(&capture).unwrap();
        assert_eq!(capture["frames"].as_array().unwrap().len(), 2);
        assert_eq!(capture["frames"][0]["warmup"], true);
        assert_eq!(capture["frames"][1]["previous_matches_local"], true);
        server.abort();
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn capture_keeps_correlations_without_identifiers_or_content() {
        let value = json!({"model":"webbridge/diagnostic","input":[{"text":"PRIVATE_PROMPT"}],"client_metadata":{"session_id":"PRIVATE_SESSION","thread_id":"PRIVATE_THREAD","turn_id":"PRIVATE_TURN","x-codex-turn-metadata":json!({"turn_id":"PRIVATE_TURN","context_window_id":"PRIVATE_CONTEXT"}).to_string()},"previous_response_id":"PRIVATE_RESPONSE"});
        let observed = observe(
            &value,
            Some("PRIVATE_SESSION"),
            Some("PRIVATE_THREAD"),
            Some("PRIVATE_RESPONSE"),
        );
        for key in [
            "session_matches_handshake",
            "thread_matches_handshake",
            "turn_id_matches_metadata",
            "previous_matches_local",
            "context_window_present",
        ] {
            assert_eq!(observed[key], true);
        }
        assert!(!observed.to_string().contains("PRIVATE"));
        assert_eq!(observed["input_items"], 1);
    }
}
