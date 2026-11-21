use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

fn parse_artifact(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Suppressor".to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn torpor_orb_is_native_and_has_no_unknown_lexemes() {
    let parsed = parse_artifact("Creatures entering don't cause abilities to trigger.");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(
        ability
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown")
    );
    assert_eq!(
        ability.rule.as_ref().unwrap()["modifiers"][0],
        json!({
            "kind": "suppressTriggerCreation",
            "causedBy": {
                "kind": "enterBattlefield",
                "object": {
                    "kind": "eventObject",
                    "where": { "kind": "cardTypeContains", "value": "Creature" },
                },
            },
        })
    );
}

#[test]
fn entering_object_collection_generalizes_across_types_and_optional_zone_phrase() {
    let parsed = parse_artifact(
        "Artifacts and creatures entering the battlefield don't cause abilities to trigger.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["modifiers"][0]["causedBy"]["object"]["where"],
        json!({
            "kind": "or",
            "operands": [
                { "kind": "cardTypeContains", "value": "Artifact" },
                { "kind": "cardTypeContains", "value": "Creature" },
            ],
        })
    );
}

#[test]
fn claimed_near_miss_fails_in_the_trigger_suppression_child() {
    let parsed = parse_artifact("Creatures entering don't cause activated abilities to trigger.");
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("trigger-creation-suppression")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("static.trigger-suppression.ability-noun")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("activated"));
}
