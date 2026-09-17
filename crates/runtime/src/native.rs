//! Native transport has a fixed reviewed destination and never calls the browser.
use axum::{
    body::{Body, Bytes},
    http::{HeaderMap, Method, Response, StatusCode},
};
use futures_util::StreamExt;
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;

#[derive(Clone, Copy)]
pub enum NativeRoute {
    Models,
    Responses,
    Compact,
}
impl NativeRoute {
    fn path(self) -> &'static str {
        match self {
            Self::Models => "/models",
            Self::Responses => "/responses",
            Self::Compact => "/responses/compact",
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
    slots: Arc<Semaphore>,
}

impl NativeTransport {
    pub async fn upgrade(
        &self,
        upgrade: axum::extract::WebSocketUpgrade,
        query: Option<&str>,
        headers: HeaderMap,
    ) -> Response<Body> {
        crate::native_ws::upgrade(&self.base, self.slots.clone(), upgrade, query, headers).await
    }
    /// Only the subscription route is reviewed. API keys, residency overrides
    /// and existing third-party proxies require separately qualified adapters.
    pub fn subscription() -> Result<Self, &'static str> {
        Self::new("https://chatgpt.com/backend-api/codex".to_owned())
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
            base,
            slots: Arc::new(Semaphore::new(16)),
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
        let response = self
            .client
            .request(route.method(), url)
            .headers(headers)
            .body(body)
            .send()
            .await
            .map_err(|_| "E_NATIVE_UNAVAILABLE")?;
        if response.status().is_redirection() && response.status() != StatusCode::NOT_MODIFIED {
            // Never expose an upstream redirect to a local client that might
            // forward the native authorization to the Location destination.
            return Err("E_NATIVE_REDIRECT");
        }
        let status = response.status();
        let headers = end_to_end_headers(response.headers());
        let stream = response.bytes_stream().map(move |item| {
            let _keep_slot = &permit;
            item.map_err(|_| std::io::Error::other("E_NATIVE_STREAM"))
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
