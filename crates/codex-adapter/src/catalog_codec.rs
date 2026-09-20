//! Reviewed catalog encodings, independent of native entries and credentials.
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CatalogCodec {
    Cli01551,
    App01550Alpha92,
}

impl CatalogCodec {
    pub fn for_build(build: &str) -> Option<Self> {
        match build {
            "0.155.1" => Some(Self::Cli01551),
            "0.155.0-alpha.9.2" => Some(Self::App01550Alpha92),
            _ => None,
        }
    }
    /// Query versions omit prerelease suffixes; the native User-Agent includes
    /// the full build. Require agreement so an unknown alpha cannot inherit a
    /// reviewed codec merely by sharing its major/minor/patch query version.
    /// This is compatibility selection, not client authentication.
    pub fn select(query_version: &str, user_agent: &str) -> Option<Self> {
        if user_agent.len() > 4096 || !user_agent.is_ascii() {
            return None;
        }
        // The native desktop originator is "Codex Desktop" and legitimately
        // includes a space. The slash, not the first whitespace, separates its
        // name from the backend build; later UA tokens describe the OS/client.
        let (originator, remainder) = user_agent.split_once('/')?;
        let version = remainder.split_ascii_whitespace().next()?;
        if originator.trim().is_empty() || originator.len() > 128 {
            return None;
        }
        match (query_version, version) {
            ("0.155.1", "0.155.1") => Some(Self::Cli01551),
            ("0.155.0", "0.155.0-alpha.9.2") => Some(Self::App01550Alpha92),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Cli01551 => "codex-model-info-0.155.1",
            Self::App01550Alpha92 => "codex-model-info-0.155.0-alpha.9.2",
        }
    }

    /// The same budget must govern execution. Activation still requires live
    /// qualification; a diagnostic estimate must not certify provider capacity.
    pub fn encode_with_context_budget(
        self,
        route: &CatalogRoute,
        budget: crate::context_budget::LocalContextBudget,
    ) -> Result<Value, &'static str> {
        let mut entry = self.encode(route)?;
        entry["context_window"] = json!(budget.estimated_tokens());
        entry["max_context_window"] = json!(budget.estimated_tokens());
        entry["auto_compact_token_limit"] = json!(budget.estimated_tokens() * 9 / 10);
        entry["description"] = json!(format!(
            "{}. Local estimated context budget: {} tokens; encoded input ceiling: {} bytes. Not a ChatGPT capacity claim.",
            entry["description"].as_str().ok_or("E_CATALOG_ROUTE")?,
            budget.estimated_tokens(),
            budget.normal_bytes(),
        ));
        Ok(entry)
    }

    /// The activation owner must separately qualify the route and these exact
    /// capabilities. Construct a row from observations, never a native template.
    pub fn encode(self, route: &CatalogRoute) -> Result<Value, &'static str> {
        let suffix = route
            .id
            .strip_prefix(cxweb_domain::OWNED_MODEL_PREFIX)
            .ok_or("E_CATALOG_ROUTE")?;
        if suffix.is_empty()
            || route.id.len() > 256
            || !suffix
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._/-".contains(&b))
            || !suffix.as_bytes()[0].is_ascii_alphanumeric()
        {
            return Err("E_CATALOG_ROUTE");
        }
        if route.observed_label.trim() != route.observed_label
            || route.observed_label.is_empty()
            || route.observed_label.len() > 160
            || !route
                .observed_label
                .chars()
                .all(|c| c.is_ascii_graphic() || c == ' ' || c == '·')
            || !matches!(
                route.effort.as_str(),
                "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
            )
        {
            return Err("E_CATALOG_OBSERVATION");
        }
        // An unknown browser token capacity is not the native model's capacity.
        // No fabricated context, compaction hash, upgrade, native instructions,
        // service tier, multi-agent policy or image capability is copied.
        let label = format!("ChatGPT Web · {}", route.observed_label);
        Ok(json!({
            "slug":route.id,"display_name":label,
            "description":format!("{label}. Hosted search unavailable"),
            "default_reasoning_level":route.effort,
            "supported_reasoning_levels":[{"effort":route.effort,"description":route.observed_label}],
            "shell_type":if route.coding {"unified_exec"} else {"disabled"},
            "apply_patch_tool_type":if route.coding {Some("freeform")} else {None},
            "visibility":"list","supported_in_api":true,"priority":1000,
            "availability_nux":null,"upgrade":null,
            "base_instructions":"Follow the user's task and applicable instructions. Use client-provided tools when needed. Tool execution is performed by the client. Report unavailable capabilities accurately.",
            "support_verbosity":false,"default_verbosity":null,
            "supports_reasoning_summary_parameter":false,
            "truncation_policy":{"mode":"bytes","limit":10000},
            "input_modalities":["text"],"tool_mode":"direct",
            "experimental_supported_tools":[],"additional_speed_tiers":[],"service_tiers":[],
            "supports_search_tool":false,"supports_experimental_context":false,"use_responses_lite":false
        }))
    }
}

