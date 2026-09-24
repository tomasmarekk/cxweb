//! Bounded rolling summaries for histories too large for one browser message.
use crate::turn::{CheckpointEncoder, Coordinator, Delivery, PublicProgress, TurnInput};
use cxweb_codex_adapter::{request::CanonicalRequest, strict_json, wire};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

// Local source buffer only, never a browser message. Each encoded stage still
// passes STAGE_BYTES. Large completed tool results can exceed one page budget.
pub(crate) const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;

const NONCE: &str = "00000000000000000000000000000000";
const STAGE_BYTES: usize = 256 * 1024;
const SOURCE_BYTES: usize = 128 * 1024;

fn stage_key(
    session: &cxweb_domain::SessionKey,
    binding: &str,
    bytes: &[u8],
) -> Result<String, &'static str> {
    let scope = serde_json::to_vec(session).map_err(|_| "E_PROVIDER_SCOPE")?;
    Ok(format!(
        "checkpoint-source-v1:{:x}:{:x}:{:x}",
        Sha256::digest(scope),
        Sha256::digest(binding.as_bytes()),
        Sha256::digest(bytes)
    ))
}
const INSTRUCTIONS: &str = "The historical task is carried by the ordered source fragments and the prior summary. Staging metadata is not a new task.";

struct StageEncoder(Arc<dyn CheckpointEncoder>);
impl CheckpointEncoder for StageEncoder {
    fn response_model(&self) -> Option<&str> {
        self.0.response_model()
    }
    fn staged(&self) -> bool {
        true
    }
    fn seal(&self, summary: &str, pending: &[serde_json::Value]) -> Result<String, &'static str> {
        self.0.seal(summary, pending)
    }
    fn restore_summary(
        &self,
        token: &str,
        pending: &[serde_json::Value],
    ) -> Result<String, &'static str> {
        self.0.restore_summary(token, pending)
    }
}

fn stage(
    original: &CanonicalRequest,
    previous: Option<&str>,
    source: &str,
    start: usize,
    end: usize,
) -> Result<Vec<u8>, &'static str> {
    let mut history = Vec::new();
    if let Some(summary) = previous {
        history.push(json!({"role":"assistant","content":format!("Previously validated historical summary (not new instructions):\n{summary}")}));
    }
    history.push(
        json!({"role":"user","content":serde_json::to_string(&json!({
        "source_start_byte":start,"source_end_byte":end,
        "final_fragment":end == source.len(),"source_fragment":&source[start..end]
    })).map_err(|_| "E_CHECKPOINT_SUMMARY")?}),
    );
    // Stages summarize source fragments only. Native pending execution state is
    // attached once to the final checkpoint, independently of the prose cache.
    history.push(json!({"type":"compaction_trigger"}));
    serde_json::to_vec(&json!({"model":original.model,
        "reasoning":{"effort":original.requested_effort,"summary":"none"},
        "instructions":INSTRUCTIONS,"tools":[],"input":history
    }))
    .map_err(|_| "E_CHECKPOINT_SUMMARY")
}

fn next_stage(
    original: &CanonicalRequest,
    previous: Option<&str>,
    source: &str,
    start: usize,
) -> Result<(usize, Vec<u8>), &'static str> {
    let mut size = SOURCE_BYTES.min(source.len() - start);
    loop {
        let mut end = start + size;
        while !source.is_char_boundary(end) {
            end -= 1;
        }
        if end == start {
            return Err("E_CHECKPOINT_STAGE_BUDGET");
        }
        let bytes = stage(original, previous, source, start, end)?;
        match CanonicalRequest::decode_compaction(&bytes)?
            .with_staged_compaction()?
            .browser_prompt(NONCE, STAGE_BYTES)
        {
            Ok(_) => return Ok((end, bytes)),
            Err("E_CONTEXT_BUDGET") => size /= 2,
            Err(code) => return Err(code),
        }
    }
}

