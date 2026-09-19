//! Lossless native catalog augmentation. Synthetic entries are diagnostic only.
use cxweb_domain::OWNED_MODEL_PREFIX;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("unsupported catalog envelope")]
    InvalidEnvelope,
    #[error("catalog contains a conflicting owned model")]
    Collision,
    #[error("only owned models can be appended")]
    ForeignModel,
}

pub fn append_owned(mut native: Value, owned: &[Value]) -> Result<Value, CatalogError> {
    let models = native
        .get_mut("models")
        .and_then(Value::as_array_mut)
        .ok_or(CatalogError::InvalidEnvelope)?;
    for model in owned {
        let slug = model
            .get("slug")
            .and_then(Value::as_str)
            .filter(|s| s.starts_with(OWNED_MODEL_PREFIX))
            .ok_or(CatalogError::ForeignModel)?;
        if models
            .iter()
            .any(|m| m.get("slug").and_then(Value::as_str) == Some(slug))
        {
            return Err(CatalogError::Collision);
        }
        models.push(model.clone());
    }
    Ok(native)
}

/// Caller must separately partition caches by verified account/client scope.
pub fn etag(bytes: &[u8]) -> String {
    format!("\"{:x}\"", Sha256::digest(bytes))
}

/// Diagnostic metadata based on the 0.153.4 ModelInfo contract. Never advertise
/// this synthetic route in a real account catalog or treat it as certification.
pub fn synthetic_model() -> Value {
    json!({
        "slug": "webbridge/diagnostic", "display_name": "ChatGPT Web · Diagnostic",
        "description": "ChatGPT Web · Diagnostic. Local synthetic probe; no remote inference",
        "default_reasoning_level": "medium",
        "supported_reasoning_levels": [{"effort": "medium", "description": "Diagnostic"}],
        "shell_type": "disabled", "visibility": "list", "supported_in_api": true,
        "priority": 1000, "availability_nux": null, "upgrade": null,
        "base_instructions": "Respond to the diagnostic request.",
        "support_verbosity": false, "default_verbosity": null,
        "apply_patch_tool_type": null, "truncation_policy": {"mode": "bytes", "limit": 10000},
        "supports_reasoning_summary": false, "supports_reasoning_summary_parameter": false,
        "supports_parallel_tool_calls": false, "input_modalities": ["text"],
        "tool_mode": "direct", "experimental_supported_tools": []
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_native_fields_order_and_unknown_metadata() {
        let original = json!({"models":[{"slug":"native", "future":{"a":[1,2]}}], "unknown": 9});
        let merged = append_owned(original.clone(), &[synthetic_model()]).unwrap();
        assert_eq!(merged["models"][0], original["models"][0]);
        assert_eq!(merged["unknown"], 9);
        assert_eq!(merged["models"].as_array().unwrap().len(), 2);
    }
    #[test]
    fn rejects_collision_and_foreign_append() {
        assert!(append_owned(json!({"models":[synthetic_model()]}), &[synthetic_model()]).is_err());
        assert!(append_owned(json!({"models":[]}), &[json!({"slug":"native"})]).is_err());
    }
}
