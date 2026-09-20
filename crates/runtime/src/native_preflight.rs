//! Read native configuration through a reviewed backend's own status APIs.
//! No model requests, login, configuration writes or credential-file reads.
use cxweb_codex_adapter::{
    catalog_codec::CatalogCodec,
    preflight::{Assessment, assess},
    strict_json,
};
use cxweb_platform::{atomic_file::Snapshot, target_path::TargetPathGuard};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    process::Command,
};

const MAX_LINE: usize = 8 * 1024 * 1024;

#[derive(Serialize)]
pub struct Report {
    /// Native catalog IDs, read through the client's public model/list API.
    pub native_models: Vec<String>,
    pub client_build: &'static str,
    pub catalog_codec: &'static str,
    pub executable_sha256: String,
    pub assessment: Assessment,
    pub user_config_unchanged: bool,
    pub selected_config_and_parent_access_verified: bool,
    pub selected_target_access_verified: bool,
    pub target_path_identity_verified: bool,
    pub executable_unchanged: bool,
    pub model_requests: u32,
    pub activation_eligible: bool,
    pub remaining_checks: Vec<&'static str>,
}

pub(crate) fn reviewed(hash: &str) -> Option<(&'static str, CatalogCodec)> {
    match hash {
        "eba0f32c976667cb9298efafd98513e823eeda7b576a03ec658bb8be8d336316" => {
            Some(("0.155.1", CatalogCodec::Cli01551))
        }
        "bc45017e8239dc150258f69309ced9df6bbcdf5b8e4f346decf780ac0999e226" => {
            Some(("0.155.0-alpha.9.2", CatalogCodec::App01550Alpha92))
        }
        _ => None,
    }
}

fn config_capture_error(error: std::io::Error) -> &'static str {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        "E_PREFLIGHT_CONFIG_PERMISSIONS"
    } else {
        "E_PREFLIGHT_CONFIG_IDENTITY"
    }
}

pub(crate) async fn fingerprint(path: &Path) -> Result<String, &'static str> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|_| "E_PREFLIGHT_EXECUTABLE")?;
    if file
        .metadata()
        .await
        .map_err(|_| "E_PREFLIGHT_EXECUTABLE")?
        .len()
        > 512 * 1024 * 1024
    {
        return Err("E_PREFLIGHT_EXECUTABLE");
    }
    let mut hash = Sha256::new();
    // This future is also created from Tauri's Windows event callback. Keeping
    // a 64 KiB array inside it multiplies stack copies through command dispatch
    // and can exhaust the smaller GUI callback stack before polling begins.
    let mut buffer = vec![0u8; 64 * 1024];
    let mut total = 0usize;
    loop {
        let count = file
            .read(&mut buffer)
            .await
            .map_err(|_| "E_PREFLIGHT_EXECUTABLE")?;
        if count == 0 {
            break;
        }
        total += count;
        if total > 512 * 1024 * 1024 {
            return Err("E_PREFLIGHT_EXECUTABLE");
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub(crate) async fn line(
    reader: &mut (impl AsyncBufRead + Unpin),
) -> Result<Vec<u8>, &'static str> {
    let mut line = Vec::new();
    loop {
        let buffer = reader.fill_buf().await.map_err(|_| "E_PREFLIGHT_RPC_IO")?;
        if buffer.is_empty() {
            return Err("E_PREFLIGHT_RPC_CLOSED");
        }
        let end = buffer.iter().position(|b| *b == b'\n');
        let count = end.map_or(buffer.len(), |i| i + 1);
        if line.len() + count > MAX_LINE {
            return Err("E_PREFLIGHT_RPC_LIMIT");
        }
        line.extend_from_slice(&buffer[..count]);
        reader.consume(count);
        if end.is_some() {
            return Ok(line);
        }
    }
}

async fn send(input: &mut (impl AsyncWrite + Unpin), message: Value) -> Result<(), &'static str> {
    let mut bytes = serde_json::to_vec(&message).map_err(|_| "E_PREFLIGHT_RPC_SCHEMA")?;
    bytes.push(b'\n');
    input
        .write_all(&bytes)
        .await
        .map_err(|_| "E_PREFLIGHT_RPC_IO")
}
async fn rpc(
    input: &mut (impl AsyncWrite + Unpin),
    output: &mut (impl AsyncBufRead + Unpin),
    id: u64,
    method: &str,
    params: Value,
) -> Result<Value, &'static str> {
    send(input, json!({"id":id,"method":method,"params":params})).await?;
    for _ in 0..128 {
        let message = strict_json::parse(&line(output).await?, MAX_LINE)
            .map_err(|_| "E_PREFLIGHT_RPC_SCHEMA")?;
        if message.get("method").is_some() {
            if let Some(id) = message.get("id") {
                // No server-originated action is authorized by an inspection.
                send(
                    input,
                    json!({"id":id,"error":{"code":-32601,"message":"Preflight is read-only"}}),
                )
                .await?;
            }
            continue;
        }
        if message["id"] != id || message.get("error").is_some() {
            return Err("E_PREFLIGHT_RPC_REJECTED");
        }
        return message
            .get("result")
            .cloned()
            .ok_or("E_PREFLIGHT_RPC_SCHEMA");
    }
    Err("E_PREFLIGHT_RPC_LIMIT")
}

