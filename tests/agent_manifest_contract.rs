use mtg_engine::agent_protocol::{AGENT_MANIFEST_VERSION, AgentManifest, AgentType, DeckSelection};
use serde_json::Value;

const SCHEMA: &str = include_str!("../contracts/agent-manifest/v1/schema.json");
const MINIMAL: &str = include_str!("../contracts/agent-manifest/v1/fixtures/minimal.json");
const MAXIMAL: &str = include_str!("../contracts/agent-manifest/v1/fixtures/maximal.json");
const INVALID: &str = include_str!("../contracts/agent-manifest/v1/fixtures/invalid.json");

#[test]
fn contract_bundle_version_matches_the_rust_manifest_version() {
    let schema: Value = serde_json::from_str(SCHEMA).expect("valid contract schema JSON");

    assert_eq!(
        schema["$id"],
        "https://deepdeckleague.com/contracts/agent-manifest/v1/schema.json"
    );
    assert_eq!(
        schema["properties"]["schemaVersion"]["const"],
        AGENT_MANIFEST_VERSION
    );
    assert_eq!(schema["properties"]["agentType"]["default"], "other");
}

#[test]
fn minimal_fixture_uses_backward_compatible_defaults() {
    let manifest: AgentManifest = serde_json::from_str(MINIMAL).expect("minimal manifest parses");

    manifest.validate().expect("minimal manifest validates");
    assert_eq!(manifest.agent_type, AgentType::Other);
    assert!(manifest.description.is_empty());
    assert!(manifest.repository.is_none());
    assert!(manifest.publications.is_empty());
    assert!(matches!(manifest.compatibility.decks, DeckSelection::All));

    let serialized = serde_json::to_string(&manifest).expect("minimal manifest serializes");
    let reparsed: AgentManifest =
        serde_json::from_str(&serialized).expect("serialized minimal manifest reparses");
    reparsed
        .validate()
        .expect("reparsed minimal manifest validates");
}

#[test]
fn maximal_fixture_validates_and_round_trips_without_loss() {
    let fixture: Value = serde_json::from_str(MAXIMAL).expect("maximal fixture JSON");
    let manifest: AgentManifest =
        serde_json::from_value(fixture.clone()).expect("maximal manifest parses");

    manifest.validate().expect("maximal manifest validates");
    assert_eq!(manifest.agent_type, AgentType::Hybrid);
    assert_eq!(
        manifest.authors[0].orcid.as_deref(),
        Some("0000-0002-1825-0097")
    );
    assert_eq!(manifest.publications.len(), 1);
    assert_eq!(
        serde_json::to_value(manifest).expect("maximal manifest serializes"),
        fixture
    );
}

#[test]
fn invalid_fixture_is_rejected_by_semantic_validation() {
    let manifest: AgentManifest = serde_json::from_str(INVALID).expect("invalid fixture parses");

    assert_eq!(
        manifest.validate(),
        Err("at least one named author is required".to_string())
    );
}
