use cxweb_codex_adapter::envelope::*;
use serde_json::{Value, json};

fn registry() -> Registry {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/tool-registry.json")).unwrap();
    let definitions: Vec<Value> = fixture["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| {
            let mut native = json!({"type":tool["kind"],"name":tool["native_name"]});
            if tool["kind"] == "function" {
                native["parameters"] = tool["input_schema"].clone();
            }
            if tool["namespace"].is_string() {
                json!({"type":"namespace","name":tool["namespace"],"tools":[native]})
            } else {
                native
            }
        })
        .collect();
    Registry::from_native(&definitions).unwrap()
}

#[test]
fn all_prd_envelope_fixtures() {
    let index: Value = serde_json::from_str(include_str!("fixtures/index.json")).unwrap();
    let registry = registry();
    for fixture in index["envelopes"].as_array().unwrap() {
        let file = fixture["path"].as_str().unwrap();
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(file),
        )
        .unwrap();
        let context = Context {
            nonce: "11111111111111111111111111111111",
            purpose: if fixture["purpose"] == "compaction" {
                Purpose::Compaction
            } else {
                Purpose::Normal
            },
            parallel: true,
            choice: ToolChoice::Auto,
            registry: &registry,
        };
        assert_eq!(
            validate(&bytes, &context).is_ok(),
            fixture["valid"].as_bool().unwrap(),
            "{file}"
        );
        assert_eq!(
            validate_detailed(&bytes, &context).is_ok(),
            validate(&bytes, &context).is_ok(),
            "diagnostics must not change acceptance: {file}"
        );
    }
}

#[test]
fn diagnostic_codes_distinguish_failures_without_exporting_model_content() {
    let registry = registry();
    let context = Context {
        nonce: "11111111111111111111111111111111",
        purpose: Purpose::Normal,
        parallel: true,
        choice: ToolChoice::Auto,
        registry: &registry,
    };
    for (bytes, expected) in [
        (&b"PRIVATE_NON_JSON"[..], "E_TOOL_ENVELOPE_JSON"),
        (
            &br#"{"PRIVATE_KEY":"PRIVATE_VALUE"}"#[..],
            "E_TOOL_ENVELOPE_SHAPE",
        ),
        (
            include_bytes!("fixtures/invalid-nonce.json").as_slice(),
            "E_TOOL_ENVELOPE_IDENTITY",
        ),
        (
            include_bytes!("fixtures/invalid-unknown-tool.json").as_slice(),
            "E_TOOL_ENVELOPE_UNKNOWN_TOOL",
        ),
        (
            include_bytes!("fixtures/invalid-function-schema.json").as_slice(),
            "E_TOOL_INPUT_SCHEMA",
        ),
        (
            include_bytes!("fixtures/valid-checkpoint.json").as_slice(),
            "E_TOOL_ENVELOPE_PURPOSE",
        ),
    ] {
        let error = validate_detailed(bytes, &context).unwrap_err();
        assert_eq!(error, expected);
        assert!(!error.contains("PRIVATE"));
    }
    let required = Context {
        choice: ToolChoice::Required,
        ..context
    };
    assert_eq!(
        validate_detailed(include_bytes!("fixtures/valid-final.json"), &required).unwrap_err(),
        "E_TOOL_CHOICE"
    );
}

#[test]
fn namespace_and_custom_wire_types_survive() {
    let registry = registry();
    let context = Context {
        nonce: "11111111111111111111111111111111",
        purpose: Purpose::Normal,
        parallel: true,
        choice: ToolChoice::Auto,
        registry: &registry,
    };
    let ValidatedOutput::Calls(calls) =
        validate(include_bytes!("fixtures/valid-namespaced.json"), &context).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        native_call(&calls[0], "item1", "call1")["namespace"],
        "alpha"
    );
    assert_eq!(
        native_call(&calls[1], "item2", "call2")["namespace"],
        "beta"
    );
    let ValidatedOutput::Calls(calls) =
        validate(include_bytes!("fixtures/valid-custom.json"), &context).unwrap()
    else {
        panic!()
    };
    let item = native_call(&calls[0], "item3", "call3");
    assert_eq!(item["type"], "custom_tool_call");
    assert!(item["input"].is_string());
    assert!(item.get("arguments").is_none());
}

#[test]
fn context_constraints_fail_before_emission() {
    let registry = registry();
    let mut context = Context {
        nonce: "11111111111111111111111111111111",
        purpose: Purpose::Normal,
        parallel: false,
        choice: ToolChoice::Auto,
        registry: &registry,
    };
    assert!(validate(include_bytes!("fixtures/valid-namespaced.json"), &context).is_err());
    context.choice = ToolChoice::Required;
    assert!(validate(include_bytes!("fixtures/valid-final.json"), &context).is_err());
    context.choice = ToolChoice::None;
    assert!(validate(include_bytes!("fixtures/valid-function.json"), &context).is_err());
    context.choice = ToolChoice::Exact("tool_0002");
    assert!(validate(include_bytes!("fixtures/valid-function.json"), &context).is_err());
}

