//! Isolated native qualification. No user home or native login; fixture tools only.
use crate::{control::GenerationSession, native_preflight};
use cxweb_codex_adapter::{catalog_codec::CatalogCodec, strict_json};
use cxweb_platform::{state::protected_directory, target_path::TargetPathGuard};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{path::Path, process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};
use tokio_util::sync::CancellationToken;

mod automatic_checkpoint;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Exercise {
    #[default]
    Text,
    ReadPatch,
    ReadPatchTest,
    ReadTestRepair,
    DeniedRead,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointTarget {
    pub client: std::path::PathBuf,
    pub websocket: bool,
    #[serde(default)]
    pub capture_failure: bool,
    #[serde(default)]
    pub automatic: bool,
    #[serde(default)]
    pub tool_result: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolCheckpointEvidence {
    read_verified: bool,
    recall_verified: bool,
    #[serde(default)]
    post_compaction_tools_verified: bool,
}
fn save_tool_checkpoint_evidence(
    cwd: &Path,
    recall_verified: bool,
    post_compaction_tools_verified: bool,
) -> Result<(), &'static str> {
    std::fs::write(
        cwd.join("tool-checkpoint-progress.json"),
        serde_json::to_vec(&ToolCheckpointEvidence {
            read_verified: true,
            recall_verified,
            post_compaction_tools_verified,
        })
        .map_err(|_| "E_NATIVE_PROBE_REPORT")?,
    )
    .map_err(|_| "E_NATIVE_PROBE_REPORT")
}

/// Uses an already leased installed browser, but a disposable signed-out native
/// home. No installed config, credentials or production capabilities are changed.
pub(crate) async fn qualify_installed_checkpoint(
    selected: &CheckpointTarget,
    driver: &crate::managed_driver::ManagedDriver,
    binding: &crate::managed_driver::Binding,
    installation_directory: &Path,
    cancellation: CancellationToken,
) -> Result<(), &'static str> {
    if selected.automatic && selected.tool_result {
        return Err("E_NATIVE_PROBE_CONFIG");
    }
    let target =
        TargetPathGuard::capture(&selected.client, false).map_err(|_| "E_NATIVE_PROBE_TARGET")?;
    let hash = native_preflight::fingerprint(&selected.client).await?;
    let (build, codec) = native_preflight::describe(&selected.client).await?;
    let directory =
        installation_directory.join(format!("checkpoint-client-{:032x}", rand::random::<u128>()));
    protected_directory(&directory).map_err(|_| "E_NATIVE_PROBE_DIRECTORY")?;
    let _capture = if selected.capture_failure {
        Some(
            cxweb_browser_adapter::FailureCapture::enable(directory.join("scope-failure.png"))
                .map_err(|_| "E_NATIVE_PROBE_CAPTURE")?,
        )
    } else {
        None
    };
    let marker = format!("CXWEB_NATIVE_CHECKPOINT_{:032x}", rand::random::<u128>());
    let fixture = selected.tool_result.then(|| {
        let mut fixture =
            crate::native_fixture::Fixture::new(directory.join("workspace"), marker.clone());
        fixture.checkpoint = true;
        fixture.test = true;
        fixture.capture_rejection = selected.capture_failure;
        Arc::new(fixture)
    });
    let descriptor = directory.join("runtime/connection.json");
    let report_path = installation_directory.join("native-compaction-probe-report.json");
    let mut report = json!({"schema":"cxweb.native-checkpoint.v1","stage":"starting","completed":false,
        "client_build":build,"catalog_codec":codec.id(),"executable_sha256":hash,"websocket":selected.websocket,
        "saved_browser_reused":true,"isolated_native_home":true,"native_tools_executed":if selected.tool_result { Value::Null } else { json!(0) },
        "production_capability_published":false});
    tokio::fs::write(&report_path, report.to_string())
        .await
        .map_err(|_| "E_NATIVE_PROBE_REPORT")?;
    let stop = CancellationToken::new();
    let _stop_on_drop = stop.clone().drop_guard();
    let server = crate::live_probe::serve_installed_checkpoint(
        &descriptor,
        driver,
        binding,
        codec,
        selected,
        fixture.clone(),
        stop.clone().cancelled_owned(),
    );
    let client = async {
        let result = async {
            let mut connection = tokio::select! {
                () = cancellation.cancelled() => return Err("E_NATIVE_PROBE_CANCELLED"),
                result = tokio::time::timeout(Duration::from_secs(15), async {
                    loop {
                        if let Ok(bytes) = tokio::fs::read(&descriptor).await
                            && let Ok(value) = strict_json::parse(&bytes, 1024 * 1024) { return value; }
                        tokio::time::sleep(Duration::from_millis(25)).await;
                    }
                }) => result.map_err(|_| "E_NATIVE_PROBE_ENDPOINT")?,
            };
            connection["verify_checkpoint"] = json!(true);
            connection["automatic_checkpoint"] = json!(selected.automatic);
            connection["tool_result_checkpoint"] = json!(selected.tool_result);
            let route = binding.routes.first().ok_or("E_MODEL_UNAVAILABLE")?;
            let endpoint = Endpoint::parse(connection, &route.id, codec)?;
            let expected = fixture.as_ref().map(|f| f.expected()).unwrap_or(&marker);
            run_client(&selected.client, &target, &directory, &endpoint, expected, &cancellation, fixture.clone()).await
        }.await;
        stop.cancel();
        result
    };
    let (runtime, client) = tokio::join!(server, client);
    report["tool_result_requested"] = json!(selected.tool_result);
    if let Some(fixture) = &fixture {
        report["action_rejection"] = json!(fixture.rejection_diagnostic());
    }
    if selected.tool_result {
        report["native_tools_executed"] = Value::Null;
        report["tool_result_recall_verified"] = json!(false);
        report["post_compaction_tools_verified"] = json!(false);
        if let Ok(bytes) =
            tokio::fs::read(directory.join("workspace/tool-checkpoint-progress.json")).await
            && let Ok(progress) = serde_json::from_slice::<ToolCheckpointEvidence>(&bytes)
        {
            if progress.read_verified {
                report["native_tools_executed"] = json!(1);
            }
            report["tool_result_recall_verified"] =
                json!(progress.recall_verified && client.is_ok());
            if progress.post_compaction_tools_verified && client.is_ok() {
                report["native_tools_executed"] = json!(3);
                report["post_compaction_tools_verified"] = json!(true);
            }
        }
    }
    report["automatic_requested"] = json!(selected.automatic);
    if selected.automatic {
        report["diagnostic_budget"] = json!({"normal_encoded_bytes":96 * 1024,"summary_encoded_bytes":256 * 1024,"production_capacity_qualified":false});
        if let Ok(bytes) =
            tokio::fs::read(directory.join("workspace/automatic-checkpoint-report.json")).await
            && let Ok(progress) = serde_json::from_slice::<automatic_checkpoint::Report>(&bytes)
        {
            report["automatic_checkpoint"] = serde_json::to_value(progress).unwrap_or(Value::Null);
        }
    }
    let result = async {
        let runtime = runtime?;
        // ScopeDiagnostic contains only bounded structural observations and
        // fixed UI labels; ScopeSurface's account/workspace IDs are excluded.
        report["browser_scope"] = runtime["diagnostic"]["scope"].clone();
        // The driver exports fixed structural flags/counts only, never text.
        report["output_shape"] = runtime["diagnostic"]["output_shape"].clone();
        report["transport"] = json!({
            "failures":runtime["failures"],"context_refusals":runtime["context_refusals"],"compaction_requests":runtime["compaction_requests"],
            "checkpoint_continuations":runtime["checkpoint_continuations"],
            "checkpoint_continuations_with_plaintext_assistant_or_tools":runtime["checkpoint_continuations_with_plaintext_assistant_or_tools"],
            "first_checkpoint_continuation_with_plaintext_assistant_or_tools":runtime["first_checkpoint_continuation_with_plaintext_assistant_or_tools"],
            "websocket_requests":runtime["websocket_requests"],"native_websocket_frames":runtime["native_websocket_frames"]});
        client?;
        if selected.automatic {
            if report["automatic_checkpoint"]["completed"] != true { return Err("E_NATIVE_PROBE_AUTOMATIC_REPORT"); }
            if runtime["context_refusals"] != 1 { return Err("E_NATIVE_PROBE_AUTOMATIC_REFUSAL"); }
        }
        if runtime["failures"] != json!([]) || runtime["compaction_requests"] != 1
            || runtime["checkpoint_continuations"] != if selected.tool_result { 4 } else { 1 }
            || runtime["first_checkpoint_continuation_with_plaintext_assistant_or_tools"] != 0
            || (!selected.tool_result && runtime["checkpoint_continuations_with_plaintext_assistant_or_tools"] != 0)
            || (selected.tool_result && report["post_compaction_tools_verified"] != true)
            || runtime["native_websocket_frames"] != 0
            || (selected.websocket && runtime["websocket_requests"].as_u64().unwrap_or(0) < 3) {
            return Err("E_NATIVE_PROBE_CHECKPOINT_TRANSPORT");
        }
        target.verify_unchanged().map_err(|_| "E_NATIVE_PROBE_TARGET_CHANGED")?;
        if native_preflight::fingerprint(&selected.client).await? != hash {
            return Err("E_NATIVE_PROBE_TARGET_CHANGED");
        }
        Ok(())
    }.await;
    let cleanup = driver.verify_idle().await;
    report["cleanup_confirmed"] = json!(cleanup.is_ok());
    let result = cleanup.and(result);
    report["completed"] = json!(result.is_ok());
    report["stage"] = json!(if result.is_ok() {
        "completed"
    } else {
        "failed"
    });
    report["failure"] = json!(result.as_ref().err());
    report["observed_at"] = json!(cxweb_platform::clock::utc_timestamp());
    let saved = tokio::fs::write(report_path, report.to_string())
        .await
        .map_err(|_| "E_NATIVE_PROBE_REPORT");
    result.and(saved)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    #[serde(default)]
    pub exercise: Exercise,
    pub client_build: String,
    pub catalog_codec: String,
    pub executable_sha256: String,
    pub exact_text_received: bool,
    pub native_tools_executed: u32,
    pub browser_reused: bool,
    pub routing_installed: bool,
    pub actual_picker_verified: bool,
}

