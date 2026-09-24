//! Fixed live checkpoint diagnostic inside an exclusively leased installed host.
//! Uses synthetic history; it does not claim native-client compaction qualification.
use crate::{
    gateway::{WebRequest, WebTransport},
    managed_driver::{Binding, ManagedDriver},
    web_provider::{CoordinatorProvider, WebIdentity},
};
use axum::http::HeaderMap;
use cxweb_codex_adapter::catalog_codec::CatalogCodec;
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};
use tokio_util::sync::CancellationToken;

#[derive(Default, serde::Serialize)]
struct Report {
    stage: &'static str,
    completed: bool,
    checkpoint_encrypted: bool,
    replay_identical: bool,
    exact_continuation: bool,
    pending_result_continuation: bool,
    cleanup_confirmed: bool,
    native_client_tested: bool,
    output_shape: Option<Value>,
    failure: Option<&'static str>,
    observed_at: Option<String>,
}
impl Report {
    async fn save(&self, directory: &Path) -> Result<(), &'static str> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|_| "E_COMPACTION_REPORT")?;
        tokio::fs::write(directory.join("compaction-probe-report.json"), bytes)
            .await
            .map_err(|_| "E_COMPACTION_REPORT")
    }
}

fn request(
    payload: Value,
    task: &str,
    turn: &str,
    context: &str,
    cancel: &CancellationToken,
) -> Result<WebRequest, &'static str> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "session-id",
        task.parse().map_err(|_| "E_COMPACTION_PROBE")?,
    );
    headers.insert("thread-id", task.parse().map_err(|_| "E_COMPACTION_PROBE")?);
    headers.insert(
        "x-codex-turn-metadata",
        json!({"turn_id":turn,"context_window_id":context})
            .to_string()
            .parse()
            .map_err(|_| "E_COMPACTION_PROBE")?,
    );
    Ok(WebRequest {
        payload,
        identity: WebIdentity::from_headers(&headers),
        compact: false,
        cancellation: cancel.clone(),
        transport: WebTransport::Http,
        progress: None,
    })
}
async fn execute(
    provider: &CoordinatorProvider,
    request: WebRequest,
) -> Result<Vec<u8>, &'static str> {
    let response = provider.execute(request).await?;
    if !response.status().is_success() {
        return Err("E_COMPACTION_RESPONSE");
    }
    Ok(axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .map_err(|_| "E_COMPACTION_RESPONSE")?
        .to_vec())
}
fn checkpoint(bytes: &[u8], marker: &str) -> Result<Value, &'static str> {
    let response: Value = serde_json::from_slice(bytes).map_err(|_| "E_COMPACTION_RESPONSE")?;
    let output = response["output"]
        .as_array()
        .filter(|items| items.len() == 1)
        .ok_or("E_COMPACTION_RESPONSE")?;
    let item = &output[0];
    if item["type"] != "compaction"
        || !item["encrypted_content"]
            .as_str()
            .is_some_and(|s| s.starts_with("wbr1:"))
        || String::from_utf8_lossy(bytes).contains(marker)
    {
        return Err("E_COMPACTION_RESPONSE");
    }
    Ok(item.clone())
}
fn continuation(bytes: &[u8], marker: &str) -> bool {
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .is_some_and(|response| {
            response["output"].as_array().is_some_and(|output| {
                output.len() == 1
                    && output[0]["type"] == "message"
                    && output[0]["role"] == "assistant"
                    && output[0]["content"].as_array().is_some_and(|content| {
                        content.len() == 1
                            && content[0]["type"] == "output_text"
                            && content[0]["text"] == marker
                    })
            })
        })
}

pub(crate) async fn run(
    provider: CoordinatorProvider,
    driver: &ManagedDriver,
    binding: &Binding,
    directory: &Path,
    cancel: CancellationToken,
) -> Result<(), &'static str> {
    let mut report = Report {
        stage: "prepare",
        ..Default::default()
    };
    report.save(directory).await?;
    let result = run_inner(provider, binding, directory, &cancel, &mut report).await;
    if result.is_err() {
        report.output_shape = driver
            .diagnostic()
            .await
            .ok()
            .and_then(|v| v.get("output_shape").cloned());
    }
    let cleanup = driver.verify_idle().await;
    report.cleanup_confirmed = cleanup.is_ok();
    let result = cleanup.and(result);
    report.completed = result.is_ok();
    report.failure = result.as_ref().err().copied();
    report.observed_at = cxweb_platform::clock::utc_timestamp();
    let saved = report.save(directory).await;
    result.and(saved)
}

