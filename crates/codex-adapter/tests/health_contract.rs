use cxweb_domain::health::{Component, ComponentState, Evidence, Health};

#[test]
fn health_serialization_matches_the_published_contract() {
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../../integration-tests/contracts/health.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let mut health = Health::default();
    assert!(validator.is_valid(&serde_json::to_value(&health).unwrap()));
    health.components.runtime = Component {
        state: ComponentState::Healthy,
        evidence: Evidence::LocalProbe,
        observed_at: Some("2026-09-20T00:00:00.000Z".into()),
        code: None,
    };
    let mut value = serde_json::to_value(health).unwrap();
    assert!(validator.is_valid(&value));
    value["capability"] = serde_json::json!("must never appear");
    assert!(!validator.is_valid(&value));
    value.as_object_mut().unwrap().remove("capability");
    value["components"]
        .as_object_mut()
        .unwrap()
        .remove("codex_app");
    assert!(!validator.is_valid(&value));
}