/// Scope is exactly this executable, home, cwd and inherited environment, with
/// no CLI overrides or selected profile. It says nothing about another terminal
/// or an already-running desktop client's environment. The native process may
/// maintain its own logs/cache; cxweb never invokes native mutation methods.
pub async fn inspect(client: &Path, home: &Path, cwd: &Path) -> Result<Report, &'static str> {
    if !client.is_absolute() || !home.is_absolute() || !cwd.is_absolute() {
        return Err("E_PREFLIGHT_TARGET");
    }
    // Preserve original path provenance: canonicalization would hide junctions.
    // Keep all guards alive until the inspected child has stopped.
    let targets = [
        TargetPathGuard::capture(client, false),
        TargetPathGuard::capture(home, true),
        TargetPathGuard::capture(cwd, true),
    ];
    let targets = targets
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "E_PREFLIGHT_TARGET_IDENTITY")?;
    // Qualification is read-only: unsafe/unsupported ancestor grants become a
    // fixed conflict, never an ACL repair or a blanket trust in sandbox groups.
    // The repository working directory is not an installation target.
    // The reviewed executable is hashed while held against write/delete for
    // its entire execution. Its parent ACL need not authorize persistent data
    // storage. The selected native home is the persistent write target.
    let target_access = targets[1..2]
        .iter()
        .map(TargetPathGuard::capture_access)
        .collect::<Result<Vec<_>, _>>();
    let client = client.canonicalize().map_err(|_| "E_PREFLIGHT_TARGET")?;
    let home = home.canonicalize().map_err(|_| "E_PREFLIGHT_TARGET")?;
    let cwd = cwd.canonicalize().map_err(|_| "E_PREFLIGHT_TARGET")?;
    if !home.is_dir() || !cwd.is_dir() {
        return Err("E_PREFLIGHT_TARGET");
    }
    // Unknown binaries, including scripts supplied by a repository, are never
    // executed simply to ask them what version they claim to be.
    let hash = fingerprint(&client).await?;
    let (build, codec) = reviewed(&hash).ok_or("E_PREFLIGHT_CLIENT_UNQUALIFIED")?;
    let original =
        Snapshot::capture_native_config(&home.join("config.toml")).map_err(config_capture_error)?;
    let text = std::str::from_utf8(original.original()).map_err(|_| "E_PREFLIGHT_CONFIG_PARSE")?;
    let file_report =
        cxweb_codex_adapter::config::inspect(text).map_err(|_| "E_PREFLIGHT_CONFIG_PARSE")?;
    let mut command = Command::new(&client);
    command
        .arg("app-server")
        .current_dir(&cwd)
        .env("CODEX_HOME", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    for target in &targets {
        target
            .verify_unchanged()
            .map_err(|_| "E_PREFLIGHT_TARGET_CHANGED")?;
    }
    let mut child = command.spawn().map_err(|_| "E_PREFLIGHT_START")?;
    let mut input = child.stdin.take().ok_or("E_PREFLIGHT_START")?;
    let mut output = BufReader::new(child.stdout.take().ok_or("E_PREFLIGHT_START")?);
    let result = tokio::time::timeout(Duration::from_secs(45), async {
        rpc(&mut input, &mut output, 1, "initialize", json!({"clientInfo":{"name":"cxweb_preflight","version":"0.1.0"},"capabilities":{"experimentalApi":true}})).await?;
        send(&mut input, json!({"method":"initialized","params":{}})).await?;
        let config = rpc(&mut input, &mut output, 2, "config/read", json!({"includeLayers":true,"cwd":cwd})).await?;
        let requirements = rpc(&mut input, &mut output, 3, "configRequirements/read", json!({})).await?;
        let account = rpc(&mut input, &mut output, 4, "account/read", json!({"refreshToken":false})).await?;
        let mut assessment = assess(&config, &requirements, &account)?;
        let layers = config["layers"].as_array().ok_or("E_PREFLIGHT_SCHEMA")?;
        for layer in layers {
            if layer["name"]["type"] == "user" && layer["name"]["profile"].is_null() {
                let file = layer["name"]["file"].as_str().ok_or("E_PREFLIGHT_SCHEMA")?;
                let reported = Snapshot::capture_native_config(Path::new(file)).map_err(config_capture_error)?;
                if reported.path() != original.path() || reported.original() != original.original() {
                    return Err("E_PREFLIGHT_HOME_MISMATCH");
                }
            }
        }
        for conflict in file_report.conflicts {
            if !assessment.conflicts.contains(&conflict) { assessment.conflicts.push(conflict); }
        }
        // Environment routing can be applied after config/read's TOML view.
        if std::env::var_os("OPENAI_BASE_URL").is_some_and(|value| !value.is_empty()) {
            assessment.conflicts.push("environment_route");
        }
        if ["OPENAI_API_KEY", "CODEX_API_KEY"].iter().any(|key| std::env::var_os(key).is_some_and(|value| !value.is_empty())) {
            assessment.conflicts.push("environment_auth");
        }
        assessment.conflicts.sort_unstable();
        assessment.configuration_compatible = assessment.conflicts.is_empty();
        let mut native_models = Vec::new();
        if assessment.configuration_compatible {
            let mut cursor = Value::Null;
            for page in 0..16 {
                let models = rpc(&mut input, &mut output, 5 + page, "model/list", json!({"limit":100,"cursor":cursor})).await?;
                native_models.extend(model_ids(&models)?);
                cursor = models.get("nextCursor").cloned().unwrap_or(Value::Null);
                if cursor.is_null() { break; }
                if !cursor.is_string() || page == 15 { return Err("E_PREFLIGHT_MODELS"); }
            }
            native_models.sort();
            native_models.dedup();
        }
        Ok::<_, &'static str>((assessment, native_models))
    }).await.map_err(|_| "E_PREFLIGHT_TIMEOUT").and_then(|result| result);
    drop(input);
    drop(output);
    // This is only the child we created; never stop existing Codex processes.
    if child
        .try_wait()
        .map_err(|_| "E_PREFLIGHT_CLEANUP")?
        .is_none()
    {
        tokio::time::timeout(Duration::from_secs(5), child.kill())
            .await
            .map_err(|_| "E_PREFLIGHT_CLEANUP")?
            .map_err(|_| "E_PREFLIGHT_CLEANUP")?;
    }
    original
        .verify_unchanged()
        .map_err(|_| "E_PREFLIGHT_CONFIG_CHANGED")?;
    for target in &targets {
        target
            .verify_unchanged()
            .map_err(|_| "E_PREFLIGHT_TARGET_CHANGED")?;
    }
    if fingerprint(&client).await? != hash {
        return Err("E_PREFLIGHT_EXECUTABLE_CHANGED");
    }
    let selected_target_access_verified = target_access.is_ok_and(|access| {
        targets[1..2]
            .iter()
            .zip(&access)
            .all(|(guard, access)| guard.verify_access(access).is_ok())
    });
    let (mut assessment, native_models) = result?;
    if !selected_target_access_verified {
        assessment.configuration_compatible = false;
        assessment.conflicts.push("target_permissions");
        assessment.conflicts.sort_unstable();
    }
    Ok(Report {
        native_models,
        client_build: build,
        catalog_codec: codec.id(),
        executable_sha256: hash,
        assessment,
        user_config_unchanged: true,
        selected_config_and_parent_access_verified: true,
        selected_target_access_verified,
        target_path_identity_verified: true,
        executable_unchanged: true,
        model_requests: 0,
        activation_eligible: false,
        remaining_checks: vec![
            "client target qualification",
            "actual client picker and native coexistence",
            "browser route and coding qualification",
        ],
    })
}