async fn run_inner(
    provider: CoordinatorProvider,
    binding: &Binding,
    directory: &Path,
    cancel: &CancellationToken,
    report: &mut Report,
) -> Result<(), &'static str> {
    if cancel.is_cancelled() {
        return Err("E_CANCELLED");
    }
    let route = binding
        .routes
        .first()
        .filter(|_| binding.routes.len() == 1)
        .ok_or("E_MODEL_UNAVAILABLE")?;
    let provider = if provider.checkpoints_enabled() {
        provider
    } else {
        let installation = binding.installation.clone();
        let key_path = directory.join("compaction-probe-key.bin");
        let key = tokio::task::spawn_blocking(move || {
            crate::checkpoint::Codec::load_or_create(&key_path, &installation)
        })
        .await
        .map_err(|_| "E_CHECKPOINT_KEY")??;
        // Legacy, unqualified providers are cloned for this isolated probe only.
        provider.with_checkpoints(Arc::new(key), CatalogCodec::CliModelInfoV1)?
    };
    let task = format!("compaction-probe-{:032x}", rand::random::<u128>());
    let marker = format!("CXWEB_CHECKPOINT_{:032x}", rand::random::<u128>());
    let payload = json!({"model":route.id,"reasoning":{"effort":route.effort},"stream":false,"tools":[],"input":[
        {"role":"user","content":"Remember the exact result of the completed fixture read. Preserve it verbatim in the checkpoint for the next question. Never run a tool to recover it."},
        {"type":"function_call","call_id":"fixture-read","name":"fixture_read","arguments":"{}"},
        {"type":"function_call_output","call_id":"fixture-read","output":marker},
        {"type":"custom_tool_call","call_id":"fixture-pending","name":"fixture_pending","input":"Synthetic pending execution; do not run"},
        {"type":"compaction_trigger"}
    ]});
    report.stage = "checkpoint";
    report.save(directory).await?;
    let first = execute(
        &provider,
        request(payload.clone(), &task, "compact", "before", cancel)?,
    )
    .await?;
    let item = checkpoint(&first, &marker)?;
    report.checkpoint_encrypted = true;
    report.stage = "replay";
    report.save(directory).await?;
    let replay = execute(
        &provider,
        request(payload, &task, "compact", "before", cancel)?,
    )
    .await?;
    if replay != first {
        return Err("E_COMPACTION_REPLAY");
    }
    report.replay_identical = true;
    report.stage = "continuation";
    report.save(directory).await?;
    let resumed = json!({"model":route.id,"reasoning":{"effort":route.effort},"stream":false,"tools":[],"input":[item,
        {"type":"custom_tool_call_output","call_id":"fixture-pending","output":"DENIED: synthetic fixture; no execution occurred"},
        {"role":"user","content":"Return exactly the complete line from the earlier fixture read. Use only the checkpoint, without tools or extra text."}
    ]});
    let answer = execute(
        &provider,
        request(resumed, &task, "continue", "after", cancel)?,
    )
    .await?;
    if !continuation(&answer, &marker) {
        return Err("E_COMPACTION_RECALL");
    }
    report.exact_continuation = true;
    report.pending_result_continuation = true;
    report.stage = "completed";
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkpoint_and_continuation_require_exact_safe_shapes() {
        let checkpoint_bytes =
            json!({"output":[{"type":"compaction","encrypted_content":"wbr1:opaque"}]}).to_string();
        assert!(checkpoint(checkpoint_bytes.as_bytes(), "marker").is_ok());
        assert!(checkpoint(checkpoint_bytes.as_bytes(), "opaque").is_err());
        assert!(checkpoint(br#"{"output":[]}"#, "marker").is_err());
        let answer = json!({"output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"marker"}]}]});
        assert!(continuation(answer.to_string().as_bytes(), "marker"));
        assert!(!continuation(answer.to_string().as_bytes(), "other"));
        let mut invalid = answer;
        invalid["output"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type":"function_call"}));
        assert!(!continuation(invalid.to_string().as_bytes(), "marker"));
    }
}
