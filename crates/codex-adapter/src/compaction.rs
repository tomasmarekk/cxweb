//! Structured summaries and exact preservation of unresolved client tool calls.
use crate::strict_json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Summary {
    pub goal: String,
    pub constraints: Vec<String>,
    pub changed_files: Vec<String>,
    pub decisions: Vec<String>,
    pub outstanding_work: Vec<String>,
    pub test_results: Vec<String>,
    pub unresolved_tool_ids: Vec<String>,
}

impl Summary {
    pub fn parse(text: &str, pending: &[Value]) -> Result<Self, &'static str> {
        let value =
            strict_json::parse(text.as_bytes(), 256 * 1024).map_err(|_| "E_CHECKPOINT_SUMMARY")?;
        let summary: Self = serde_json::from_value(value).map_err(|_| "E_CHECKPOINT_SUMMARY")?;
        let lists = [
            &summary.constraints,
            &summary.changed_files,
            &summary.decisions,
            &summary.outstanding_work,
            &summary.test_results,
            &summary.unresolved_tool_ids,
        ];
        if summary.goal.trim().is_empty()
            || summary.goal.len() > 32 * 1024
            || lists.iter().any(|list| {
                list.len() > 1024
                    || list
                        .iter()
                        .any(|s| s.trim().is_empty() || s.len() > 32 * 1024)
            })
        {
            return Err("E_CHECKPOINT_SUMMARY");
        }
        let expected: BTreeSet<_> = pending
            .iter()
            .map(|call| call["call_id"].as_str())
            .collect();
        let actual: BTreeSet<_> = summary
            .unresolved_tool_ids
            .iter()
            .map(|id| Some(id.as_str()))
            .collect();
        if expected.contains(&None)
            || expected != actual
            || actual.len() != summary.unresolved_tool_ids.len()
        {
            return Err("E_CHECKPOINT_PENDING_TOOLS");
        }
        Ok(summary)
    }
}

/// Observed Codex App context, not a receipt for an executed tool call.
pub(crate) fn is_app_context(item: &Value) -> bool {
    item["type"] == "function_call_output"
        && item.get("call_id").is_none()
        && item["namespace"] == "codex_app"
        && item["name"] == "send_message_to_thread"
        && item["output"].is_string()
}

/// Completed results, including denials and errors, remain in the summary input.
/// Unresolved calls additionally survive verbatim inside the authenticated token.
pub fn pending_calls(history: &[Value]) -> Result<Vec<Value>, &'static str> {
    let mut calls = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for (index, item) in history.iter().enumerate() {
        // Codex App supplies cross-task context without an initiating tool call.
        // Keep it in history, but it cannot resolve any pending execution.
        if is_app_context(item) {
            continue;
        }
        let kind = item["type"].as_str().unwrap_or("message");
        match kind {
            "function_call" | "custom_tool_call" | "tool_search_call" => {
                let id = item["call_id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or("E_CHECKPOINT_PENDING_TOOLS")?;
                if !seen.insert(id) {
                    return Err("E_CHECKPOINT_PENDING_TOOLS");
                }
                calls.insert(id, (index, item));
            }
            "function_call_output" | "custom_tool_call_output" | "tool_search_output" => {
                let id = item["call_id"]
                    .as_str()
                    .ok_or("E_CHECKPOINT_PENDING_TOOLS")?;
                let (_, call) = calls.remove(id).ok_or("E_CHECKPOINT_PENDING_TOOLS")?;
                let expected = match kind {
                    "function_call_output" => "function_call",
                    "custom_tool_call_output" => "custom_tool_call",
                    _ => "tool_search_call",
                };
                if call["type"] != expected {
                    return Err("E_CHECKPOINT_PENDING_TOOLS");
                }
            }
            _ => (),
        }
    }
    let mut pending: Vec<_> = calls.into_values().collect();
    pending.sort_by_key(|(index, _)| *index);
    Ok(pending.into_iter().map(|(_, item)| item.clone()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unresolved_calls_survive_exactly_and_ambiguous_results_fail() {
        let completed =
            json!({"type":"function_call","call_id":"done","name":"read","arguments":"{}"});
        let pending = json!({"type":"custom_tool_call","call_id":"pending","name":"apply_patch","input":"literal\n🦀"});
        let output = json!({"type":"function_call_output","call_id":"done","output":"DENIED"});
        assert_eq!(
            pending_calls(&[completed.clone(), pending.clone(), output.clone()]).unwrap(),
            vec![pending.clone()]
        );
        for history in [
            vec![output.clone()],
            vec![completed.clone(), completed.clone()],
            vec![
                completed,
                json!({"type":"custom_tool_call_output","call_id":"done","output":"ok"}),
            ],
            vec![pending.clone(), output],
        ] {
            assert!(pending_calls(&history).is_err());
        }
        let summary = json!({"goal":"Continue the fixture","constraints":[],"changed_files":[],"decisions":[],"outstanding_work":["Await patch result"],"test_results":["Read denied"],"unresolved_tool_ids":["pending"]});
        assert!(Summary::parse(&summary.to_string(), std::slice::from_ref(&pending)).is_ok());
        for ids in [json!([]), json!(["other"]), json!(["pending", "pending"])] {
            let mut invalid = summary.clone();
            invalid["unresolved_tool_ids"] = ids;
            assert!(Summary::parse(&invalid.to_string(), std::slice::from_ref(&pending)).is_err());
        }
        assert!(Summary::parse("plain summary", &[]).is_err());
        let mut invalid = summary;
        invalid["extra"] = json!(true);
        assert!(Summary::parse(&invalid.to_string(), &[pending]).is_err());
    }
}
