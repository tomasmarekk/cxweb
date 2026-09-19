//! Inspect protocol references only; literal conversation text is portable.
use serde_json::Value;

fn owned_response(id: &str) -> bool {
    id.starts_with("resp_cxweb_") || id.starts_with("resp_web_warmup_")
}

pub(crate) fn has_owned_reference(payload: &Value) -> bool {
    payload["previous_response_id"]
        .as_str()
        .is_some_and(owned_response)
        || payload["input"].as_array().is_some_and(|items| {
            items.iter().any(|item| match item["type"].as_str() {
                Some("compaction" | "compaction_summary" | "context_compaction") => {
                    item["encrypted_content"]
                        .as_str()
                        .is_some_and(|value| value.starts_with("wbr1:"))
                }
                Some("item_reference") => item["id"]
                    .as_str()
                    .is_some_and(|id| owned_response(id) || id.starts_with("cmp_cxweb_")),
                _ => false,
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_references_and_literal_mentions_are_not_owned_context() {
        for payload in [
            json!({"previous_response_id":"resp_native","input":[{"type":"compaction","encrypted_content":"native-opaque"}]}),
            json!({"input":[{"role":"user","content":"Explain wbr1: and resp_cxweb_ IDs"}]}),
            json!({"input":[{"type":"function_call_output","call_id":"resp_cxweb_1_call_0","output":"wbr1:literal"}]}),
        ] {
            assert!(!has_owned_reference(&payload));
        }
        for payload in [
            json!({"previous_response_id":"resp_cxweb_1"}),
            json!({"previous_response_id":"resp_web_warmup_1"}),
            json!({"input":[{"type":"compaction","encrypted_content":"wbr1:fixture"}]}),
            json!({"input":[{"type":"compaction_summary","encrypted_content":"wbr1:fixture"}]}),
            json!({"input":[{"type":"context_compaction","encrypted_content":"wbr1:fixture"}]}),
            json!({"input":[{"type":"item_reference","id":"cmp_cxweb_fixture"}]}),
            json!({"input":[{"type":"item_reference","id":"resp_cxweb_1_message"}]}),
        ] {
            assert!(has_owned_reference(&payload));
        }
    }
}
