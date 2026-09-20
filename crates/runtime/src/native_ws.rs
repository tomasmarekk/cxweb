//! Bounded native relay with per-frame owned Responses dispatch. No tool execution.
use crate::native::{end_to_end_headers, unavailable};
use axum::{
    body::Body,
    extract::{
        WebSocketUpgrade,
        ws::{Message as LocalMessage, WebSocket},
    },
    http::{HeaderMap, Response},
};
use cxweb_codex_adapter::strict_json;
use futures_util::{SinkExt, StreamExt};
use std::{sync::Arc, time::Duration};
use tokio::{net::TcpStream, sync::Semaphore};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{Message, client::IntoClientRequest, protocol::WebSocketConfig},
};

const LIMIT: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SocketRoute {
    Responses,
    Realtime,
    Live,
}
impl SocketRoute {
    fn path(self) -> &'static str {
        match self {
            Self::Responses => "/responses",
            Self::Realtime => "/realtime",
            Self::Live => "/live",
        }
    }
}

pub(crate) struct ConnectionContext {
    pub web: Option<crate::gateway::Gateway>,
    pub health: Option<crate::native_health::Tracker>,
}

pub(crate) async fn upgrade(
    base: &str,
    route: SocketRoute,
    slots: Arc<Semaphore>,
    upgrade: WebSocketUpgrade,
    query: Option<&str>,
    headers: HeaderMap,
    context: ConnectionContext,
) -> Response<Body> {
    let ConnectionContext { web, health } = context;
    let Ok(permit) = slots.try_acquire_owned() else {
        return unavailable("E_NATIVE_BUSY");
    };
    let base = base
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    let mut url = format!("{base}{}", route.path());
    if let Some(query) = query {
        url.push('?');
        url.push_str(query);
    }
    if route != SocketRoute::Responses {
        let Ok(parsed) = reqwest::Url::parse(&url) else {
            return unavailable("E_NATIVE_TRANSPORT");
        };
        let models: Vec<_> = parsed
            .query_pairs()
            .filter(|(name, _)| name == "model")
            .collect();
        if models.len() > 1
            || models
                .iter()
                .any(|(_, model)| model.starts_with(cxweb_domain::OWNED_MODEL_PREFIX))
        {
            return unavailable("E_WEB_CAPABILITY_UNSUPPORTED");
        }
    }
    let Ok(mut request) = url.into_client_request() else {
        return unavailable("E_NATIVE_TRANSPORT");
    };
    // Each hop owns its WebSocket handshake. In particular never copy a client's
    // key, compression negotiation or protocol into the upstream handshake.
    for (name, value) in &end_to_end_headers(&headers) {
        if !name.as_str().starts_with("sec-websocket-") && name != "content-length" {
            request.headers_mut().append(name, value.clone());
        }
    }
    let config = WebSocketConfig::default()
        .max_message_size(Some(LIMIT))
        .max_frame_size(Some(LIMIT));
    let observation = health.map(|health| {
        let sequence = health.begin();
        (health, sequence)
    });
    let connected = tokio::time::timeout(
        Duration::from_secs(15),
        tokio_tungstenite::connect_async_with_config(request, Some(config), false),
    )
    .await;
    let (upstream, handshake) = match connected {
        Ok(Ok(value)) => value,
        Ok(Err(tokio_tungstenite::tungstenite::Error::Http(response))) => {
            if let Some((health, sequence)) = &observation {
                let outcome = crate::native_health::Outcome::status(response.status());
                health.observe(
                    *sequence,
                    if outcome == crate::native_health::Outcome::Received {
                        crate::native_health::Outcome::RequestRejected
                    } else {
                        outcome
                    },
                );
            }
            if response.status().is_redirection() {
                return unavailable("E_NATIVE_WEBSOCKET");
            }
            let (parts, body) = response.into_parts();
            let mut outgoing = Response::new(Body::from(body.unwrap_or_default()));
            *outgoing.status_mut() = parts.status;
            *outgoing.headers_mut() = end_to_end_headers(&parts.headers);
            return outgoing;
        }
        _ => {
            if let Some((health, sequence)) = &observation {
                health.observe(*sequence, crate::native_health::Outcome::TransportError);
            }
            return unavailable("E_NATIVE_WEBSOCKET");
        }
    };
    if let Some((health, sequence)) = &observation {
        health.observe(*sequence, crate::native_health::Outcome::Connected);
    }
    let web = web.map(|gateway| crate::web_ws::Connection::new(gateway, &headers));
    let mut response = upgrade
        .max_message_size(LIMIT)
        .max_frame_size(LIMIT)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            relay(socket, upstream, route, web, observation).await;
        });
    for (name, value) in &end_to_end_headers(handshake.headers()) {
        if !name.as_str().starts_with("sec-websocket-") && name != "content-length" {
            response.headers_mut().append(name, value.clone());
        }
    }
    response
}

