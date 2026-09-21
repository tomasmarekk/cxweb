//! Pure configuration planning. No file writes and no native credential access.
use serde::Serialize;
use toml_edit::{DocumentMut, value};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("configuration is not valid TOML")]
    Parse,
    #[error("configuration contains a provider, profile, catalog or route conflict")]
    Conflict,
    #[error("loopback route is invalid")]
    InvalidRoute,
}

#[derive(Debug, Serialize)]
pub struct Preflight {
    pub can_plan: bool,
    pub conflicts: Vec<&'static str>,
}

pub fn inspect(text: &str) -> Result<Preflight, ConfigError> {
    let doc: DocumentMut = text.parse().map_err(|_| ConfigError::Parse)?;
    let mut conflicts = Vec::new();
    for key in [
        "openai_base_url",
        "chatgpt_base_url",
        "model_catalog_json",
        "profile",
        "profiles",
    ] {
        if doc.contains_key(key) {
            conflicts.push(key);
        }
    }
    if doc
        .get("model_provider")
        .is_some_and(|v| v.as_str() != Some("openai"))
    {
        conflicts.push("model_provider");
    }
    Ok(Preflight {
        can_plan: conflicts.is_empty(),
        conflicts,
    })
}

pub struct RoutePatch {
    installed: String,
    previous_model: Option<String>,
    web_tools: Option<(String, String)>,
}

fn remove_preserving_comments(doc: &mut DocumentMut, name: &str) {
    let prefix = doc
        .as_table()
        .key(name)
        .and_then(|key| key.leaf_decor().prefix())
        .and_then(|raw| raw.as_str())
        .unwrap_or("")
        .to_owned();
    let suffix = doc
        .get(name)
        .and_then(|item| item.as_value())
        .and_then(|value| value.decor().suffix())
        .and_then(|raw| raw.as_str())
        .unwrap_or("")
        .to_owned();
    doc.remove(name);
    let mut comments = String::new();
    for decoration in [prefix, suffix] {
        if decoration.contains('#') {
            comments.push_str(&decoration);
            if !comments.ends_with('\n') {
                comments.push('\n');
            }
        }
    }
    if !comments.is_empty() {
        let trailing = doc.trailing().as_str().unwrap_or("");
        doc.set_trailing(format!("{trailing}{comments}"));
    }
}

impl RoutePatch {
    /// Add one installation-owned MCP entry to the same configuration transaction.
    /// The installation supplies its private immutable executable location.
    pub fn plan_with_web_tools(
        original: &str,
        port: u16,
        capability: &str,
        name: &str,
        executable: &str,
    ) -> Result<(Self, String), ConfigError> {
        if !name.starts_with("cxweb_web_")
            || name.len() != 42
            || !name[10..].bytes().all(|b| b.is_ascii_hexdigit())
            || executable.is_empty()
            || executable.len() > 32767
            || executable.chars().any(char::is_control)
        {
            return Err(ConfigError::InvalidRoute);
        }
        let (mut patch, candidate) = Self::plan(original, port, capability)?;
        let mut doc: DocumentMut = candidate.parse().map_err(|_| ConfigError::Parse)?;
        if !doc.contains_key("mcp_servers") {
            let mut table = toml_edit::Table::new();
            table.set_implicit(true);
            doc["mcp_servers"] = toml_edit::Item::Table(table);
        }
        let servers = doc["mcp_servers"]
            .as_table_mut()
            .ok_or(ConfigError::Conflict)?;
        if servers.contains_key(name) {
            return Err(ConfigError::Conflict);
        }
        let mut entry = toml_edit::Table::new();
        entry["command"] = value(executable);
        entry["args"] = value(toml_edit::Array::from_iter(["--web-tools"]));
        servers[name] = toml_edit::Item::Table(entry);
        patch.web_tools = Some((name.into(), servers[name].to_string()));
        Ok((patch, doc.to_string()))
    }
    /// A saved route may resume after unrelated edits, but never after its key
    /// or effective provider/profile/catalog was replaced by the user.
    pub fn can_resume(&self, current: &str) -> bool {
        let Ok(mut doc) = current.parse::<DocumentMut>() else {
            return false;
        };
        if doc.get("openai_base_url").and_then(|item| item.as_str())
            != Some(self.installed.as_str())
        {
            return false;
        }
        doc.remove("openai_base_url");
        inspect(&doc.to_string()).is_ok_and(|result| result.can_plan)
    }

    pub fn plan(
        original: &str,
        port: u16,
        capability: &str,
    ) -> Result<(Self, String), ConfigError> {
        Self::plan_with_suffix(original, port, capability, "/backend-api/codex")
    }

    /// Reproduce an existing v1 journal for validation and undo only. New
    /// installations must retain Codex's subscription request-shape selection.
    pub fn legacy_plan(
        original: &str,
        port: u16,
        capability: &str,
    ) -> Result<(Self, String), ConfigError> {
        Self::plan_with_suffix(original, port, capability, "/v1")
    }

