//! Local encoded-byte guardrails, not provider token capacity or billing usage.
#[cfg(any(windows, test))]
use cxweb_codex_adapter::{context_budget::LocalContextBudget, request::CanonicalRequest};
use serde_json::{Value, json};

pub(crate) const HARD_PROMPT_BYTES: usize = cxweb_codex_adapter::context_budget::MAX_PROMPT_BYTES;
// Production nonces have this exact encoded width. Only the length is measured.
#[cfg(any(windows, test))]
const SIZING_NONCE: &str = "00000000000000000000000000000000";

/// Ask the reviewed client to compact only when a summary request still fits.
/// This never modifies the submitted transcript or prepares a browser target.
#[cfg(any(windows, test))]
pub(crate) fn needs_compaction(
    payload: &Value,
    request: &CanonicalRequest,
    budget: LocalContextBudget,
) -> Result<bool, &'static str> {
    match request.browser_prompt(SIZING_NONCE, budget.normal_bytes()) {
        Ok(_) => return Ok(false),
        Err("E_CONTEXT_BUDGET") => (),
        Err(code) => return Err(code),
    }
    let input = payload["input"].as_array().ok_or("E_CONTEXT_BUDGET")?;
    let mut compact = payload.clone();
    compact["input"]
        .as_array_mut()
        .ok_or("E_CONTEXT_BUDGET")?
        .push(json!({"type":"compaction_trigger"}));
    let summary = CanonicalRequest::decode_compaction(
        &serde_json::to_vec(&compact).map_err(|_| "E_INVALID_REQUEST")?,
    )?;
    summary.browser_prompt(SIZING_NONCE, budget.summary_bytes())?;

    // A lower bound only: native compaction retains user/developer/system
    // messages and our checkpoint restores pending calls. If those alone exceed
    // the operating ceiling, do not suggest a compaction that cannot help.
    // This projected input is NEVER sent to a model or returned as history.
    let retained: Vec<_> = input
        .iter()
        .filter(|item| {
            item["role"] != "assistant"
                && !matches!(
                    item["type"].as_str(),
                    Some(
                        "function_call"
                            | "custom_tool_call"
                            | "function_call_output"
                            | "custom_tool_call_output"
                    )
                )
        })
        .cloned()
        .collect();
    if retained.len() == input.len() {
        return Err("E_CONTEXT_BUDGET");
    }
    let mut minimum = payload.clone();
    let mut retained = retained;
    retained.extend_from_slice(summary.compaction_pending().ok_or("E_COMPACTION_TRIGGER")?);
    minimum["input"] = json!(retained);
    CanonicalRequest::decode(&serde_json::to_vec(&minimum).map_err(|_| "E_INVALID_REQUEST")?)?
        .browser_prompt(SIZING_NONCE, budget.normal_bytes())?;
    Ok(true)
}

/// The reviewed backends recognize this terminal event and compact before the
/// next turn. They do not retry the failed turn automatically. No usage is added.
fn context_failure(id: &str) -> Value {
    json!({"type":"response.failed","sequence_number":0,"response":{
        "id":id,
        "object":"response","status":"failed","output":[],
        "error":{"code":"context_length_exceeded","message":"The cxweb local context budget was reached before submission. Codex can compact the history before the next turn."}
    }})
}

#[cfg(any(windows, test))]
pub(crate) fn failure_response() -> axum::response::Response {
    let event = context_failure(&format!(
        "resp_cxweb_context_{:032x}",
        rand::random::<u128>()
    ));
    axum::response::Response::builder()
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-store")
        .body(axum::body::Body::from(format!(
            "event: response.failed\ndata: {event}\n\n"
        )))
        .expect("fixed response headers")
}

