//! Retain active instructions and the newest result while summarizing history.
use serde_json::{Value, json};

pub(crate) struct Projection {
    pub compact: Value,
    pub continuation: Value,
    pub checkpoint_index: usize,
}

pub(crate) fn plan(payload: &Value) -> Result<Projection, &'static str> {
    let input = payload["input"].as_array().ok_or("E_CONTEXT_BUDGET")?;
    let cut = input
        .len()
        .checked_sub(1)
        .filter(|cut| *cut > 0)
        .ok_or("E_CONTEXT_BUDGET")?;
    let latest_user = input.iter().rposition(|item| item["role"] == "user");
    let mut compact = payload.clone();
    let mut history = input[..cut].to_vec();
    history.push(json!({"type":"compaction_trigger"}));
    compact["input"] = json!(history);
    // Current policies and the latest user instruction remain exact, even when
    // they precede completed tool exchanges in the summarized prefix. Pending
    // calls come from the authenticated checkpoint, before the newest result.
    let mut retained: Vec<_> = input[..cut]
        .iter()
        .enumerate()
        .filter(|(index, item)| {
            matches!(item["role"].as_str(), Some("system" | "developer"))
                || Some(*index) == latest_user
        })
        .map(|(_, item)| item.clone())
        .collect();
    let checkpoint_index = retained.len();
    retained.push(json!({"type":"compaction"}));
    retained.extend_from_slice(&input[cut..]);
    let mut continuation = payload.clone();
    continuation["input"] = json!(retained);
    Ok(Projection {
        compact,
        continuation,
        checkpoint_index,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_keeps_policies_current_user_and_latest_tool_result_exact() {
        let payload = json!({"model":"webbridge/test","reasoning":{"effort":"max"},
            "tools":[{"type":"function","name":"read","parameters":{"type":"object"}}],
            "input":[{"role":"user","content":"Original goal"},
                {"role":"developer","content":"Exact policy"},
                {"role":"user","content":"Current instruction"},
                {"type":"function_call","call_id":"a","name":"read","arguments":"{}"},
                {"type":"function_call_output","call_id":"a","output":"exact result"}]});
        let projection = plan(&payload).unwrap();
        assert_eq!(projection.compact["input"][0], payload["input"][0]);
        assert_eq!(projection.compact["input"][3], payload["input"][3]);
        assert_eq!(projection.continuation["input"][0], payload["input"][1]);
        assert_eq!(projection.continuation["input"][1], payload["input"][2]);
        assert_eq!(projection.continuation["input"][3], payload["input"][4]);
        assert_eq!(projection.continuation["tools"], payload["tools"]);
        assert_eq!(projection.continuation["reasoning"], payload["reasoning"]);
        assert_eq!(projection.checkpoint_index, 2);
    }
}
