use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const UNDERCITY_SEWERS_ORACLE: &str = "({T}: Add {U} or {B}.)\nThis land enters tapped.\nWhen this land enters, surveil 1. (Look at the top card of your library. You may put it into your graveyard.)";

fn parse(card_name: &str, type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

fn surveil_ability(
    parsed: &OracleCardParseResult,
) -> &deepdeck_oracle::oracle::ParsedOracleAbility {
    parsed
        .abilities
        .iter()
        .find(|ability| ability.source.text.to_ascii_lowercase().contains("surveil"))
        .expect("surveil ability exists")
}

fn expected_surveil_rule(count: i64) -> serde_json::Value {
    json!({
        "kind": "triggeredAbility",
        "source": { "kind": "self" },
        "event": { "kind": "enterBattlefield", "object": { "kind": "self" } },
        "effects": [
            {
                "kind": "surveil",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "count": { "kind": "integer", "value": count },
            },
            {
                "kind": "emitAbilitySignal",
                "abilityId": "surveil",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            },
        ],
    })
}

#[test]
fn undercity_sewers_entry_surveil_is_native_with_registered_completion_signal() {
    let parsed = parse(
        "Undercity Sewers",
        "Land — Island Swamp",
        UNDERCITY_SEWERS_ORACLE,
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 3);
    let ability = surveil_ability(&parsed);
    assert_eq!(ability.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.rule, Some(expected_surveil_rule(1)));
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
}

#[test]
fn a_resolved_printed_name_and_arbitrary_numeric_literal_reuse_the_children() {
    let parsed = parse(
        "Test Observatory",
        "Artifact",
        "When Test Observatory enters, surveil two.",
    );

    assert_eq!(parsed.status, "canonical");
    let ability = surveil_ability(&parsed);
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.rule, Some(expected_surveil_rule(2)));
}

#[test]
fn an_unknown_surveil_amount_is_a_committed_numeric_failure() {
    let parsed = parse(
        "Test Observatory",
        "Artifact",
        "When this artifact enters, surveil many.",
    );
    let ability = surveil_ability(&parsed);

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(ability.rule.is_none());
    assert!(ability.entities.iter().any(|entity| {
        entity.raw.eq_ignore_ascii_case("many") && entity.token_type == "unknown"
    }));
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("source-enters-surveil-trigger")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("numeric.atom.start")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("many"));
}

#[test]
fn an_unresolved_different_source_name_is_not_treated_as_self() {
    let parsed = parse(
        "Test Observatory",
        "Artifact",
        "When Other Observatory enters, surveil 1.",
    );
    let ability = surveil_ability(&parsed);

    assert_eq!(parsed.status, "unsupported");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(ability.rule.is_none());
}