#[derive(Clone)]
pub struct CatalogRoute {
    pub id: String,
    pub observed_label: String,
    /// A route fixes an observed family/effort pair; the picker must not offer
    /// effort values that would silently select a different browser route.
    pub effort: String,
    pub coding: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route() -> CatalogRoute {
        CatalogRoute {
            id: "webbridge/observed-xhigh".into(),
            observed_label: "Observed model · Extra High".into(),
            effort: "xhigh".into(),
            coding: false,
        }
    }

    #[test]
    fn local_budget_metadata_is_explicit_and_does_not_claim_provider_usage() {
        use crate::context_budget::{LocalContextBudget, MAX_PROMPT_BYTES};
        for codec in [CatalogCodec::Cli01551, CatalogCodec::App01550Alpha92] {
            let budget = LocalContextBudget::new(128 * 1024, 256 * 1024).unwrap();
            let entry = codec.encode_with_context_budget(&route(), budget).unwrap();
            assert_eq!(entry["context_window"], 32768);
            assert_eq!(entry["max_context_window"], 32768);
            assert_eq!(entry["auto_compact_token_limit"], 32768 * 9 / 10);
            assert!(
                entry["description"]
                    .as_str()
                    .unwrap()
                    .contains("Local estimated context budget")
            );
            assert!(entry.get("usage").is_none());
            assert!(
                codec
                    .encode(&route())
                    .unwrap()
                    .get("context_window")
                    .is_none()
            );
        }
        for (normal, summary) in [
            (0, 8192),
            (8192, 8192),
            (8192, 4096),
            (8192, MAX_PROMPT_BYTES + 1),
        ] {
            assert_eq!(
                LocalContextBudget::new(normal, summary).err(),
                Some("E_CONTEXT_BUDGET_CONFIG")
            );
        }
    }

    #[test]
    fn full_build_and_whole_query_must_agree() {
        for (query, agent, expected) in [
            (
                "0.155.1",
                "codex_cli_rs/0.155.1 (Windows 11; x64)",
                Some(CatalogCodec::Cli01551),
            ),
            (
                "0.155.0",
                "codex_desktop/0.155.0-alpha.9.2 (Windows 11)",
                Some(CatalogCodec::App01550Alpha92),
            ),
            ("0.155.0", "codex_desktop/0.155.0-alpha.9.3", None),
            ("0.155.1", "codex_cli_rs/0.155.0-alpha.9.2", None),
            ("0.155.0", "codex_cli_rs/0.155.1", None),
            ("0.156.0", "codex_cli_rs/0.156.0", None),
            ("0.155.1", "unknown", None),
            ("0.155.1", "/0.155.1", None),
        ] {
            assert_eq!(CatalogCodec::select(query, agent), expected);
        }
    }

    #[test]
    fn codecs_encode_observations_without_native_or_unverified_capabilities() {
        for codec in [CatalogCodec::Cli01551, CatalogCodec::App01550Alpha92] {
            let mut route = route();
            let text = codec.encode(&route).unwrap();
            assert_eq!(
                text["display_name"],
                "ChatGPT Web · Observed model · Extra High"
            );
            assert_eq!(
                text["supported_reasoning_levels"].as_array().unwrap().len(),
                1
            );
            assert_eq!(text["shell_type"], "disabled");
            assert!(text["apply_patch_tool_type"].is_null());
            assert_eq!(text["input_modalities"], json!(["text"]));
            for key in [
                "context_window",
                "max_context_window",
                "auto_compact_token_limit",
                "comp_hash",
                "multi_agent_version",
                "guardian",
                "model_messages",
            ] {
                assert!(text.get(key).is_none());
            }
            route.coding = true;
            let coding = codec.encode(&route).unwrap();
            assert_eq!(coding["shell_type"], "unified_exec");
            assert_eq!(coding["apply_patch_tool_type"], "freeform");
        }
    }

    #[test]
    fn foreign_ids_localized_labels_and_unknown_efforts_are_not_published() {
        for variant in 0..5 {
            let mut route = route();
            match variant {
                0 => route.id = "native-model".into(),
                1 => route.id = "webbridge/".into(),
                2 => route.observed_label = "Localized \u{010d} label".into(),
                3 => route.effort = "unobserved".into(),
                _ => route.observed_label = "Observed\nInjected row".into(),
            }
            assert!(CatalogCodec::Cli01551.encode(&route).is_err());
        }
    }
}
