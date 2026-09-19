//! Local validation of structured final text. No schema retrieval or repair.
use crate::{envelope::ValidatedOutput, strict_json};
use serde_json::Value;

pub(crate) struct OutputFormat {
    pub definition: Value,
    validator: Option<jsonschema::Validator>,
}

impl OutputFormat {
    pub fn decode(text: Option<&Value>) -> Result<Self, &'static str> {
        let invalid = "E_UNSUPPORTED_OUTPUT_FORMAT";
        let format = match text {
            None | Some(Value::Null) => None,
            Some(Value::Object(text)) => text.get("format"),
            _ => return Err(invalid),
        };
        let Some(format) = format.filter(|value| !value.is_null()) else {
            return Ok(Self {
                definition: Value::Null,
                validator: None,
            });
        };
        let object = format.as_object().ok_or(invalid)?;
        match object.get("type").and_then(Value::as_str) {
            Some("text") if object.len() == 1 => Ok(Self {
                definition: format.clone(),
                validator: None,
            }),
            Some("json_schema") => {
                if object.keys().any(|key| {
                    !matches!(
                        key.as_str(),
                        "type" | "name" | "schema" | "strict" | "description"
                    )
                }) || object
                    .get("name")
                    .and_then(Value::as_str)
                    .is_none_or(|name| {
                        name.is_empty()
                            || name.len() > 64
                            || !name.bytes().all(|byte| {
                                byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
                            })
                    })
                    || object
                        .get("strict")
                        .is_some_and(|value| !value.is_null() && !value.is_boolean())
                    || object
                        .get("description")
                        .is_some_and(|value| !value.is_string())
                {
                    return Err(invalid);
                }
                let schema = object.get("schema").ok_or(invalid)?;
                if schema.to_string().len() > 256 * 1024
                    || crate::envelope::has_external_reference(schema)
                {
                    return Err(invalid);
                }
                let validator = jsonschema::validator_for(schema).map_err(|_| invalid)?;
                Ok(Self {
                    definition: format.clone(),
                    validator: Some(validator),
                })
            }
            _ => Err(invalid),
        }
    }

    pub fn validate(&self, output: &ValidatedOutput) -> Result<(), &'static str> {
        if let (Some(validator), ValidatedOutput::Final(text)) = (&self.validator, output) {
            let value = strict_json::parse(text.as_bytes(), 8 * 1024 * 1024)
                .map_err(|_| "E_OUTPUT_SCHEMA")?;
            if !validator.is_valid(&value) {
                return Err("E_OUTPUT_SCHEMA");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn title_format() -> Value {
        json!({"type":"json_schema","name":"thread_title","strict":true,"schema":{"type":"object","properties":{"title":{"type":"string","minLength":1,"maxLength":36}},"required":["title"],"additionalProperties":false}})
    }

    #[test]
    fn title_contract_accepts_only_valid_complete_json_without_repair() {
        let format = title_format();
        let parsed = OutputFormat::decode(Some(&json!({"format":format}))).unwrap();
        assert_eq!(parsed.definition, format);
        assert!(
            parsed
                .validate(&ValidatedOutput::Final(
                    r#"{"title":"Verify background response"}"#.into()
                ))
                .is_ok()
        );
        for text in [
            "plain answer",
            "```json\n{}\n```",
            r#"{"title":""}"#,
            r#"{"title":1}"#,
            r#"{"title":"valid","extra":true}"#,
            r#"{"title":"valid","title":"duplicate"}"#,
            r#"{"title":"This title exceeds the thirty-six character limit"}"#,
        ] {
            assert_eq!(
                parsed.validate(&ValidatedOutput::Final(text.into())),
                Err("E_OUTPUT_SCHEMA")
            );
        }
        // Structured final output does not change valid intermediate tool calls.
        assert!(parsed.validate(&ValidatedOutput::Calls(vec![])).is_ok());
    }

    #[test]
    fn malformed_formats_and_external_schema_references_fail_before_send() {
        for format in [
            json!({"type":"json_schema","name":"title","schema":{"$ref":"https://example.invalid/schema"}}),
            json!({"type":"json_schema","name":"title","schema":{"type":123}}),
            json!({"type":"json_schema","schema":{}}),
            json!({"type":"json_schema","name":"title","strict":"true","schema":{}}),
            json!({"type":"json_schema","name":"title","schema":{},"unknown":true}),
            json!({"type":"text","schema":{}}),
            json!({"type":"unknown"}),
            json!(42),
        ] {
            assert!(OutputFormat::decode(Some(&json!({"format":format}))).is_err());
        }
        let format = json!({"type":"json_schema","name":"local","schema":{"$defs":{"text":{"type":"string"}},"$ref":"#/$defs/text"}});
        assert!(OutputFormat::decode(Some(&json!({"format":format}))).is_ok());
    }

    #[test]
    fn absent_or_null_format_means_unstructured_text() {
        for text in [
            Value::Null,
            json!({}),
            json!({"format":null}),
            json!({"format":{"type":"text"}}),
        ] {
            assert!(
                OutputFormat::decode(Some(&text))
                    .unwrap()
                    .validate(&ValidatedOutput::Final("plain answer".into()))
                    .is_ok()
            );
        }
        assert!(OutputFormat::decode(None).is_ok());
        assert!(OutputFormat::decode(Some(&json!("malformed"))).is_err());
    }
}