fn validate_message(text: &str, route: SocketRoute) -> Result<(), &'static str> {
    let value = strict_json::parse(text.as_bytes(), LIMIT).map_err(|_| "E_NATIVE_FRAME")?;
    let kind = value
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or("E_NATIVE_FRAME")?;
    if route == SocketRoute::Responses {
        if kind != "response.create" {
            return Err("E_NATIVE_FRAME");
        }
        let model = value
            .get("model")
            .and_then(|v| v.as_str())
            .ok_or("E_NATIVE_FRAME")?;
        if model.starts_with(cxweb_domain::OWNED_MODEL_PREFIX) {
            return Err("E_WEB_WEBSOCKET_UNQUALIFIED");
        }
        if crate::context_boundary::has_owned_reference(&value) {
            return Err("E_NONPORTABLE_CONTEXT");
        }
    } else {
        // Preserve native event types and audio payloads verbatim. Inspect only
        // protocol model fields, never references inside conversation content.
        for model in [
            value.get("model"),
            value.get("session").and_then(|v| v.get("model")),
            value.get("response").and_then(|v| v.get("model")),
        ]
        .into_iter()
        .flatten()
        {
            if model.is_null() {
                continue;
            }
            let model = model.as_str().ok_or("E_NATIVE_FRAME")?;
            if model.starts_with(cxweb_domain::OWNED_MODEL_PREFIX) {
                return Err("E_WEB_CAPABILITY_UNSUPPORTED");
            }
        }
    }
    Ok(())
}

