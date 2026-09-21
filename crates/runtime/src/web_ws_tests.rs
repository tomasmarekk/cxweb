//! Real socket routing and cancellation tests with a local native peer.
use crate::{
    gateway::{Gateway, WebFuture, WebProvider, WebRequest},
    native::NativeTransport,
};
use axum::{body::Body, response::Response};
use cxweb_codex_adapter::{envelope::ValidatedOutput, wire};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{Message, client::IntoClientRequest},
};

#[derive(Default)]
struct Provider {
    calls: AtomicUsize,
    waiting: bool,
    started: Notify,
    cancelled: Arc<Notify>,
    release: Arc<Notify>,
}
impl WebProvider for Provider {
    fn validate_warmup(&self, request: &WebRequest) -> Result<(), &'static str> {
        request.identity.as_ref().ok_or("E_REQUEST_IDENTITY")?;
        Ok(())
    }
    fn respond(&self, request: WebRequest) -> WebFuture {
        let count = self.calls.fetch_add(1, Ordering::SeqCst);
        let waiting = self.waiting;
        let cancelled = self.cancelled.clone();
        let release = self.release.clone();
        self.started.notify_one();
        Box::pin(async move {
            if request.payload["instructions"] == "fixture-context-limit" {
                return crate::context_budget::failure_response();
            }
            if request.payload["instructions"]
                .as_str()
                .is_some_and(|value| value.starts_with("fixture-public-status"))
            {
                let summary = vec!["Thinking".to_string()];
                let id = format!("resp_cxweb_fixture_{count}");
                let prefix =
                    wire::public_summary_prefix("webbridge/test", &id, 1, &summary).unwrap();
                request
                    .progress
                    .as_ref()
                    .unwrap()
                    .send(prefix)
                    .await
                    .unwrap();
                tokio::select! {
                    _ = request.cancellation.cancelled() => {
                        cancelled.notify_one();
                        return crate::web_provider::web_failure("E_CANCELLED");
                    }
                    _ = release.notified() => (),
                }
                if request.payload["instructions"] == "fixture-public-status-failure" {
                    return crate::web_provider::web_failure("E_TOOL_ENVELOPE_INVALID");
                }
                let summary = if request.payload["instructions"] == "fixture-public-status-revision"
                {
                    vec!["Revised public status".into()]
                } else {
                    summary
                };
                let output = wire::encode_with_summary(
                    &ValidatedOutput::Final("fixed socket reply".into()),
                    "webbridge/test",
                    &id,
                    1,
                    &summary,
                )
                .unwrap();
                return Response::new(Body::from(output.sse()));
            }
            if waiting {
                request.cancellation.cancelled().await;
                cancelled.notify_one();
                release.notified().await;
            }
            assert!(!request.payload.to_string().contains("PRIVATE_"));
            let output = wire::encode(
                &ValidatedOutput::Final("fixed socket reply".into()),
                "webbridge/test",
                &format!("resp_cxweb_fixture_{count}"),
                1,
            )
            .unwrap();
            let mut response = Response::new(Body::from(output.sse()));
            response
                .extensions_mut()
                .insert(crate::web_provider::BrowserEvidence::Verified);
            response
        })
    }
}

