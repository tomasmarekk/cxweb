//! User-selected summarization route; never changes the native task model.
use cxweb_platform::atomic_file::Snapshot;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub model: String,
    pub effort: String,
}

pub fn load(directory: &Path) -> Result<Option<Choice>, &'static str> {
    let file = Snapshot::capture(&directory.join("compaction-model.json"))
        .map_err(|_| "E_COMPACTION_SETTINGS")?;
    if !file.existed() {
        return Ok(None);
    }
    let value = cxweb_codex_adapter::strict_json::parse(file.original(), 4096)
        .map_err(|_| "E_COMPACTION_SETTINGS")?;
    serde_json::from_value(value).map_err(|_| "E_COMPACTION_SETTINGS")
}

pub fn validate(
    choice: &Choice,
    families: &[crate::control_protocol::ReasoningFamily],
) -> Result<(), &'static str> {
    if families.iter().any(|family| {
        family.model == choice.model
            && family
                .levels
                .iter()
                .any(|level| level.effort == choice.effort)
    }) {
        Ok(())
    } else {
        Err("E_COMPACTION_MODEL_UNAVAILABLE")
    }
}

pub fn save(directory: &Path, choice: Option<&Choice>) -> Result<(), &'static str> {
    let file = Snapshot::capture(&directory.join("compaction-model.json"))
        .map_err(|_| "E_COMPACTION_SETTINGS")?;
    let bytes = serde_json::to_vec(&choice).map_err(|_| "E_COMPACTION_SETTINGS")?;
    if bytes.len() > 4096 {
        return Err("E_COMPACTION_SETTINGS");
    }
    let staged = file
        .stage(
            &format!(".cxweb-compaction-{:032x}.tmp", rand::random::<u128>()),
            &bytes,
        )
        .map_err(|_| "E_COMPACTION_SETTINGS")?;
    file.commit(&staged, &bytes)
        .map_err(|_| "E_COMPACTION_SETTINGS")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_choice_is_optional_persistent_and_rejects_unknown_fields() {
        let dir = std::env::temp_dir().join(format!(
            "cxweb-compaction-policy-{:032x}",
            rand::random::<u128>()
        ));
        cxweb_platform::state::protected_directory(&dir).unwrap();
        assert_eq!(load(&dir).unwrap(), None);
        let choice = Choice {
            model: "webbridge/test".into(),
            effort: "medium".into(),
        };
        save(&dir, Some(&choice)).unwrap();
        assert_eq!(load(&dir).unwrap(), Some(choice));
        save(&dir, None).unwrap();
        assert_eq!(load(&dir).unwrap(), None);
        std::fs::write(
            dir.join("compaction-model.json"),
            br#"{"model":"webbridge/test","effort":"medium","extra":true}"#,
        )
        .unwrap();
        assert!(load(&dir).is_err());
        std::fs::remove_file(dir.join("compaction-model.json")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
