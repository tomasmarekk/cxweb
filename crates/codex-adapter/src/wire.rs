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
    let items: Vec<Value> = match output {
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
    let mut response = json!({"id":response_id,"object":"response","created_at":created_at,"status":"in_progress","model":model,"output":[]});
    let mut events = vec![
        json!({"type":"response.created","response":response}),
        json!({"type":"response.in_progress","response":response}),
    ];
    for (i, item) in items.iter().enumerate() {
        let mut initial = item.clone();
        initial["status"] = json!("in_progress");
        match item["type"].as_str() {
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
            input: json!("first\nžluťoučký\nsecond"),
        }]);
        let encoded = encode(&output, "webbridge/test", "resp_1", 1).unwrap();
        assert_eq!(encoded.response["output"][0]["type"], "custom_tool_call");
        assert_eq!(
            encoded.response["output"][0]["input"],
            "first\nžluťoučký\nsecond"
        );
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
