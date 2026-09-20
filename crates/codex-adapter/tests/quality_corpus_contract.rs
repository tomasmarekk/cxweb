use cxweb_codex_adapter::quality_corpus;
use serde_json::Value;

#[test]
fn generated_cases_match_the_frozen_pre_sampling_contract() {
    let frozen: Value = serde_json::from_str(include_str!(
        "../../../integration-tests/quality/protocol-corpus-v1.json"
    ))
    .unwrap();
    assert_eq!(quality_corpus::manifest().unwrap(), frozen);
    assert_eq!(
        frozen["corpus_sha256"],
        "ebbaf51a3c453a614ed4c126b81aeba48bb3acae4bd89c7763fc143f0d5e34f1"
    );
}