/// One explicit test message through a reviewed client and the existing browser.
/// A text result is not coding, picker or native-subscription qualification.
pub async fn qualify_text(
    executable: &Path,
    session: Arc<GenerationSession>,
    websocket: bool,
    cancellation: CancellationToken,
) -> Result<Report, &'static str> {
    let executable = executable.to_owned();
    // Accepted qualification owns its child and cleanup independently of a UI
    // waiter. Explicit cancellation still stops the child and drains generation.
    tokio::spawn(async move {
        qualify_owned(
            &executable,
            session,
            websocket,
            Exercise::Text,
            cancellation,
        )
        .await
    })
    .await
    .map_err(|_| "E_NATIVE_PROBE_WORKER")?
}

/// One fixed read/patch/test/repair or command-denial exercise in a disposable workspace. This does not
/// certify the complete coding corpus or publish a production coding route.
pub async fn qualify_tools(
    executable: &Path,
    session: Arc<GenerationSession>,
    exercise: Exercise,
    cancellation: CancellationToken,
) -> Result<Report, &'static str> {
    let executable = executable.to_owned();
    tokio::spawn(
        async move { qualify_owned(&executable, session, false, exercise, cancellation).await },
    )
    .await
    .map_err(|_| "E_NATIVE_PROBE_WORKER")?
}

async fn qualify_owned(
    executable: &Path,
    session: Arc<GenerationSession>,
    websocket: bool,
    exercise: Exercise,
    cancellation: CancellationToken,
) -> Result<Report, &'static str> {
    if cancellation.is_cancelled() {
        return Err("E_NATIVE_PROBE_CANCELLED");
    }
    let target =
        TargetPathGuard::capture(executable, false).map_err(|_| "E_NATIVE_PROBE_TARGET")?;
    let hash = native_preflight::fingerprint(executable).await?;
    let (build, codec) = native_preflight::describe(executable).await?;
    let paths = cxweb_platform::state::StatePaths::open().map_err(|_| "E_STATE_PERMISSIONS")?;
    let directory = paths
        .state
        .join(format!("native-probe-{:032x}", rand::random::<u128>()));
    protected_directory(&directory).map_err(|_| "E_NATIVE_PROBE_DIRECTORY")?;
    let descriptor = directory.join("runtime/connection.json");
    let stop = CancellationToken::new();
    let _stop_on_drop = stop.clone().drop_guard();
    let marker = format!("cxweb native fixture {:032x}", rand::random::<u128>());
    let fixture = (exercise != Exercise::Text).then(|| {
        let mut fixture =
            crate::native_fixture::Fixture::new(directory.join("workspace"), marker.clone());
        fixture.denial = exercise == Exercise::DeniedRead;
        fixture.test = matches!(exercise, Exercise::ReadPatchTest | Exercise::ReadTestRepair);
        fixture.repair = exercise == Exercise::ReadTestRepair;
        Arc::new(fixture)
    });
    let expected = fixture
        .as_ref()
        .map(|fixture| fixture.expected().to_owned())
        .unwrap_or(marker);
    let probe = async {
        if let Some(fixture) = &fixture {
            crate::live_probe::serve_generation_fixture(
                &descriptor,
                &session,
                codec,
                fixture.clone(),
                stop.clone().cancelled_owned(),
            )
            .await
        } else {
            crate::live_probe::serve_generation(
                &descriptor,
                &session,
                websocket,
                codec,
                false,
                false,
                stop.clone().cancelled_owned(),
            )
            .await
        }
    };
    let client = async {
        let result = async {
            let connection = tokio::select! {
                () = cancellation.cancelled() => return Err("E_NATIVE_PROBE_CANCELLED"),
                result = tokio::time::timeout(Duration::from_secs(15), async {
                    loop {
                        if let Ok(bytes) = tokio::fs::read(&descriptor).await {
                            if bytes.len() > 1024 * 1024 { return Err("E_NATIVE_PROBE_ENDPOINT"); }
                            if let Ok(value) = strict_json::parse(&bytes, 1024 * 1024) { return Ok(value); }
                        }
                        tokio::time::sleep(Duration::from_millis(25)).await;
                    }
                }) => result.map_err(|_| "E_NATIVE_PROBE_ENDPOINT")??,
            };
            let endpoint = Endpoint::parse(connection, &session.route.id, codec)?;
            run_client(executable, &target, &directory, &endpoint, &expected, &cancellation, fixture.clone()).await
        }.await;
        stop.cancel();
        result
    };
    let (runtime, result) = tokio::join!(probe, client);
    let runtime = runtime?;
    result?;
    if runtime["failures"] != json!([]) || runtime["native_websocket_frames"] != 0 {
        return Err("E_NATIVE_PROBE_GENERATION");
    }
    target
        .verify_unchanged()
        .map_err(|_| "E_NATIVE_PROBE_TARGET_CHANGED")?;
    if native_preflight::fingerprint(executable).await? != hash {
        return Err("E_NATIVE_PROBE_TARGET_CHANGED");
    }
    Ok(Report {
        exercise,
        client_build: build,
        catalog_codec: codec.id().into(),
        executable_sha256: hash,
        exact_text_received: true,
        native_tools_executed: if exercise == Exercise::ReadTestRepair {
            4
        } else if exercise == Exercise::ReadPatchTest {
            3
        } else if exercise == Exercise::ReadPatch {
            2
        } else {
            0
        },
        browser_reused: true,
        routing_installed: false,
        actual_picker_verified: false,
    })
}

// Constructed only from our private live probe or a test-owned synthetic server.
struct Endpoint {
    base: String,
    catalog: Value,
    model: String,
    verify_checkpoint: bool,
    automatic_checkpoint: bool,
    tool_result_checkpoint: bool,
}
impl Endpoint {
    fn parse(value: Value, model: &str, codec: CatalogCodec) -> Result<Self, &'static str> {
        let base = value["base_url"]
            .as_str()
            .ok_or("E_NATIVE_PROBE_ENDPOINT")?;
        let url = reqwest::Url::parse(base).map_err(|_| "E_NATIVE_PROBE_ENDPOINT")?;
        if url.scheme() != "http"
            || url.host_str() != Some("127.0.0.1")
            || url.port().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.path().starts_with("/wb/")
            || !url.path().ends_with("/backend-api/codex")
            || value["model"] != model
            || value["catalog_codec"] != codec.id()
            || value["catalog"]["models"]
                .as_array()
                .is_none_or(|models| models.len() != 1 || models[0]["slug"] != model)
        {
            return Err("E_NATIVE_PROBE_ENDPOINT");
        }
        Ok(Self {
            base: base.into(),
            catalog: value["catalog"].clone(),
            model: model.into(),
            verify_checkpoint: value["verify_checkpoint"] == true,
            automatic_checkpoint: value["automatic_checkpoint"] == true,
            tool_result_checkpoint: value["tool_result_checkpoint"] == true,
        })
    }
}