async fn relay(
    mut local: WebSocket,
    mut upstream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    route: SocketRoute,
    mut web: Option<crate::web_ws::Connection>,
    observation: Option<(crate::native_health::Tracker, u64)>,
) {
    let mut native_pending = 0usize;
    loop {
        // Leave room for the coordinator's 30-minute generation ceiling,
        // preparation, submission, completion verification and cleanup.
        // Buffered keepalives do not extend either coordinator deadline.
        let seconds = if route == SocketRoute::Responses {
            2100
        } else {
            300
        };
        let outcome = tokio::time::timeout(Duration::from_secs(seconds), async {
            tokio::select! {
                incoming = local.next() => {
                    let Some(Ok(message)) = incoming else { return false; };
                    let message = match message {
                        LocalMessage::Text(text) => {
                            if route == SocketRoute::Responses
                                && let Ok(value) = strict_json::parse(text.as_bytes(), LIMIT)
                                && value["model"].as_str().is_some_and(|m| m.starts_with(cxweb_domain::OWNED_MODEL_PREFIX))
                                && let Some(web) = web.as_mut()
                            {
                                if native_pending != 0 {
                                    let _ = local.send(LocalMessage::Text(crate::web_ws::error(409, "E_WEBSOCKET_BUSY").to_string().into())).await;
                                    return false;
                                }
                                return serve_owned(&mut local, &mut upstream, web, value, &observation).await;
                            }
                            // Validate model fields on every message, including
                            // reused connections. Client binary frames remain unqualified.
                            if let Err(code) = validate_message(&text, route) {
                                let error = crate::web_ws::error(422, code).to_string();
                                let _ = local.send(LocalMessage::Text(error.into())).await;
                                return false;
                            }
                            if route == SocketRoute::Responses {
                                native_pending = native_pending.saturating_add(1);
                                if let Some(web) = web.as_mut() { web.clear(); }
                            }
                            Message::Text(text.to_string().into())
                        }
                        LocalMessage::Ping(data) => Message::Ping(data),
                        LocalMessage::Pong(data) => Message::Pong(data),
                        LocalMessage::Close(frame) => {
                            let frame = frame.map(|frame| tokio_tungstenite::tungstenite::protocol::CloseFrame { code: frame.code.into(), reason: frame.reason.to_string().into() });
                            let _ = upstream.close(frame).await;
                            return false;
                        }
                        LocalMessage::Binary(_) => return false,
                    };
                    let sent = upstream.send(message).await.is_ok();
                    if !sent && let Some((health, sequence)) = &observation {
                        health.observe(*sequence, crate::native_health::Outcome::StreamError);
                    }
                    sent
                }
                incoming = upstream.next() => {
                    if matches!(&incoming, Some(Err(_)))
                        && let Some((health, sequence)) = &observation
                    {
                        health.observe(*sequence, crate::native_health::Outcome::StreamError);
                    }
                    let Some(Ok(message)) = incoming else { return false; };
                    let message = match message {
                        Message::Text(text) => {
                            if route == SocketRoute::Responses
                                && let Ok(value) = strict_json::parse(text.as_bytes(), LIMIT)
                                && matches!(value["type"].as_str(), Some("response.completed" | "response.failed" | "response.incomplete" | "error"))
                            { native_pending = native_pending.saturating_sub(1); }
                            LocalMessage::Text(text.to_string().into())
                        },
                        Message::Binary(data) => LocalMessage::Binary(data),
                        Message::Ping(data) => LocalMessage::Ping(data),
                        Message::Pong(data) => LocalMessage::Pong(data),
                        Message::Close(frame) => {
                            let frame = frame.map(|frame| axum::extract::ws::CloseFrame { code: frame.code.into(), reason: frame.reason.to_string().into() });
                            let _ = local.send(LocalMessage::Close(frame)).await;
                            return false;
                        }
                        Message::Frame(_) => return false,
                    };
                    local.send(message).await.is_ok()
                }
            }
        }).await;
        if outcome != Ok(true) {
            break;
        }
    }
}