struct Fixture {
    gateway: Gateway,
    provider: Arc<Provider>,
    native: Arc<Mutex<Vec<String>>>,
    native_events: tokio::sync::mpsc::Sender<String>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
impl Fixture {
    async fn start(waiting: bool) -> Self {
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let native =
            NativeTransport::new(format!("http://{}", upstream.local_addr().unwrap())).unwrap();
        let frames = Arc::new(Mutex::new(Vec::new()));
        let captured = frames.clone();
        let (native_events, mut outgoing) = tokio::sync::mpsc::channel::<String>(4);
        let native_task = tokio::spawn(async move {
            let (stream, _) = upstream.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            loop {
                let message = tokio::select! {
                    text = outgoing.recv() => {
                        let Some(text) = text else { break; };
                        if socket.send(Message::Text(text.into())).await.is_err() { break; }
                        continue;
                    }
                    message = socket.next() => message,
                };
                let Some(Ok(message)) = message else {
                    break;
                };
                if let Message::Text(text) = message {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    assert_eq!(value["model"], "native-fixture");
                    captured.lock().unwrap().push(text.to_string());
                    socket.send(Message::Text(json!({"type":"response.completed","response":{"id":"native-response","output":[]}}).to_string().into())).await.unwrap();
                }
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let provider = Arc::new(Provider {
            waiting,
            ..Default::default()
        });
        let gateway = Gateway::new(
            listener.local_addr().unwrap().port(),
            native,
            provider.clone(),
        );
        let router = gateway.clone().router();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            gateway,
            provider,
            native: frames,
            native_events,
            tasks: vec![native_task, task],
        }
    }
    async fn connect(&self) -> WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>> {
        let mut request = format!("{}/responses", self.gateway.base_url())
            .replacen("http:", "ws:", 1)
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("session-id", "fixture-session".parse().unwrap());
        request
            .headers_mut()
            .insert("thread-id", "fixture-thread".parse().unwrap());
        request.headers_mut().insert(
            "authorization",
            "Bearer PRIVATE_CREDENTIAL".parse().unwrap(),
        );
        request.headers_mut().insert(
            "user-agent",
            "Codex Desktop/0.155.0-alpha.9.2 (PRIVATE_HOST)"
                .parse()
                .unwrap(),
        );
        tokio_tungstenite::connect_async(request).await.unwrap().0
    }
}
fn frame(turn: &str) -> Value {
    json!({"type":"response.create","model":"webbridge/test","input":[],"client_metadata":{"session_id":"fixture-session","thread_id":"fixture-thread","turn_id":turn,"x-codex-turn-metadata":json!({"turn_id":turn,"context_window_id":"fixture-context"}).to_string(),"extra":"PRIVATE_TRACE"}})
}
async fn terminal(socket: &mut WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>) -> Value {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let value: Value =
                serde_json::from_str(&socket.next().await.unwrap().unwrap().into_text().unwrap())
                    .unwrap();
            if matches!(
                value["type"].as_str(),
                Some("response.completed" | "response.failed" | "error")
            ) {
                return value;
            }
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn public_status_streams_on_the_socket_before_completion_without_duplicates() {
    let fixture = Fixture::start(false).await;
    let mut socket = fixture.connect().await;
    let mut request = frame("progress");
    request["instructions"] = json!("fixture-public-status");
    socket
        .send(Message::Text(request.to_string().into()))
        .await
        .unwrap();
    let expected = wire::public_summary_prefix(
        "webbridge/test",
        "resp_cxweb_fixture_0",
        1,
        &["Thinking".into()],
    )
    .unwrap();
    let mut observed = Vec::new();
    for event in &expected {
        let message = tokio::time::timeout(Duration::from_secs(2), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let value: Value = serde_json::from_str(&message.into_text().unwrap()).unwrap();
        assert_eq!(&value, event);
        observed.push(value);
    }
    // Completion is held by the fixture: these events really crossed the socket early.
    assert_eq!(fixture.gateway.health().active_turns, 1);
    fixture.provider.release.notify_one();
    loop {
        let message = tokio::time::timeout(Duration::from_secs(2), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let value: Value = serde_json::from_str(&message.into_text().unwrap()).unwrap();
        let done = value["type"] == "response.completed";
        observed.push(value);
        if done {
            break;
        }
    }
    let expected = wire::encode_with_summary(
        &ValidatedOutput::Final("fixed socket reply".into()),
        "webbridge/test",
        "resp_cxweb_fixture_0",
        1,
        &["Thinking".into()],
    )
    .unwrap();
    assert_eq!(observed, expected.events);
    assert_eq!(fixture.provider.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn failed_or_revised_output_after_public_status_never_completes() {
    for (mode, code) in [
        ("failure", "E_TOOL_ENVELOPE_INVALID"),
        ("revision", "E_STREAM_REVISION"),
    ] {
        let fixture = Fixture::start(false).await;
        let mut socket = fixture.connect().await;
        let mut request = frame("progress-failure");
        request["instructions"] = json!(format!("fixture-public-status-{mode}"));
        socket
            .send(Message::Text(request.to_string().into()))
            .await
            .unwrap();
        for _ in 0..7 {
            let message = tokio::time::timeout(Duration::from_secs(2), socket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let event: Value = serde_json::from_str(&message.into_text().unwrap()).unwrap();
            assert_ne!(event["type"], "response.completed");
        }
        fixture.provider.release.notify_one();
        let failure = terminal(&mut socket).await;
        assert_eq!(failure["type"], "error");
        assert_eq!(failure["status"], 400);
        assert_eq!(failure["error"]["code"], code);
        assert_eq!(fixture.provider.calls.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn closing_socket_after_public_status_cancels_and_drains_the_same_worker() {
    let fixture = Fixture::start(false).await;
    let mut socket = fixture.connect().await;
    let mut request = frame("progress-close");
    request["instructions"] = json!("fixture-public-status");
    socket
        .send(Message::Text(request.to_string().into()))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    socket.close(None).await.unwrap();
    tokio::time::timeout(
        Duration::from_secs(2),
        fixture.provider.cancelled.notified(),
    )
    .await
    .unwrap();
    fixture
        .gateway
        .disconnect_web(Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(fixture.gateway.health().active_turns, 0);
    assert_eq!(fixture.provider.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn context_failure_preserves_socket_and_native_passthrough() {
    let fixture = Fixture::start(false).await;
    let mut socket = fixture.connect().await;
    let mut request = frame("budget");
    request["instructions"] = json!("fixture-context-limit");
    socket
        .send(Message::Text(request.to_string().into()))
        .await
        .unwrap();
    let failed = terminal(&mut socket).await;
    assert!(crate::context_budget::is_context_failure(&[failed]));
    socket
        .send(Message::Text(frame("next").to_string().into()))
        .await
        .unwrap();
    assert_eq!(terminal(&mut socket).await["type"], "response.completed");
    let native = r#"{"type":"response.create", "model":"native-fixture","input":"native fixture"}"#;
    socket.send(Message::Text(native.into())).await.unwrap();
    assert_eq!(
        terminal(&mut socket).await["response"]["id"],
        "native-response"
    );
    assert_eq!(*fixture.native.lock().unwrap(), vec![native]);
    assert_eq!(fixture.provider.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn owned_compaction_cannot_escape_over_a_native_websocket_frame() {
    for disconnected in [false, true] {
        let fixture = Fixture::start(false).await;
        if disconnected {
            fixture
                .gateway
                .disconnect_web(Duration::from_secs(1))
                .await
                .unwrap();
        }
        let mut socket = fixture.connect().await;
        let request = json!({"type":"response.create","model":"native-fixture","input":[{"type":"compaction","encrypted_content":"wbr1:fixture"}]});
        socket
            .send(Message::Text(request.to_string().into()))
            .await
            .unwrap();
        assert_eq!(
            terminal(&mut socket).await["error"]["code"],
            "E_NONPORTABLE_CONTEXT"
        );
        assert!(fixture.native.lock().unwrap().is_empty());
        assert_eq!(fixture.provider.calls.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn one_socket_preserves_native_bytes_and_dispatches_owned_warmup_and_continuation() {
    let fixture = Fixture::start(false).await;
    let mut socket = fixture.connect().await;
    let native = r#"{"type":"response.create", "model":"native-fixture","input":"native fixture"}"#;
    socket.send(Message::Text(native.into())).await.unwrap();
    assert_eq!(
        terminal(&mut socket).await["response"]["id"],
        "native-response"
    );
    let mut warmup = frame("warmup");
    warmup["generate"] = json!(false);
    socket
        .send(Message::Text(warmup.to_string().into()))
        .await
        .unwrap();
    let first = terminal(&mut socket).await;
    assert_eq!(fixture.provider.calls.load(Ordering::SeqCst), 0);
    #[cfg(windows)]
    assert!(fixture.gateway.health().clients.app.is_none());
    let mut request = frame("one");
    request["previous_response_id"] = first["response"]["id"].clone();
    request["input"] = json!([{"type":"message","role":"user","content":[{"type":"input_text","text":"fixture"}]}]);
    socket
        .send(Message::Text(request.to_string().into()))
        .await
        .unwrap();
    let first = terminal(&mut socket).await;
    #[cfg(windows)]
    assert!(
        fixture
            .gateway
            .health()
            .clients
            .app
            .as_ref()
            .unwrap()
            .succeeded
    );
    request["previous_response_id"] = first["response"]["id"].clone();
    request["client_metadata"] = frame("two")["client_metadata"].clone();
    socket
        .send(Message::Text(request.to_string().into()))
        .await
        .unwrap();
    assert_eq!(terminal(&mut socket).await["type"], "response.completed");
    assert_eq!(fixture.provider.calls.load(Ordering::SeqCst), 2);
    // A native request after web completion still uses the same native socket.
    socket.send(Message::Text(native.into())).await.unwrap();
    assert_eq!(
        terminal(&mut socket).await["response"]["id"],
        "native-response"
    );
    assert_eq!(*fixture.native.lock().unwrap(), vec![native, native]);
    // Once native work intervenes, an owned prior response is no longer portable.
    socket
        .send(Message::Text(request.to_string().into()))
        .await
        .unwrap();
    let error = terminal(&mut socket).await;
    assert_eq!(error["error"]["code"], "E_NONPORTABLE_CONTEXT");
    assert_eq!(error["status"], 422);
    assert_eq!(fixture.provider.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn buffered_keepalive_has_no_response_claim_and_preserves_cancellation() {
    let fixture = Fixture::start(true).await;
    let mut socket = fixture.connect().await;
    socket
        .send(Message::Text(frame("buffered").to_string().into()))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), fixture.provider.started.notified())
        .await
        .unwrap();
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(16)).await;
    tokio::time::resume();
    let received = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .into_text()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&received).unwrap(),
        json!({"type":"cxweb.keepalive","buffered":true})
    );
    assert_eq!(fixture.provider.calls.load(Ordering::SeqCst), 1);
    assert!(fixture.native.lock().unwrap().is_empty());
    socket.close(None).await.unwrap();
    tokio::time::timeout(
        Duration::from_secs(3),
        fixture.provider.cancelled.notified(),
    )
    .await
    .unwrap();
    fixture.provider.release.notify_one();
    fixture
        .gateway
        .disconnect_web(Duration::from_secs(3))
        .await
        .unwrap();
}

#[tokio::test]
async fn closed_socket_cancels_generation_but_drain_waits_for_cleanup() {
    let fixture = Fixture::start(true).await;
    let mut socket = fixture.connect().await;
    socket
        .send(Message::Text(frame("one").to_string().into()))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), fixture.provider.started.notified())
        .await
        .unwrap();
    socket.close(None).await.unwrap();
    tokio::time::timeout(
        Duration::from_secs(3),
        fixture.provider.cancelled.notified(),
    )
    .await
    .unwrap();
    assert_eq!(
        fixture
            .gateway
            .disconnect_web(Duration::from_millis(30))
            .await,
        Err("E_WEB_DRAIN_TIMEOUT")
    );
    fixture.provider.release.notify_one();
    fixture
        .gateway
        .disconnect_web(Duration::from_secs(3))
        .await
        .unwrap();
    assert_eq!(fixture.provider.calls.load(Ordering::SeqCst), 1);
    assert!(fixture.native.lock().unwrap().is_empty());
}

#[tokio::test]
async fn overlapping_create_rejects_and_cancels_instead_of_sending_again() {
    let fixture = Fixture::start(true).await;
    let mut socket = fixture.connect().await;
    socket
        .send(Message::Text(frame("one").to_string().into()))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), fixture.provider.started.notified())
        .await
        .unwrap();
    socket
        .send(Message::Text(frame("two").to_string().into()))
        .await
        .unwrap();
    assert_eq!(
        terminal(&mut socket).await["error"]["code"],
        "E_WEBSOCKET_BUSY"
    );
    tokio::time::timeout(
        Duration::from_secs(3),
        fixture.provider.cancelled.notified(),
    )
    .await
    .unwrap();
    fixture.provider.release.notify_one();
    fixture
        .gateway
        .disconnect_web(Duration::from_secs(3))
        .await
        .unwrap();
    assert_eq!(fixture.provider.calls.load(Ordering::SeqCst), 1);
    assert!(fixture.native.lock().unwrap().is_empty());
}

#[tokio::test]
async fn native_quota_events_survive_during_owned_generation_without_response_interleaving() {
    let fixture = Fixture::start(true).await;
    let mut socket = fixture.connect().await;
    socket
        .send(Message::Text(frame("one").to_string().into()))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), fixture.provider.started.notified())
        .await
        .unwrap();
    let quota = r#"{"type":"codex.rate_limits", "rate_limits":{"limit_id":"codex","primary":{"used_percent":12.0}}}"#;
    fixture.native_events.send(quota.into()).await.unwrap();
    let received = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .into_text()
        .unwrap();
    assert_eq!(received, quota);
    assert!(
        tokio::time::timeout(
            Duration::from_millis(30),
            fixture.provider.cancelled.notified()
        )
        .await
        .is_err()
    );
    // Response-specific native events cannot masquerade as an owned response.
    fixture
        .native_events
        .send(r#"{"type":"response.completed","response":{"id":"native-must-not-escape"}}"#.into())
        .await
        .unwrap();
    tokio::time::timeout(
        Duration::from_secs(3),
        fixture.provider.cancelled.notified(),
    )
    .await
    .unwrap();
    let received = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap();
    assert!(!matches!(received, Some(Ok(Message::Text(_)))));
    fixture.provider.release.notify_one();
    fixture
        .gateway
        .disconnect_web(Duration::from_secs(3))
        .await
        .unwrap();
    assert_eq!(fixture.provider.calls.load(Ordering::SeqCst), 1);
}
