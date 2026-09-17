//! Bounded native WebSocket relay. No browser driver or tool execution.
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

pub(crate) async fn upgrade(
    base: &str,
    route: SocketRoute,
    slots: Arc<Semaphore>,
    upgrade: WebSocketUpgrade,
    query: Option<&str>,
    headers: HeaderMap,
) -> Response<Body> {
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
    let connected = tokio::time::timeout(
        Duration::from_secs(15),
        tokio_tungstenite::connect_async_with_config(request, Some(config), false),
    )
    .await;
    let (upstream, handshake) = match connected {
        Ok(Ok(value)) => value,
        Ok(Err(tokio_tungstenite::tungstenite::Error::Http(response)))
            if !response.status().is_redirection() =>
        {
            let (parts, body) = response.into_parts();
            let mut outgoing = Response::new(Body::from(body.unwrap_or_default()));
            *outgoing.status_mut() = parts.status;
            *outgoing.headers_mut() = end_to_end_headers(&parts.headers);
            return outgoing;
        }
        _ => return unavailable("E_NATIVE_WEBSOCKET"),
    };
    let mut response = upgrade
        .max_message_size(LIMIT)
        .max_frame_size(LIMIT)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            relay(socket, upstream, route).await;
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
) {
    loop {
        let outcome = tokio::time::timeout(Duration::from_secs(300), async {
            tokio::select! {
                incoming = local.next() => {
                    let Some(Ok(message)) = incoming else { return false; };
                    let message = match message {
                        LocalMessage::Text(text) => {
                            // Validate model fields on every message, including
                            // reused connections. Client binary frames remain unqualified.
                            if let Err(code) = validate_message(&text, route) {
                                let error = serde_json::json!({"type":"error","error":{"code":code,"message":code}}).to_string();
                                let _ = local.send(LocalMessage::Text(error.into())).await;
                                return false;
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
                    upstream.send(message).await.is_ok()
                }
                incoming = upstream.next() => {
                    let Some(Ok(message)) = incoming else { return false; };
                    let message = match message {
                        Message::Text(text) => LocalMessage::Text(text.to_string().into()),
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
        assert!(
            error
                .into_text()
                .unwrap()
                .contains("E_WEB_WEBSOCKET_UNQUALIFIED")
        );
        assert_eq!(received.load(Ordering::SeqCst), 2);
        task.abort();
        upstream_task.abort();
    }

    #[tokio::test]
    async fn denied_upgrade_preserves_native_failure_and_redirect_is_not_returned() {
        for (status, expected) in [(401, 401), (302, 502)] {
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
            let task = tokio::spawn(async move {
                axum::serve(listener, gateway.router()).await.unwrap();
            });
            let error = tokio_tungstenite::connect_async(url).await.unwrap_err();
            let tokio_tungstenite::tungstenite::Error::Http(response) = error else {
                panic!("expected HTTP rejection")
            };
            assert_eq!(response.status(), expected);
            if status == 401 {
                assert_eq!(response.headers()["x-native-status"], "preserved");
                assert_eq!(response.body().as_ref().unwrap(), b"native denial");
            } else {
                assert!(!response.headers().contains_key("location"));
            }
            task.abort();
            upstream.abort();
        }
    }
}
