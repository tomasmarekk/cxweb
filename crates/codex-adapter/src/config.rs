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
}

impl RoutePatch {
    pub fn plan(
        original: &str,
        port: u16,
        capability: &str,
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
        let installed = format!("http://127.0.0.1:{port}/wb/{capability}/v1");
        let mut doc: DocumentMut = original.parse().map_err(|_| ConfigError::Parse)?;
        doc["openai_base_url"] = value(&installed);
        Ok((Self { installed }, doc.to_string()))
    }

    pub fn remove(&self, current: &str) -> Result<String, ConfigError> {
        let mut doc: DocumentMut = current.parse().map_err(|_| ConfigError::Parse)?;
        match doc.get("openai_base_url") {
            None => return Ok(current.to_owned()),
            Some(item) if item.as_str() == Some(&self.installed) => {
                doc.remove("openai_base_url");
            }
            _ => return Err(ConfigError::Conflict),
        }
        Ok(doc.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const CAP: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
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
}
