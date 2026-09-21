use cxweb_codex_adapter::{envelope, request::CanonicalRequest, wire};
use serde_json::{Value, json};

#[test]
fn required_client_tools_remain_usable_when_optional_hosted_search_is_attached() {
    let request = json!({"model":"webbridge/test","input":"Search with MCP","tool_choice":"required","tools":[
        {"type":"web_search"},
        {"type":"function","name":"mcp__cxweb_web__search","parameters":{"type":"object"}}
    ]});
    assert!(CanonicalRequest::decode(request.to_string().as_bytes()).is_ok());
}

#[test]
fn deferred_tool_discovery_round_trips_as_a_client_search_not_a_function() {
    let request = json!({"model":"webbridge/test","input":"Find a tool","tools":[{
        "type":"tool_search","execution":"client","description":"Find MCP tools",
        "parameters":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false}
    }]});
    let decoded = CanonicalRequest::decode(request.to_string().as_bytes()).unwrap();
    let nonce = "0123456789abcdef0123456789abcdef";
    let output = json!({"protocol":"webbridge.tool.v1","turn_nonce":nonce,"kind":"tool_calls","calls":[{"tool_key":"tool_0001","input":{"query":"public web search"}}]});
    let validated =
        envelope::validate(output.to_string().as_bytes(), &decoded.context(nonce)).unwrap();
    let encoded = wire::encode(&validated, "webbridge/test", "resp_fixture", 1).unwrap();
    let call = encoded.response["output"][0].clone();
    assert_eq!(call["type"], "tool_search_call");
    assert_eq!(call["execution"], "client");
    assert!(call["arguments"].is_object());
    assert!(call.get("name").is_none());
    let result = json!({"type":"tool_search_output","call_id":call["call_id"],"execution":"client","status":"completed","tools":[{"type":"function","name":"mcp__web__search","parameters":{"type":"object"}}]});
    let next = json!({"model":"webbridge/test","input":[call,result]});
    assert!(CanonicalRequest::decode(next.to_string().as_bytes()).is_ok());
    let mut invalid = next;
    invalid["input"][1]["call_id"] = json!("foreign");
    assert!(CanonicalRequest::decode(invalid.to_string().as_bytes()).is_err());
}

#[test]
fn discovered_mcp_definition_gets_a_valid_callable_key_on_the_next_turn() {
    let search = json!({"type":"tool_search","execution":"client","parameters":{"type":"object"}});
    let definition = json!({"type":"namespace","name":"mcp__cxweb_web","tools":[{"type":"function","name":"search","parameters":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}}]});
    let request = json!({"model":"webbridge/test","tools":[search],"input":[
        {"type":"tool_search_call","call_id":"s1","execution":"client","arguments":{"query":"public search"}},
        {"type":"tool_search_output","call_id":"s1","execution":"client","status":"completed","tools":[definition]}
    ]});
    let decoded = CanonicalRequest::decode(request.to_string().as_bytes()).unwrap();
    let key = decoded
        .registry
        .key_for("search", Some("mcp__cxweb_web"))
        .unwrap();
    let nonce = "0123456789abcdef0123456789abcdef";
    let output = json!({"protocol":"webbridge.tool.v1","turn_nonce":nonce,"kind":"tool_calls","calls":[{"tool_key":key,"input":{"query":"Rust language"}}]});
    let validated =
        envelope::validate(output.to_string().as_bytes(), &decoded.context(nonce)).unwrap();
    let encoded = wire::encode(&validated, "webbridge/test", "r", 1).unwrap();
    assert_eq!(encoded.response["output"][0]["type"], "function_call");
    assert_eq!(encoded.response["output"][0]["namespace"], "mcp__cxweb_web");
    let mut disabled = request;
    disabled["tools"] = json!([]);
    assert!(
        CanonicalRequest::decode(disabled.to_string().as_bytes())
            .unwrap()
            .registry
            .key_for("search", Some("mcp__cxweb_web"))
            .is_none()
    );
}

#[test]
fn public_summary_is_separate_from_answer_and_survives_followup_history() {
    let summary = vec!["Checking the returned file contents".to_owned()];
    let output = envelope::ValidatedOutput::Final("Done".to_owned());
    let response =
        wire::encode_with_summary(&output, "webbridge/test", "resp_fixture", 1, &summary).unwrap();
    assert_eq!(response.response["output"][0]["type"], "reasoning");
    assert_eq!(response.response["output"][1]["content"][0]["text"], "Done");
    assert!(response.events.iter().any(|event| event["type"]
        == "response.reasoning_summary_text.delta"
        && event["delta"] == summary[0]));
    for (index, event) in response.events.iter().enumerate() {
        assert_eq!(event["sequence_number"], index);
    }
    let request = json!({"model":"webbridge/test","input":response.response["output"],"reasoning":{"summary":"auto"}});
    assert!(CanonicalRequest::decode(request.to_string().as_bytes()).is_ok());
    let mut encrypted = request;
    encrypted["input"][0]["encrypted_content"] = json!("opaque-native-state");
    assert!(CanonicalRequest::decode(encrypted.to_string().as_bytes()).is_err());
    let empty =
        wire::encode_with_summary(&output, "webbridge/test", "resp_fixture", 1, &[]).unwrap();
    assert_eq!(empty.response["output"][0]["type"], "message");
    assert!(empty.response.get("usage").is_none());
}

#[test]
fn structured_mcp_text_outputs_are_preserved_without_flattening() {
    let history = json!([
        {"type":"function_call","name":"mcp__example__read","call_id":"c1","arguments":"{}"},
        {"type":"function_call_output","call_id":"c1","output":[{"type":"text","text":"DENIED"},{"type":"input_text","text":"Do not retry"}]}
    ]);
    let request = CanonicalRequest::decode(
        json!({"model":"webbridge/test","input":history})
            .to_string()
            .as_bytes(),
    )
    .unwrap();
    let prompt = request
        .browser_prompt("0123456789abcdef0123456789abcdef", 100_000)
        .unwrap();
    let data: Value =
        serde_json::from_str(prompt.split_once("\nCLIENT_DATA_JSON\n").unwrap().1).unwrap();
    assert_eq!(data["history"], history);
}
