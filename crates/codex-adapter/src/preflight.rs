//! Sanitized assessment of native configuration/status RPC responses.
//! Never serialize arbitrary configuration, account metadata, URLs or errors.
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    Subscription,
    ApiKey,
    Bedrock,
    SignedOut,
    Unknown,
}

#[derive(Debug, Serialize)]
pub struct Assessment {
    pub configuration_compatible: bool,
    pub auth_mode: AuthMode,
    pub active_layers: Vec<&'static str>,
    pub disabled_layer_count: usize,
    pub conflicts: Vec<&'static str>,
}

fn present(value: &Value, key: &str) -> bool {
    value.get(key).is_some_and(|v| !v.is_null())
}

fn routing_conflicts(config: &Value, conflicts: &mut BTreeSet<&'static str>) {
    for key in [
        "openai_base_url",
        "chatgpt_base_url",
        "model_catalog_json",
        "profile",
        "profiles",
    ] {
        if present(config, key) {
            conflicts.insert(key);
        }
    }
    if present(config, "model_provider") && config["model_provider"] != "openai" {
        conflicts.insert("model_provider");
    }
    if config
        .get("model_providers")
        .is_some_and(|v| v.get("openai").is_some())
    {
        conflicts.insert("reserved_provider_override");
    }
}

/// The caller validates the user layer's canonical file against its selected
/// target. This function deliberately cannot authorize a config transaction.
pub fn assess(
    config: &Value,
    requirements: &Value,
    account: &Value,
) -> Result<Assessment, &'static str> {
    let effective = config
        .get("config")
        .filter(|v| v.is_object())
        .ok_or("E_PREFLIGHT_SCHEMA")?;
    let origins = config
        .get("origins")
        .and_then(Value::as_object)
        .ok_or("E_PREFLIGHT_SCHEMA")?;
    let layers = config
        .get("layers")
        .and_then(Value::as_array)
        .filter(|v| v.len() <= 128)
        .ok_or("E_PREFLIGHT_SCHEMA")?;
    let requirements = requirements
        .get("requirements")
        .filter(|v| v.is_object() || v.is_null())
        .ok_or("E_PREFLIGHT_SCHEMA")?;
    let requires_auth = account
        .get("requiresOpenaiAuth")
        .and_then(Value::as_bool)
        .ok_or("E_PREFLIGHT_SCHEMA")?;
    let auth_mode = match account.get("account") {
        None | Some(Value::Null) => AuthMode::SignedOut,
        Some(value) => match value.get("type").and_then(Value::as_str) {
            Some("chatgpt") if requires_auth => AuthMode::Subscription,
            Some("apiKey") => AuthMode::ApiKey,
            Some("amazonBedrock") => AuthMode::Bedrock,
            _ => AuthMode::Unknown,
        },
    };
    let mut conflicts = BTreeSet::new();
    if auth_mode != AuthMode::Subscription {
        conflicts.insert("subscription_auth_required");
    }
    // config/read includes serialized defaults (including an empty profiles
    // table and a ChatGPT URL). Only provenance-backed values are declarations.
    let declared: serde_json::Map<String, Value> = [
        "openai_base_url",
        "chatgpt_base_url",
        "model_catalog_json",
        "profile",
        "profiles",
        "model_provider",
        "model_providers",
    ]
    .into_iter()
    .filter(|key| {
        let prefix = format!("{key}.");
        origins
            .keys()
            .any(|origin| origin == *key || origin.starts_with(&prefix))
    })
    .filter_map(|key| {
        effective
            .get(key)
            .map(|value| (key.to_owned(), value.clone()))
    })
    .collect();
    routing_conflicts(&Value::Object(declared), &mut conflicts);
    if present(effective, "model_provider") && effective["model_provider"] != "openai" {
        conflicts.insert("model_provider");
    }
    let mut active_layers = BTreeSet::new();
    let mut disabled_layer_count = 0;
    let mut user_count = 0;
    for layer in layers {
        let contents = layer
            .get("config")
            .filter(|v| v.is_object())
            .ok_or("E_PREFLIGHT_SCHEMA")?;
        let source = layer
            .get("name")
            .filter(|v| v.is_object())
            .ok_or("E_PREFLIGHT_SCHEMA")?;
        if present(layer, "disabledReason") {
            if !layer["disabledReason"].is_string() {
                return Err("E_PREFLIGHT_SCHEMA");
            }
            disabled_layer_count += 1;
            continue;
        }
        let kind = match source.get("type").and_then(Value::as_str) {
            Some("packagedDefaults") => "packaged_defaults",
            Some("system") => "system",
            Some("enterpriseManaged") => "enterprise_managed",
            Some("user") => {
                if present(source, "profile") {
                    conflicts.insert("selected_profile");
                    "profile"
                } else {
                    user_count += 1;
                    "user"
                }
            }
            Some("project") => "project",
            Some("sessionFlags") => "session_flags",
            Some("mdm" | "legacyManagedConfigTomlFromMdm" | "legacyManagedConfigTomlFromFile") => {
                "managed"
            }
            _ => {
                conflicts.insert("unknown_config_layer");
                "unknown"
            }
        };
        active_layers.insert(kind);
        // A shadowed route can become active after a higher layer changes. Do
        // not overwrite or silently chain any pre-existing routing declaration.
        routing_conflicts(contents, &mut conflicts);
    }
    if user_count != 1 {
        conflicts.insert("ambiguous_user_config");
    }
    // Other approval/sandbox/tool constraints remain native-owned. A managed
    // route, catalog or residency requires a separately reviewed integration.
    for key in ["chatgptBaseUrl", "modelCatalogJson", "enforceResidency"] {
        if present(requirements, key) {
            conflicts.insert("managed_routing");
        }
    }
    Ok(Assessment {
        configuration_compatible: conflicts.is_empty(),
        auth_mode,
        active_layers: active_layers.into_iter().collect(),
        disabled_layer_count,
        conflicts: conflicts.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn config() -> Value {
        json!({"config":{"model_provider":"openai","chatgpt_base_url":"native-default","profiles":{}},"origins":{},"layers":[{"name":{"type":"user","file":"fixture/config.toml","profile":null},"config":{}}]})
    }
    fn account() -> Value {
        json!({"account":{"type":"chatgpt","email":"PRIVATE_EMAIL","planType":"pro"},"requiresOpenaiAuth":true})
    }

    #[test]
    fn native_policy_is_preserved_and_diagnostics_do_not_export_values() {
        let mut config = config();
        config["config"]["developer_instructions"] = json!("PRIVATE_INSTRUCTIONS");
        let result = assess(&config, &json!({"requirements":{"allowedSandboxModes":["workspace-write"],"allowedApprovalPolicies":["on-request"]}}), &account()).unwrap();
        assert!(result.configuration_compatible);
        assert_eq!(result.auth_mode, AuthMode::Subscription);
        assert!(!serde_json::to_string(&result).unwrap().contains("PRIVATE"));
        config["origins"]["chatgpt_base_url"] = json!({"name":{"type":"user"},"version":"fixture"});
        let result = assess(&config, &json!({"requirements":null}), &account()).unwrap();
        assert!(result.conflicts.contains(&"chatgpt_base_url"));
    }
    #[test]
    fn every_active_layer_and_native_auth_mode_are_checked() {
        for kind in [
            "project",
            "system",
            "sessionFlags",
            "enterpriseManaged",
            "legacyManagedConfigTomlFromFile",
        ] {
            let mut config = config();
            config["layers"]
                .as_array_mut()
                .unwrap()
                .push(json!({"name":{"type":kind},"config":{"openai_base_url":"PRIVATE_URL"}}));
            let result = assess(&config, &json!({"requirements":null}), &account()).unwrap();
            assert!(!result.configuration_compatible);
            assert!(result.conflicts.contains(&"openai_base_url"));
            assert!(
                !serde_json::to_string(&result)
                    .unwrap()
                    .contains("PRIVATE_URL")
            );
        }
        for account in [
            json!({"account":null,"requiresOpenaiAuth":true}),
            json!({"account":{"type":"apiKey"},"requiresOpenaiAuth":true}),
            json!({"account":{"type":"future"},"requiresOpenaiAuth":true}),
        ] {
            assert!(
                !assess(&config(), &json!({"requirements":null}), &account)
                    .unwrap()
                    .configuration_compatible
            );
        }
    }
    #[test]
    fn disabled_projects_do_not_override_but_profiles_managed_routes_and_unknown_layers_block() {
        let mut config = config();
        config["layers"].as_array_mut().unwrap().push(json!({"name":{"type":"project"},"config":{"model_provider":"other"},"disabledReason":"PRIVATE_REASON"}));
        let result = assess(&config, &json!({"requirements":null}), &account()).unwrap();
        assert!(result.configuration_compatible);
        assert_eq!(result.disabled_layer_count, 1);
        for name in [
            json!({"type":"user","profile":"PRIVATE_PROFILE"}),
            json!({"type":"future"}),
        ] {
            config["layers"][0]["name"] = name;
            assert!(
                !assess(&config, &json!({"requirements":null}), &account())
                    .unwrap()
                    .configuration_compatible
            );
        }
        for key in ["chatgptBaseUrl", "modelCatalogJson", "enforceResidency"] {
            assert!(
                !assess(
                    &super::tests::config(),
                    &json!({"requirements":{key:"PRIVATE_VALUE"}}),
                    &account()
                )
                .unwrap()
                .configuration_compatible
            );
        }
        assert!(
            assess(
                &json!({"config":{}}),
                &json!({"requirements":null}),
                &account()
            )
            .is_err()
        );
    }
}
