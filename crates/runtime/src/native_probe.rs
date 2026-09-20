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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
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
        qualify_owned(&executable, session, websocket, false, cancellation).await
    })
    .await
    .map_err(|_| "E_NATIVE_PROBE_WORKER")?
}

/// One fixed read/patch exercise in a disposable workspace. This does not
/// certify the complete coding corpus or publish a production coding route.
pub async fn qualify_tools(
    executable: &Path,
    session: Arc<GenerationSession>,
    cancellation: CancellationToken,
) -> Result<Report, &'static str> {
    let executable = executable.to_owned();
    tokio::spawn(
        async move { qualify_owned(&executable, session, false, true, cancellation).await },
    )
    .await
    .map_err(|_| "E_NATIVE_PROBE_WORKER")?
}

async fn qualify_owned(
    executable: &Path,
    session: Arc<GenerationSession>,
    websocket: bool,
    coding: bool,
    cancellation: CancellationToken,
) -> Result<Report, &'static str> {
    if cancellation.is_cancelled() {
        return Err("E_NATIVE_PROBE_CANCELLED");
    }
    let target =
        TargetPathGuard::capture(executable, false).map_err(|_| "E_NATIVE_PROBE_TARGET")?;
    let hash = native_preflight::fingerprint(executable).await?;
    let (build, codec) =
        native_preflight::reviewed(&hash).ok_or("E_NATIVE_PROBE_CLIENT_UNQUALIFIED")?;
    let paths = cxweb_platform::state::StatePaths::open().map_err(|_| "E_STATE_PERMISSIONS")?;
    let directory = paths
        .state
        .join(format!("native-probe-{:032x}", rand::random::<u128>()));
    protected_directory(&directory).map_err(|_| "E_NATIVE_PROBE_DIRECTORY")?;
    let descriptor = directory.join("runtime/connection.json");
    let stop = CancellationToken::new();
    let _stop_on_drop = stop.clone().drop_guard();
    let expected = format!("cxweb native fixture {:032x}", rand::random::<u128>());
    let fixture = coding.then(|| {
        Arc::new(crate::native_fixture::Fixture::new(
            directory.join("workspace"),
            expected.clone(),
        ))
    });
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
        client_build: build.into(),
        catalog_codec: codec.id().into(),
        executable_sha256: hash,
        exact_text_received: true,
        native_tools_executed: if coding { 2 } else { 0 },
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
    patch_approved: bool,
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
            if scope && let Some(fixture) = &self.fixture {
                match message["method"].as_str() {
                    Some("item/commandExecution/requestApproval") if !self.read_approved => {
                        accepted = fixture.approve_read(params);
                        self.read_approved |= accepted;
                    }
                    Some("item/fileChange/requestApproval") if !self.patch_approved => {
                        if let Some(started) = self.observations.iter().rev().find(|event| {
                            event["method"] == "item/started"
                                && event["params"]["item"]["id"] == params["itemId"]
                        }) {
                            accepted = fixture.approve_patch(params, started);
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
            return if accepted {
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
        ) && !(self.fixture.is_some()
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
            || config["model_catalog_json"].as_str().map(Path::new) != Some(catalog)
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
        if !models["data"]
            .as_array()
            .is_some_and(|models| models.iter().any(|model| model["model"] == endpoint.model))
        {
            return Err("E_NATIVE_PROBE_MODEL");
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
        let prompt = self
            .fixture
            .as_ref()
            .map(|fixture| fixture.prompt())
            .unwrap_or_else(|| {
                format!("Use no tools. Return a final answer with exactly this text: {expected}")
            });
        let turn = self.rpc(5, "turn/start", json!({"threadId":thread,"input":[{"type":"text","text":prompt,"text_elements":[]}]})).await?;
        let turn = turn["turn"]["id"].as_str().ok_or("E_NATIVE_PROBE_RPC")?;
        if self.turn.as_deref().is_some_and(|old| old != turn) {
            return Err("E_NATIVE_PROBE_ACTION");
        }
        self.turn = Some(turn.into());
        loop {
            if text_complete(&self.observations, &thread, turn, expected)? {
                if let Some(fixture) = &self.fixture {
                    verify_tools(fixture, &self.observations, &thread, turn)?;
                }
                return Ok(());
            }
            let message = self.next().await?;
            self.observe(message).await?;
        }
    }
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
        if fixture.cwd != cwd || fixture.marker != expected {
            return Err("E_NATIVE_PROBE_DIRECTORY");
        }
        std::fs::write(cwd.join("probe-input.txt"), format!("{}\n", fixture.marker))
            .map_err(|_| "E_NATIVE_PROBE_DIRECTORY")?;
    }
    let catalog = home.join("catalog.json");
    std::fs::write(&catalog, endpoint.catalog.to_string()).map_err(|_| "E_NATIVE_PROBE_CONFIG")?;
    // JSON quoted strings are valid TOML basic strings for these generated paths.
    let config = format!(
        "openai_base_url = {}\nmodel_catalog_json = {}\n",
        json!(endpoint.base),
        json!(catalog.to_string_lossy().replace('\\', "/"))
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
        patch_approved: false,
    };
    let result = tokio::select! {
        () = cancellation.cancelled() => Err("E_NATIVE_PROBE_CANCELLED"),
        result = tokio::time::timeout(Duration::from_secs(600), client.verify(&cwd, endpoint, &catalog, expected)) => result.map_err(|_| "E_NATIVE_PROBE_TIMEOUT").and_then(|result| result),
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
    if commands.len() != 1 || patches.len() != 1 {
        return Err("E_NATIVE_PROBE_ACTION");
    }
    let command = commands[0];
    let patch = patches[0];
    if command["status"] != "completed"
        || command["exitCode"] != 0
        || !fixture.approve_read(command)
        || !command["aggregatedOutput"]
            .as_str()
            .is_some_and(|s| s.contains(&fixture.marker))
        || patch["status"] != "completed"
        || !fixture.patch_changes(patch)
    {
        return Err("E_NATIVE_PROBE_ACTION");
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use cxweb_codex_adapter::catalog_codec::CatalogRoute;

    #[test]
    fn endpoint_requires_owned_loopback_and_exact_route_and_codec() {
        let valid = json!({"base_url":"http://127.0.0.1:43127/wb/fixture/backend-api/codex","model":"webbridge/test","catalog_codec":CatalogCodec::Cli01551.id(),"catalog":{"models":[{"slug":"webbridge/test"}]}});
        assert!(Endpoint::parse(valid.clone(), "webbridge/test", CatalogCodec::Cli01551).is_ok());
        for base in [
            "https://api.openai.com/v1",
            "http://localhost:43127/wb/fixture/backend-api/codex",
            "http://127.0.0.1:43127/wb/fixture/backend-api/codex?secret=fixture",
            "http://fixture@127.0.0.1:43127/wb/fixture/backend-api/codex",
        ] {
            let mut value = valid.clone();
            value["base_url"] = json!(base);
            assert!(matches!(
                Endpoint::parse(value, "webbridge/test", CatalogCodec::Cli01551),
                Err("E_NATIVE_PROBE_ENDPOINT")
            ));
        }
        assert!(Endpoint::parse(valid.clone(), "webbridge/other", CatalogCodec::Cli01551).is_err());
        assert!(Endpoint::parse(valid, "webbridge/test", CatalogCodec::App01550Alpha92).is_err());
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

    #[tokio::test]
    #[ignore = "requires CXWEB_NATIVE_PROBE_BACKEND; executes only the exact local read/patch fixture through a reviewed native backend"]
    async fn actual_native_backend_executes_guarded_read_and_patch_fixture() {
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
        let (_, codec) = native_preflight::reviewed(&hash).unwrap();
        let directory = std::env::temp_dir().join(format!(
            "cxweb-native-tools-{:032x}",
            rand::random::<u128>()
        ));
        protected_directory(&directory).unwrap();
        let fixture = Arc::new(crate::native_fixture::Fixture::new(
            directory.join("workspace"),
            format!("cxweb fixture {:032x}", rand::random::<u128>()),
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let state = crate::ProbeState::new(listener.local_addr().unwrap().port());
        let endpoint = Endpoint::parse(json!({"base_url":state.base_url(),"model":"webbridge/diagnostic","catalog_codec":codec.id(),"catalog":{"models":[codec.encode(&CatalogRoute {id:"webbridge/diagnostic".into(),observed_label:"Synthetic read and patch".into(),effort:"medium".into(),coding:true}).unwrap()]}}), "webbridge/diagnostic", codec).unwrap();
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
                    let payload = strict_json::parse(&body, 8 * 1024 * 1024).unwrap();
                    let index = count.fetch_add(1, Ordering::SeqCst);
                    let name = if index == 0 { "exec_command" } else { "apply_patch" };
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
                        1 => {
                            assert!(payload["input"].as_array().unwrap().iter().any(|item|
                                item["type"] == "function_call_output" && item["output"].to_string().contains(&fixture.marker)));
                            ValidatedOutput::Calls(vec![ValidatedCall {native_name:name.into(), namespace,
                                kind:ToolKind::Custom, input:json!(fixture.patch())}])
                        }
                        2 => {
                            assert!(payload["input"].as_array().unwrap().iter().any(|item| item["type"] == "custom_tool_call_output"));
                            ValidatedOutput::Final(fixture.marker.clone())
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
            &fixture.marker,
            &CancellationToken::new(),
            Some(fixture.clone()),
        )
        .await;
        server.abort();
        let _ = server.await;
        assert_eq!(result, Ok(()));
        assert_eq!(count.load(Ordering::SeqCst), 3);
        assert_eq!(
            native_preflight::fingerprint(&executable).await.unwrap(),
            hash
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    #[ignore = "requires CXWEB_NATIVE_PROBE_BACKEND; runs the reviewed backend against a synthetic local server, no account or browser"]
    async fn actual_native_backend_text_round_trip_uses_isolated_home() {
        let executable = std::path::PathBuf::from(
            std::env::var_os("CXWEB_NATIVE_PROBE_BACKEND").expect("select a reviewed backend"),
        );
        let target = TargetPathGuard::capture(&executable, false).unwrap();
        let hash = native_preflight::fingerprint(&executable).await.unwrap();
        let (_, codec) = native_preflight::reviewed(&hash).expect("reviewed backend");
        let directory =
            std::env::temp_dir().join(format!("cxweb-native-text-{:032x}", rand::random::<u128>()));
        protected_directory(&directory).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let state = crate::ProbeState::new(listener.local_addr().unwrap().port());
        let endpoint = Endpoint::parse(json!({"base_url":state.base_url(),"model":"webbridge/diagnostic","catalog_codec":codec.id(),"catalog":{"models":[codec.encode(&CatalogRoute {id:"webbridge/diagnostic".into(),observed_label:"Synthetic text".into(),effort:"medium".into(),coding:false}).unwrap()]}}), "webbridge/diagnostic", codec).unwrap();
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
