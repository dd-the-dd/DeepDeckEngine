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

fn assert_surveil_rule(rule: &serde_json::Value, count: i64) {
    assert_eq!(rule["kind"], "triggeredAbility");
    assert_eq!(
        rule["event"],
        json!({"kind":"enterBattlefield","object":{"kind":"self"}})
    );
    let effects = rule["effects"].as_array().unwrap();
    assert_eq!(
        effects
            .iter()
            .map(|effect| effect["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "lookAtTopCards",
            "chooseCards",
            "chooseOrder",
            "moveCards",
            "chooseOrder",
            "moveCards",
            "emitAbilitySignal"
        ]
    );
    assert_eq!(effects[0]["count"], json!({"kind":"integer","value":count}));
    assert_eq!(effects[3]["to"]["kind"], "graveyard");
    assert_eq!(effects[5]["to"]["position"], "top");
    assert_eq!(effects[6]["abilityId"], "surveil");
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
    assert_surveil_rule(ability.rule.as_ref().unwrap(), 1);
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
    assert_surveil_rule(ability.rule.as_ref().unwrap(), 2);
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
        // `many` is the quantity-anaphor lexeme (`that many`), never a
        // numeric atom by itself.
        entity.raw.eq_ignore_ascii_case("many") && entity.token_type == "quantityAnaphorWord"
    }));
    // The shared trigger program and the surveil family both stop at the
    // same numeric child.
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("generic-trigger-program")
    );
    assert!(ability.diagnostics.iter().any(|diagnostic| {
        diagnostic.automaton.as_deref() == Some("source-enters-surveil-trigger")
            && diagnostic.state.as_deref() == Some("numeric.atom.start")
    }));
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
