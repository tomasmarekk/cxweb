//! Immutable catalog publication bound to the same scope and routes as execution.
use crate::{catalog_proxy::OwnedCatalog, web_provider::ProviderScope};
use cxweb_codex_adapter::catalog_codec::{CatalogCodec, CatalogRoute};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct CatalogSnapshot(BTreeMap<CatalogCodec, OwnedCatalog>);

impl CatalogSnapshot {
    /// Inputs are an explicit qualification decision by the activation owner.
    /// Observing a route alone must never call this constructor.
    pub(crate) fn new(
        scope: &ProviderScope,
        executable: &BTreeSet<String>,
        generation: u64,
        qualified: Vec<(CatalogCodec, Vec<CatalogRoute>)>,
        context_budget: Option<cxweb_codex_adapter::context_budget::LocalContextBudget>,
    ) -> Result<Self, &'static str> {
        if generation == 0 || qualified.is_empty() {
            return Err("E_CATALOG_SNAPSHOT");
        }
        let mut hash = Sha256::new();
        for part in [&scope.installation, &scope.account, &scope.workspace] {
            hash.update((part.len() as u64).to_le_bytes());
            hash.update(part.as_bytes());
        }
        hash.update(scope.epoch.to_le_bytes());
        let web_scope = format!("{:x}", hash.finalize());
        let mut catalogs = BTreeMap::new();
        let mut observations = BTreeMap::new();
        for (codec, routes) in qualified {
            if catalogs.contains_key(&codec) || routes.is_empty() || routes.len() > 256 {
                return Err("E_CATALOG_SNAPSHOT");
            }
            let mut ids = BTreeSet::new();
            let mut entries = Vec::with_capacity(routes.len());
            for route in routes {
                if !executable.contains(&route.id) || !ids.insert(route.id.clone()) {
                    return Err("E_CATALOG_SNAPSHOT");
                }
                // Client-specific tool qualification may differ, but a route ID
                // must retain its observed model/effort identity in every picker.
                let observation = (
                    route.observed_label.clone(),
                    route.effort.clone(),
                    route.reasoning_levels()?,
                );
                if observations
                    .insert(route.id.clone(), observation.clone())
                    .is_some_and(|previous| previous != observation)
                {
                    return Err("E_CATALOG_SNAPSHOT");
                }
                entries.push(match context_budget {
                    Some(budget) => codec.encode_with_context_budget(&route, budget)?,
                    None => codec.encode(&route)?,
                });
            }
            catalogs.insert(
                codec,
                OwnedCatalog {
                    codec: codec.id().into(),
                    web_scope: web_scope.clone(),
                    generation,
                    entries,
                },
            );
        }
        Ok(Self(catalogs))
    }

    pub(crate) fn for_client(&self, codec: CatalogCodec) -> Option<OwnedCatalog> {
        self.0.get(&codec).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_scope() -> ProviderScope {
        ProviderScope {
            installation: "fixture".into(),
            account: "fixture-account".into(),
            workspace: "fixture-workspace".into(),
            epoch: 1,
        }
    }
    fn route() -> CatalogRoute {
        CatalogRoute {
            id: "webbridge/fixture-high".into(),
            observed_label: "Fixture High".into(),
            effort: "high".into(),
            reasoning: vec![],
            coding: false,
        }
    }
    fn routes() -> BTreeSet<String> {
        BTreeSet::from([route().id])
    }
    fn snapshot(scope: &ProviderScope, generation: u64) -> CatalogSnapshot {
        CatalogSnapshot::new(
            scope,
            &routes(),
            generation,
            vec![(CatalogCodec::CliModelInfoV1, vec![route()])],
            None,
        )
        .unwrap()
    }

    #[test]
    fn publication_is_client_specific_and_bound_to_execution_scope() {
        let mut scope = fixture_scope();
        let first = snapshot(&scope, 1);
        assert!(first.for_client(CatalogCodec::AppModelInfoV1).is_none());
        let first = first.for_client(CatalogCodec::CliModelInfoV1).unwrap();
        assert_eq!(first.entries[0]["slug"], route().id);
        assert!(!first.web_scope.contains("fixture"));
        for change in 0..4 {
            match change {
                0 => scope.installation.push('2'),
                1 => scope.account.push('2'),
                2 => scope.workspace.push('2'),
                _ => scope.epoch += 1,
            }
            assert_ne!(
                snapshot(&scope, 1)
                    .for_client(CatalogCodec::CliModelInfoV1)
                    .unwrap()
                    .web_scope,
                first.web_scope
            );
            scope = fixture_scope();
        }
        let second = snapshot(&scope, 2)
            .for_client(CatalogCodec::CliModelInfoV1)
            .unwrap();
        assert_eq!(second.web_scope, first.web_scope);
        assert_ne!(second.generation, first.generation);
    }

    #[test]
    fn cannot_advertise_unroutable_duplicate_or_invalid_entries() {
        let codec = CatalogCodec::CliModelInfoV1;
        for (generation, executable, qualified) in [
            (0, routes(), vec![(codec, vec![route()])]),
            (1, BTreeSet::new(), vec![(codec, vec![route()])]),
            (1, routes(), vec![(codec, vec![route(), route()])]),
            (
                1,
                routes(),
                vec![(codec, vec![route()]), (codec, vec![route()])],
            ),
            (1, routes(), vec![(codec, vec![])]),
            (1, routes(), vec![]),
        ] {
            assert!(
                CatalogSnapshot::new(&fixture_scope(), &executable, generation, qualified, None)
                    .is_err()
            );
        }
        let mut invalid = route();
        invalid.effort = "unknown".into();
        assert!(
            CatalogSnapshot::new(
                &fixture_scope(),
                &routes(),
                1,
                vec![(codec, vec![invalid])],
                None
            )
            .is_err()
        );
        let mut other = route();
        other.reasoning = vec![cxweb_codex_adapter::catalog_codec::ReasoningLevel {
            effort: "high".into(),
            description: "Different browser label".into(),
        }];
        assert!(
            CatalogSnapshot::new(
                &fixture_scope(),
                &routes(),
                1,
                vec![
                    (codec, vec![route()]),
                    (CatalogCodec::AppModelInfoV1, vec![other])
                ],
                None
            )
            .is_err()
        );
        let mut other = route();
        other.effort = "medium".into();
        assert!(
            CatalogSnapshot::new(
                &fixture_scope(),
                &routes(),
                1,
                vec![
                    (codec, vec![route()]),
                    (CatalogCodec::AppModelInfoV1, vec![other]),
                ],
                None,
            )
            .is_err()
        );
    }
}
