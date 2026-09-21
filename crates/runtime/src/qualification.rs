//! Fixed diagnostic requests. These tools are protocol fixtures, never executed.
use cxweb_codex_adapter::{envelope::ValidatedOutput, request::CanonicalRequest};
use serde_json::json;

/// Fixed structural observations only; never include response or account text.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolDiagnostic {
    pub response_bytes: usize,
    pub json_object: bool,
    pub protocol_matches: bool,
    pub nonce_matches: bool,
    pub result: ProtocolResult,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolResult {
    Accepted,
    UnexpectedOutput,
    InvalidEnvelope,
    UnsupportedTool,
    InvalidInput,
    ToolChoice,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Tools,
}

impl Kind {
    pub fn diagnose(
        self,
        response: &str,
        request: &CanonicalRequest,
        nonce: &str,
    ) -> ProtocolDiagnostic {
        use cxweb_codex_adapter::envelope::{self, ProtocolError};
        let value = serde_json::from_str::<serde_json::Value>(response).ok();
        let result = match envelope::validate(response.as_bytes(), &request.context(nonce)) {
            Ok(output) if self.accepts(&output) => ProtocolResult::Accepted,
            Ok(_) => ProtocolResult::UnexpectedOutput,
            Err(ProtocolError::InvalidEnvelope) => ProtocolResult::InvalidEnvelope,
            Err(ProtocolError::UnsupportedTool) => ProtocolResult::UnsupportedTool,
            Err(ProtocolError::InvalidInput) => ProtocolResult::InvalidInput,
            Err(ProtocolError::ToolChoice) => ProtocolResult::ToolChoice,
        };
        ProtocolDiagnostic {
            response_bytes: response.len(),
            json_object: value.as_ref().is_some_and(|value| value.is_object()),
            protocol_matches: value
                .as_ref()
                .is_some_and(|value| value["protocol"] == "webbridge.tool.v1"),
            nonce_matches: value
                .as_ref()
                .is_some_and(|value| value["turn_nonce"] == nonce),
            result,
        }
    }

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
    fn protocol_diagnostics_distinguish_rejection_without_exporting_content() {
        let nonce = "0123456789abcdef0123456789abcdef";
        let request = Kind::Text.request("webbridge/fixture").unwrap();
        let mut response = json!({"protocol":"webbridge.tool.v1","turn_nonce":nonce,
            "kind":"final","text":"PRIVATE_RESPONSE"});
        let diagnostic = Kind::Text.diagnose(&response.to_string(), &request, nonce);
        assert_eq!(diagnostic.result, ProtocolResult::UnexpectedOutput);
        assert!(diagnostic.json_object && diagnostic.protocol_matches && diagnostic.nonce_matches);
        assert!(
            !serde_json::to_string(&diagnostic)
                .unwrap()
                .contains("PRIVATE_RESPONSE")
        );
        response["text"] = json!("cxweb live qualification passed");
        assert_eq!(
            Kind::Text
                .diagnose(&response.to_string(), &request, nonce)
                .result,
            ProtocolResult::Accepted
        );
        response["turn_nonce"] = json!("PRIVATE_WRONG_NONCE");
        let diagnostic = Kind::Text.diagnose(&response.to_string(), &request, nonce);
        assert_eq!(diagnostic.result, ProtocolResult::InvalidEnvelope);
        assert!(!diagnostic.nonce_matches);
        assert!(
            !serde_json::to_string(&diagnostic)
                .unwrap()
                .contains("PRIVATE_WRONG_NONCE")
        );
        let diagnostic = Kind::Text.diagnose("PRIVATE_NON_JSON", &request, nonce);
        assert_eq!(diagnostic.result, ProtocolResult::InvalidEnvelope);
        assert!(!diagnostic.json_object);
    }

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