    fn plan_with_suffix(
        original: &str,
        port: u16,
        capability: &str,
        suffix: &str,
    ) -> Result<(Self, String), ConfigError> {
        if port == 0
            || capability.len() != 43
            || !capability
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(ConfigError::InvalidRoute);
        }
        if !inspect(original)?.can_plan {
            return Err(ConfigError::Conflict);
        }
        let installed = format!("http://127.0.0.1:{port}/wb/{capability}{suffix}");
        let mut doc: DocumentMut = original.parse().map_err(|_| ConfigError::Parse)?;
        let previous_model = match doc.get("model") {
            None => None,
            Some(item) => Some(item.as_str().ok_or(ConfigError::Conflict)?.to_owned()),
        };
        doc["openai_base_url"] = value(&installed);
        Ok((
            Self {
                installed,
                previous_model,
                web_tools: None,
            },
            doc.to_string(),
        ))
    }

    pub fn remove(&self, current: &str) -> Result<String, ConfigError> {
        self.remove_with_selection(current, &[], &[])
    }

    /// Catalog receipts must contain the exact routes this installation
    /// published and native selections currently verified for the target codec.
    /// A prefix alone never proves ownership of a persisted model selection.
    pub fn remove_with_selection(
        &self,
        current: &str,
        published_routes: &[String],
        available_native_models: &[String],
    ) -> Result<String, ConfigError> {
        if published_routes
            .iter()
            .any(|id| !id.starts_with("webbridge/") || id.len() <= 10)
        {
            return Err(ConfigError::Conflict);
        }
        let mut doc: DocumentMut = current.parse().map_err(|_| ConfigError::Parse)?;
        match doc.get("openai_base_url") {
            None => {}
            Some(item) if item.as_str() == Some(&self.installed) => {
                remove_preserving_comments(&mut doc, "openai_base_url");
            }
            _ => return Err(ConfigError::Conflict),
        }
        let owned_selection = doc
            .get("model")
            .and_then(|item| item.as_str())
            .is_some_and(|selected| published_routes.iter().any(|id| id == selected));
        if owned_selection {
            if let Some(previous) = self.previous_model.as_ref().filter(|previous| {
                !previous.starts_with("webbridge/") && available_native_models.contains(previous)
            }) {
                // Keep the user's comments attached to the selected model.
                let item = doc["model"].as_value_mut().ok_or(ConfigError::Conflict)?;
                let decoration = item.decor().clone();
                *item = toml_edit::Value::from(previous.as_str());
                *item.decor_mut() = decoration;
            } else {
                remove_preserving_comments(&mut doc, "model");
            }
        }
        if let Some((name, installed)) = &self.web_tools
            && let Some(servers) = doc
                .get_mut("mcp_servers")
                .and_then(|item| item.as_table_mut())
        {
            // A changed entry belongs to the user. Never erase their added flags,
            // environment, comments or replacement command during route removal.
            if servers
                .get(name)
                .is_some_and(|entry| entry.to_string() == *installed)
            {
                servers.remove(name);
                if servers.is_empty() && servers.is_implicit() {
                    doc.remove("mcp_servers");
                }
            }
        }
        Ok(doc.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const CAP: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    #[test]
    fn owned_web_search_round_trips_and_preserves_other_servers_and_user_edits() {
        let name = format!("cxweb_web_{}", "1".repeat(32));
        let original = "# personal settings\nmodel = 'native'\n[mcp_servers.personal]\ncommand = 'my-server'\n";
        let executable = r"C:\Users\Fixture User\cxweb\cxweb-daemon.exe";
        let (patch, installed) =
            RoutePatch::plan_with_web_tools(original, 12345, CAP, &name, executable).unwrap();
        let doc: DocumentMut = installed.parse().unwrap();
        assert_eq!(
            doc["mcp_servers"][&name]["command"].as_str(),
            Some(executable)
        );
        assert_eq!(
            doc["mcp_servers"][&name]["args"]
                .as_array()
                .unwrap()
                .get(0)
                .unwrap()
                .as_str(),
            Some("--web-tools")
        );
        assert_eq!(patch.remove(&installed).unwrap(), original);
        let mut edited = doc;
        edited["mcp_servers"][&name]["enabled"] = value(false);
        let removed = patch.remove(&edited.to_string()).unwrap();
        assert!(removed.contains("enabled = false"));
        assert!(removed.contains("my-server"));
        assert!(!removed.contains("openai_base_url"));
        assert_eq!(patch.remove(&removed).unwrap(), removed);
        assert!(RoutePatch::plan_with_web_tools(&removed, 12345, CAP, &name, executable).is_err());
        let (empty, installed) =
            RoutePatch::plan_with_web_tools("", 12345, CAP, &name, executable).unwrap();
        assert_eq!(empty.remove(&installed).unwrap(), "");
    }
    #[test]
    fn recovery_requires_owned_route_and_no_new_provider_or_profile_conflict() {
        let (patch, installed) = RoutePatch::plan("model = 'native'\n", 43127, CAP).unwrap();
        assert!(patch.can_resume(&installed));
        assert!(patch.can_resume(&(installed.clone() + "# user comment\ntheme = 'dark'\n")));
        assert!(!patch.can_resume(""));
        assert!(
            !patch
                .can_resume(&installed.replace(CAP, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"))
        );
        for conflict in [
            "model_provider = 'other'",
            "profile = 'other'",
            "[profiles.other]",
            "model_catalog_json = 'user.json'",
            "chatgpt_base_url = 'http://example.invalid'",
        ] {
            assert!(!patch.can_resume(&(installed.clone() + conflict)));
        }
        assert!(!patch.can_resume("invalid ["));
    }
    #[test]
    fn new_routes_keep_subscription_shape_and_legacy_undo_remains_exact() {
        let (current, candidate) = RoutePatch::plan("", 43127, CAP).unwrap();
        let (legacy, old) = RoutePatch::legacy_plan("", 43127, CAP).unwrap();
        // The reviewed clients choose JSON realtime call bodies using this
        // literal backend-api marker, including on loopback overrides.
        assert!(candidate.contains(&format!("/wb/{CAP}/backend-api/codex")));
        assert!(old.contains(&format!("/wb/{CAP}/v1")));
        assert!(current.remove(&old).is_err());
        assert!(legacy.remove(&candidate).is_err());
        assert_eq!(legacy.remove(&old).unwrap(), "");
    }
    #[test]
    fn removal_preserves_unrelated_edits_and_comments() {
        let original = "# user's preference\nmodel = 'native'\n";
        let (patch, installed) = RoutePatch::plan(original, 43127, CAP).unwrap();
        let changed = installed.replace("'native'", "'new-native'") + "# added later\n";
        let removed = patch.remove(&changed).unwrap();
        assert!(removed.contains("new-native"));
        assert!(removed.contains("# user's preference"));
        assert!(removed.contains("# added later"));
        assert!(!removed.contains("openai_base_url"));
        assert_eq!(patch.remove(&removed).unwrap(), removed);
    }
    #[test]
    fn changed_owned_key_is_never_overwritten() {
        let (patch, _) = RoutePatch::plan("", 43127, CAP).unwrap();
        assert!(
            patch
                .remove("openai_base_url = 'https://example.com'\n")
                .is_err()
        );
        assert!(RoutePatch::plan("profile = 'work'", 43127, CAP).is_err());
        assert!(RoutePatch::plan("model_provider = 'custom'", 43127, CAP).is_err());
    }

    #[test]
    fn disconnect_restores_only_exact_published_selection_to_a_verified_native_model() {
        let (patch, installed) =
            RoutePatch::plan("model = 'native' # keep comment\n", 43127, CAP).unwrap();
        let selected = installed.replace("'native'", "'webbridge/qualified'");
        let restored = patch
            .remove_with_selection(
                &selected,
                &["webbridge/qualified".into()],
                &["native".into()],
            )
            .unwrap();
        assert!(restored.contains("model = \"native\" # keep comment"));
        assert!(!restored.contains("openai_base_url"));
        assert_eq!(
            patch
                .remove_with_selection(
                    &restored,
                    &["webbridge/qualified".into()],
                    &["native".into()]
                )
                .unwrap(),
            restored
        );
        for foreign in ["native-new", "thirdparty/model", "webbridge/not-published"] {
            let current = installed.replace("'native'", &format!("'{foreign}'"));
            let removed = patch
                .remove_with_selection(
                    &current,
                    &["webbridge/qualified".into()],
                    &["native".into()],
                )
                .unwrap();
            assert!(removed.contains(foreign));
        }
    }

    #[test]
    fn removed_native_selection_is_not_restored_and_missing_route_still_cleans_owned_model() {
        let (patch, _) = RoutePatch::plan("model = 'no-longer-available'\n", 43127, CAP).unwrap();
        let current = "# user note\nmodel = 'webbridge/qualified'\nother = true\n";
        let removed = patch
            .remove_with_selection(current, &["webbridge/qualified".into()], &["native".into()])
            .unwrap();
        assert!(!removed.contains("model ="));
        assert!(removed.contains("other = true"));
        // Comments attached to a removed model must survive as user content.
        assert!(removed.contains("# user note"));
    }

    #[test]
    fn undo_keeps_comments_on_removed_keys_and_refuses_foreign_route_receipts() {
        let (patch, installed) = RoutePatch::plan("", 43127, CAP).unwrap();
        let edited = format!("# route note\n{} # inline note\n", installed.trim_end());
        let removed = patch.remove(&edited).unwrap();
        assert!(removed.contains("# route note"));
        assert!(removed.contains("# inline note"));
        assert!(!removed.contains(CAP));
        assert!(removed.parse::<DocumentMut>().is_ok());
        assert_eq!(patch.remove(&removed).unwrap(), removed);
        assert!(
            patch
                .remove_with_selection(&installed, &["native".into()], &[])
                .is_err()
        );
        let changed = installed.replace(CAP, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        assert!(
            patch
                .remove_with_selection(&changed, &["webbridge/qualified".into()], &[])
                .is_err()
        );
    }
}