fn model_ids(models: &Value) -> Result<Vec<String>, &'static str> {
    let rows = models["data"]
        .as_array()
        .filter(|rows| rows.len() <= 100)
        .ok_or("E_PREFLIGHT_MODELS")?;
    rows.iter()
        .map(|row| {
            let id = row["id"]
                .as_str()
                .filter(|id| {
                    !id.is_empty()
                        && id.len() <= 256
                        && id.bytes().all(|b| b.is_ascii_graphic())
                        && !id.starts_with(cxweb_domain::OWNED_MODEL_PREFIX)
                })
                .ok_or("E_PREFLIGHT_MODELS")?;
            Ok(id.to_owned())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_ids_are_native_bounded_and_never_arbitrary_payloads() {
        assert_eq!(
            model_ids(&json!({"data":[{"id":"native/model"}]})).unwrap(),
            vec!["native/model"]
        );
        for value in [
            json!({"data":[{"id":"webbridge/owned"}]}),
            json!({"data":[{"id":"bad\nvalue"}]}),
            json!({"data":[{"model":"missing-id"}]}),
            json!({"error":"PRIVATE"}),
        ] {
            assert_eq!(model_ids(&value).unwrap_err(), "E_PREFLIGHT_MODELS");
        }
    }
    #[test]
    fn native_inspection_futures_fit_gui_command_dispatch_stacks() {
        let path = Path::new(r"C:\fixture");
        assert!(std::mem::size_of_val(&fingerprint(path)) < 4096);
        assert!(std::mem::size_of_val(&inspect(path, path, path)) < 16 * 1024);
        assert!(std::mem::size_of_val(&crate::native_discovery::discover()) < 16 * 1024);
    }
    #[tokio::test]
    async fn inspection_denies_server_actions_and_never_echoes_native_errors() {
        for response in [
            json!({"id":1,"result":{"fixture":true}}),
            json!({"id":1,"error":{"message":"PRIVATE_NATIVE_ERROR"}}),
            json!({"id":2,"result":{"fixture":true}}),
        ] {
            let expected_ok = response.get("result").is_some() && response["id"] == 1;
            let frames = format!(
                "{}\n{}\n",
                json!({"id":"server-action","method":"item/commandExecution/requestApproval","params":{"command":"PRIVATE_COMMAND"}}),
                response
            );
            let mut output = BufReader::new(frames.as_bytes());
            let mut input = Vec::new();
            let result = rpc(
                &mut input,
                &mut output,
                1,
                "config/read",
                json!({"includeLayers":true}),
            )
            .await;
            assert_eq!(result.is_ok(), expected_ok);
            let requests: Vec<Value> = std::str::from_utf8(&input)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            assert_eq!(requests.len(), 2);
            assert_eq!(requests[0]["method"], "config/read");
            assert_eq!(requests[1]["error"]["code"], -32601);
            assert!(!std::str::from_utf8(&input).unwrap().contains("PRIVATE"));
            if let Err(error) = result {
                assert_eq!(error, "E_PREFLIGHT_RPC_REJECTED");
            }
        }
    }
    #[tokio::test]
    async fn frames_are_bounded_and_buffered_messages_are_not_lost() {
        let mut reader = BufReader::with_capacity(3, &b"one\ntwo\n"[..]);
        assert_eq!(line(&mut reader).await.unwrap(), b"one\n");
        assert_eq!(line(&mut reader).await.unwrap(), b"two\n");
        assert_eq!(
            line(&mut reader).await.unwrap_err(),
            "E_PREFLIGHT_RPC_CLOSED"
        );
        let large = vec![b'x'; MAX_LINE + 1];
        assert_eq!(
            line(&mut BufReader::new(large.as_slice()))
                .await
                .unwrap_err(),
            "E_PREFLIGHT_RPC_LIMIT"
        );
    }
}