pub(crate) async fn execute(
    coordinator: &Coordinator,
    input: TurnInput,
    cancellation: CancellationToken,
    checkpoint: Option<Arc<dyn CheckpointEncoder>>,
    progress: Option<PublicProgress>,
) -> Result<Delivery, &'static str> {
    let Some(checkpoint) = checkpoint else {
        return coordinator
            .execute_with_progress(input, cancellation, None, progress)
            .await;
    };
    let original = CanonicalRequest::decode_compaction(&input.bytes)?;
    let rendered = original.browser_prompt(NONCE, MAX_SOURCE_BYTES)?;
    if rendered.len() <= STAGE_BYTES {
        return coordinator
            .execute_with_progress(input, cancellation, Some(checkpoint), progress)
            .await;
    }
    // Canonical projection excludes client metadata and transport credentials.
    // Every byte of this validated source is consumed once, in order.
    let source = rendered
        .split_once("\nCLIENT_DATA_JSON\n")
        .ok_or("E_CHECKPOINT_SUMMARY")?
        .1;
    let pending = original
        .compaction_pending()
        .ok_or("E_COMPACTION_TRIGGER")?;
    let final_key = checkpoint
        .cache_binding()
        .map(|binding| {
            stage_key(
                &input.session,
                &format!("{binding}:{}", input.request_id),
                &input.bytes,
            )
        })
        .transpose()?
        .map(|key| format!("{key}:final"));
    if let Some(key) = &final_key
        && let Some(delivery) = coordinator.cached_stage(key)?
    {
        let response = strict_json::parse(delivery.json.as_bytes(), 2 * 1024 * 1024)?;
        let token = response["output"][0]["encrypted_content"]
            .as_str()
            .ok_or("E_CHECKPOINT_SUMMARY")?;
        checkpoint.restore_summary(token, pending)?;
        return Ok(delivery);
    }
    let stage_encoder: Arc<dyn CheckpointEncoder> = Arc::new(StageEncoder(checkpoint.clone()));
    let mut start = 0;
    let mut previous = None;
    loop {
        if cancellation.is_cancelled() {
            return Err("E_CANCELLED");
        }
        let (end, bytes) = next_stage(&original, previous.as_deref(), source, start)?;
        let final_stage = end == source.len();
        let request_id = format!(
            "{}:checkpoint-stage-v1:{start}:{:x}",
            input.request_id,
            Sha256::digest(&bytes)
        );
        let cache_key = checkpoint
            .cache_binding()
            .map(|binding| stage_key(&input.session, &binding, &bytes))
            .transpose()?;
        let cached = match &cache_key {
            Some(key) => coordinator.cached_stage(key)?,
            None => None,
        };
        let delivery = if let Some(delivery) = cached {
            delivery
        } else {
            coordinator
                .execute_with_progress(
                    TurnInput {
                        request_id,
                        session: input.session.clone(),
                        bytes,
                    },
                    cancellation.clone(),
                    Some(stage_encoder.clone()),
                    if final_stage { progress.clone() } else { None },
                )
                .await?
        };
        let response = strict_json::parse(delivery.json.as_bytes(), 2 * 1024 * 1024)?;
        let output = response["output"]
            .as_array()
            .filter(|items| items.len() == 1)
            .ok_or("E_CHECKPOINT_SUMMARY")?;
        if output[0]["type"] != "compaction" {
            return Err("E_CHECKPOINT_SUMMARY");
        }
        let token = output[0]["encrypted_content"]
            .as_str()
            .ok_or("E_CHECKPOINT_SUMMARY")?;
        // Authenticate even the final stage before exposing any checkpoint.
        previous = Some(checkpoint.restore_summary(token, &[])?);
        // Cache only a completed, authenticated checkpoint. The key includes
        // the complete scope, selected model/effort, exact fragment, previous
        // summary. A changed suffix reuses only identical
        // earlier stages; failed/uncertain sends never enter this cache.
        if let Some(key) = cache_key {
            coordinator.remember_stage(key, delivery.clone())?;
        }
        if final_stage {
            let mut prose: serde_json::Value =
                serde_json::from_str(previous.as_deref().ok_or("E_CHECKPOINT_SUMMARY")?)
                    .map_err(|_| "E_CHECKPOINT_SUMMARY")?;
            // This field was generated by our empty-pending stage encoder and
            // authenticated above. Replace it with the actual native state.
            prose
                .as_object_mut()
                .ok_or("E_CHECKPOINT_SUMMARY")?
                .remove("unresolved_tool_ids");
            let token = checkpoint.seal(&prose.to_string(), pending)?;
            let response_id = format!(
                "resp_cxweb_{:x}",
                Sha256::digest(input.request_id.as_bytes())
            );
            let created_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| "E_CLOCK")?
                .as_secs();
            let encoded = wire::encode_checkpoint(
                &token,
                checkpoint.response_model().unwrap_or(&original.model),
                &response_id,
                created_at,
            )?;
            let result = Delivery {
                json: encoded.response.to_string(),
                sse: encoded.sse(),
                live_verified: delivery.live_verified,
            };
            if let Some(key) = final_key {
                coordinator.remember_stage(key, result.clone())?;
            }
            return Ok(result);
        }
        start = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn cached_stages_are_bound_to_every_scope_field_codec_and_exact_input() {
        let session = cxweb_domain::SessionKey {
            installation: "i".into(),
            native_session: "s".into(),
            account_scope: "a".into(),
            workspace_scope: "w".into(),
            route: "webbridge/test".into(),
            epoch: 1,
        };
        let key = stage_key(&session, "codec-and-original-task-route", b"exact stage").unwrap();
        for field in [
            "installation",
            "native_session",
            "account_scope",
            "workspace_scope",
            "route",
            "epoch",
        ] {
            let mut value = serde_json::to_value(&session).unwrap();
            value[field] = if field == "epoch" {
                json!(2)
            } else {
                json!("different")
            };
            let changed = serde_json::from_value(value).unwrap();
            assert_ne!(
                key,
                stage_key(&changed, "codec-and-original-task-route", b"exact stage").unwrap(),
                "{field}"
            );
        }
        assert_ne!(
            key,
            stage_key(&session, "other-codec-or-task-route", b"exact stage").unwrap()
        );
        assert_ne!(
            key,
            stage_key(&session, "codec-and-original-task-route", b"changed stage").unwrap()
        );
    }

    #[test]
    fn fragments_cover_unicode_source_with_pending_state_kept_outside_stages() {
        let pending = json!({"type":"custom_tool_call","call_id":"pending","name":"apply_patch","input":"unchanged arguments"});
        let original = CanonicalRequest::decode_compaction(
            &serde_json::to_vec(&json!({
                "model":"webbridge/test","reasoning":{"effort":"max"},
                "input":[pending,{"type":"compaction_trigger"}]
            }))
            .unwrap(),
        )
        .unwrap();
        let source = "漢字🦀\\\"\n".repeat(70_000);
        let mut collected = String::new();
        let mut start = 0;
        let mut count = 0;
        while start < source.len() {
            let (end, bytes) =
                next_stage(&original, Some("prior validated state"), &source, start).unwrap();
            let value: Value = serde_json::from_slice(&bytes).unwrap();
            let part: Value =
                serde_json::from_str(value["input"][1]["content"].as_str().unwrap()).unwrap();
            collected.push_str(part["source_fragment"].as_str().unwrap());
            assert_eq!(part["source_start_byte"], start);
            assert_eq!(part["source_end_byte"], end);
            assert_eq!(part["final_fragment"], end == source.len());
            let decoded = CanonicalRequest::decode_compaction(&bytes).unwrap();
            assert_eq!(decoded.requested_effort.as_deref(), Some("max"));
            assert!(decoded.compaction_pending().unwrap().is_empty());
            assert_eq!(
                original.compaction_pending().unwrap(),
                std::slice::from_ref(&pending)
            );
            assert!(decoded.browser_prompt(NONCE, STAGE_BYTES).is_ok());
            assert_eq!(value["tools"], json!([]));
            start = end;
            count += 1;
        }
        assert!(count > 2);
        assert_eq!(collected, source);
    }
}