struct Client {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    count: usize,
    bytes: usize,
    observations: Vec<Value>,
    fixture: Option<Arc<crate::native_fixture::Fixture>>,
    thread: Option<String>,
    turn: Option<String>,
    read_approved: bool,
    read_denied: bool,
    patch_approved: bool,
    tests_approved: u8,
    compacting: bool,
}
impl Client {
    async fn send(&mut self, value: Value) -> Result<(), &'static str> {
        let mut bytes = serde_json::to_vec(&value).map_err(|_| "E_NATIVE_PROBE_RPC")?;
        bytes.push(b'\n');
        self.input
            .write_all(&bytes)
            .await
            .map_err(|_| "E_NATIVE_PROBE_RPC")
    }
    async fn next(&mut self) -> Result<Value, &'static str> {
        let bytes = native_preflight::line(&mut self.output)
            .await
            .map_err(|_| "E_NATIVE_PROBE_RPC")?;
        self.count += 1;
        self.bytes += bytes.len();
        if self.count > 4096 || self.bytes > 64 * 1024 * 1024 {
            return Err("E_NATIVE_PROBE_LIMIT");
        }
        strict_json::parse(&bytes, 8 * 1024 * 1024).map_err(|_| "E_NATIVE_PROBE_RPC")
    }
    async fn observe(&mut self, message: Value) -> Result<(), &'static str> {
        if message["method"] == "turn/started" {
            if message["params"]["threadId"].as_str() != self.thread.as_deref() {
                return Err("E_NATIVE_PROBE_ACTION");
            }
            let turn = message["params"]["turn"]["id"]
                .as_str()
                .ok_or("E_NATIVE_PROBE_RPC")?;
            if self.turn.as_deref().is_some_and(|old| old != turn) {
                return Err("E_NATIVE_PROBE_ACTION");
            }
            self.turn = Some(turn.into());
        }
        if let Some(id) = message.get("id") {
            let params = &message["params"];
            let scope = self.thread.is_some()
                && self.turn.is_some()
                && params["threadId"].as_str() == self.thread.as_deref()
                && params["turnId"].as_str() == self.turn.as_deref();
            let mut accepted = false;
            let mut intentional_denial = false;
            if scope && let Some(fixture) = &self.fixture {
                match message["method"].as_str() {
                    Some("item/commandExecution/requestApproval") => {
                        if !self.read_approved && !self.read_denied && fixture.approve_read(params)
                        {
                            intentional_denial = fixture.denial;
                            accepted = !fixture.denial;
                            self.read_approved |= accepted;
                            self.read_denied |= intentional_denial;
                        } else if self.tests_approved < if fixture.repair { 2 } else { 1 }
                            && fixture.approve_test(params)
                            && self.observations.iter().any(|event| {
                                event["method"] == "item/completed"
                                    && event["params"]["threadId"] == params["threadId"]
                                    && event["params"]["turnId"] == params["turnId"]
                                    && if fixture.repair && self.tests_approved == 0 {
                                        read_result(fixture, &event["params"]["item"])
                                    } else {
                                        event["params"]["item"]["type"] == "fileChange"
                                            && event["params"]["item"]["status"] == "completed"
                                            && fixture.patch_changes(&event["params"]["item"])
                                    }
                            })
                        {
                            accepted = true;
                            self.tests_approved += 1;
                        }
                    }
                    Some("item/fileChange/requestApproval")
                        if !self.patch_approved && !fixture.denial =>
                    {
                        if let Some(started) = self.observations.iter().rev().find(|event| {
                            event["method"] == "item/started"
                                && event["params"]["item"]["id"] == params["itemId"]
                        }) {
                            accepted = fixture.approve_patch(params, started)
                                && (!fixture.repair
                                    || self.observations.iter().any(|event| {
                                        event["method"] == "item/completed"
                                            && event["params"]["threadId"] == params["threadId"]
                                            && event["params"]["turnId"] == params["turnId"]
                                            && test_result(fixture, &event["params"]["item"], false)
                                    }));
                            self.patch_approved |= accepted;
                        }
                    }
                    _ => (),
                }
            }
            let approval = matches!(
                message["method"].as_str(),
                Some("item/commandExecution/requestApproval" | "item/fileChange/requestApproval")
            );
            let reply = if approval {
                json!({"id":id,"result":{"decision":if accepted {"accept"} else {"decline"}}})
            } else {
                json!({"id":id,"error":{"code":-32601,"message":"Diagnostic does not authorize this action"}})
            };
            self.send(reply).await?;
            return if accepted || intentional_denial {
                Ok(())
            } else {
                Err("E_NATIVE_PROBE_ACTION")
            };
        }
        if matches!(
            message["method"].as_str(),
            Some("item/started" | "item/completed")
        ) && !matches!(
            message["params"]["item"]["type"].as_str(),
            Some("agentMessage" | "userMessage" | "reasoning")
        ) && !(self.compacting && message["params"]["item"]["type"] == "contextCompaction")
            && !(self.fixture.is_some()
                && matches!(
                    message["params"]["item"]["type"].as_str(),
                    Some("commandExecution" | "fileChange")
                ))
        {
            return Err("E_NATIVE_PROBE_ACTION");
        }
        if matches!(
            message["method"].as_str(),
            Some("item/completed" | "turn/completed")
        ) || (self.fixture.is_some() && message["method"] == "item/started")
        {
            if self.observations.len() >= 128 {
                return Err("E_NATIVE_PROBE_LIMIT");
            }
            self.observations.push(message);
        }
        Ok(())
    }
    async fn rpc(&mut self, id: u64, method: &str, params: Value) -> Result<Value, &'static str> {
        self.send(json!({"id":id,"method":method,"params":params}))
            .await?;
        loop {
            let message = self.next().await?;
            if message.get("method").is_some() {
                self.observe(message).await?;
                continue;
            }
            if message["id"] != id || message.get("error").is_some() {
                return Err("E_NATIVE_PROBE_REJECTED");
            }
            return message.get("result").cloned().ok_or("E_NATIVE_PROBE_RPC");
        }
    }
    async fn verify(
        &mut self,
        cwd: &Path,
        endpoint: &Endpoint,
        catalog: &Path,
        expected: &str,
    ) -> Result<(), &'static str> {
        self.rpc(1, "initialize", json!({"clientInfo":{"name":"cxweb_native_qualification","version":"0.1.0"},"capabilities":{"experimentalApi":true}})).await?;
        self.send(json!({"method":"initialized","params":{}}))
            .await?;
        let config = self
            .rpc(2, "config/read", json!({"includeLayers":false,"cwd":cwd}))
            .await?;
        let config = &config["config"];
        if config["openai_base_url"] != endpoint.base
            || !same_catalog_path(config["model_catalog_json"].as_str(), catalog)
            || config
                .get("model_provider")
                .is_some_and(|v| !v.is_null() && v != "openai")
            || config["model_providers"].get("openai").is_some()
            || config.get("profile").is_some_and(|value| !value.is_null())
        {
            return Err("E_NATIVE_PROBE_CONFIG");
        }
        let account = self
            .rpc(6, "account/read", json!({"refreshToken":false}))
            .await?;
        // The diagnostic uses a fresh signed-out home and only our local
        // capability. No subscription login or API key is required by this test.
        if account.get("account") != Some(&Value::Null) {
            return Err("E_NATIVE_PROBE_AUTH");
        }
        let models = self
            .rpc(3, "model/list", json!({"includeHidden":true}))
            .await?;
        let selected = models["data"]
            .as_array()
            .and_then(|models| models.iter().find(|model| model["model"] == endpoint.model));
        let Some(selected) = selected else {
            return Err("E_NATIVE_PROBE_MODEL");
        };
        let expected_model = endpoint.catalog["models"]
            .as_array()
            .and_then(|models| models.iter().find(|model| model["slug"] == endpoint.model))
            .ok_or("E_NATIVE_PROBE_MODEL")?;
        let levels = selected["supportedReasoningEfforts"]
            .as_array()
            .ok_or("E_NATIVE_PROBE_REASONING")?;
        let expected_levels = expected_model["supported_reasoning_levels"]
            .as_array()
            .ok_or("E_NATIVE_PROBE_REASONING")?;
        if selected["defaultReasoningEffort"] != expected_model["default_reasoning_level"]
            || levels.len() != expected_levels.len()
            || expected_levels.iter().any(|expected| {
                !levels.iter().any(|actual| {
                    actual["reasoningEffort"] == expected["effort"]
                        && actual["description"] == expected["description"]
                })
            })
        {
            return Err("E_NATIVE_PROBE_REASONING");
        }
        let thread = self
            .rpc(
                4,
                "thread/start",
                json!({"cwd":cwd,"model":endpoint.model,"ephemeral":true,"approvalPolicy":"untrusted","sandbox":"read-only"}),
            )
            .await?;
        let thread = thread["thread"]["id"]
            .as_str()
            .ok_or("E_NATIVE_PROBE_RPC")?
            .to_owned();
        self.thread = Some(thread.clone());
        if endpoint.automatic_checkpoint {
            return self.verify_automatic_checkpoint(cwd, &thread).await;
        }
        let prompt = self
            .fixture
            .as_ref()
            .map(|fixture| fixture.prompt())
            .unwrap_or_else(|| {
                if endpoint.verify_checkpoint {
                    "Use no tools. Invent a fresh random value of exactly 32 lowercase hexadecimal characters. Return exactly CXWEB_NATIVE_CHECKPOINT_ followed by that value, with no spaces or other text. Remember the complete line for a later question; preserve it verbatim in any task checkpoint.".into()
                } else {
                    format!("Use no tools. Return a final answer with exactly this text: {expected}")
                }
            });
        let turn = self.rpc(5, "turn/start", json!({"threadId":thread,"input":[{"type":"text","text":prompt,"text_elements":[]}]})).await?;
        let turn = turn["turn"]["id"].as_str().ok_or("E_NATIVE_PROBE_RPC")?;
        if self.turn.as_deref().is_some_and(|old| old != turn) {
            return Err("E_NATIVE_PROBE_ACTION");
        }
        self.turn = Some(turn.into());
        loop {
            if endpoint.tool_result_checkpoint {
                if text_complete(
                    &self.observations,
                    &thread,
                    turn,
                    crate::native_fixture::READ_ACK,
                )? {
                    let fixture = self.fixture.as_ref().ok_or("E_NATIVE_PROBE_ACTION")?;
                    verify_checkpoint_read(fixture, &self.observations, &thread, turn)?;
                    verify_checkpoint_file(fixture)?;
                    save_tool_checkpoint_evidence(cwd, false, false)?;
                    let expected = fixture.marker.clone();
                    self.verify_checkpoint(&thread, &expected).await?;
                    verify_checkpoint_file(self.fixture.as_ref().ok_or("E_NATIVE_PROBE_ACTION")?)?;
                    save_tool_checkpoint_evidence(cwd, true, false)?;
                    self.verify_post_checkpoint_tools(&thread).await?;
                    return save_tool_checkpoint_evidence(cwd, true, true);
                }
            } else if endpoint.verify_checkpoint {
                if let Some(seed) = checkpoint_seed(&self.observations, &thread, turn)? {
                    return self.verify_checkpoint(&thread, &seed).await;
                }
            } else if text_complete(&self.observations, &thread, turn, expected)? {
                if let Some(fixture) = &self.fixture {
                    if fixture.denial {
                        if !self.read_denied || self.read_approved || self.patch_approved {
                            return Err("E_NATIVE_PROBE_DENIAL");
                        }
                        verify_denial(fixture, &self.observations, &thread, turn)?;
                    } else {
                        verify_tools(fixture, &self.observations, &thread, turn)?;
                    }
                }
                return Ok(());
            }
            let message = self.next().await?;
            self.observe(message).await?;
        }
    }

    async fn verify_post_checkpoint_tools(&mut self, thread: &str) -> Result<(), &'static str> {
        self.observations.clear();
        self.turn = None;
        let prompt = format!(
            "Use the apply_patch custom tool exactly once to add probe-output.txt containing the complete line remembered from the checkpoint followed by a newline. Wait for the patch result, then use exec_command with cmd exactly {:?}, login=false and the current working directory. Wait for the actual test result. Return exactly {:?} only after the test exits with code 0 and prints that text. Do not read the input again, run other commands, change other files, request elevated permissions or access the network.",
            crate::native_fixture::TEST,
            crate::native_fixture::TEST_PASSED
        );
        let started = self.rpc(9, "turn/start", json!({"threadId":thread,"input":[{"type":"text","text":prompt,"text_elements":[]}]})).await?;
        let turn = started["turn"]["id"].as_str().ok_or("E_NATIVE_PROBE_RPC")?;
        if self.turn.as_deref().is_some_and(|old| old != turn) {
            return Err("E_NATIVE_PROBE_ACTION");
        }
        self.turn = Some(turn.into());
        loop {
            if text_complete(
                &self.observations,
                thread,
                turn,
                crate::native_fixture::TEST_PASSED,
            )? {
                return verify_post_checkpoint_tools(
                    self.fixture.as_ref().ok_or("E_NATIVE_PROBE_ACTION")?,
                    &self.observations,
                    thread,
                    turn,
                );
            }
            let message = self.next().await?;
            self.observe(message).await?;
        }
    }

    async fn verify_checkpoint(
        &mut self,
        thread: &str,
        expected: &str,
    ) -> Result<(), &'static str> {
        self.observations.clear();
        self.turn = None;
        self.compacting = true;
        self.rpc(7, "thread/compact/start", json!({"threadId":thread}))
            .await?;
        loop {
            if let Some(turn) = self.turn.as_deref()
                && checkpoint_complete(&self.observations, thread, turn)?
            {
                break;
            }
            let message = self.next().await?;
            self.observe(message).await?;
        }
        self.compacting = false;
        self.observations.clear();
        self.turn = None;
        let resumed = self.rpc(8, "turn/start", json!({"threadId":thread,"input":[{"type":"text","text":"Return exactly the complete line remembered earlier. Recover it from the task checkpoint. Use no tools and add no other text.","text_elements":[]}]})).await?;
        let turn = resumed["turn"]["id"].as_str().ok_or("E_NATIVE_PROBE_RPC")?;
        if self.turn.as_deref().is_some_and(|old| old != turn) {
            return Err("E_NATIVE_PROBE_ACTION");
        }
        self.turn = Some(turn.into());
        loop {
            if text_complete(&self.observations, thread, turn, expected)? {
                return Ok(());
            }
            let message = self.next().await?;
            self.observe(message).await?;
        }
    }
}

