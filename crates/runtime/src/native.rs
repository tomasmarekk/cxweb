//! Native transports use fixed reviewed destinations and never call the browser.
use crate::native_health::{Outcome, Tracker};
use axum::{
    body::{Body, Bytes},
    http::{HeaderMap, Method, Response, StatusCode},
};
use futures_util::Stream;
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;

#[derive(Clone, Copy)]
pub enum NativeRoute {
    Models,
    Responses,
    Compact,
    MemorySummarize,
    Search,
    ImageGeneration,
    ImageEdit,
    RealtimeCall,
}
impl NativeRoute {
    fn path(self) -> &'static str {
        match self {
            Self::Models => "/models",
            Self::Responses => "/responses",
            Self::Compact => "/responses/compact",
            Self::MemorySummarize => "/memories/trace_summarize",
            Self::Search => "/alpha/search",
            Self::ImageGeneration => "/images/generations",
            Self::ImageEdit => "/images/edits",
            Self::RealtimeCall => "/realtime/calls",
        }
    }
    fn method(self) -> Method {
        match self {
            Self::Models => Method::GET,
            _ => Method::POST,
        }
    }
}

#[derive(Clone)]
pub struct NativeTransport {
    client: reqwest::Client,
    base: String,
    realtime_base: String,
    slots: Arc<Semaphore>,
    pub(crate) health: Tracker,
}

impl NativeTransport {
    pub(crate) async fn upgrade(
        &self,
        route: crate::native_ws::SocketRoute,
        upgrade: axum::extract::WebSocketUpgrade,
        query: Option<&str>,
        headers: HeaderMap,
        web: Option<crate::gateway::Gateway>,
    ) -> Response<Body> {
        let base = if route == crate::native_ws::SocketRoute::Responses {
            &self.base
        } else {
            &self.realtime_base
        };
        crate::native_ws::upgrade(
            base,
            route,
            self.slots.clone(),
            upgrade,
            query,
            headers,
            crate::native_ws::ConnectionContext {
                web,
                health: (route == crate::native_ws::SocketRoute::Responses)
                    .then(|| self.health.clone()),
            },
        )
        .await
    }
    /// Subscription HTTP/Responses plus the client's standalone realtime API
    /// route. General API-key inference, residency overrides and existing
    /// third-party proxies require separately qualified adapters.
    pub fn subscription() -> Result<Self, &'static str> {
        let mut transport = Self::new("https://chatgpt.com/backend-api/codex".to_owned())?;
        // The reviewed client uses API-key auth for standalone realtime sockets,
        // even when Responses and WebRTC call creation use subscription auth.
        transport.realtime_base = "https://api.openai.com/v1".to_owned();
        Ok(transport)
    }
    pub(crate) fn new(base: String) -> Result<Self, &'static str> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(300))
            .pool_max_idle_per_host(8)
            .build()
            .map_err(|_| "E_NATIVE_TRANSPORT")?;
        Ok(Self {
            client,
            realtime_base: base.clone(),
            base,
            slots: Arc::new(Semaphore::new(16)),
            health: Tracker::default(),
        })
    }

    pub async fn forward(
        &self,
        route: NativeRoute,
        query: Option<&str>,
        headers: HeaderMap,
        body: Bytes,
    ) -> Result<Response<Body>, &'static str> {
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| "E_NATIVE_BUSY")?;
        // A route enum fixes the path. A query cannot change scheme/host/path.
        let mut url = format!("{}{}", self.base, route.path());
        if let Some(query) = query {
            url.push('?');
            url.push_str(query);
        }
        let headers = end_to_end_headers(&headers);
        let sequence = self.health.begin();
        let response = self
            .client
            .request(route.method(), url)
            .headers(headers)
            .body(body)
            .send()
            .await
            .map_err(|_| {
                self.health.observe(sequence, Outcome::TransportError);
                "E_NATIVE_UNAVAILABLE"
            })?;
        if response.status().is_redirection() && response.status() != StatusCode::NOT_MODIFIED {
            // Never expose an upstream redirect to a local client that might
            // forward the native authorization to the Location destination.
            self.health.observe(sequence, Outcome::Redirect);
            return Err("E_NATIVE_REDIRECT");
        }
        let status = response.status();
        let headers = end_to_end_headers(response.headers());
        let outcome = Outcome::status(status);
        if outcome != Outcome::Received {
            self.health.observe(sequence, outcome);
        }
        let health = self.health.clone();
        let mut input = Box::pin(response.bytes_stream());
        let mut failed = false;
        let mut finished = false;
        let stream = futures_util::stream::poll_fn(move |cx| {
            let _keep_slot = &permit;
            if finished {
                return std::task::Poll::Ready(None);
            }
            match input.as_mut().poll_next(cx) {
                std::task::Poll::Ready(Some(Err(_))) => {
                    failed = true;
                    health.observe(sequence, Outcome::StreamError);
                    std::task::Poll::Ready(Some(Err(std::io::Error::other("E_NATIVE_STREAM"))))
                }
                std::task::Poll::Ready(None) => {
                    finished = true;
                    if !failed && outcome == Outcome::Received {
                        health.observe(sequence, outcome);
                    }
                    std::task::Poll::Ready(None)
                }
                std::task::Poll::Ready(Some(Ok(bytes))) => std::task::Poll::Ready(Some(Ok(bytes))),
                std::task::Poll::Pending => std::task::Poll::Pending,
            }
        });
        let mut outgoing = Response::new(Body::from_stream(stream));
        *outgoing.status_mut() = status;
        *outgoing.headers_mut() = headers;
        Ok(outgoing)
    }
}

