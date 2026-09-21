//! Buffered complete-output encoding. Emitted chunks are not live browser streaming.
use crate::envelope::{ValidatedOutput, native_call};
use serde_json::{Value, json};

pub struct CompletedResponse {
    pub response: Value,
    pub events: Vec<Value>,
}

pub fn encode(
    output: &ValidatedOutput,
    model: &str,
    response_id: &str,
    created_at: u64,
) -> Result<CompletedResponse, &'static str> {
    encode_with_summary(output, model, response_id, created_at, &[])
}

/// Summaries must come from the attributed public browser DOM, never an envelope.
pub fn encode_with_summary(
    output: &ValidatedOutput,
    model: &str,
    response_id: &str,
    created_at: u64,
    summary: &[String],
) -> Result<CompletedResponse, &'static str> {
    let mut items: Vec<Value> = match output {
        ValidatedOutput::Final(text) => vec![
            json!({"id":format!("{response_id}_message"),"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":text,"annotations":[]}]}),
        ],
        ValidatedOutput::Calls(calls) => calls
            .iter()
            .enumerate()
            .map(|(i, c)| {
                native_call(
                    c,
                    &format!("{response_id}_item_{i}"),
                    &format!("{response_id}_call_{i}"),
                )
            })
            .collect(),
        ValidatedOutput::Checkpoint(_) => return Err("E_COMPACTION_CODEC_REQUIRED"),
    };
    if !summary.is_empty() {
        if summary.len() > 64
            || summary
                .iter()
                .any(|text| text.is_empty() || text.len() > 8192)
        {
            return Err("E_REASONING_SUMMARY_LIMIT");
        }
        items.insert(0, json!({"type":"reasoning","id":format!("{response_id}_reasoning"),
            "summary":summary.iter().map(|text| json!({"type":"summary_text","text":text})).collect::<Vec<_>>() }));
    }
    encode_items(items, model, response_id, created_at)
}

/// The caller must supply a locally authenticated token, never model text.
pub fn encode_checkpoint(
    encrypted_content: &str,
    model: &str,
    response_id: &str,
    created_at: u64,
) -> Result<CompletedResponse, &'static str> {
    if !encrypted_content.starts_with("wbr1:") || encrypted_content.len() <= 5 {
        return Err("E_COMPACTION_CODEC_REQUIRED");
    }
    encode_items(
        vec![
            json!({"type":"compaction","id":format!("cmp_cxweb_{response_id}"),"encrypted_content":encrypted_content}),
        ],
        model,
        response_id,
        created_at,
    )
}

fn encode_items(
    items: Vec<Value>,
    model: &str,
    response_id: &str,
    created_at: u64,
) -> Result<CompletedResponse, &'static str> {
    let mut response = json!({"id":response_id,"object":"response","created_at":created_at,"status":"in_progress","model":model,"output":[]});
    let mut events = vec![
        json!({"type":"response.created","response":response}),
        json!({"type":"response.in_progress","response":response}),
    ];
    for (i, item) in items.iter().enumerate() {
        let mut initial = item.clone();
        initial["status"] = json!("in_progress");
        match item["type"].as_str() {
            Some("reasoning") => {
                initial["summary"] = json!([]);
                events.push(
                    json!({"type":"response.output_item.added","output_index":i,"item":initial}),
                );
                for (index, part) in item["summary"]
                    .as_array()
                    .ok_or("E_UNSUPPORTED_OUTPUT")?
                    .iter()
                    .enumerate()
                {
                    events.push(json!({"type":"response.reasoning_summary_part.added","item_id":item["id"],"output_index":i,"summary_index":index,"part":{"type":"summary_text","text":""}}));
                    events.push(json!({"type":"response.reasoning_summary_text.delta","item_id":item["id"],"output_index":i,"summary_index":index,"delta":part["text"]}));
                    events.push(json!({"type":"response.reasoning_summary_text.done","item_id":item["id"],"output_index":i,"summary_index":index,"text":part["text"]}));
                    events.push(json!({"type":"response.reasoning_summary_part.done","item_id":item["id"],"output_index":i,"summary_index":index,"part":part}));
                }
            }
            Some("compaction" | "tool_search_call") => {
                events.push(
                    json!({"type":"response.output_item.added","output_index":i,"item":item}),
                );
            }
            Some("message") => {
                initial["content"] = json!([]);
                events.push(
                    json!({"type":"response.output_item.added","output_index":i,"item":initial}),
                );
                let part = &item["content"][0];
                events.push(json!({"type":"response.content_part.added","item_id":item["id"],"output_index":i,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}));
                events.push(json!({"type":"response.output_text.delta","item_id":item["id"],"output_index":i,"content_index":0,"delta":part["text"]}));
                events.push(json!({"type":"response.output_text.done","item_id":item["id"],"output_index":i,"content_index":0,"text":part["text"]}));
                events.push(json!({"type":"response.content_part.done","item_id":item["id"],"output_index":i,"content_index":0,"part":part}));
            }
            Some("function_call" | "custom_tool_call") => {
                let function = item["type"] == "function_call";
                let key = if function { "arguments" } else { "input" };
                let event = if function {
                    "response.function_call_arguments"
                } else {
                    "response.custom_tool_call_input"
                };
                initial[key] = json!("");
                events.push(
                    json!({"type":"response.output_item.added","output_index":i,"item":initial}),
                );
                events.push(json!({"type":format!("{event}.delta"),"item_id":item["id"],"output_index":i,"delta":item[key]}));
                let mut done =
                    json!({"type":format!("{event}.done"),"item_id":item["id"],"output_index":i});
                done[key] = item[key].clone();
                events.push(done);
            }
            _ => return Err("E_UNSUPPORTED_OUTPUT"),
        }
        events.push(json!({"type":"response.output_item.done","output_index":i,"item":item}));
    }
    response["status"] = json!("completed");
    response["output"] = json!(items);
    // Web token usage is unavailable: never manufacture usage=0 or native reasoning.
    events.push(json!({"type":"response.completed","response":response}));
    for (sequence, event) in events.iter_mut().enumerate() {
        event["sequence_number"] = json!(sequence);
    }
    Ok(CompletedResponse { response, events })
}

impl CompletedResponse {
    pub fn sse(&self) -> String {
        self.events
            .iter()
            .map(|event| {
                format!(
                    "event: {}\ndata: {}\n\n",
                    event["type"].as_str().unwrap_or("error"),
                    event
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::{ToolKind, ValidatedCall};
    #[test]
    fn literal_custom_payload_and_stable_ids_survive_completed_events() {
        let output = ValidatedOutput::Calls(vec![ValidatedCall {
            native_name: "apply_patch".into(),
            namespace: None,
            kind: ToolKind::Custom,
            input: json!("first\n🦀\nsecond"),
        }]);
        let encoded = encode(&output, "webbridge/test", "resp_1", 1).unwrap();
        assert_eq!(encoded.response["output"][0]["type"], "custom_tool_call");
        assert_eq!(encoded.response["output"][0]["input"], "first\n🦀\nsecond");
        assert!(
            encoded
                .sse()
                .contains("response.custom_tool_call_input.delta")
        );
        assert_eq!(encoded.events.last().unwrap()["type"], "response.completed");
        assert_eq!(
            encoded.sse(),
            encode(&output, "webbridge/test", "resp_1", 1)
                .unwrap()
                .sse()
        );
        for (i, event) in encoded.events.iter().enumerate() {
            assert_eq!(event["sequence_number"], i);
        }
    }
}
