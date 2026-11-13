use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::{Value, json};

const CONTAINMENT_PRIEST_ORACLE: &str =
    "Flash\nIf a nontoken creature would enter and it wasn't cast, exile it instead.";

fn parse(card_name: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Creature — Human Cleric".to_string(),
        mana_cost: Some("{1}{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

fn replacement_ability(
    parsed: &OracleCardParseResult,
) -> &deepdeck_oracle::oracle::ParsedOracleAbility {
    parsed
        .abilities
        .iter()
        .find(|ability| ability.source.text.starts_with("If "))
        .expect("entry replacement ability exists")
}

fn expected_rule(where_predicate: Value) -> Value {
    json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "modifiers": [{
            "kind": "prohibitUncastBattlefieldEntry",
            "where": where_predicate,
            "destination": "exile",
        }],
    })
}

#[test]
fn containment_priest_is_fully_native_with_primitive_replacement_ir() {
    let parsed = parse("Containment Priest", CONTAINMENT_PRIEST_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    let ability = replacement_ability(&parsed);
    assert_eq!(ability.status, "canonical");
    assert_eq!(
        ability.rule,
        Some(expected_rule(json!({
            "kind": "and",
            "operands": [
                { "kind": "not", "operand": { "kind": "isToken" } },
                { "kind": "cardTypeContains", "value": "Creature" },
            ],
        })))
    );
    let serialized = serde_json::to_string(&parsed).expect("parse result serializes");
    assert!(!serialized.contains("resolveTriggeredInstruction"));
}

#[test]
fn explicit_battlefield_destination_and_permanent_type_are_generic() {
    let parsed = parse(
        "Entry Guard",
        "If a nontoken artifact would enter the battlefield and it wasn't cast, exile it instead.",
    );

    assert_eq!(parsed.status, "canonical");
    let ability = replacement_ability(&parsed);
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule,
        Some(expected_rule(json!({
            "kind": "and",
            "operands": [
                { "kind": "not", "operand": { "kind": "isToken" } },
                { "kind": "cardTypeContains", "value": "Artifact" },
            ],
        })))
    );
}

#[test]
fn a_positive_token_predicate_is_preserved_without_negation() {
    let parsed = parse(
        "Entry Guard",
        "If a token creature would enter and it wasn't cast, exile it instead.",
    );
    let ability = replacement_ability(&parsed);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule,
        Some(expected_rule(json!({
            "kind": "and",
            "operands": [
                { "kind": "isToken" },
                { "kind": "cardTypeContains", "value": "Creature" },
            ],
        })))
    );
}

#[test]
fn a_different_replacement_anaphor_is_a_committed_failure() {
    let parsed = parse(
        "Entry Guard",
        "If a nontoken creature would enter and it wasn't cast, exile that instead.",
    );
    let ability = replacement_ability(&parsed);

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("uncast-entry-replacement")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("that"));
}
