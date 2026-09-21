//! Opt-in loopback measurement; never contacts OpenAI or installed app state.
use super::*;
use axum::{Router, body::Bytes, http::HeaderMap, routing::post};
use serde_json::json;
use std::{io::Write, sync::atomic::AtomicUsize, time::Instant};

const WARMUP: usize = 100;
const SAMPLES: usize = 1000;
const PAYLOAD_BYTES: usize = 32 * 1024;
const AUTH: &str = "Bearer CXWEB_BENCHMARK_SYNTHETIC_AUTH";

fn statistics(values: &[f64]) -> serde_json::Value {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    json!({
        "samples": sorted.len(),
        "median_ms": (sorted[(sorted.len() - 1) / 2] + sorted[sorted.len() / 2]) / 2.0,
        "p95_ms": sorted[(sorted.len() * 95).div_ceil(100) - 1],
        "min_ms": sorted[0],
        "max_ms": sorted[sorted.len() - 1],
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "explicit release benchmark; 1000 paired loopback requests after warmup, no live account"]
async fn paired_native_loopback_latency() {
    if cfg!(debug_assertions) {
        panic!("Run this benchmark with --release");
    }
    let empty = serde_json::to_vec(&json!({"model":"benchmark-native", "input":""})).unwrap();
    let payload = Bytes::from(
        serde_json::to_vec(&json!({
            "model":"benchmark-native", "input":"x".repeat(PAYLOAD_BYTES - empty.len())
        }))
        .unwrap(),
    );
    assert_eq!(payload.len(), PAYLOAD_BYTES);
    let upstream = loopback::bind(0).unwrap();
    let upstream_url = format!("http://{}", upstream.local_addr().unwrap());
    let observed = Arc::new(AtomicUsize::new(0));
    let expected = payload.clone();
    let counter = observed.clone();
    let mock = Router::new().route(
        "/responses",
        post(move |headers: HeaderMap, bytes: Bytes| {
            let expected = expected.clone();
            let counter = counter.clone();
            async move {
                assert_eq!(headers.get("authorization").unwrap(), AUTH);
                assert_eq!(headers.get("content-type").unwrap(), "application/json");
                assert_eq!(bytes, expected);
                counter.fetch_add(1, Ordering::Relaxed);
                ([("content-type", "application/json")], bytes)
            }
        }),
    );
    let mock_task = tokio::spawn(async move { axum::serve(upstream, mock).await.unwrap() });
    let listener = loopback::bind(0).unwrap();
    let gateway = Gateway::new(
        listener.local_addr().unwrap().port(),
        NativeTransport::new(upstream_url.clone()).unwrap(),
        Arc::new(crate::gateway::UnqualifiedProvider),
    );
    let via = format!("{}/responses", gateway.base_url());
    let direct = format!("{upstream_url}/responses");
    // Exercise the same per-user TCP admission boundary as the installed host.
    let gateway_task = tokio::spawn(async move {
        axum::serve(UserListener(listener), gateway.router())
            .await
            .unwrap()
    });
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let measure = |url: String| {
        let client = client.clone();
        let payload = payload.clone();
        async move {
            let started = Instant::now();
            let response = client
                .post(url)
                .header("authorization", AUTH)
                .header("content-type", "application/json")
                .header("user-agent", "cxweb-local-benchmark/1")
                .body(payload.clone())
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            assert_eq!(
                response.headers().get("content-type").unwrap(),
                "application/json"
            );
            assert_eq!(response.bytes().await.unwrap(), payload);
            started.elapsed().as_secs_f64() * 1000.0
        }
    };
    let cold_direct = measure(direct.clone()).await;
    let cold_gateway = measure(via.clone()).await;
    let mut baseline = Vec::with_capacity(SAMPLES);
    let mut bridged = Vec::with_capacity(SAMPLES);
    let mut differences = Vec::with_capacity(SAMPLES);
    tokio::time::timeout(Duration::from_secs(120), async {
        for index in 0..WARMUP + SAMPLES {
            // Alternate order to avoid consistently favoring a warm second path.
            let (direct_ms, gateway_ms) = if index % 2 == 0 {
                (measure(direct.clone()).await, measure(via.clone()).await)
            } else {
                let gateway_ms = measure(via.clone()).await;
                (measure(direct.clone()).await, gateway_ms)
            };
            if index >= WARMUP {
                baseline.push(direct_ms);
                bridged.push(gateway_ms);
                differences.push(gateway_ms - direct_ms);
            }
        }
    })
    .await
    .expect("benchmark deadline");
    assert_eq!(observed.load(Ordering::Relaxed), (1 + WARMUP + SAMPLES) * 2);
    let report = json!({
        "schema": "cxweb.loopback-benchmark.v1", "evidence_level": "benchmark",
        "observed_at": cxweb_platform::clock::utc_timestamp(),
        "fixture": "echo-native-json-32k.v1", "payload_bytes_each_direction": PAYLOAD_BYTES,
        "warmup_pairs": WARMUP, "measured_pairs": SAMPLES, "order": "alternating paired",
        "build": "release", "architecture": std::env::consts::ARCH,
        "synthetic_auth_only": true, "external_requests": 0,
        "production_tcp_peer_verification": true, "keep_alive": true,
        "cold_first_request_ms": {"direct": cold_direct, "gateway": cold_gateway},
        "warm_direct": statistics(&baseline), "warm_gateway": statistics(&bridged),
        "paired_incremental": statistics(&differences),
        "target_incremental_p95_ms": 20,
        "target_met": statistics(&differences)["p95_ms"].as_f64().unwrap() <= 20.0,
        "limitations": ["Single shared workstation run; record OS, hardware and power mode separately.",
            "HTTP keep-alive native passthrough only; not browser latency, WebSocket overhead or an idle-resource measurement.",
            "Cold figures are one request per path, not cold-start percentiles."]
    });
    gateway_task.abort();
    mock_task.abort();
    let _ = gateway_task.await;
    let _ = mock_task.await;
    let bytes = serde_json::to_vec_pretty(&report).unwrap();
    if let Some(path) = std::env::var_os("CXWEB_BENCHMARK_REPORT") {
        let path = std::path::PathBuf::from(path);
        assert!(path.is_absolute(), "Report path must be absolute");
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .unwrap()
            .write_all(&bytes)
            .unwrap();
    }
    eprintln!("{}", String::from_utf8(bytes).unwrap());
}

#[test]
fn summaries_keep_negative_differences_and_use_nearest_rank_p95() {
    let values: Vec<_> = (-50..50).map(f64::from).collect();
    let summary = statistics(&values);
    assert_eq!(summary["median_ms"], -0.5);
    assert_eq!(summary["p95_ms"], 44.0);
    assert_eq!(summary["samples"], 100);
}