fn same_catalog_path(reported: Option<&str>, expected: &Path) -> bool {
    let Some(reported) = reported.map(Path::new).filter(|path| path.is_absolute()) else {
        return false;
    };
    let Ok(_guard) = TargetPathGuard::capture(reported, false) else {
        return false;
    };
    match (reported.canonicalize(), expected.canonicalize()) {
        (Ok(reported), Ok(expected)) => reported == expected,
        _ => false,
    }
}

fn checkpoint_complete(events: &[Value], thread: &str, turn: &str) -> Result<bool, &'static str> {
    let Some(done) = events.iter().find(|event| {
        event["method"] == "turn/completed"
            && event["params"]["threadId"] == thread
            && event["params"]["turn"]["id"] == turn
    }) else {
        return Ok(false);
    };
    if done["params"]["turn"]["status"] != "completed" {
        return Err("E_NATIVE_PROBE_COMPACTION");
    }
    let items: Vec<_> = events
        .iter()
        .filter(|event| {
            event["method"] == "item/completed"
                && event["params"]["threadId"] == thread
                && event["params"]["turnId"] == turn
        })
        .collect();
    if items.len() != 1 || items[0]["params"]["item"]["type"] != "contextCompaction" {
        return Err("E_NATIVE_PROBE_COMPACTION");
    }
    Ok(true)
}

// The first answer supplies the unpredictable fixture. Neither user message
// contains it, so retaining user messages cannot make checkpoint recall pass.
fn checkpoint_seed(
    events: &[Value],
    thread: &str,
    turn: &str,
) -> Result<Option<String>, &'static str> {
    let text = events
        .iter()
        .find(|event| {
            event["method"] == "item/completed"
                && event["params"]["threadId"] == thread
                && event["params"]["turnId"] == turn
                && event["params"]["item"]["type"] == "agentMessage"
        })
        .and_then(|event| event["params"]["item"]["text"].as_str())
        .unwrap_or("");
    if !text_complete(events, thread, turn, text)? {
        return Ok(None);
    }
    if !text
        .strip_prefix("CXWEB_NATIVE_CHECKPOINT_")
        .is_some_and(|value| {
            value.len() == 32
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
    {
        return Err("E_NATIVE_PROBE_CHECKPOINT_SEED");
    }
    Ok(Some(text.into()))
}

fn text_complete(
    events: &[Value],
    thread: &str,
    turn: &str,
    expected: &str,
) -> Result<bool, &'static str> {
    let Some(done) = events.iter().find(|value| {
        value["method"] == "turn/completed"
            && value["params"]["threadId"] == thread
            && value["params"]["turn"]["id"] == turn
    }) else {
        return Ok(false);
    };
    if done["params"]["turn"]["status"] != "completed" {
        return Err("E_NATIVE_PROBE_GENERATION");
    }
    let mut messages = events.iter().filter(|value| {
        value["method"] == "item/completed"
            && value["params"]["threadId"] == thread
            && value["params"]["turnId"] == turn
            && value["params"]["item"]["type"] == "agentMessage"
    });
    if !messages
        .next()
        .is_some_and(|message| message["params"]["item"]["text"] == expected)
        || messages.next().is_some()
    {
        return Err("E_NATIVE_PROBE_TEXT");
    }
    Ok(true)
}

async fn run_client(
    executable: &Path,
    target: &TargetPathGuard,
    directory: &Path,
    endpoint: &Endpoint,
    expected: &str,
    cancellation: &CancellationToken,
    fixture: Option<Arc<crate::native_fixture::Fixture>>,
) -> Result<(), &'static str> {
    let home = directory.join("home");
    let cwd = directory.join("workspace");
    if home.exists() || cwd.exists() {
        return Err("E_NATIVE_PROBE_DIRECTORY");
    }
    protected_directory(&home).map_err(|_| "E_NATIVE_PROBE_DIRECTORY")?;
    protected_directory(&cwd).map_err(|_| "E_NATIVE_PROBE_DIRECTORY")?;
    if let Some(fixture) = &fixture {
        if fixture.cwd != cwd || fixture.expected() != expected {
            return Err("E_NATIVE_PROBE_DIRECTORY");
        }
        std::fs::write(cwd.join("probe-input.txt"), format!("{}\n", fixture.marker))
            .map_err(|_| "E_NATIVE_PROBE_DIRECTORY")?;
        if fixture.repair {
            std::fs::write(
                cwd.join("probe-output.txt"),
                format!("{}\n", crate::native_fixture::BROKEN_OUTPUT),
            )
            .map_err(|_| "E_NATIVE_PROBE_DIRECTORY")?;
        }
    }
    let catalog = home.join("catalog.json");
    std::fs::write(&catalog, endpoint.catalog.to_string()).map_err(|_| "E_NATIVE_PROBE_CONFIG")?;
    // JSON quoted strings are valid TOML basic strings for these generated paths.
    let config = format!(
        "openai_base_url = {}\nmodel_catalog_json = {}\n",
        json!(endpoint.base),
        // Preserve a verbatim Windows prefix. Converting its backslashes to
        // slashes makes the native client's path round trip ambiguous.
        json!(catalog.to_string_lossy())
    );
    std::fs::write(home.join("config.toml"), config).map_err(|_| "E_NATIVE_PROBE_CONFIG")?;
    let mut command = Command::new(executable);
    command
        .args(["app-server", "--listen", "stdio://"])
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .creation_flags(0x0800_0000);
    for (key, _) in std::env::vars_os() {
        if ["CODEX_", "OPENAI_", "CHATGPT_"].iter().any(|prefix| {
            key.to_string_lossy()
                .to_ascii_uppercase()
                .starts_with(prefix)
        }) {
            command.env_remove(key);
        }
    }
    command.env("CODEX_HOME", &home);
    target
        .verify_unchanged()
        .map_err(|_| "E_NATIVE_PROBE_TARGET_CHANGED")?;
    let mut child = command.spawn().map_err(|_| "E_NATIVE_PROBE_START")?;
    let job =
        cxweb_platform::child_job::ChildJob::attach(&child).map_err(|_| "E_NATIVE_PROBE_START")?;
    let input = child.stdin.take().ok_or("E_NATIVE_PROBE_START")?;
    let output = BufReader::new(child.stdout.take().ok_or("E_NATIVE_PROBE_START")?);
    let mut client = Client {
        child,
        input,
        output,
        count: 0,
        bytes: 0,
        observations: Vec::new(),
        fixture,
        thread: None,
        turn: None,
        read_approved: false,
        read_denied: false,
        patch_approved: false,
        tests_approved: 0,
        compacting: false,
    };
    let verification_timeout = Duration::from_secs(if endpoint.automatic_checkpoint {
        3600
    } else {
        600
    });
    let result = tokio::select! {
        () = cancellation.cancelled() => Err("E_NATIVE_PROBE_CANCELLED"),
        result = tokio::time::timeout(verification_timeout, client.verify(&cwd, endpoint, &catalog, expected)) => result.map_err(|_| "E_NATIVE_PROBE_TIMEOUT").and_then(|result| result),
    };
    job.terminate_and_wait()
        .await
        .map_err(|_| "E_NATIVE_PROBE_CLEANUP")?;
    tokio::time::timeout(Duration::from_secs(5), client.child.wait())
        .await
        .map_err(|_| "E_NATIVE_PROBE_CLEANUP")?
        .map_err(|_| "E_NATIVE_PROBE_CLEANUP")?;
    target
        .verify_unchanged()
        .map_err(|_| "E_NATIVE_PROBE_TARGET_CHANGED")?;
    result
}

#[cfg(test)]
async fn run_text(
    executable: &Path,
    target: &TargetPathGuard,
    directory: &Path,
    endpoint: &Endpoint,
    expected: &str,
    cancellation: &CancellationToken,
) -> Result<(), &'static str> {
    run_client(
        executable,
        target,
        directory,
        endpoint,
        expected,
        cancellation,
        None,
    )
    .await
}

