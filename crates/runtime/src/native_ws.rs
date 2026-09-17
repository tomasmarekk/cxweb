//! Bounded native Responses WebSocket relay. No browser driver or tool execution.
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

pub(crate) async fn upgrade(
    base: &str,
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
    let mut url = format!("{base}/responses");
    if let Some(query) = query {
        url.push('?');
        url.push_str(query);
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
            relay(socket, upstream).await;
        });
    for (name, value) in &end_to_end_headers(handshake.headers()) {
        if !name.as_str().starts_with("sec-websocket-") && name != "content-length" {
            response.headers_mut().append(name, value.clone());
        }
    }
    response
}

async fn relay(mut local: WebSocket, mut upstream: WebSocketStream<MaybeTlsStream<TcpStream>>) {
    loop {
        let outcome = tokio::time::timeout(Duration::from_secs(300), async {
            tokio::select! {
                incoming = local.next() => {
                    let Some(Ok(message)) = incoming else { return false; };
                    let message = match message {
                        LocalMessage::Text(text) => {
                            // Reclassify every create, including reused connections.
                            // Unknown/binary requests fail rather than bypass routing.
                            let Ok(value) = strict_json::parse(text.as_bytes(), LIMIT) else { return false; };
                            if value.get("type").and_then(|v| v.as_str()) != Some("response.create") { return false; }
                            let Some(model) = value.get("model").and_then(|v| v.as_str()) else { return false; };
                            if model.starts_with(cxweb_domain::OWNED_MODEL_PREFIX) {
                                let _ = local.send(LocalMessage::Text("{\"type\":\"error\",\"error\":{\"code\":\"E_WEB_WEBSOCKET_UNQUALIFIED\",\"message\":\"Web routes require the qualified HTTP transport\"}}".into())).await;
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