pub(crate) fn end_to_end_headers(input: &HeaderMap) -> HeaderMap {
    let connection_tokens: Vec<String> = input
        .get_all("connection")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(|v| v.trim().to_ascii_lowercase())
        .collect();
    let mut output = HeaderMap::new();
    for (name, value) in input {
        if matches!(
            name.as_str(),
            "host"
                | "connection"
                | "keep-alive"
                | "proxy-authenticate"
                | "proxy-authorization"
                | "te"
                | "trailer"
                | "transfer-encoding"
                | "upgrade"
        ) || connection_tokens.iter().any(|s| s == name.as_str())
        {
            continue;
        }
        output.append(name.clone(), value.clone());
    }
    output
}

pub fn unavailable(code: &'static str) -> Response<Body> {
    let mut response = Response::new(Body::from(
        serde_json::json!({"error":{"code":code,"message":code}}).to_string(),
    ));
    *response.status_mut() = StatusCode::BAD_GATEWAY;
    response.headers_mut().insert(
        "content-type",
        "application/json".parse().expect("static header"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, routing::any};
    use http_body_util::BodyExt;

    async fn mock(router: Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        (url, task)
    }

    #[tokio::test]
    async fn observes_complete_http_response_and_auth_errors_without_retaining_content() {
        for (status, expected) in [
            (200, Outcome::Received),
            (304, Outcome::Received),
            (401, Outcome::AuthRequired),
            (403, Outcome::Forbidden),
            (429, Outcome::RateLimited),
            (500, Outcome::ServerError),
            (400, Outcome::RequestRejected),
        ] {
            let (url, server) = mock(Router::new().route(
                "/models",
                any(move || async move {
                    Response::builder()
                        .status(status)
                        .body(Body::from("MOCK_RESPONSE_SECRET"))
                        .unwrap()
                }),
            ))
            .await;
            let transport = NativeTransport::new(url).unwrap();
            let response = transport
                .forward(NativeRoute::Models, None, HeaderMap::new(), Bytes::new())
                .await
                .unwrap();
            if expected == Outcome::Received {
                assert_eq!(transport.health.snapshot(), None);
            } else {
                assert_eq!(transport.health.snapshot().unwrap().outcome, expected);
            }
            response.into_body().collect().await.unwrap();
            let observed = transport.health.snapshot().unwrap();
            assert_eq!(observed.outcome, expected);
            assert!(!format!("{observed:?}").contains("MOCK_RESPONSE_SECRET"));
            server.abort();
        }
    }

    #[tokio::test]
    async fn truncated_http_body_cannot_become_a_successful_transport_observation() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            let mut used = 0;
            while !request[..used].windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                assert!(used < request.len());
                let received = socket.read(&mut request[used..]).await.unwrap();
                assert!(received > 0);
                used += received;
            }
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nshort",
                )
                .await
                .unwrap();
            socket.shutdown().await.unwrap();
        });
        let transport = NativeTransport::new(url).unwrap();
        let response = transport
            .forward(NativeRoute::Models, None, HeaderMap::new(), Bytes::new())
            .await
            .unwrap();
        assert_eq!(transport.health.snapshot(), None);
        assert!(response.into_body().collect().await.is_err());
        assert_eq!(
            transport.health.snapshot().unwrap().outcome,
            Outcome::StreamError
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn preserves_native_bytes_auth_status_headers_and_query() {
        let payload = Bytes::from_static(b"{\"opaque\":\"\\u1234\", \"model\":\"native\"}\n");
        let expected = payload.clone();
        let (url, server) = mock(Router::new().route(
            "/responses",
            any(
                move |uri: axum::http::Uri, headers: HeaderMap, bytes: Bytes| {
                    let expected = expected.clone();
                    async move {
                        assert_eq!(bytes, expected);
                        assert_eq!(uri.query(), Some("client_version=0.153.4"));
                        assert_eq!(headers["authorization"], "Bearer MOCK_NATIVE_SECRET");
                        assert!(!headers.contains_key("x-remove-me"));
                        Response::builder()
                            .status(429)
                            .header("retry-after", "10")
                            .header("x-future-native-header", "preserved")
                            .body(Body::from(bytes))
                            .unwrap()
                    }
                },
            ),
        ))
        .await;
        let transport = NativeTransport::new(url).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            "Bearer MOCK_NATIVE_SECRET".parse().unwrap(),
        );
        headers.insert("connection", "x-remove-me".parse().unwrap());
        headers.insert("x-remove-me", "ignored".parse().unwrap());
        let response = transport
            .forward(
                NativeRoute::Responses,
                Some("client_version=0.153.4"),
                headers,
                payload.clone(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 429);
        assert_eq!(response.headers()["retry-after"], "10");
        assert_eq!(response.headers()["x-future-native-header"], "preserved");
        assert_eq!(
            response.into_body().collect().await.unwrap().to_bytes(),
            payload
        );
        server.abort();
    }

    #[tokio::test]
    async fn redirect_is_never_followed_or_returned() {
        let (url, server) = mock(Router::new().route(
            "/models",
            any(|| async {
                Response::builder()
                    .status(302)
                    .header("location", "https://example.com/steal")
                    .body(Body::empty())
                    .unwrap()
            }),
        ))
        .await;
        let transport = NativeTransport::new(url).unwrap();
        let response = transport
            .forward(NativeRoute::Models, None, HeaderMap::new(), Bytes::new())
            .await;
        assert!(matches!(response, Err("E_NATIVE_REDIRECT")));
        server.abort();
    }

    #[tokio::test]
    async fn native_not_modified_is_a_validator_response_not_a_redirect() {
        let (url, server) = mock(Router::new().route(
            "/models",
            any(|| async {
                Response::builder()
                    .status(304)
                    .header("etag", "\"native\"")
                    .body(Body::empty())
                    .unwrap()
            }),
        ))
        .await;
        let response = NativeTransport::new(url)
            .unwrap()
            .forward(NativeRoute::Models, None, HeaderMap::new(), Bytes::new())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(response.headers()["etag"], "\"native\"");
        server.abort();
    }

    #[test]
    fn strips_all_connection_named_headers_without_losing_native_metadata() {
        let mut headers = HeaderMap::new();
        headers.append("connection", "X-Secret, Keep-Alive".parse().unwrap());
        headers.insert("x-secret", "value".parse().unwrap());
        headers.insert("chatgpt-account-id", "synthetic-scope".parse().unwrap());
        headers.insert("proxy-authorization", "must-not-forward".parse().unwrap());
        let filtered = end_to_end_headers(&headers);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered["chatgpt-account-id"], "synthetic-scope");
    }
}