fn verify_denial(
    fixture: &crate::native_fixture::Fixture,
    events: &[Value],
    thread: &str,
    turn: &str,
) -> Result<(), &'static str> {
    let items: Vec<_> = events
        .iter()
        .filter(|event| {
            event["method"] == "item/completed"
                && event["params"]["threadId"] == thread
                && event["params"]["turnId"] == turn
        })
        .map(|event| &event["params"]["item"])
        .collect();
    let commands: Vec<_> = items
        .iter()
        .filter(|item| item["type"] == "commandExecution")
        .collect();
    if commands.len() != 1 || items.iter().any(|item| item["type"] == "fileChange") {
        return Err("E_NATIVE_PROBE_DENIAL");
    }
    let command = commands[0];
    if command["status"] != "declined"
        || !fixture.approve_read(command)
        || command["aggregatedOutput"]
            .as_str()
            .is_some_and(|text| text.contains(&fixture.marker))
        || fixture.cwd.join("probe-output.txt").exists()
    {
        return Err("E_NATIVE_PROBE_DENIAL");
    }
    let input = fixture.cwd.join("probe-input.txt");
    let _guard = TargetPathGuard::capture(&input, false).map_err(|_| "E_NATIVE_PROBE_DENIAL")?;
    if std::fs::read(input).map_err(|_| "E_NATIVE_PROBE_DENIAL")?
        != format!("{}\n", fixture.marker).as_bytes()
    {
        return Err("E_NATIVE_PROBE_DENIAL");
    }
    Ok(())
}

fn read_result(fixture: &crate::native_fixture::Fixture, item: &Value) -> bool {
    item["type"] == "commandExecution"
        && item["status"] == "completed"
        && item["exitCode"] == 0
        && fixture.approve_read(item)
        && item["aggregatedOutput"]
            .as_str()
            .is_some_and(|s| s.trim() == fixture.marker)
}

fn verify_checkpoint_read(
    fixture: &crate::native_fixture::Fixture,
    events: &[Value],
    thread: &str,
    turn: &str,
) -> Result<(), &'static str> {
    if events.iter().any(|event| {
        event["method"] == "item/completed"
            && matches!(
                event["params"]["item"]["type"].as_str(),
                Some("commandExecution" | "fileChange")
            )
            && (event["params"]["threadId"] != thread || event["params"]["turnId"] != turn)
    }) {
        return Err("E_NATIVE_PROBE_TOOL_RESULT");
    }
    let items: Vec<_> = events
        .iter()
        .filter(|event| {
            event["method"] == "item/completed"
                && event["params"]["threadId"] == thread
                && event["params"]["turnId"] == turn
        })
        .map(|event| &event["params"]["item"])
        .collect();
    let commands: Vec<_> = items
        .iter()
        .filter(|item| item["type"] == "commandExecution")
        .collect();
    if commands.len() != 1
        || !read_result(fixture, commands[0])
        || items.iter().any(|item| item["type"] == "fileChange")
    {
        return Err("E_NATIVE_PROBE_TOOL_RESULT");
    }
    // The answer must acknowledge the read without repeating the marker.
    if !text_complete(events, thread, turn, crate::native_fixture::READ_ACK)? {
        return Err("E_NATIVE_PROBE_TOOL_RESULT");
    }
    Ok(())
}

fn verify_checkpoint_file(fixture: &crate::native_fixture::Fixture) -> Result<(), &'static str> {
    let input = fixture.cwd.join("probe-input.txt");
    let _guard =
        TargetPathGuard::capture(&input, false).map_err(|_| "E_NATIVE_PROBE_TOOL_RESULT")?;
    if std::fs::read(input).map_err(|_| "E_NATIVE_PROBE_TOOL_RESULT")?
        != format!("{}\n", fixture.marker).as_bytes()
        || fixture.cwd.join("probe-output.txt").exists()
    {
        return Err("E_NATIVE_PROBE_TOOL_RESULT");
    }
    Ok(())
}

fn test_result(fixture: &crate::native_fixture::Fixture, item: &Value, passed: bool) -> bool {
    item["type"] == "commandExecution"
        && (item["status"] == "completed" || (!passed && item["status"] == "failed"))
        && item["exitCode"] == if passed { 0 } else { 1 }
        && fixture.approve_test(item)
        && item["aggregatedOutput"].as_str().is_some_and(|s| {
            s.trim()
                == if passed {
                    crate::native_fixture::TEST_PASSED
                } else {
                    crate::native_fixture::TEST_FAILED
                }
        })
}

fn verify_post_checkpoint_tools(
    fixture: &crate::native_fixture::Fixture,
    events: &[Value],
    thread: &str,
    turn: &str,
) -> Result<(), &'static str> {
    let mut tools = Vec::new();
    for event in events {
        if event["method"] == "item/completed"
            && matches!(
                event["params"]["item"]["type"].as_str(),
                Some("commandExecution" | "fileChange")
            )
        {
            if event["params"]["threadId"] != thread || event["params"]["turnId"] != turn {
                return Err("E_NATIVE_PROBE_ACTION");
            }
            tools.push(&event["params"]["item"]);
        }
    }
    let [patch, test] = tools.as_slice() else {
        return Err("E_NATIVE_PROBE_ACTION");
    };
    if patch["type"] != "fileChange"
        || patch["status"] != "completed"
        || !fixture.patch_changes(patch)
        || !test_result(fixture, test, true)
    {
        return Err("E_NATIVE_PROBE_ACTION");
    }
    verify_fixture_files(fixture)
}

fn verify_fixture_files(fixture: &crate::native_fixture::Fixture) -> Result<(), &'static str> {
    for name in ["probe-input.txt", "probe-output.txt"] {
        let path = fixture.cwd.join(name);
        let _guard = TargetPathGuard::capture(&path, false).map_err(|_| "E_NATIVE_PROBE_ACTION")?;
        if std::fs::read(&path).map_err(|_| "E_NATIVE_PROBE_ACTION")?
            != format!("{}\n", fixture.marker).as_bytes()
        {
            return Err("E_NATIVE_PROBE_ACTION");
        }
    }
    Ok(())
}