async fn serve_owned(
    local: &mut WebSocket,
    upstream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    web: &mut crate::web_ws::Connection,
    value: serde_json::Value,
    observation: &Option<(crate::native_health::Tracker, u64)>,
) -> bool {
    let prepared = match web.prepare(value) {
        Ok(value) => value,
        Err(code) => {
            let _ = local
                .send(LocalMessage::Text(
                    crate::web_ws::error(422, code).to_string().into(),
                ))
                .await;
            return false;
        }
    };
    let delivery = web.deliver(prepared);
    tokio::pin!(delivery);
    // Native clients discard WebSocket ping/pong before their stream-idle
    // watchdog. An ignored, namespaced text event keeps buffered transport
    // alive without claiming model output, progress, usage, or completion.
    let period = Duration::from_secs(15);
    let mut heartbeat = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if local.send(LocalMessage::Text(r#"{"type":"cxweb.keepalive","buffered":true}"#.into())).await.is_err() {
                    return false;
                }
            }
            result = &mut delivery => {
                match result {
                    Ok(delivery) => {
                        for event in &delivery.events {
                            if local.send(LocalMessage::Text(event.to_string().into())).await.is_err() { return false; }
                        }
                        web.complete(delivery);
                        return true;
                    }
                    Err(error) => {
                        let _ = local.send(LocalMessage::Text(error.to_string().into())).await;
                        return false;
                    }
                }
            }
            incoming = local.next() => {
                match incoming {
                    Some(Ok(LocalMessage::Ping(data))) => {
                        if local.send(LocalMessage::Pong(data)).await.is_err() { return false; }
                    }
                    Some(Ok(LocalMessage::Pong(_))) => (),
                    Some(Ok(LocalMessage::Text(_))) => {
                        let _ = local.send(LocalMessage::Text(crate::web_ws::error(409, "E_WEBSOCKET_BUSY").to_string().into())).await;
                        return false;
                    }
                    _ => return false,
                }
            }
            incoming = upstream.next() => {
                if matches!(&incoming, Some(Err(_))) && let Some((health, sequence)) = observation {
                    health.observe(*sequence, crate::native_health::Outcome::StreamError);
                }
                match incoming {
                    Some(Ok(Message::Ping(data))) => {
                        if upstream.send(Message::Pong(data)).await.is_err() { return false; }
                    }
                    Some(Ok(Message::Pong(_))) => (),
                    Some(Ok(Message::Text(text))) if strict_json::parse(text.as_bytes(), LIMIT)
                        .is_ok_and(|value| value["type"] == "codex.rate_limits") => {
                        // Account-wide native quota notifications are independent
                        // of the response. Preserve them without assigning usage
                        // to the browser generation or entering browser code.
                        if local.send(LocalMessage::Text(text.to_string().into())).await.is_err() { return false; }
                    }
                    // An idle native peer must not inject events into a web turn.
                    // Dropping delivery cancels and drains the admitted worker.
                    _ => return false,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        gateway::{Gateway, UnqualifiedProvider},
        native::NativeTransport,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    #[allow(clippy::result_large_err)]
    async fn realtime_modes_preserve_session_audio_cancel_and_native_metadata() {
        for (alpha, expected_path, suffix) in [
            (None, "/realtime", "/backend-api/codex"),
            (Some("quicksilver=v1"), "/realtime", "/backend-api/codex"),
            (Some("quicksilver=v2"), "/live", "/backend-api/codex"),
            (None, "/realtime", "/v1/realtime"),
            (Some("quicksilver=v1"), "/realtime", "/v1/realtime"),
            (Some("quicksilver=v2"), "/live", "/v1"),
        ] {
            let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let native = NativeTransport::new(format!(
                "http://{}",
                upstream_listener.local_addr().unwrap()
            ))
            .unwrap();
            let observed_transport = native.health.clone();
            let received = Arc::new(AtomicUsize::new(0));
            let counter = received.clone();
            let upstream_task = tokio::spawn(async move {
                let (stream, _) = upstream_listener.accept().await.unwrap();
                let mut socket = tokio_tungstenite::accept_hdr_async(stream,
                    |request: &tokio_tungstenite::tungstenite::handshake::server::Request,
                     mut response: tokio_tungstenite::tungstenite::handshake::server::Response| {
                        assert_eq!(request.uri().path(), expected_path);
                        assert_eq!(request.uri().query(), Some("model=native-realtime&trace=1"));
                        assert_eq!(request.headers()["authorization"], "Bearer SYNTHETIC_REALTIME");
                        assert_eq!(request.headers().get("openai-alpha").and_then(|v| v.to_str().ok()), alpha);
                        response.headers_mut().insert("x-request-id", "fixture-realtime".parse().unwrap());
                        Ok(response)
                    }).await.unwrap();
                while let Some(Ok(Message::Text(text))) = socket.next().await {
                    counter.fetch_add(1, Ordering::SeqCst);
                    socket.send(Message::Text(text)).await.unwrap();
                }
            });
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let gateway = Gateway::new(
                listener.local_addr().unwrap().port(),
                native,
                Arc::new(UnqualifiedProvider),
            );
            let base = gateway.base_url().replace("/backend-api/codex", suffix);
            let url = format!("{base}?model=native-realtime&trace=1").replacen("http:", "ws:", 1);
            gateway
                .disconnect_web(Duration::from_secs(1))
                .await
                .unwrap();
            let task = tokio::spawn(async move {
                axum::serve(listener, gateway.router()).await.unwrap();
            });
            let mut request = url.into_client_request().unwrap();
            request.headers_mut().insert(
                "authorization",
                "Bearer SYNTHETIC_REALTIME".parse().unwrap(),
            );
            if let Some(alpha) = alpha {
                request
                    .headers_mut()
                    .insert("openai-alpha", alpha.parse().unwrap());
            }
            let (mut socket, response) = tokio_tungstenite::connect_async(request).await.unwrap();
            assert_eq!(response.headers()["x-request-id"], "fixture-realtime");
            for text in [
                r#"{"type":"session.update", "session":{"model":"native-realtime","instructions":"mention webbridge/reference"}}"#,
                r#"{"type":"input_audio_buffer.append","audio":"AQIDBA=="}"#,
                r#"{"type":"response.cancel"}"#,
                r#"{"type":"session.context.append","content":[{"type":"input_text","text":"fixture"}]}"#,
            ] {
                socket.send(Message::Text(text.into())).await.unwrap();
                let echoed = tokio::time::timeout(Duration::from_secs(2), socket.next())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
                assert_eq!(echoed.into_text().unwrap(), text);
            }
            socket
                .send(Message::Text(
                    r#"{"type":"session.update","session":{"model":"webbridge/test"}}"#.into(),
                ))
                .await
                .unwrap();
            let error = tokio::time::timeout(Duration::from_secs(2), socket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert!(
                error
                    .into_text()
                    .unwrap()
                    .contains("E_WEB_CAPABILITY_UNSUPPORTED")
            );
            assert_eq!(received.load(Ordering::SeqCst), 4);
            assert_eq!(
                observed_transport.snapshot(),
                None,
                "standalone realtime must not certify subscription transport"
            );
            task.abort();
            upstream_task.abort();
        }
    }

    #[tokio::test]
    async fn realtime_query_and_unknown_mode_rejections_do_not_connect_upstream() {
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let native =
            NativeTransport::new(format!("http://{}", upstream.local_addr().unwrap())).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gateway = Gateway::new(
            listener.local_addr().unwrap().port(),
            native,
            Arc::new(UnqualifiedProvider),
        );
        let base = gateway.base_url().replacen("http:", "ws:", 1);
        let task = tokio::spawn(async move {
            axum::serve(listener, gateway.router()).await.unwrap();
        });
        for (query, alpha) in [
            ("model=webbridge%2Ftest", None),
            ("model=native&model=webbridge%2ftest", None),
            ("model=native", Some("quicksilver=unknown")),
        ] {
            let mut request = format!("{base}?{query}").into_client_request().unwrap();
            if let Some(alpha) = alpha {
                request
                    .headers_mut()
                    .insert("openai-alpha", alpha.parse().unwrap());
            }
            let error = tokio_tungstenite::connect_async(request).await.unwrap_err();
            let tokio_tungstenite::tungstenite::Error::Http(response) = error else {
                panic!("expected HTTP refusal");
            };
            assert_eq!(
                response.status().as_u16(),
                if alpha.is_some() { 400 } else { 502 }
            );
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(50), upstream.accept())
                .await
                .is_err()
        );
        task.abort();
    }

    #[test]
    fn realtime_and_responses_frames_have_separate_model_boundaries() {
        let cancel = r#"{"type":"response.cancel"}"#;
        assert!(validate_message(cancel, SocketRoute::Realtime).is_ok());
        assert!(validate_message(cancel, SocketRoute::Responses).is_err());
        for text in [
            r#"{"type":"session.update","session":{"model":"native","model":"webbridge/test"}}"#,
            r#"{"type":"response.create","response":{"model":"webbridge/test"}}"#,
            r#"{"type":"session.update","session":{"model":123}}"#,
            r#"{"session":{}}"#,
        ] {
            assert!(validate_message(text, SocketRoute::Realtime).is_err());
            assert!(validate_message(text, SocketRoute::Live).is_err());
        }
    }

    #[tokio::test]
    #[allow(clippy::result_large_err)] // tungstenite's required handshake callback signature.
    async fn native_frames_and_handshake_metadata_survive_but_owned_creates_do_not_escape() {
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_address = upstream_listener.local_addr().unwrap();
        let received = Arc::new(AtomicUsize::new(0));
        let counter = received.clone();
        let upstream_task = tokio::spawn(async move {
            let (stream, _) = upstream_listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_hdr_async(stream,
                |request: &tokio_tungstenite::tungstenite::handshake::server::Request,
                 mut response: tokio_tungstenite::tungstenite::handshake::server::Response| {
                    assert_eq!(request.uri().path(), "/responses");
                    assert_eq!(request.uri().query(), Some("client_version=test"));
                    assert_eq!(request.headers()["authorization"], "Bearer SYNTHETIC_NATIVE");
                    response.headers_mut().insert("x-codex-turn-state", "opaque-test-state".parse().unwrap());
                    Ok(response)
                }).await.unwrap();
            while let Some(Ok(message)) = socket.next().await {
                if let Message::Text(text) = message {
                    counter.fetch_add(1, Ordering::SeqCst);
                    socket.send(Message::Text(text)).await.unwrap();
                }
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gateway = Gateway::new(
            listener.local_addr().unwrap().port(),
            NativeTransport::new(format!("http://{upstream_address}")).unwrap(),
            Arc::new(UnqualifiedProvider),
        );
        let url = format!("{}/responses?client_version=test", gateway.base_url())
            .replacen("http:", "ws:", 1);
        let control = gateway.clone();
        let task = tokio::spawn(async move {
            axum::serve(listener, gateway.router()).await.unwrap();
        });
        let mut request = url.into_client_request().unwrap();
        request
            .headers_mut()
            .insert("authorization", "Bearer SYNTHETIC_NATIVE".parse().unwrap());
        let (mut socket, response) = tokio_tungstenite::connect_async(request).await.unwrap();
        assert_eq!(
            response.headers()["x-codex-turn-state"],
            "opaque-test-state"
        );
        #[cfg(windows)]
        assert_eq!(
            control.health().native.unwrap().outcome,
            crate::native_health::Outcome::Connected
        );
        for text in [
            r#"{"type":"response.create", "model":"native", "input":"first"}"#,
            r#"{"type":"response.create","model":"native","previous_response_id":"opaque","input":[]}"#,
        ] {
            socket.send(Message::Text(text.into())).await.unwrap();
            let echoed = tokio::time::timeout(Duration::from_secs(3), socket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(echoed.into_text().unwrap(), text);
            // The same established native socket remains usable after web
            // admission is closed, including reuse of previous_response_id.
            control
                .disconnect_web(Duration::from_secs(1))
                .await
                .unwrap();
        }
        socket
            .send(Message::Text(
                r#"{"type":"response.create","model":"webbridge/test","input":[]}"#.into(),
            ))
            .await
            .unwrap();
        let error = tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(error.into_text().unwrap().contains("E_WEBSOCKET_BUSY"));
        assert_eq!(received.load(Ordering::SeqCst), 2);
        task.abort();
        upstream_task.abort();
    }

    #[tokio::test]
    async fn denied_upgrade_preserves_native_failure_and_redirect_is_not_returned() {
        for (status, expected) in [(401, 401), (302, 502), (200, 200)] {
            let native_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = native_listener.local_addr().unwrap();
            let upstream = tokio::spawn(async move {
                axum::serve(
                    native_listener,
                    axum::Router::new().route(
                        "/responses",
                        axum::routing::get(move || async move {
                            Response::builder()
                                .status(status)
                                .header("x-native-status", "preserved")
                                .header("location", "https://example.invalid/must-not-follow")
                                .body(Body::from("native denial"))
                                .unwrap()
                        }),
                    ),
                )
                .await
                .unwrap();
            });
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let gateway = Gateway::new(
                listener.local_addr().unwrap().port(),
                NativeTransport::new(format!("http://{address}")).unwrap(),
                Arc::new(UnqualifiedProvider),
            );
            let url = format!("{}/responses", gateway.base_url()).replacen("http:", "ws:", 1);
            #[cfg(windows)]
            let observed = gateway.clone();
            let task = tokio::spawn(async move {
                axum::serve(listener, gateway.router()).await.unwrap();
            });
            let error = tokio_tungstenite::connect_async(url).await.unwrap_err();
            let tokio_tungstenite::tungstenite::Error::Http(response) = error else {
                panic!("expected HTTP rejection")
            };
            assert_eq!(response.status(), expected);
            #[cfg(windows)]
            assert_eq!(
                observed.health().native.unwrap().outcome,
                if status == 401 {
                    crate::native_health::Outcome::AuthRequired
                } else if status == 302 {
                    crate::native_health::Outcome::Redirect
                } else {
                    crate::native_health::Outcome::RequestRejected
                }
            );
            if status == 401 {
                assert_eq!(response.headers()["x-native-status"], "preserved");
                assert_eq!(response.body().as_ref().unwrap(), b"native denial");
            } else if status == 302 {
                assert!(!response.headers().contains_key("location"));
            }
            task.abort();
            upstream.abort();
        }
    }
}
