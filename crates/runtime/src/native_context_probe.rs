// Included only in the Windows test module. Real runtime, synthetic browser.
struct ObservedProvider {
    inner: CoordinatorProvider,
    http: AtomicUsize,
    websocket: AtomicUsize,
    compactions: AtomicUsize,
    continuations: AtomicUsize,
    failures: Arc<Mutex<Vec<&'static str>>>,
}
impl WebProvider for ObservedProvider {
    fn validate_warmup(&self, request: &WebRequest) -> Result<(), &'static str> {
        self.inner.validate_warmup(request)
    }
    fn respond(&self, request: WebRequest) -> WebFuture {
        match request.transport {
            crate::gateway::WebTransport::Http => &self.http,
            crate::gateway::WebTransport::WebSocket => &self.websocket,
        }
        .fetch_add(1, Ordering::SeqCst);
        if let Some(input) = request.payload["input"].as_array() {
            if input
                .last()
                .is_some_and(|item| item["type"] == "compaction_trigger")
            {
                self.compactions.fetch_add(1, Ordering::SeqCst);
            }
            if input.iter().any(|item| {
                item["type"] == "compaction"
                    && item["encrypted_content"]
                        .as_str()
                        .is_some_and(|text| text.starts_with("wbr1:"))
            }) {
                self.continuations.fetch_add(1, Ordering::SeqCst);
            }
        }
        let provider = self.inner.clone();
        let failures = self.failures.clone();
        Box::pin(async move {
            match provider.execute(request).await {
                Ok(response) => response,
                Err(code) => {
                    failures.lock().unwrap().push(code);
                    web_failure(code)
                }
            }
        })
    }
}