fn verify_tools(
    fixture: &crate::native_fixture::Fixture,
    events: &[Value],
    thread: &str,
    turn: &str,
) -> Result<(), &'static str> {
    let items: Vec<_> = events
        .iter()
        .filter(|event| {
            event["method"] == "item/completed"
                && event["params"]["threadId"] == thread
                && event["params"]["turnId"] == turn
        })
        .map(|event| &event["params"]["item"])
        .collect();
    let commands: Vec<_> = items
        .iter()
        .filter(|item| item["type"] == "commandExecution")
        .collect();
    let patches: Vec<_> = items
        .iter()
        .filter(|item| item["type"] == "fileChange")
        .collect();
    if commands.len()
        != if fixture.repair {
            3
        } else if fixture.test {
            2
        } else {
            1
        }
        || patches.len() != 1
    {
        return Err("E_NATIVE_PROBE_ACTION");
    }
    let tool_order: Vec<_> = items
        .iter()
        .filter_map(|item| {
            item["type"]
                .as_str()
                .filter(|kind| matches!(*kind, "commandExecution" | "fileChange"))
        })
        .collect();
    let expected_order: &[&str] = if fixture.repair {
        &[
            "commandExecution",
            "commandExecution",
            "fileChange",
            "commandExecution",
        ]
    } else if fixture.test {
        &["commandExecution", "fileChange", "commandExecution"]
    } else {
        &["commandExecution", "fileChange"]
    };
    if tool_order != expected_order {
        return Err("E_NATIVE_PROBE_ACTION");
    }
    let command = commands[0];
    let patch = patches[0];
    if fixture.test
        && (!test_result(fixture, commands[if fixture.repair { 2 } else { 1 }], true)
            || (fixture.repair && !test_result(fixture, commands[1], false)))
    {
        return Err("E_NATIVE_PROBE_TEST");
    }
    if !read_result(fixture, command)
        || patch["status"] != "completed"
        || !fixture.patch_changes(patch)
    {
        return Err("E_NATIVE_PROBE_ACTION");
    }
    verify_fixture_files(fixture)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_read_requires_attributed_real_output_without_answer_leakage() {
        let fixture =
            crate::native_fixture::Fixture::new(std::env::temp_dir(), "private-marker".into());
        let read = json!({"method":"item/completed","params":{"threadId":"t","turnId":"r","item":{"type":"commandExecution","status":"completed","exitCode":0,"cwd":fixture.cwd,"command":crate::native_fixture::READ,"aggregatedOutput":"private-marker\n"}}});
        let answer = json!({"method":"item/completed","params":{"threadId":"t","turnId":"r","item":{"type":"agentMessage","text":crate::native_fixture::READ_ACK}}});
        let done = json!({"method":"turn/completed","params":{"threadId":"t","turn":{"id":"r","status":"completed"}}});
        let valid = vec![read.clone(), answer.clone(), done.clone()];
        assert_eq!(verify_checkpoint_read(&fixture, &valid, "t", "r"), Ok(()));
        assert!(
            verify_checkpoint_read(&fixture, &[answer.clone(), done.clone()], "t", "r").is_err()
        );
        for (key, value) in [
            ("exitCode", json!(1)),
            ("status", json!("declined")),
            ("aggregatedOutput", json!("different")),
            ("command", json!("Get-ChildItem")),
        ] {
            let mut bad = read.clone();
            bad["params"]["item"][key] = value;
            assert!(
                verify_checkpoint_read(&fixture, &[bad, answer.clone(), done.clone()], "t", "r")
                    .is_err()
            );
        }
        let mut leaked = answer.clone();
        leaked["params"]["item"]["text"] = json!(fixture.marker);
        assert!(
            verify_checkpoint_read(&fixture, &[read.clone(), leaked, done.clone()], "t", "r")
                .is_err()
        );
        let mut extra = valid.clone();
        extra.push(read.clone());
        assert!(verify_checkpoint_read(&fixture, &extra, "t", "r").is_err());
        let mut foreign = read;
        foreign["params"]["turnId"] = json!("other");
        let mut extra = valid;
        extra.push(foreign);
        assert!(verify_checkpoint_read(&fixture, &extra, "t", "r").is_err());
    }

    #[test]
    fn catalog_identity_accepts_native_path_normalization_but_not_other_files() {
        let directory = std::env::temp_dir().join(format!(
            "cxweb-catalog-path-{:032x}",
            rand::random::<u128>()
        ));
        protected_directory(&directory).unwrap();
        let catalog = directory.join("catalog.json");
        std::fs::write(&catalog, "{}").unwrap();
        let canonical = catalog.canonicalize().unwrap();
        assert!(same_catalog_path(catalog.to_str(), &canonical));
        assert!(same_catalog_path(canonical.to_str(), &catalog));
        let other = directory.join("other.json");
        std::fs::write(&other, "{}").unwrap();
        assert!(!same_catalog_path(other.to_str(), &catalog));
        assert!(!same_catalog_path(Some("catalog.json"), &catalog));
        assert!(!same_catalog_path(None, &catalog));
        assert!(!same_catalog_path(
            directory.join("missing.json").to_str(),
            &catalog
        ));
        std::fs::remove_file(other).unwrap();
        std::fs::remove_file(catalog).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn checkpoint_seed_requires_exact_completed_assistant_shape() {
        let marker = format!("CXWEB_NATIVE_CHECKPOINT_{}", "a".repeat(32));
        let item = json!({"method":"item/completed","params":{"threadId":"thread","turnId":"seed","item":{"type":"agentMessage","text":marker}}});
        let done = json!({"method":"turn/completed","params":{"threadId":"thread","turn":{"id":"seed","status":"completed"}}});
        assert_eq!(
            checkpoint_seed(std::slice::from_ref(&item), "thread", "seed"),
            Ok(None)
        );
        assert_eq!(
            checkpoint_seed(&[item.clone(), done.clone()], "thread", "seed"),
            Ok(Some(marker.clone()))
        );
        assert!(
            checkpoint_seed(
                &[item.clone(), item.clone(), done.clone()],
                "thread",
                "seed"
            )
            .is_err()
        );
        for text in [
            String::new(),
            format!("{marker}\n"),
            marker.to_uppercase(),
            marker[..marker.len() - 1].into(),
        ] {
            let mut changed = item.clone();
            changed["params"]["item"]["text"] = json!(text);
            assert!(checkpoint_seed(&[changed, done.clone()], "thread", "seed").is_err());
        }
        assert!(
            checkpoint_seed(&[item, done], "other", "seed")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn checkpoint_completion_requires_one_attributed_item_and_successful_turn() {
        let item = json!({"method":"item/completed","params":{"threadId":"thread","turnId":"compact","item":{"type":"contextCompaction"}}});
        let done = json!({"method":"turn/completed","params":{"threadId":"thread","turn":{"id":"compact","status":"completed"}}});
        assert_eq!(
            checkpoint_complete(std::slice::from_ref(&item), "thread", "compact"),
            Ok(false)
        );
        assert_eq!(
            checkpoint_complete(&[item.clone(), done.clone()], "thread", "compact"),
            Ok(true)
        );
        assert_eq!(
            checkpoint_complete(
                &[item.clone(), item.clone(), done.clone()],
                "thread",
                "compact"
            ),
            Err("E_NATIVE_PROBE_COMPACTION")
        );
        for (pointer, value) in [
            ("/params/threadId", json!("other")),
            ("/params/turnId", json!("other")),
            ("/params/item/type", json!("agentMessage")),
        ] {
            let mut changed = item.clone();
            *changed.pointer_mut(pointer).unwrap() = value;
            assert_eq!(
                checkpoint_complete(&[changed, done.clone()], "thread", "compact"),
                Err("E_NATIVE_PROBE_COMPACTION")
            );
        }
        let mut failed = done;
        failed["params"]["turn"]["status"] = json!("failed");
        assert_eq!(
            checkpoint_complete(&[item, failed], "thread", "compact"),
            Err("E_NATIVE_PROBE_COMPACTION")
        );
    }
    use cxweb_codex_adapter::catalog_codec::CatalogRoute;

    #[test]
    fn endpoint_requires_owned_loopback_and_exact_route_and_codec() {
        let valid = json!({"base_url":"http://127.0.0.1:43127/wb/fixture/backend-api/codex","model":"webbridge/test","catalog_codec":CatalogCodec::CliModelInfoV1.id(),"catalog":{"models":[{"slug":"webbridge/test"}]}});
        assert!(
            Endpoint::parse(
                valid.clone(),
                "webbridge/test",
                CatalogCodec::CliModelInfoV1
            )
            .is_ok()
        );
        for base in [
            "https://api.openai.com/v1",
            "http://localhost:43127/wb/fixture/backend-api/codex",
            "http://127.0.0.1:43127/wb/fixture/backend-api/codex?secret=fixture",
            "http://fixture@127.0.0.1:43127/wb/fixture/backend-api/codex",
        ] {
            let mut value = valid.clone();
            value["base_url"] = json!(base);
            assert!(matches!(
                Endpoint::parse(value, "webbridge/test", CatalogCodec::CliModelInfoV1),
                Err("E_NATIVE_PROBE_ENDPOINT")
            ));
        }
        assert!(
            Endpoint::parse(
                valid.clone(),
                "webbridge/other",
                CatalogCodec::CliModelInfoV1
            )
            .is_err()
        );
        assert!(Endpoint::parse(valid, "webbridge/test", CatalogCodec::AppModelInfoV1).is_err());
    }

    #[test]
    fn exact_text_requires_one_completed_message_from_the_requested_turn() {
        let done = json!({"method":"turn/completed","params":{"threadId":"thread","turn":{"id":"turn","status":"completed"}}});
        let message = json!({"method":"item/completed","params":{"threadId":"thread","turnId":"turn","item":{"type":"agentMessage","text":"expected"}}});
        assert_eq!(
            text_complete(std::slice::from_ref(&message), "thread", "turn", "expected"),
            Ok(false)
        );
        assert_eq!(
            text_complete(
                &[message.clone(), done.clone()],
                "thread",
                "turn",
                "expected"
            ),
            Ok(true)
        );
        assert_eq!(
            text_complete(
                &[message.clone(), done.clone()],
                "thread",
                "turn",
                "another"
            ),
            Err("E_NATIVE_PROBE_TEXT")
        );
        assert_eq!(
            text_complete(
                &[message.clone(), message.clone(), done.clone()],
                "thread",
                "turn",
                "expected"
            ),
            Err("E_NATIVE_PROBE_TEXT")
        );
        let mut other = message;
        other["params"]["turnId"] = json!("other");
        assert_eq!(
            text_complete(&[other, done.clone()], "thread", "turn", "expected"),
            Err("E_NATIVE_PROBE_TEXT")
        );
        let mut failed = done;
        failed["params"]["turn"]["status"] = json!("failed");
        failed["params"]["turn"]["error"] = json!("PRIVATE_NATIVE_ERROR");
        assert_eq!(
            text_complete(&[failed], "thread", "turn", "expected"),
            Err("E_NATIVE_PROBE_GENERATION")
        );
    }

    #[test]
    fn denial_requires_attributed_declined_command_and_untouched_fixture() {
        let directory = std::env::temp_dir().join(format!(
            "cxweb-denial-evidence-{:032x}",
            rand::random::<u128>()
        ));
        protected_directory(&directory).unwrap();
        let mut fixture =
            crate::native_fixture::Fixture::new(directory.clone(), "private-marker".into());
        fixture.denial = true;
        let input = directory.join("probe-input.txt");
        std::fs::write(&input, "private-marker\n").unwrap();
        let event = json!({"method":"item/completed","params":{"threadId":"thread","turnId":"turn","item":{"type":"commandExecution","status":"declined","command":crate::native_fixture::READ,"cwd":directory,"aggregatedOutput":"Command rejected by user"}}});
        let check = |events: &[Value]| verify_denial(&fixture, events, "thread", "turn");
        assert_eq!(check(std::slice::from_ref(&event)), Ok(()));
        assert_eq!(check(&[]), Err("E_NATIVE_PROBE_DENIAL"));
        assert_eq!(
            check(&[event.clone(), event.clone()]),
            Err("E_NATIVE_PROBE_DENIAL")
        );
        for (pointer, value) in [
            ("/params/threadId", json!("other")),
            ("/params/turnId", json!("other")),
            ("/params/item/status", json!("completed")),
            ("/params/item/status", json!("failed")),
            ("/params/item/command", json!("Get-ChildItem")),
            ("/params/item/cwd", json!("C:/other")),
            ("/params/item/aggregatedOutput", json!("private-marker")),
        ] {
            let mut changed = event.clone();
            *changed.pointer_mut(pointer).unwrap() = value;
            assert_eq!(check(&[changed]), Err("E_NATIVE_PROBE_DENIAL"));
        }
        let mut patch = event.clone();
        patch["params"]["item"]["type"] = json!("fileChange");
        assert_eq!(check(&[event.clone(), patch]), Err("E_NATIVE_PROBE_DENIAL"));
        std::fs::write(&input, "changed").unwrap();
        assert_eq!(
            check(std::slice::from_ref(&event)),
            Err("E_NATIVE_PROBE_DENIAL")
        );
        std::fs::write(&input, "private-marker\n").unwrap();
        std::fs::write(directory.join("probe-output.txt"), "unexpected").unwrap();
        assert_eq!(check(&[event]), Err("E_NATIVE_PROBE_DENIAL"));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn test_success_requires_attributed_exit_code_output_command_and_real_files() {
        let directory = std::env::temp_dir().join(format!(
            "cxweb-test-evidence-{:032x}",
            rand::random::<u128>()
        ));
        protected_directory(&directory).unwrap();
        let mut fixture =
            crate::native_fixture::Fixture::new(directory.clone(), "private-marker".into());
        fixture.test = true;
        for name in ["probe-input.txt", "probe-output.txt"] {
            std::fs::write(directory.join(name), "private-marker\n").unwrap();
        }
        let event = |item| json!({"method":"item/completed","params":{"threadId":"thread","turnId":"turn","item":item}});
        let read = event(
            json!({"type":"commandExecution","status":"completed","exitCode":0,"command":crate::native_fixture::READ,"cwd":directory,"aggregatedOutput":"private-marker\n"}),
        );
        let patch = event(
            json!({"type":"fileChange","status":"completed","changes":[{"kind":{"type":"add"},"path":"probe-output.txt","diff":"private-marker\n"}]}),
        );
        let test = event(
            json!({"type":"commandExecution","status":"completed","exitCode":0,"command":crate::native_fixture::TEST,"cwd":directory,"aggregatedOutput":crate::native_fixture::TEST_PASSED}),
        );
        assert_eq!(
            verify_post_checkpoint_tools(
                &fixture,
                &[patch.clone(), test.clone()],
                "thread",
                "turn"
            ),
            Ok(())
        );
        for invalid in [
            vec![patch.clone()],
            vec![test.clone(), patch.clone()],
            vec![patch.clone(), test.clone(), test.clone()],
            vec![read.clone(), patch.clone(), test.clone()],
        ] {
            assert!(verify_post_checkpoint_tools(&fixture, &invalid, "thread", "turn").is_err());
        }
        assert_eq!(
            verify_tools(
                &fixture,
                &[read.clone(), patch.clone(), test.clone()],
                "thread",
                "turn"
            ),
            Ok(())
        );
        for (pointer, value) in [
            ("/params/threadId", json!("other")),
            ("/params/turnId", json!("other")),
            ("/params/item/exitCode", json!(1)),
            ("/params/item/exitCode", Value::Null),
            ("/params/item/status", json!("inProgress")),
            (
                "/params/item/command",
                json!("Write-Output 'cxweb fixture tests passed'"),
            ),
            ("/params/item/cwd", json!(r"C:\other")),
            (
                "/params/item/aggregatedOutput",
                json!("cxweb fixture tests failed"),
            ),
        ] {
            let mut changed = test.clone();
            *changed.pointer_mut(pointer).unwrap() = value;
            assert!(
                verify_post_checkpoint_tools(
                    &fixture,
                    &[patch.clone(), changed.clone()],
                    "thread",
                    "turn"
                )
                .is_err()
            );
            assert!(
                verify_tools(
                    &fixture,
                    &[read.clone(), patch.clone(), changed],
                    "thread",
                    "turn"
                )
                .is_err()
            );
        }
        assert!(verify_tools(&fixture, &[read.clone(), patch.clone()], "thread", "turn").is_err());
        assert!(
            verify_tools(
                &fixture,
                &[read.clone(), test.clone(), patch.clone()],
                "thread",
                "turn"
            )
            .is_err()
        );
        assert!(
            verify_tools(
                &fixture,
                &[read.clone(), patch.clone(), test.clone(), test.clone()],
                "thread",
                "turn"
            )
            .is_err()
        );
        std::fs::write(directory.join("probe-output.txt"), "wrong output\n").unwrap();
        assert!(
            verify_post_checkpoint_tools(
                &fixture,
                &[patch.clone(), test.clone()],
                "thread",
                "turn"
            )
            .is_err()
        );
        assert!(verify_tools(&fixture, &[read, patch, test], "thread", "turn").is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn repair_requires_a_real_failure_before_the_patch_and_success_afterward() {
        let directory = std::env::temp_dir().join(format!(
            "cxweb-repair-evidence-{:032x}",
            rand::random::<u128>()
        ));
        protected_directory(&directory).unwrap();
        let mut fixture =
            crate::native_fixture::Fixture::new(directory.clone(), "private-marker".into());
        fixture.test = true;
        fixture.repair = true;
        for name in ["probe-input.txt", "probe-output.txt"] {
            std::fs::write(directory.join(name), "private-marker\n").unwrap();
        }
        let event = |item| json!({"method":"item/completed","params":{"threadId":"thread","turnId":"turn","item":item}});
        let read = event(
            json!({"type":"commandExecution","status":"completed","exitCode":0,"command":crate::native_fixture::READ,"cwd":directory,"aggregatedOutput":"private-marker\n"}),
        );
        let failed = event(
            json!({"type":"commandExecution","status":"failed","exitCode":1,"command":crate::native_fixture::TEST,"cwd":directory,"aggregatedOutput":crate::native_fixture::TEST_FAILED}),
        );
        let patch = event(
            json!({"type":"fileChange","status":"completed","changes":[{"kind":{"type":"update","movePath":null},"path":"probe-output.txt","diff":format!("@@ -1 +1 @@\n-{}\n+private-marker\n", crate::native_fixture::BROKEN_OUTPUT)}]}),
        );
        let passed = event(
            json!({"type":"commandExecution","status":"completed","exitCode":0,"command":crate::native_fixture::TEST,"cwd":directory,"aggregatedOutput":crate::native_fixture::TEST_PASSED}),
        );
        let valid = vec![read.clone(), failed.clone(), patch.clone(), passed.clone()];
        let check = |events: &[Value]| verify_tools(&fixture, events, "thread", "turn");
        assert_eq!(check(&valid), Ok(()));
        for events in [
            vec![read.clone(), patch.clone(), passed.clone()],
            vec![read.clone(), patch.clone(), failed.clone(), passed.clone()],
            vec![read.clone(), passed.clone(), patch.clone(), passed.clone()],
            vec![read.clone(), failed.clone(), patch.clone(), failed.clone()],
        ] {
            assert!(check(&events).is_err());
        }
        for (index, pointer, value) in [
            (1, "/params/threadId", json!("other")),
            (1, "/params/turnId", json!("other")),
            (1, "/params/item/exitCode", json!(0)),
            (
                1,
                "/params/item/aggregatedOutput",
                json!("unrelated failure"),
            ),
            (1, "/params/item/command", json!("exit 1")),
            (2, "/params/item/changes/0/path", json!("probe-input.txt")),
            (
                2,
                "/params/item/changes/0/kind/movePath",
                json!("other.txt"),
            ),
            (2, "/params/item/changes/0/diff", json!("unrelated edit")),
        ] {
            let mut changed = valid.clone();
            *changed[index].pointer_mut(pointer).unwrap() = value;
            assert!(check(&changed).is_err());
        }
        std::fs::write(directory.join("probe-output.txt"), "still incorrect\n").unwrap();
        assert!(check(&valid).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    #[ignore = "requires CXWEB_NATIVE_PROBE_BACKEND; executes exact local read/patch/test/repair and denial fixtures through a reviewed native backend"]
    async fn actual_native_backend_executes_guarded_tools_and_denial_fixtures() {
        use axum::{
            body::{Body, to_bytes},
            http::{Method, StatusCode},
            response::Response,
        };
        use cxweb_codex_adapter::{
            envelope::{ToolKind, ValidatedCall, ValidatedOutput},
            wire,
        };
        use std::sync::atomic::{AtomicUsize, Ordering};
        let executable = std::path::PathBuf::from(
            std::env::var_os("CXWEB_NATIVE_PROBE_BACKEND").expect("select a reviewed backend"),
        );
        let target = TargetPathGuard::capture(&executable, false).unwrap();
        let hash = native_preflight::fingerprint(&executable).await.unwrap();
        let (_, codec) = native_preflight::describe(&executable).await.unwrap();
        for (denial, test, repair, corrupt_output) in [
            (false, false, false, false),
            (true, false, false, false),
            (false, true, false, false),
            (false, true, false, true),
            (false, true, true, false),
            (false, true, true, true),
        ] {
            let directory = std::env::temp_dir().join(format!(
                "cxweb-native-tools-{:032x}",
                rand::random::<u128>()
            ));
            protected_directory(&directory).unwrap();
            let mut fixture = crate::native_fixture::Fixture::new(
                directory.join("workspace"),
                format!("cxweb fixture {:032x}", rand::random::<u128>()),
            );
            fixture.denial = denial;
            fixture.test = test;
            fixture.repair = repair;
            let fixture = Arc::new(fixture);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let state = crate::ProbeState::new(listener.local_addr().unwrap().port());
            let endpoint = Endpoint::parse(json!({"base_url":state.base_url(),"model":"webbridge/diagnostic","catalog_codec":codec.id(),"catalog":{"models":[codec.encode(&CatalogRoute {id:"webbridge/diagnostic".into(),observed_label:"Synthetic read and patch".into(),effort:"medium".into(),reasoning: vec![], coding:true}).unwrap()]}}), "webbridge/diagnostic", codec).unwrap();
            let count = Arc::new(AtomicUsize::new(0));
            let route = state.base_url().split("/wb/").nth(1).unwrap().to_owned();
            let router = crate::diagnostic_router(state).layer(axum::middleware::from_fn({
            let fixture = fixture.clone();
            let count = count.clone();
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let fixture = fixture.clone();
                let count = count.clone();
                let route = route.clone();
                async move {
                    if request.method() != Method::POST { return next.run(request).await; }
                    assert_eq!(request.uri().path(), format!("/wb/{route}/responses"));
                    for header in ["authorization", "chatgpt-account-id", "cookie"] { assert!(!request.headers().contains_key(header)); }
                    let body = to_bytes(request.into_body(), 8 * 1024 * 1024).await.unwrap();
                    // Exercise the production decoder on each actual native
                    // request, including ordered function/custom result history.
                    cxweb_codex_adapter::request::CanonicalRequest::decode(&body).unwrap();
                    let payload = strict_json::parse(&body, 8 * 1024 * 1024).unwrap();
                    let index = count.fetch_add(1, Ordering::SeqCst);
                    let name = if index == 0 || (fixture.repair && matches!(index, 1 | 3)) || (!fixture.repair && index == 2 && fixture.test) { "exec_command" } else { "apply_patch" };
                    let namespace = payload["tools"].as_array().unwrap().iter().find_map(|tool| {
                        (tool["type"] == "namespace" && tool["tools"].as_array().is_some_and(|children|
                            children.iter().any(|child| child["name"] == name)))
                            .then(|| tool["name"].as_str().unwrap().to_owned())
                    });
                    let output = match index {
                        0 => ValidatedOutput::Calls(vec![ValidatedCall {
                            native_name: name.into(), namespace, kind: ToolKind::Function,
                            input: json!({"cmd":crate::native_fixture::READ,"login":false,"max_output_tokens":1024}),
                        }]),
                        1 if fixture.repair => {
                            assert!(payload["input"].as_array().unwrap().iter().any(|item|
                                item["type"] == "function_call_output" && item["output"].to_string().contains(&fixture.marker)));
                            ValidatedOutput::Calls(vec![ValidatedCall {native_name:name.into(), namespace,
                                kind:ToolKind::Function, input:json!({"cmd":crate::native_fixture::TEST,"login":false,"max_output_tokens":1024})}])
                        }
                        2 if fixture.repair => {
                            assert!(payload["input"].as_array().unwrap().iter().any(|item|
                                item["type"] == "function_call_output" && item["output"].to_string().contains(crate::native_fixture::TEST_FAILED)));
                            assert_eq!(std::fs::read_to_string(fixture.cwd.join("probe-output.txt")).unwrap(), format!("{}\n", crate::native_fixture::BROKEN_OUTPUT));
                            ValidatedOutput::Calls(vec![ValidatedCall {native_name:name.into(), namespace,
                                kind:ToolKind::Custom, input:json!(fixture.patch())}])
                        }
                        3 if fixture.repair => {
                            assert!(payload["input"].as_array().unwrap().iter().any(|item| item["type"] == "custom_tool_call_output"));
                            if corrupt_output {
                                std::fs::write(fixture.cwd.join("probe-output.txt"), format!("{}\n", crate::native_fixture::BROKEN_OUTPUT)).unwrap();
                            }
                            ValidatedOutput::Calls(vec![ValidatedCall {native_name:name.into(), namespace,
                                kind:ToolKind::Function, input:json!({"cmd":crate::native_fixture::TEST,"login":false,"max_output_tokens":1024})}])
                        }
                        4 if fixture.repair => {
                            let outputs: Vec<_> = payload["input"].as_array().unwrap().iter()
                                .filter(|item| item["type"] == "function_call_output").collect();
                            let expected_output = if corrupt_output { crate::native_fixture::TEST_FAILED } else { crate::native_fixture::TEST_PASSED };
                            assert!(outputs.last().unwrap()["output"].to_string().contains(expected_output));
                            ValidatedOutput::Final(fixture.expected().into())
                        }
                        1 if fixture.denial => {
                            let outputs: Vec<_> = payload["input"].as_array().unwrap().iter()
                                .filter(|item| item["type"] == "function_call_output").collect();
                            assert_eq!(outputs.len(), 1);
                            let result = outputs[0]["output"].to_string().to_ascii_lowercase();
                            assert!(!result.contains(&fixture.marker.to_ascii_lowercase()));
                            assert!(result.contains("reject") || result.contains("denied") || result.contains("declin"), "expected native rejection result: {result}");
                            ValidatedOutput::Final(fixture.expected().to_owned())
                        }
                        1 => {
                            assert!(payload["input"].as_array().unwrap().iter().any(|item|
                                item["type"] == "function_call_output" && item["output"].to_string().contains(&fixture.marker)));
                            ValidatedOutput::Calls(vec![ValidatedCall {native_name:name.into(), namespace,
                                kind:ToolKind::Custom, input:json!(fixture.patch())}])
                        }
                        2 if fixture.test => {
                            assert!(payload["input"].as_array().unwrap().iter().any(|item| item["type"] == "custom_tool_call_output"));
                            if corrupt_output {
                                // Fault injection stays inside this disposable workspace.
                                // The genuine test must now fail; a fabricated successful
                                // final answer below must not pass client verification.
                                std::fs::write(fixture.cwd.join("probe-output.txt"), "incorrect fixture output\n").unwrap();
                            }
                            ValidatedOutput::Calls(vec![ValidatedCall {native_name:name.into(), namespace,
                                kind:ToolKind::Function, input:json!({"cmd":crate::native_fixture::TEST,"login":false,"max_output_tokens":1024})}])
                        }
                        2 => {
                            assert!(payload["input"].as_array().unwrap().iter().any(|item| item["type"] == "custom_tool_call_output"));
                            ValidatedOutput::Final(fixture.marker.clone())
                        }
                        3 if fixture.test => {
                            let expected_output = if corrupt_output { "cxweb fixture tests failed" } else { crate::native_fixture::TEST_PASSED };
                            assert!(payload["input"].as_array().unwrap().iter().any(|item| item["type"] == "function_call_output" && item["output"].to_string().contains(expected_output)));
                            ValidatedOutput::Final(fixture.expected().into())
                        }
                        _ => return Response::builder().status(StatusCode::BAD_REQUEST).body(Body::empty()).unwrap(),
                    };
                    let encoded = wire::encode(&output, "webbridge/diagnostic", &format!("response_fixture_{index}"), 1).unwrap();
                    fixture.check_delivery(&encoded.response.to_string()).unwrap();
                    Response::builder().header("content-type", "text/event-stream").body(Body::from(encoded.sse())).unwrap()
                }
            }
        }));
            let server = tokio::spawn(axum::serve(listener, router).into_future());
            let result = run_client(
                &executable,
                &target,
                &directory,
                &endpoint,
                fixture.expected(),
                &CancellationToken::new(),
                Some(fixture.clone()),
            )
            .await;
            server.abort();
            let _ = server.await;
            let expected = if corrupt_output {
                Err("E_NATIVE_PROBE_TEST")
            } else {
                Ok(())
            };
            assert_eq!(
                result, expected,
                "denial={denial}, test={test}, repair={repair}, corrupt_output={corrupt_output}"
            );
            assert_eq!(
                count.load(Ordering::SeqCst),
                if denial {
                    2
                } else if repair {
                    5
                } else if test {
                    4
                } else {
                    3
                }
            );
            assert_eq!(
                native_preflight::fingerprint(&executable).await.unwrap(),
                hash
            );
            std::fs::remove_dir_all(directory).unwrap();
        }
    }

    #[tokio::test]
    #[ignore = "requires CXWEB_NATIVE_PROBE_BACKEND; runs the reviewed backend against a synthetic local server, no account or browser"]
    async fn actual_native_backend_text_round_trip_uses_isolated_home() {
        let executable = std::path::PathBuf::from(
            std::env::var_os("CXWEB_NATIVE_PROBE_BACKEND").expect("select a reviewed backend"),
        );
        let target = TargetPathGuard::capture(&executable, false).unwrap();
        let hash = native_preflight::fingerprint(&executable).await.unwrap();
        let (_, codec) = native_preflight::describe(&executable)
            .await
            .expect("compatible backend");
        let directory =
            std::env::temp_dir().join(format!("cxweb-native-text-{:032x}", rand::random::<u128>()));
        protected_directory(&directory).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let state = crate::ProbeState::new(listener.local_addr().unwrap().port());
        let reasoning = [
            ("low", "Instant"),
            ("medium", "Medium"),
            ("high", "High"),
            ("xhigh", "Extra High"),
            ("max", "6 PRO"),
        ]
        .into_iter()
        .map(
            |(effort, description)| cxweb_codex_adapter::catalog_codec::ReasoningLevel {
                effort: effort.into(),
                description: description.into(),
            },
        )
        .collect();
        let endpoint = Endpoint::parse(json!({"base_url":state.base_url(),"model":"webbridge/diagnostic","catalog_codec":codec.id(),"catalog":{"models":[codec.encode(&CatalogRoute {id:"webbridge/diagnostic".into(),observed_label:"Synthetic text".into(),effort:"medium".into(),reasoning, coding:false}).unwrap()]}}), "webbridge/diagnostic", codec).unwrap();
        let hold = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = CancellationToken::new();
        let router = crate::diagnostic_router(state).layer(axum::middleware::from_fn({
            let hold = hold.clone();
            let entered = entered.clone();
            let release = release.clone();
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let hold = hold.clone();
                let entered = entered.clone();
                let release = release.clone();
                async move {
                    if request.method() == axum::http::Method::POST
                        && request.uri().path().ends_with("/responses")
                    {
                        assert!(
                            ["authorization", "chatgpt-account-id", "cookie"]
                                .iter()
                                .all(|name| request.headers().get(*name).is_none()),
                            "isolated signed-out client must not send account credentials"
                        );
                        if hold.load(std::sync::atomic::Ordering::SeqCst) {
                            entered.notify_one();
                            release.cancelled().await;
                        }
                    }
                    next.run(request).await
                }
            }
        }));
        let server = tokio::spawn(axum::serve(listener, router).into_future());
        let result = tokio::time::timeout(
            Duration::from_secs(60),
            run_text(
                &executable,
                &target,
                &directory,
                &endpoint,
                "cxweb diagnostic round-trip succeeded",
                &CancellationToken::new(),
            ),
        )
        .await;
        assert!(result.is_ok(), "native text probe timed out");
        assert_eq!(result.unwrap(), Ok(()));
        hold.store(true, std::sync::atomic::Ordering::SeqCst);
        let cancelled_directory = directory.join("cancelled");
        protected_directory(&cancelled_directory).unwrap();
        let cancellation = CancellationToken::new();
        let run = run_text(
            &executable,
            &target,
            &cancelled_directory,
            &endpoint,
            "synthetic",
            &cancellation,
        );
        let cancel = async {
            tokio::time::timeout(Duration::from_secs(45), entered.notified())
                .await
                .unwrap();
            cancellation.cancel();
        };
        let (result, ()) = tokio::join!(run, cancel);
        release.cancel();
        server.abort();
        let _ = server.await;
        assert_eq!(result, Err("E_NATIVE_PROBE_CANCELLED"));
        assert_eq!(
            native_preflight::fingerprint(&executable).await.unwrap(),
            hash
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}