/// Only our exact, empty-output budget failure may clear the WS delta cache.
pub(crate) fn is_context_failure(events: &[Value]) -> bool {
    if events.len() != 1 {
        return false;
    }
    let event = &events[0];
    let Some(id) = event["response"]["id"].as_str() else {
        return false;
    };
    let Some(suffix) = id.strip_prefix("resp_cxweb_context_") else {
        return false;
    };
    if suffix.len() != 32 || !suffix.bytes().all(|c| c.is_ascii_hexdigit()) {
        return false;
    }
    event == &context_failure(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(payload: &Value) -> Result<bool, &'static str> {
        let request = CanonicalRequest::decode(&serde_json::to_vec(payload).unwrap())?;
        needs_compaction(payload, &request, LocalContextBudget::DIAGNOSTIC)
    }

    #[test]
    fn budget_reserves_summary_space_without_truncating_any_input() {
        let mut payload = json!({"model":"webbridge/test","instructions":"Current policy","input":[{"role":"user","content":"Keep task state"},{"role":"assistant","content":"a".repeat(400_000)}]});
        let original = payload.clone();
        assert_eq!(check(&payload), Ok(true));
        assert_eq!(payload, original);
        payload["input"][1]["content"] = json!("a".repeat(550_000));
        assert_eq!(check(&payload), Err("E_CONTEXT_BUDGET"));
        payload["input"][1]["content"] = json!("a".repeat(300_000));
        assert_eq!(check(&payload), Ok(false));
        // Raw JSON fits, but Markdown-safe escaping pushes the full prompt over
        // the ordinary ceiling. The compaction prompt still fits the reserve.
        payload["input"][1]["content"] = json!("*".repeat(70_000));
        assert_eq!(check(&payload), Ok(true));
    }

    #[test]
    fn irreducible_policy_user_content_and_pending_calls_do_not_offer_recovery() {
        let mut payload = json!({"model":"webbridge/test","input":[{"role":"user","content":"a".repeat(400_000)}]});
        assert_eq!(check(&payload), Err("E_CONTEXT_BUDGET"));
        payload["input"] = json!([{"role":"assistant","content":"old answer"}]);
        payload["instructions"] = json!("a".repeat(400_000));
        assert_eq!(check(&payload), Err("E_CONTEXT_BUDGET"));
        payload.as_object_mut().unwrap().remove("instructions");
        payload["tools"] = json!([{"type":"function","name":"fixture","parameters":{"type":"object","description":"*".repeat(70_000)}}]);
        assert_eq!(check(&payload), Err("E_CONTEXT_BUDGET"));
        payload.as_object_mut().unwrap().remove("tools");
        payload["input"] = json!([{"type":"custom_tool_call","name":"apply_patch","call_id":"pending","input":"a".repeat(400_000)}]);
        assert_eq!(check(&payload), Err("E_CONTEXT_BUDGET"));
        // Once the result is present, the old tool transcript is summarizable.
        payload["input"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type":"custom_tool_call_output","call_id":"pending","output":"DENIED"}));
        assert_eq!(check(&payload), Ok(true));
    }

    #[test]
    fn failed_wire_is_single_terminal_and_refuses_other_payloads() {
        let event = context_failure("resp_cxweb_context_0123456789abcdef0123456789abcdef");
        assert!(is_context_failure(std::slice::from_ref(&event)));
        assert!(event["response"].get("usage").is_none());
        for variant in 0..6 {
            let mut changed = event.clone();
            match variant {
                0 => changed["response"]["output"] = json!([{"type":"message"}]),
                1 => changed["response"]["status"] = json!("completed"),
                2 => changed["response"]["error"]["code"] = json!("other"),
                3 => changed["response"]["id"] = json!("resp_native"),
                4 => changed["response"]["usage"] = json!({"total_tokens": 100}),
                _ => changed["response"]["error"]["message"] = json!("untrusted message"),
            }
            assert!(!is_context_failure(&[changed]));
        }
        assert!(!is_context_failure(&[event.clone(), event]));
        assert!(!is_context_failure(&[]));
    }
}