#[tokio::test]
#[ignore = "requires CXWEB_CONTEXT_PROBE_BACKEND pointing to a reviewed executable; uses synthetic content only"]
async fn actual_backend_compacts_through_runtime_http_and_websocket() {
    use cxweb_codex_adapter::{
        catalog_codec::{CatalogCodec, CatalogRoute},
        context_budget::LocalContextBudget,
    };
    use futures_util::StreamExt;
    use std::{future::IntoFuture, path::PathBuf, time::Duration};
    let executable = PathBuf::from(
        std::env::var_os("CXWEB_CONTEXT_PROBE_BACKEND").expect("select a reviewed backend"),
    );
    assert!(executable.is_absolute());
    let fingerprint = format!("{:x}", Sha256::digest(std::fs::read(&executable).unwrap()));
    let (codec, build) = match fingerprint.as_str() {
        "eba0f32c976667cb9298efafd98513e823eeda7b576a03ec658bb8be8d336316" => {
            (CatalogCodec::Cli01551, "0.155.1")
        }
        "bc45017e8239dc150258f69309ced9df6bbcdf5b8e4f346decf780ac0999e226" => {
            (CatalogCodec::App01550Alpha92, "0.155.0-alpha.9.2")
        }
        _ => panic!("unreviewed backend"),
    };
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let root = repo.join(format!(
        ".local/probes/runtime-context-{:032x}",
        rand::random::<u128>()
    ));
    cxweb_platform::state::protected_directory(&root).unwrap();
    for websocket in [false, true] {
        let directory = root.join(if websocket { "websocket" } else { "http" });
        cxweb_platform::state::protected_directory(&directory).unwrap();
        let (provider, browser) = provider_fixture(false);
        browser.answer_bytes.store(64 * 1024, Ordering::SeqCst);
        let key = Arc::new(
            crate::checkpoint::Codec::load_or_create(
                &directory.join("key.dpapi"),
                "fixture-installation",
            )
            .unwrap(),
        );
        let budget = LocalContextBudget::new(96 * 1024, 256 * 1024).unwrap();
        let provider = Arc::new(ObservedProvider {
            inner: provider
                .with_checkpoints(key, codec)
                .unwrap()
                .with_context_budget(budget)
                .unwrap(),
            http: AtomicUsize::new(0),
            websocket: AtomicUsize::new(0),
            compactions: AtomicUsize::new(0),
            continuations: AtomicUsize::new(0),
            failures: Arc::default(),
        });
        let native_frames = Arc::new(AtomicUsize::new(0));
        let observed_frames = native_frames.clone();
        let native_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let native =
            NativeTransport::new(format!("http://{}", native_listener.local_addr().unwrap()))
                .unwrap();
        let mut servers = tokio::task::JoinSet::new();
        servers.spawn(
            axum::serve(
                native_listener,
                axum::Router::new()
                    .route(
                        "/responses",
                        axum::routing::get(move |upgrade: axum::extract::WebSocketUpgrade| {
                            let frames = observed_frames.clone();
                            async move {
                                use axum::response::IntoResponse;
                                if !websocket {
                                    return StatusCode::UPGRADE_REQUIRED.into_response();
                                }
                                upgrade.on_upgrade(move |mut socket| async move {
                                    while let Some(Ok(message)) = socket.next().await {
                                        match message {
                                            axum::extract::ws::Message::Ping(bytes) => {
                                                if socket
                                                    .send(axum::extract::ws::Message::Pong(bytes))
                                                    .await
                                                    .is_err()
                                                {
                                                    break;
                                                }
                                            }
                                            axum::extract::ws::Message::Pong(_) => (),
                                            axum::extract::ws::Message::Text(_) => {
                                                frames.fetch_add(1, Ordering::SeqCst);
                                                break;
                                            }
                                            _ => break,
                                        }
                                    }
                                })
                            }
                        }),
                    )
                    .fallback(|| async { StatusCode::SERVICE_UNAVAILABLE }),
            )
            .into_future(),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gateway = Gateway::new(
            listener.local_addr().unwrap().port(),
            native,
            provider.clone(),
        );
        let catalog = codec
            .encode_with_context_budget(
                &CatalogRoute {
                    id: "webbridge/test".into(),
                    observed_label: "Context fixture".into(),
                    effort: "medium".into(),
                    coding: false,
                },
                budget,
            )
            .unwrap();
        let descriptor = directory.join("descriptor.json");
        std::fs::write(
            &descriptor,
            json!({"base_url":gateway.base_url(),"catalog":{"models":[catalog]}}).to_string(),
        )
        .unwrap();
        servers.spawn(axum::serve(listener, gateway.clone().router()).into_future());
        let report_path = directory.join("evidence.json");
        let mut child = tokio::process::Command::new("node");
        child
            .arg(repo.join("scripts/probe-context-runtime-client.mjs"))
            .arg(&executable)
            .arg(&descriptor)
            .arg(&report_path)
            .kill_on_drop(true)
            .creation_flags(0x08000000)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let status = tokio::time::timeout(Duration::from_secs(150), child.status())
            .await
            .unwrap()
            .unwrap();
        let mut report: Value =
            serde_json::from_slice(&std::fs::read(&report_path).unwrap()).unwrap();
        let summary_restored = browser.prompt.lock().unwrap()["history"]
            .to_string()
            .contains("Preserve fixture goal");
        report["schema"] = json!("cxweb.native-runtime-context-probe.v1");
        report["client"] = json!(build);
        report["transport"] = json!(if websocket { "websocket" } else { "http" });
        report["fixture_limits"] = json!({"normal_prompt_bytes":budget.normal_bytes(),"summary_prompt_bytes":budget.summary_bytes(),"estimated_context_tokens":budget.estimated_tokens(),"browser_answer_bytes":64 * 1024});
        report["runtime"] = json!({"http_requests":provider.http.load(Ordering::SeqCst),"websocket_requests":provider.websocket.load(Ordering::SeqCst),"compactions":provider.compactions.load(Ordering::SeqCst),"encrypted_continuations":provider.continuations.load(Ordering::SeqCst),"browser_stub_submissions":browser.sends.load(Ordering::SeqCst),"native_upstream_frames":native_frames.load(Ordering::SeqCst),"summary_restored":summary_restored,"errors":*provider.failures.lock().unwrap()});
        let runtime_verified = status.success()
            && provider.compactions.load(Ordering::SeqCst) == 1
            && provider.continuations.load(Ordering::SeqCst) >= 1
            && browser.sends.load(Ordering::SeqCst) == 4
            && native_frames.load(Ordering::SeqCst) == 0
            && summary_restored
            && provider.failures.lock().unwrap().is_empty()
            && provider.http.load(Ordering::SeqCst) == if websocket { 0 } else { 5 }
            && provider.websocket.load(Ordering::SeqCst) == if websocket { 5 } else { 0 };
        report["runtime_verified"] = json!(runtime_verified);
        if !runtime_verified {
            report["result"] = json!("FAIL runtime context recovery");
        }
        std::fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        gateway
            .disconnect_web(Duration::from_secs(3))
            .await
            .unwrap();
        servers.abort_all();
        eprintln!("Context evidence: {}", report_path.display());
        assert!(
            runtime_verified,
            "native context sequence failed; inspect sanitized evidence"
        );
    }
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(executable).unwrap())),
        fingerprint
    );
}
