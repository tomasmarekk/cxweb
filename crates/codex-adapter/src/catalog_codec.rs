//! Reviewed catalog encodings, independent of native entries and credentials.
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CatalogCodec {
    CliModelInfoV1,
    AppModelInfoV1,
}

impl CatalogCodec {
    /// Builds are diagnostic labels, not a compatibility allowlist. Both
    /// clients consume the same ModelInfo wire representation.
    pub fn for_build(build: &str) -> Option<Self> {
        Self::build_version(build)?;
        Some(if build.contains('-') {
            Self::AppModelInfoV1
        } else {
            Self::CliModelInfoV1
        })
    }

    fn build_version(build: &str) -> Option<&str> {
        if build.len() > 128 || !build.is_ascii() {
            return None;
        }
        let core = build.split(['-', '+']).next()?;
        let numbers: Vec<_> = core.split('.').collect();
        if numbers.len() != 3
            || numbers
                .iter()
                .any(|n| n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()))
        {
            return None;
        }
        if build.len() > core.len() {
            let suffix = &build[core.len() + 1..];
            if suffix.is_empty()
                || !suffix
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b".-+".contains(&b))
            {
                return None;
            }
        }
        Some(core)
    }

    /// Validate request metadata consistency without pinning client releases.
    /// The response envelope is checked separately before catalog augmentation.
    pub fn select(query_version: &str, user_agent: &str) -> Option<Self> {
        let (_, build) = Self::user_agent_parts(user_agent)?;
        if query_version != Self::build_version(build)? {
            return None;
        }
        Self::from_user_agent(user_agent)
    }

    /// Client family is health metadata, never authorization or qualification.
    pub fn from_user_agent(user_agent: &str) -> Option<Self> {
        let (originator, build) = Self::user_agent_parts(user_agent)?;
        Self::build_version(build)?;
        Some(if originator.to_ascii_lowercase().contains("desktop") {
            Self::AppModelInfoV1
        } else {
            Self::CliModelInfoV1
        })
    }

    fn user_agent_parts(user_agent: &str) -> Option<(&str, &str)> {
        if user_agent.len() > 4096 || !user_agent.is_ascii() {
            return None;
        }
        let (originator, remainder) = user_agent.split_once('/')?;
        let version = remainder.split_ascii_whitespace().next()?;
        if originator.trim().is_empty() || originator.len() > 128 {
            return None;
        }
        Some((originator, version))
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::CliModelInfoV1 => "codex-model-info-v1-cli",
            Self::AppModelInfoV1 => "codex-model-info-v1-app",
        }
    }

    /// Historical encryption domain labels are retained so saved task
    /// checkpoints remain readable. These do not restrict client builds.
    pub fn checkpoint_id(self) -> &'static str {
        match self {
            Self::CliModelInfoV1 => "codex-model-info-0.155.1",
            Self::AppModelInfoV1 => "codex-model-info-0.155.0-alpha.9.2",
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
        let levels = route.reasoning_levels()?;
        let label = format!("ChatGPT Web · {}", route.observed_label);
        let mut description = format!("{label}. Connected through cxweb");
        if levels
            .iter()
            .any(|level| level.effort == "low" && level.description == "Instant")
        {
            description.push_str(". Low selects Instant (shown as Light in App)");
        }
        if let Some(pro) = levels.iter().find(|level| {
            level.effort == "max" && matches!(level.description.as_str(), "Pro" | "6 PRO")
        }) {
            description.push_str(&format!(". Max selects {}", pro.description));
        }
        Ok(json!({
            "slug":route.id,"display_name":label,
            "description":description,
            "default_reasoning_level":route.effort,
            "supported_reasoning_levels":levels,
            "shell_type":if route.coding {"unified_exec"} else {"disabled"},
            "apply_patch_tool_type":if route.coding {Some("freeform")} else {None},
            "visibility":"list","supported_in_api":true,"priority":1000,
            "availability_nux":null,"upgrade":null,
            "base_instructions":"Follow the user's task and applicable instructions. Use client-provided tools when needed. Tool execution is performed by the client. Report unavailable capabilities accurately.",
            "support_verbosity":false,"default_verbosity":null,
            "supports_reasoning_summary_parameter":true,
            "truncation_policy":{"mode":"bytes","limit":10000},
            "input_modalities":["text"],"tool_mode":"direct",
            "experimental_supported_tools":[],"additional_speed_tiers":[],"service_tiers":[],
            "supports_search_tool":true,"supports_experimental_context":false,"use_responses_lite":false
        }))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReasoningLevel {
    pub effort: String,
    pub description: String,
}

#[derive(Clone)]
pub struct CatalogRoute {
    pub id: String,
    pub observed_label: String,
    /// The default observed effort. Additional qualified choices must resolve
    /// within the same browser model family without a silent model fallback.
    pub effort: String,
    /// Empty preserves the original single-effort route. Every additional level
    /// must be backed by an independently verified browser selection.
    pub reasoning: Vec<ReasoningLevel>,
    pub coding: bool,
}

impl CatalogRoute {
    pub fn reasoning_levels(&self) -> Result<Vec<ReasoningLevel>, &'static str> {
        let levels = if self.reasoning.is_empty() {
            vec![ReasoningLevel {
                effort: self.effort.clone(),
                description: self.observed_label.clone(),
            }]
        } else {
            self.reasoning.clone()
        };
        let mut efforts = std::collections::BTreeSet::new();
        if levels.len() > 7
            || levels.iter().any(|level| {
                !matches!(
                    level.effort.as_str(),
                    "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
                ) || !efforts.insert(&level.effort)
                    || level.description.is_empty()
                    || level.description.len() > 160
                    || level.description.trim() != level.description
                    || !level
                        .description
                        .chars()
                        .all(|c| c.is_ascii_graphic() || c == ' ' || c == '·')
            })
            || !efforts.contains(&self.effort)
        {
            return Err("E_CATALOG_OBSERVATION");
        }
        Ok(levels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route() -> CatalogRoute {
        CatalogRoute {
            id: "webbridge/observed-xhigh".into(),
            observed_label: "Observed model · Extra High".into(),
            effort: "xhigh".into(),
            reasoning: vec![],
            coding: false,
        }
    }

    #[test]
    fn local_budget_metadata_is_explicit_and_does_not_claim_provider_usage() {
        use crate::context_budget::{LocalContextBudget, MAX_PROMPT_BYTES};
        for codec in [CatalogCodec::CliModelInfoV1, CatalogCodec::AppModelInfoV1] {
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
                "0.156.1",
                "codex_cli_rs/0.156.1 (Windows 11)",
                Some(CatalogCodec::CliModelInfoV1),
            ),
            (
                "0.155.0",
                "Codex Desktop/0.155.0-alpha.16.3 (Windows 11)",
                Some(CatalogCodec::AppModelInfoV1),
            ),
            ("0.155.1", "codex_cli_rs/0.156.1", None),
            ("0.156.1", "codex_cli_rs/0.155.1", None),
            (
                "0.155.1",
                "codex_cli_rs/0.155.1 (Windows 11; x64)",
                Some(CatalogCodec::CliModelInfoV1),
            ),
            (
                "0.155.0",
                "codex_desktop/0.155.0-alpha.9.2 (Windows 11)",
                Some(CatalogCodec::AppModelInfoV1),
            ),
            (
                "0.155.0",
                "Codex Desktop/0.155.0-alpha.16 (Windows 11)",
                Some(CatalogCodec::AppModelInfoV1),
            ),
            (
                "0.155.0",
                "codex_desktop/0.155.0-alpha.9.3",
                Some(CatalogCodec::AppModelInfoV1),
            ),
            ("0.155.1", "codex_cli_rs/0.155.0-alpha.9.2", None),
            ("0.155.0", "codex_cli_rs/0.155.1", None),
            (
                "0.156.0",
                "codex_cli_rs/0.156.0",
                Some(CatalogCodec::CliModelInfoV1),
            ),
            ("0.155.1", "unknown", None),
            ("0.155.1", "/0.155.1", None),
        ] {
            assert_eq!(CatalogCodec::select(query, agent), expected);
        }
    }

    #[test]
    fn future_releases_use_the_same_contract_without_registration() {
        for major in [0, 1, 42] {
            for minor in [1, 157, 999] {
                let core = format!("{major}.{minor}.7");
                for suffix in ["", "-alpha.900.5", "-beta.2+build.91"] {
                    let build = format!("{core}{suffix}");
                    assert!(CatalogCodec::for_build(&build).is_some());
                    for (origin, family) in [
                        ("codex_cli_rs", CatalogCodec::CliModelInfoV1),
                        ("Codex Desktop", CatalogCodec::AppModelInfoV1),
                    ] {
                        assert_eq!(
                            CatalogCodec::select(&core, &format!("{origin}/{build} (Windows)")),
                            Some(family)
                        );
                    }
                }
            }
        }
        assert_eq!(
            CatalogCodec::CliModelInfoV1.encode(&route()).unwrap(),
            CatalogCodec::AppModelInfoV1.encode(&route()).unwrap()
        );
        for bad in ["", "unknown", "1.2", "1.2.3-", "1.2.3/evil", "1.2.x"] {
            assert!(CatalogCodec::for_build(bad).is_none());
        }
    }

    #[test]
    fn codecs_encode_observations_without_native_or_unverified_capabilities() {
        for codec in [CatalogCodec::CliModelInfoV1, CatalogCodec::AppModelInfoV1] {
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
    fn family_catalog_exposes_all_qualified_levels_without_inventing_choices() {
        let mut route = route();
        route.observed_label = "Latest".into();
        route.reasoning = [
            ("low", "Instant"),
            ("medium", "Medium"),
            ("high", "High"),
            ("xhigh", "Extra High"),
            ("max", "6 PRO"),
        ]
        .into_iter()
        .map(|(effort, description)| ReasoningLevel {
            effort: effort.into(),
            description: description.into(),
        })
        .collect();
        for codec in [CatalogCodec::CliModelInfoV1, CatalogCodec::AppModelInfoV1] {
            let encoded = codec.encode(&route).unwrap();
            assert_eq!(encoded["display_name"], "ChatGPT Web · Latest");
            assert_eq!(encoded["default_reasoning_level"], "xhigh");
            assert!(
                encoded["description"]
                    .as_str()
                    .unwrap()
                    .contains("Low selects Instant")
            );
            assert!(
                encoded["description"]
                    .as_str()
                    .unwrap()
                    .contains("Max selects 6 PRO")
            );
            assert_eq!(
                encoded["supported_reasoning_levels"],
                serde_json::to_value(&route.reasoning).unwrap()
            );
            for mutation in 0..4 {
                let mut invalid = route.clone();
                match mutation {
                    0 => invalid.reasoning.push(invalid.reasoning[0].clone()),
                    1 => invalid.reasoning.retain(|level| level.effort != "xhigh"),
                    2 => invalid.reasoning[0].effort = "invented".into(),
                    _ => invalid.reasoning[0].description = "Injected\nrow".into(),
                }
                assert!(codec.encode(&invalid).is_err());
            }
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
            assert!(CatalogCodec::CliModelInfoV1.encode(&route).is_err());
        }
    }
}
