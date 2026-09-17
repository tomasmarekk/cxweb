//! Request-scoped augmentation: no native catalog cache or credential export.
use crate::native::{NativeRoute, NativeTransport, unavailable};
use axum::{
    body::{Body, Bytes, to_bytes},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use cxweb_codex_adapter::{catalog::append_owned, strict_json};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Created by the qualified web provider, never from client-supplied metadata.
pub struct OwnedCatalog {
    pub codec: String,
    pub web_scope: String,
    pub generation: u64,
    pub entries: Vec<Value>,
}

pub async fn forward(
    native: &NativeTransport,
    query: Option<&str>,
    mut headers: HeaderMap,
    catalog: OwnedCatalog,
) -> Response {
    let conditional: Vec<_> = headers
        .get_all("if-none-match")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(str::to_owned)
        .collect();
    let mut hash = Sha256::new();
    // Length framing prevents collisions between adjacent scope components.
    let mut component = |bytes: &[u8]| {
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    };
    for name in ["authorization", "chatgpt-account-id"] {
        component(name.as_bytes());
        for value in headers.get_all(name) {
            component(value.as_bytes());
        }
    }
    component(query.unwrap_or("").as_bytes());
    component(catalog.codec.as_bytes());
    component(catalog.web_scope.as_bytes());
    component(&catalog.generation.to_le_bytes());
    // Local validators describe a different representation. Always fetch the
    // native body, so a changed web snapshot cannot be hidden by upstream 304.
    for name in [
        "if-none-match",
        "if-modified-since",
        "if-match",
        "if-unmodified-since",
        "if-range",
        "range",
    ] {
        headers.remove(name);
    }
    headers.insert(
        "accept-encoding",
        "identity".parse().expect("static header"),
    );
    let response = match native
        .forward(NativeRoute::Models, query, headers, Bytes::new())
        .await
    {
        Ok(response) => response,
        Err(code) => return unavailable(code),
    };
    if response.status() == StatusCode::NOT_MODIFIED {
        return unavailable("E_CATALOG_UNEXPECTED_NOT_MODIFIED");
    }
    if response.status() != StatusCode::OK {
        return response;
    }
    if response
        .headers()
        .get("content-encoding")
        .is_some_and(|v| v != "identity")
    {
        return unavailable("E_CATALOG_ENCODING");
    }
    let (mut parts, body) = response.into_parts();
    let bytes = match to_bytes(body, 16 * 1024 * 1024).await {
        Ok(bytes) => bytes,
        Err(_) => return unavailable("E_CATALOG_BODY"),
    };
    let merged = strict_json::parse(&bytes, 16 * 1024 * 1024)
        .ok()
        .and_then(|value| append_owned(value, &catalog.entries).ok());
    let Some(merged) = merged else {
        return unavailable("E_CATALOG_COMPATIBILITY");
    };
    let bytes = match serde_json::to_vec(&merged) {
        Ok(bytes) if bytes.len() <= 16 * 1024 * 1024 => bytes,
        _ => return unavailable("E_CATALOG_BODY"),
    };
    component(&bytes);
    let tag = format!("\"{:x}\"", hash.finalize());
    let unchanged = conditional
        .iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .any(|value| value == "*" || value.strip_prefix("W/").unwrap_or(value) == tag);
    for name in [
        "etag",
        "last-modified",
        "content-length",
        "content-encoding",
        "content-range",
        "content-md5",
        "digest",
        "content-digest",
        "repr-digest",
        "accept-ranges",
    ] {
        parts.headers.remove(name);
    }
    parts
        .headers
        .insert("etag", tag.parse().expect("hex digest header"));
    parts.headers.insert(
        "cache-control",
        "private, no-cache".parse().expect("static header"),
    );
    parts.headers.insert(
        "content-type",
        "application/json".parse().expect("static header"),
    );
    if unchanged {
        parts.status = StatusCode::NOT_MODIFIED;
        Response::from_parts(parts, Body::empty())
    } else {
        Response::from_parts(parts, Body::from(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, routing::get};
    use cxweb_codex_adapter::catalog::synthetic_model;
    use http_body_util::BodyExt;
    use serde_json::json;

    fn snapshot(generation: u64) -> OwnedCatalog {
        OwnedCatalog {
            codec: "test-only".into(),
            web_scope: "synthetic-web-account/workspace".into(),
            generation,
            entries: vec![synthetic_model()],
        }
    }
    fn headers(account: &str, conditional: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer SYNTHETIC".parse().unwrap());
        headers.insert("chatgpt-account-id", account.parse().unwrap());
        if let Some(tag) = conditional {
            headers.insert("if-none-match", tag.parse().unwrap());
        }
        headers
    }
    async fn server(router: Router) -> (NativeTransport, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let transport =
            NativeTransport::new(format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        (transport, task)
    }

    #[tokio::test]
    async fn merge_preserves_native_metadata_and_validators_are_scoped_to_each_snapshot() {
        let native_body =
            json!({"models":[{"slug":"native","unknown":{"enabled":true}}],"future":"preserved"});
        let served = native_body.clone();
        let (native, task) = server(Router::new().route(
            "/models",
            get(move |headers: HeaderMap, uri: axum::http::Uri| {
                let body = served.clone();
                async move {
                    assert_eq!(headers["authorization"], "Bearer SYNTHETIC");
                    assert_eq!(headers["accept-encoding"], "identity");
                    assert!(!headers.contains_key("if-none-match"));
                    assert_eq!(uri.query(), Some("client_version=test"));
                    Response::builder()
                        .header("etag", "\"native-validator\"")
                        .header("x-native-metadata", "preserved")
                        .header("content-md5", "obsolete")
                        .body(Body::from(body.to_string()))
                        .unwrap()
                }
            }),
        ))
        .await;
        let first = forward(
            &native,
            Some("client_version=test"),
            headers("account-a", None),
            snapshot(1),
        )
        .await;
        assert_eq!(first.status(), StatusCode::OK);
        assert_eq!(first.headers()["x-native-metadata"], "preserved");
        assert!(!first.headers().contains_key("content-md5"));
        let tag = first.headers()["etag"].to_str().unwrap().to_owned();
        assert_ne!(tag, "\"native-validator\"");
        let body = first.into_body().collect().await.unwrap().to_bytes();
        let merged: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(merged["models"][0], native_body["models"][0]);
        assert_eq!(merged["future"], "preserved");
        assert_eq!(merged["models"].as_array().unwrap().len(), 2);
        let same = forward(
            &native,
            Some("client_version=test"),
            headers("account-a", Some(&format!("W/{tag}"))),
            snapshot(1),
        )
        .await;
        assert_eq!(same.status(), StatusCode::NOT_MODIFIED);
        assert!(
            same.into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .is_empty()
        );
        for (account, generation) in [("account-a", 2), ("account-b", 1)] {
            let changed = forward(
                &native,
                Some("client_version=test"),
                headers(account, Some(&tag)),
                snapshot(generation),
            )
            .await;
            assert_eq!(changed.status(), StatusCode::OK);
            assert_ne!(changed.headers()["etag"], tag);
        }
        task.abort();
    }

    #[tokio::test]
    async fn unauthorized_native_catalog_is_never_replaced_with_owned_models() {
        let (native, task) = server(Router::new().route(
            "/models",
            get(|| async {
                Response::builder()
                    .status(401)
                    .header("x-native-error", "preserved")
                    .body(Body::from("native denial"))
                    .unwrap()
            }),
        ))
        .await;
        let response = forward(&native, None, headers("account", None), snapshot(1)).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()["x-native-error"], "preserved");
        assert_eq!(
            response.into_body().collect().await.unwrap().to_bytes(),
            "native denial"
        );
        task.abort();
    }

    #[tokio::test]
    async fn duplicate_native_json_keys_and_unexpected_304_fail_explicitly() {
        for (status, body) in [(200, "{\"models\":[],\"models\":[]}"), (304, "")] {
            let (native, task) = server(Router::new().route(
                "/models",
                get(move || async move {
                    Response::builder()
                        .status(status)
                        .body(Body::from(body))
                        .unwrap()
                }),
            ))
            .await;
            assert_eq!(
                forward(&native, None, headers("account", None), snapshot(1))
                    .await
                    .status(),
                StatusCode::BAD_GATEWAY
            );
            task.abort();
        }
    }
}
