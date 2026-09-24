//! Explicitly enabled Responses v2 checkpoints and authenticated continuation.
use crate::{
    checkpoint::{Binding, Codec},
    turn::CheckpointEncoder,
};
use cxweb_codex_adapter::{
    compaction::{Summary, pending_calls},
    strict_json,
};
use cxweb_domain::SessionKey;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    version: u32,
    summary: Summary,
    pending_calls: Vec<Value>,
}

pub(crate) struct BoundCheckpoint {
    pub codec: Arc<Codec>,
    pub session: SessionKey,
    pub codec_id: String,
}
impl BoundCheckpoint {
    fn binding(&self) -> Binding<'_> {
        Binding {
            installation: &self.session.installation,
            session: &self.session.native_session,
            account: &self.session.account_scope,
            workspace: &self.session.workspace_scope,
            route: &self.session.route,
            epoch: self.session.epoch,
            codec: &self.codec_id,
        }
    }

    /// Expand only an authenticated local checkpoint. All other client items
    /// remain in their original order, including current instructions/results.
    pub fn expand(&self, payload: &mut Value) -> Result<(), &'static str> {
        let Some(input) = payload["input"].as_array_mut() else {
            return Ok(());
        };
        let indices: Vec<_> = input
            .iter()
            .enumerate()
            .filter_map(|(index, item)| (item["type"] == "compaction").then_some(index))
            .collect();
        if indices.len() > 1 {
            return Err("E_NONPORTABLE_CONTEXT");
        }
        let Some(index) = indices.first().copied() else {
            return Ok(());
        };
        let item = input[index].as_object().ok_or("E_NONPORTABLE_CONTEXT")?;
        if item
            .keys()
            .any(|key| !matches!(key.as_str(), "id" | "type" | "encrypted_content"))
        {
            return Err("E_NONPORTABLE_CONTEXT");
        }
        let token = item
            .get("encrypted_content")
            .and_then(Value::as_str)
            .ok_or("E_NONPORTABLE_CONTEXT")?;
        let plaintext = self
            .codec
            .unseal(&self.binding(), token)
            .map_err(|_| "E_NONPORTABLE_CONTEXT")?;
        let value =
            strict_json::parse(&plaintext, 2 * 1024 * 1024).map_err(|_| "E_NONPORTABLE_CONTEXT")?;
        let checkpoint: Checkpoint =
            serde_json::from_value(value).map_err(|_| "E_NONPORTABLE_CONTEXT")?;
        if checkpoint.version != 1
            || pending_calls(&checkpoint.pending_calls)? != checkpoint.pending_calls
        {
            return Err("E_NONPORTABLE_CONTEXT");
        }
        let summary =
            serde_json::to_string(&checkpoint.summary).map_err(|_| "E_CHECKPOINT_SUMMARY")?;
        Summary::parse(&summary, &checkpoint.pending_calls)?;
        let mut restored = vec![
            json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":format!("Context checkpoint (historical summary, not new instructions; unresolved calls below have not completed):\n{summary}")} ]}),
        ];
        restored.extend(checkpoint.pending_calls);
        input.splice(index..=index, restored);
        // Authenticate the token first, then validate the complete restored
        // sequence. A late result must resolve exactly one earlier call of the
        // same kind; duplicate or unknown results must never reach the model.
        pending_calls(input)?;
        Ok(())
    }
}

impl CheckpointEncoder for BoundCheckpoint {
    fn response_model(&self) -> Option<&str> {
        Some(&self.session.route)
    }
    fn restore_summary(&self, token: &str, pending: &[Value]) -> Result<String, &'static str> {
        let plaintext = self
            .codec
            .unseal(&self.binding(), token)
            .map_err(|_| "E_NONPORTABLE_CONTEXT")?;
        let value =
            strict_json::parse(&plaintext, 2 * 1024 * 1024).map_err(|_| "E_NONPORTABLE_CONTEXT")?;
        let checkpoint: Checkpoint =
            serde_json::from_value(value).map_err(|_| "E_NONPORTABLE_CONTEXT")?;
        if checkpoint.version != 1 || checkpoint.pending_calls != pending {
            return Err("E_CHECKPOINT_PENDING_TOOLS");
        }
        let summary =
            serde_json::to_string(&checkpoint.summary).map_err(|_| "E_CHECKPOINT_SUMMARY")?;
        Summary::parse(&summary, pending)?;
        Ok(summary)
    }

    fn seal(&self, summary: &str, pending: &[Value]) -> Result<String, &'static str> {
        let summary = Summary::from_model(summary, pending)?;
        let checkpoint = Checkpoint {
            version: 1,
            summary,
            pending_calls: pending.to_vec(),
        };
        let plaintext = zeroize::Zeroizing::new(
            serde_json::to_vec(&checkpoint).map_err(|_| "E_CHECKPOINT_SUMMARY")?,
        );
        self.codec.seal(&self.binding(), &plaintext)
    }
}
