//! Private, installation-separated bindings derived from observed browser UI.
use cxweb_browser_adapter::ScopeSurface;
use sha2::{Digest, Sha256};

// Intentionally no Debug or Serialize: only opaque IDs enter the session ledger.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct BrowserScope {
    pub account: String,
    pub workspace: String,
}

fn hash(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_le_bytes());
        digest.update(part.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

impl BrowserScope {
    pub fn from_surface(installation: &str, surface: &ScopeSurface) -> Result<Self, &'static str> {
        let valid = |value: &str| {
            !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
        };
        let account = surface
            .account
            .as_deref()
            .filter(|value| valid(value))
            .ok_or("E_SESSION_SCOPE")?;
        let d = &surface.diagnostic;
        if !valid(installation)
            || d.failure.is_some()
            || !d.menu_present
            || !(d.account_candidates == 1
                || (d.settings_account_candidates == 1
                    && d.settings_account_selected
                    && d.settings_panel_present
                    && !d.settings_loading))
        {
            return Err("E_SESSION_SCOPE");
        }
        let account = hash(&["cxweb/account/v1", installation, account]);
        let workspace = match (surface.workspace.as_deref(), surface.default_workspace) {
            (Some(workspace), false)
                if valid(workspace)
                    && d.selected_workspace_candidates == 1
                    && d.has_workspace_id =>
            {
                hash(&[
                    "cxweb/workspace/observed/v1",
                    installation,
                    &account,
                    workspace,
                ])
            }
            (None, true) if d.default_context_verified && d.account_switcher_matches_settings => {
                hash(&["cxweb/workspace/account-default/v1", installation, &account])
            }
            _ => return Err("E_SESSION_SCOPE"),
        };
        Ok(Self { account, workspace })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cxweb_browser_adapter::ScopeDiagnostic;

    fn surface() -> ScopeSurface {
        ScopeSurface {
            account: Some("fixture@example.invalid".into()),
            workspace: Some("workspace-a".into()),
            default_workspace: false,
            diagnostic: ScopeDiagnostic {
                menu_present: true,
                account_candidates: 1,
                selected_workspace_candidates: 1,
                has_workspace_id: true,
                ..Default::default()
            },
        }
    }
    #[test]
    fn bindings_separate_installations_accounts_and_workspace_kinds_without_raw_identifiers() {
        let original = BrowserScope::from_surface("installation-a", &surface()).unwrap();
        assert!(original == BrowserScope::from_surface("installation-a", &surface()).unwrap());
        assert!(original != BrowserScope::from_surface("installation-b", &surface()).unwrap());
        for changed_account in [true, false] {
            let mut changed = surface();
            if changed_account {
                changed.account = Some("other@example.invalid".into());
            } else {
                changed.workspace = Some("workspace-b".into());
            }
            assert!(original != BrowserScope::from_surface("installation-a", &changed).unwrap());
        }
        let mut default = surface();
        default.workspace = None;
        default.default_workspace = true;
        default.diagnostic.default_context_verified = true;
        default.diagnostic.account_switcher_matches_settings = true;
        let default = BrowserScope::from_surface("installation-a", &default).unwrap();
        assert!(default.account == original.account && default.workspace != original.workspace);
        for value in [&original.account, &original.workspace, &default.workspace] {
            assert!(value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()));
        }
    }
    #[test]
    fn missing_ambiguous_or_failed_evidence_cannot_create_a_binding() {
        let mutations: &[fn(&mut ScopeSurface)] = &[
            |s| s.account = None,
            |s| s.account = Some(String::new()),
            |s| s.workspace = None,
            |s| s.workspace = Some("bad\nworkspace".into()),
            |s| s.default_workspace = true,
            |s| s.diagnostic.account_candidates = 2,
            |s| s.diagnostic.selected_workspace_candidates = 2,
            |s| s.diagnostic.has_workspace_id = false,
            |s| s.diagnostic.failure = Some("E_ACCOUNT_SCOPE".into()),
        ];
        for mutate in mutations {
            let mut surface = surface();
            mutate(&mut surface);
            assert!(BrowserScope::from_surface("installation", &surface).is_err());
        }
    }
}
