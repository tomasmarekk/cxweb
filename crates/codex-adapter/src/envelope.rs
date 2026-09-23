//! Contextual validation of complete browser output. This module cannot execute tools.
use crate::strict_json;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;

const MAX_ENVELOPE: usize = 8 * 1024 * 1024;
const MAX_SCHEMA: usize = 256 * 1024;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("E_INVALID_TOOL_ENVELOPE")]
    InvalidEnvelope,
    #[error("E_UNSUPPORTED_TOOL")]
    UnsupportedTool,
    #[error("E_TOOL_INPUT_SCHEMA")]
    InvalidInput,
    #[error("E_TOOL_CHOICE")]
    ToolChoice,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolKind {
    Function,
    Custom,
    ToolSearch,
}

pub struct Tool {
    pub key: String,
    pub name: String,
    pub namespace: Option<String>,
    pub kind: ToolKind,
    schema: Option<jsonschema::Validator>,
    definition: Value,
    patch_grammar: bool,
}

pub struct Registry {
    tools: Vec<Tool>,
}

impl Registry {
    pub fn from_native(definitions: &[Value]) -> Result<Self, ProtocolError> {
        let mut tools = Vec::new();
        for definition in definitions {
            if definition["type"] == "namespace" {
                let namespace = definition["name"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or(ProtocolError::UnsupportedTool)?;
                let children = definition["tools"]
                    .as_array()
                    .ok_or(ProtocolError::UnsupportedTool)?;
                for child in children {
                    Self::add(&mut tools, child, Some(namespace))?;
                }
            } else {
                Self::add(&mut tools, definition, None)?;
            }
        }
        let mut identities = HashSet::new();
        if tools
            .iter()
            .any(|t| !identities.insert((t.namespace.clone(), t.name.clone())))
        {
            return Err(ProtocolError::UnsupportedTool);
        }
        Ok(Self { tools })
    }

    fn add(
        tools: &mut Vec<Tool>,
        definition: &Value,
        namespace: Option<&str>,
    ) -> Result<(), ProtocolError> {
        if tools.len() >= 9999 {
            return Err(ProtocolError::UnsupportedTool);
        }
        let search = definition["type"] == "tool_search";
        if search && (namespace.is_some() || definition["execution"] != "client") {
            return Err(ProtocolError::UnsupportedTool);
        }
        let name = if search {
            Some("tool_search")
        } else {
            definition["name"].as_str()
        }
        .filter(|s| !s.is_empty() && !s.contains('\0'))
        .ok_or(ProtocolError::UnsupportedTool)?;
        let patch_grammar = definition["type"] == "custom"
            && name == "apply_patch"
            && definition["format"]["type"] == "grammar"
            && definition["format"]["syntax"] == "lark"
            && definition["format"]["definition"]
                .as_str()
                .is_some_and(crate::patch_grammar::recognizes);
        let (kind, schema) = match definition["type"].as_str() {
            Some("function" | "tool_search") => {
                let schema = definition
                    .get("parameters")
                    .ok_or(ProtocolError::UnsupportedTool)?;
                if schema.to_string().len() > MAX_SCHEMA || has_external_reference(schema) {
                    return Err(ProtocolError::UnsupportedTool);
                }
                let validator = jsonschema::validator_for(schema)
                    .map_err(|_| ProtocolError::UnsupportedTool)?;
                (
                    if search {
                        ToolKind::ToolSearch
                    } else {
                        ToolKind::Function
                    },
                    Some(validator),
                )
            }
            Some("custom")
                if patch_grammar
                    || definition.get("format").is_none_or(|f| f["type"] == "text") =>
            {
                (ToolKind::Custom, None)
            }
            // A grammar is not just a string type. Unqualified grammars fail before submission.
            _ => return Err(ProtocolError::UnsupportedTool),
        };
        tools.push(Tool {
            key: format!("tool_{:04}", tools.len() + 1),
            name: name.to_owned(),
            namespace: namespace.map(str::to_owned),
            kind,
            schema,
            definition: definition.clone(),
            patch_grammar,
        });
        Ok(())
    }

    pub fn prompt_definitions(&self) -> Vec<Value> {
        self.tools
            .iter()
            .map(|t| json!({"tool_key":t.key,"definition":t.definition,"namespace":t.namespace}))
            .collect()
    }

    /// Codex returns deferred definitions in tool_search_output history, not
    /// necessarily in the next request's top-level tools. The caller validates
    /// call/result correlation before including these client-provided schemas.
    pub(crate) fn include_discovered(&mut self, history: &[Value]) -> Result<(), ProtocolError> {
        if !self
            .tools
            .iter()
            .any(|tool| tool.kind == ToolKind::ToolSearch)
        {
            return Ok(());
        }
        let explicit_count = self.tools.len();
        for item in history.iter().filter(|item| {
            item["type"] == "tool_search_output"
                && item["execution"] == "client"
                && item["status"] == "completed"
        }) {
            let definitions = item["tools"]
                .as_array()
                .ok_or(ProtocolError::UnsupportedTool)?;
            for mut tool in Self::from_native(definitions)?.tools {
                if let Some(index) = self.tools.iter().position(|current| {
                    current.name == tool.name && current.namespace == tool.namespace
                }) {
                    // Current explicit definitions win; later discovery replaces
                    // earlier discovery for the same identity without changing keys.
                    if index >= explicit_count {
                        tool.key = self.tools[index].key.clone();
                        self.tools[index] = tool;
                    }
                } else {
                    if self.tools.len() >= 9999 {
                        return Err(ProtocolError::UnsupportedTool);
                    }
                    tool.key = format!("tool_{:04}", self.tools.len() + 1);
                    self.tools.push(tool);
                }
            }
        }
        Ok(())
    }

    pub fn key_for(&self, name: &str, namespace: Option<&str>) -> Option<&str> {
        self.tools
            .iter()
            .find(|t| t.name == name && t.namespace.as_deref() == namespace)
            .map(|t| t.key.as_str())
    }
}

pub(crate) fn has_external_reference(schema: &Value) -> bool {
    match schema {
        Value::Object(o) => o.iter().any(|(key, v)| {
            (matches!(key.as_str(), "$ref" | "$dynamicRef" | "$recursiveRef")
                && v.as_str().is_none_or(|s| !s.starts_with('#')))
                || (key == "$id" && v.as_str().is_some_and(|s| !s.starts_with('#')))
                || has_external_reference(v)
        }),
        Value::Array(a) => a.iter().any(has_external_reference),
        _ => false,
    }
}

#[derive(Clone, Copy)]
pub enum Purpose {
    Normal,
    Compaction,
}
pub enum ToolChoice<'a> {
    Auto,
    None,
    Required,
    Exact(&'a str),
}
pub struct Context<'a> {
    pub nonce: &'a str,
    pub purpose: Purpose,
    pub parallel: bool,
    pub choice: ToolChoice<'a>,
    pub registry: &'a Registry,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Envelope {
    Final {
        protocol: String,
        turn_nonce: String,
        text: String,
    },
    ToolCalls {
        protocol: String,
        turn_nonce: String,
        calls: Vec<Call>,
    },
    Checkpoint {
        protocol: String,
        turn_nonce: String,
        summary: CheckpointSummary,
    },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CheckpointSummary {
    // Accept previously produced checkpoints while new prompts use one JSON
    // layer. Semantic validation remains in Summary::parse for both forms.
    Encoded(String),
    Structured(serde_json::Map<String, Value>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Call {
    tool_key: String,
    input: Value,
}

#[derive(Debug)]
pub struct ValidatedCall {
    pub native_name: String,
    pub namespace: Option<String>,
    pub kind: ToolKind,
    pub input: Value,
}
#[derive(Debug)]
pub enum ValidatedOutput {
    Final(String),
    Calls(Vec<ValidatedCall>),
    Checkpoint(String),
}

pub fn validate(bytes: &[u8], context: &Context<'_>) -> Result<ValidatedOutput, ProtocolError> {
    let envelope = decode_envelope(bytes, context).map_err(|_| ProtocolError::InvalidEnvelope)?;
    validate_body(envelope, context)
}

/// Fixed diagnostic codes only: never expose a response, argument, schema,
/// nonce or parser error through native errors or exported diagnostics.
pub fn validate_detailed(
    bytes: &[u8],
    context: &Context<'_>,
) -> Result<ValidatedOutput, &'static str> {
    let envelope = decode_envelope(bytes, context)?;
    validate_body(envelope, context).map_err(|error| match error {
        ProtocolError::InvalidEnvelope => "E_TOOL_ENVELOPE_PURPOSE",
        ProtocolError::UnsupportedTool => "E_TOOL_ENVELOPE_UNKNOWN_TOOL",
        ProtocolError::InvalidInput => "E_TOOL_INPUT_SCHEMA",
        ProtocolError::ToolChoice => "E_TOOL_CHOICE",
    })
}

fn decode_envelope(bytes: &[u8], context: &Context<'_>) -> Result<Envelope, &'static str> {
    let value = strict_json::parse_detailed(bytes, MAX_ENVELOPE).map_err(|code| match code {
        "E_INVALID_JSON_ESCAPE" => "E_TOOL_ENVELOPE_JSON_ESCAPE",
        "E_INVALID_JSON_CONTROL" => "E_TOOL_ENVELOPE_JSON_CONTROL",
        _ => "E_TOOL_ENVELOPE_JSON",
    })?;
    let envelope: Envelope = serde_json::from_value(value).map_err(|_| "E_TOOL_ENVELOPE_SHAPE")?;
    let (protocol, nonce) = match &envelope {
        Envelope::Final {
            protocol,
            turn_nonce,
            ..
        }
        | Envelope::ToolCalls {
            protocol,
            turn_nonce,
            ..
        }
        | Envelope::Checkpoint {
            protocol,
            turn_nonce,
            ..
        } => (protocol, turn_nonce),
    };
    if protocol != "webbridge.tool.v1"
        || nonce != context.nonce
        || nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("E_TOOL_ENVELOPE_IDENTITY");
    }
    Ok(envelope)
}

fn validate_body(
    envelope: Envelope,
    context: &Context<'_>,
) -> Result<ValidatedOutput, ProtocolError> {
    match (context.purpose, envelope) {
        (Purpose::Compaction, Envelope::Checkpoint { summary, .. }) => {
            let summary = match summary {
                CheckpointSummary::Encoded(text) => text,
                CheckpointSummary::Structured(object) => {
                    serde_json::to_string(&object).map_err(|_| ProtocolError::InvalidEnvelope)?
                }
            };
            if summary.is_empty() || summary.len() > 2 * 1024 * 1024 {
                return Err(ProtocolError::InvalidEnvelope);
            }
            Ok(ValidatedOutput::Checkpoint(summary))
        }
        (Purpose::Normal, Envelope::Final { text, .. }) => {
            if matches!(context.choice, ToolChoice::Required | ToolChoice::Exact(_)) {
                return Err(ProtocolError::ToolChoice);
            }
            Ok(ValidatedOutput::Final(text))
        }
        (Purpose::Normal, Envelope::ToolCalls { calls, .. }) => {
            if calls.is_empty()
                || calls.len() > 16
                || (!context.parallel && calls.len() > 1)
                || matches!(context.choice, ToolChoice::None)
            {
                return Err(ProtocolError::ToolChoice);
            }
            let mut validated = Vec::with_capacity(calls.len());
            for call in calls {
                let tool = context
                    .registry
                    .tools
                    .iter()
                    .find(|t| t.key == call.tool_key)
                    .ok_or(ProtocolError::UnsupportedTool)?;
                if let ToolChoice::Exact(key) = context.choice
                    && key != tool.key
                {
                    return Err(ProtocolError::ToolChoice);
                }
                match tool.kind {
                    ToolKind::Function | ToolKind::ToolSearch
                        if !call.input.is_object()
                            || !tool
                                .schema
                                .as_ref()
                                .is_some_and(|v| v.is_valid(&call.input)) =>
                    {
                        return Err(ProtocolError::InvalidInput);
                    }
                    ToolKind::Custom if !call.input.is_string() => {
                        return Err(ProtocolError::InvalidInput);
                    }
                    _ => {}
                }
                if tool.patch_grammar
                    && !call.input.as_str().is_some_and(crate::patch_grammar::valid)
                {
                    return Err(ProtocolError::InvalidInput);
                }
                validated.push(ValidatedCall {
                    native_name: tool.name.clone(),
                    namespace: tool.namespace.clone(),
                    kind: tool.kind.clone(),
                    input: call.input,
                });
            }
            Ok(ValidatedOutput::Calls(validated))
        }
        _ => Err(ProtocolError::InvalidEnvelope),
    }
}

/// IDs are supplied by the durable delivery ledger, never by model output.
pub fn native_call(call: &ValidatedCall, item_id: &str, call_id: &str) -> Value {
    let mut item = match call.kind {
        ToolKind::Function => {
            json!({"type":"function_call","id":item_id,"call_id":call_id,"name":call.native_name,"arguments":call.input.to_string(),"status":"completed"})
        }
        ToolKind::Custom => {
            json!({"type":"custom_tool_call","id":item_id,"call_id":call_id,"name":call.native_name,"input":call.input,"status":"completed"})
        }
        ToolKind::ToolSearch => {
            json!({"type":"tool_search_call","id":item_id,"call_id":call_id,"execution":"client","arguments":call.input,"status":"completed"})
        }
    };
    if let Some(namespace) = &call.namespace {
        item["namespace"] = json!(namespace);
    }
    item
}
