//! Fixed diagnostic requests. These tools are protocol fixtures, never executed.
use cxweb_codex_adapter::{envelope::ValidatedOutput, request::CanonicalRequest};
use serde_json::json;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Tools,
}

impl Kind {
    pub fn request(self, model: &str) -> Result<CanonicalRequest, &'static str> {
        let body = match self {
            Self::Text => json!({
                "model":model,
                "input":"Complete the cxweb transport qualification. Return the requested final protocol envelope with the exact final text: cxweb live qualification passed",
                "tool_choice":"none", "parallel_tool_calls":false
            }),
            Self::Tools => json!({
                "model":model,
                "input":"This is a protocol-only diagnostic. Request exactly two tools in one tool_calls envelope: fixture_read with path equal to fixture.txt, and fixture_literal with the literal input cxweb custom fixture. Use the tool keys from CLIENT_DATA_JSON. Do not execute anything and do not return a final answer.",
                "tools":[
                    {"type":"function","name":"fixture_read","parameters":{"type":"object","properties":{"path":{"type":"string","const":"fixture.txt"}},"required":["path"],"additionalProperties":false}},
                    {"type":"custom","name":"fixture_literal","format":{"type":"text"}}
                ],
                "tool_choice":"required", "parallel_tool_calls":true
            }),
        };
        CanonicalRequest::decode(body.to_string().as_bytes())
    }

    pub fn accepts(self, output: &ValidatedOutput) -> bool {
        match (self, output) {
            (Self::Text, ValidatedOutput::Final(text)) => text == "cxweb live qualification passed",
            (Self::Tools, ValidatedOutput::Calls(calls)) => {
                calls.len() == 2
                    && calls.iter().any(|call| {
                        call.native_name == "fixture_read"
                            && call.namespace.is_none()
                            && call.input == json!({"path":"fixture.txt"})
                    })
                    && calls.iter().any(|call| {
                        call.native_name == "fixture_literal"
                            && call.namespace.is_none()
                            && call.input == "cxweb custom fixture"
                    })
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cxweb_codex_adapter::envelope;

    #[test]
    fn fixed_tool_diagnostic_requires_both_exact_typed_calls_and_nonce() {
        let nonce = "0123456789abcdef0123456789abcdef";
        let request = Kind::Tools.request("webbridge/fixture").unwrap();
        let mut response = json!({"protocol":"webbridge.tool.v1","turn_nonce":nonce,"kind":"tool_calls","calls":[
            {"tool_key":"tool_0001","input":{"path":"fixture.txt"}},
            {"tool_key":"tool_0002","input":"cxweb custom fixture"}
        ]});
        let validate = |value: &serde_json::Value| {
            envelope::validate(value.to_string().as_bytes(), &request.context(nonce))
        };
        assert!(Kind::Tools.accepts(&validate(&response).unwrap()));
        response["calls"][1]["input"] = json!("different literal");
        assert!(!Kind::Tools.accepts(&validate(&response).unwrap()));
        response["calls"][0]["input"]["path"] = json!("other.txt");
        assert!(validate(&response).is_err());
        response["turn_nonce"] = json!("11111111111111111111111111111111");
        assert!(validate(&response).is_err());
    }
}