#[test]
fn schema_cannot_read_files_or_access_network_and_unknown_grammar_fails() {
    for reference in [
        "file:///secret",
        "http://127.0.0.1:9999/schema",
        "https://example.com/schema",
    ] {
        assert!(
            Registry::from_native(&[
                json!({"type":"function","name":"test","parameters":{"$ref":reference}})
            ])
            .is_err()
        );
    }
    assert!(Registry::from_native(&[json!({"type":"custom","name":"test","format":{"type":"grammar","syntax":"lark","definition":"start: x"}})]).is_err());
    assert!(Registry::from_native(&[json!({"type":"unknown","name":"test"})]).is_err());
}

#[test]
fn native_patch_grammar_accepts_reviewed_windows_asset_without_relaxing_the_protocol() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/apply-patch-grammar.json")).unwrap();
    let lf = fixture["definition"].as_str().unwrap();
    let patch = "*** Begin Patch\n*** Add File: fixture.txt\n+literal payload\n*** End Patch\n";
    for definition in [lf.to_owned(), lf.replace('\n', "\r\n")] {
        let tool = json!({"type":"custom","name":"apply_patch","format":{"type":"grammar","syntax":"lark","definition":definition}});
        let registry = Registry::from_native(std::slice::from_ref(&tool)).unwrap();
        let context = Context {
            nonce: "11111111111111111111111111111111",
            purpose: Purpose::Normal,
            parallel: false,
            choice: ToolChoice::Auto,
            registry: &registry,
        };
        let mut response = json!({"protocol":"webbridge.tool.v1","turn_nonce":context.nonce,"kind":"tool_calls","calls":[{"tool_key":"tool_0001","input":patch}]});
        let ValidatedOutput::Calls(calls) =
            validate(response.to_string().as_bytes(), &context).unwrap()
        else {
            panic!()
        };
        assert_eq!(native_call(&calls[0], "item", "call")["input"], patch);
        response["calls"][0]["input"] = json!(format!("{patch}unframed text"));
        assert!(validate(response.to_string().as_bytes(), &context).is_err());
        let mut changed = tool;
        changed["format"]["definition"] = json!(definition.replace("hunk+", "hunk*"));
        assert!(Registry::from_native(&[changed]).is_err());
    }
}

#[test]
fn rendered_safe_tool_strings_keep_paths_quotes_and_literal_patch_newlines() {
    use cxweb_codex_adapter::request::CanonicalRequest;
    let body = json!({
        "model":"webbridge/fixture", "input":"Read the fixture and patch it.",
        "tools":[
            {"type":"function","name":"exec_command","parameters":{"type":"object","properties":{"cmd":{"type":"string"},"cwd":{"type":"string"}},"required":["cmd","cwd"],"additionalProperties":false}},
            {"type":"custom","name":"apply_patch"}
        ]
    });
    let request = CanonicalRequest::decode(body.to_string().as_bytes()).unwrap();
    let nonce = "11111111111111111111111111111111";
    let prompt = request.browser_prompt(nonce, 100000).unwrap();
    assert!(prompt.contains("exactly one fenced json code block"));
    assert!(prompt.contains("Use standard JSON string escaping inside the code block"));
    let rendered = br#"{"protocol":"webbridge.tool.v1","turn_nonce":"11111111111111111111111111111111","kind":"tool_calls","calls":[{"tool_key":"tool_0001","input":{"cmd":"Get-Content -LiteralPath \u0022C:\u005cfixture\u005cinput.txt\u0022","cwd":"C:\u005cfixture"}},{"tool_key":"tool_0002","input":"\u002a\u002a\u002a Begin Patch\n\u002a\u002a\u002a Add File: output.txt\n+\u0060quoted\u0060 C:\u005cfixture\n\u002a\u002a\u002a End Patch\n"}]}"#;
    let standard = serde_json::from_slice::<Value>(rendered)
        .unwrap()
        .to_string();
    for rendered in [rendered.as_slice(), standard.as_bytes()] {
        let ValidatedOutput::Calls(calls) =
            validate_detailed(rendered, &request.context(nonce)).unwrap()
        else {
            panic!("expected exact tool calls");
        };
        let arguments: Value = serde_json::from_str(
            native_call(&calls[0], "item1", "call1")["arguments"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            arguments,
            json!({"cmd":r#"Get-Content -LiteralPath "C:\fixture\input.txt""#,"cwd":r"C:\fixture"})
        );
        assert_eq!(
            native_call(&calls[1], "item2", "call2")["input"],
            "*** Begin Patch\n*** Add File: output.txt\n+`quoted` C:\\fixture\n*** End Patch\n"
        );
    }
}
