use deepdeck_oracle_engine_contracts::{
    engine::rule_is_executable,
    oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card},
};
use serde_json::Value;

fn parse(
    name: &str,
    oracle_text: &str,
) -> deepdeck_oracle_engine_contracts::oracle::OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Creature — Lhurgoyf".to_string(),
        mana_cost: Some("{2}{B}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

fn rules(parsed: &deepdeck_oracle_engine_contracts::oracle::OracleCardParseResult) -> Vec<&Value> {
    parsed
        .abilities
        .iter()
        .map(|ability| ability.rule.as_ref().expect("canonical ability emits IR"))
        .collect()
}

#[test]
fn exact_barrowgoyf_fixture_is_fully_native_primitive_and_executable() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../fixtures/oracle-ground-truth/cards/barrowgoyf.json"
    ))
    .expect("Barrowgoyf fixture is valid JSON");
    let context = &fixture["context"];
    let parsed = parse(
        context["name"].as_str().expect("fixture name"),
        context["oracleText"].as_str().expect("fixture Oracle text"),
    );

    assert_eq!(
        parsed.status,
        "canonical",
        "{}",
        serde_json::to_string_pretty(&parsed).expect("parse result serializes")
    );
    assert_eq!(parsed.abilities.len(), 3);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native),
        "every Oracle ability must be consumed by promoted local automata"
    );
    for (ability, expected) in parsed
        .abilities
        .iter()
        .zip(fixture["abilities"].as_array().expect("fixture abilities"))
    {
        assert_eq!(ability.source.text, expected["source"]["text"]);
        assert_eq!(ability.rule.as_ref(), Some(&expected["expectedRule"]));
        assert!(rule_is_executable(
            ability.rule.as_ref().expect("canonical rule")
        ));
    }
}

#[test]
fn distinct_card_type_characteristic_reuses_zone_and_arithmetic_children() {
    let parsed = parse(
        "Test Goyf",
        "Test Goyf's power is equal to the number of card types among cards in your graveyard and its toughness is equal to that number plus 2.",
    );
    assert_eq!(
        parsed.status,
        "canonical",
        "{}",
        serde_json::to_string_pretty(&parsed).expect("parse result serializes")
    );
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = rules(&parsed)[0];
    let modifier = &rule["modifiers"][0];
    assert_eq!(modifier["power"]["kind"], "countDistinctCardTypes");
    assert_eq!(modifier["power"]["zone"]["kind"], "graveyard");
    assert_eq!(modifier["toughness"]["kind"], "add");
    assert_eq!(modifier["toughness"]["right"]["value"], 2);
    assert!(rule_is_executable(rule));
}

#[test]
fn optional_damage_bound_mill_recovery_reuses_selector_and_bound_set_children() {
    let parsed = parse(
        "Test Goyf",
        "Whenever Test Goyf deals combat damage to a player, you may mill that many cards. If you do, you may put an artifact creature card from among those cards into your hand.",
    );
    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = rules(&parsed)[0];
    assert_eq!(rule["event"]["kind"], "combatDamageToPlayer");
    assert_eq!(
        rule["effects"][0]["action"]["count"]["decisionId"],
        "damageAmount"
    );
    assert_eq!(rule["effects"][0]["action"]["bind"], "milledCards");
    assert_eq!(
        rule["effects"][0]["onPerformed"][0]["from"]["binding"],
        "milledCards"
    );
    assert_eq!(rule["effects"][0]["onPerformed"][0]["where"]["kind"], "and");
    assert!(rule_is_executable(rule));
}

#[test]
fn characteristic_numeric_anaphor_must_resolve_to_the_prior_count() {
    let parsed = parse(
        "Test Goyf",
        "Test Goyf's power is equal to the number of card types among cards in all graveyards and its toughness is equal to another number plus 1.",
    );
    assert_eq!(parsed.status, "unsupported");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    // `another number` is no anaphor of the prior count: the numeric child
    // rejects it at its first token instead of borrowing that count.
    let diagnostic = &parsed.abilities[0].diagnostics[0];
    assert_eq!(diagnostic.state.as_deref(), Some("numeric.atom.start"));
    assert_eq!(diagnostic.found.as_deref(), Some("another"));
}

#[test]
fn damage_bound_recovery_rejects_an_unresolved_collection_anaphor() {
    let parsed = parse(
        "Test Goyf",
        "Whenever this creature deals combat damage to a player, you may mill that many cards. If you do, you may put a creature card from among the exiled cards into your hand.",
    );
    assert_eq!(parsed.status, "unsupported");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let diagnostics = serde_json::to_string(&parsed.abilities[0].diagnostics)
        .expect("structured diagnostics serialize");
    assert!(diagnostics.contains("bound"));
}

#[test]
fn promoted_families_reject_trailing_unconsumed_oracle_tokens() {
    for text in [
        "Test Goyf's power is equal to the number of card types among cards in all graveyards and its toughness is equal to that number plus 1. Ignore this sentence.",
        "Whenever this creature deals combat damage to a player, you may mill that many cards. If you do, you may put a creature card from among them into your hand. Ignore this sentence.",
    ] {
        let parsed = parse("Test Goyf", text);
        assert_eq!(parsed.status, "unsupported", "must reject: {text}");
        assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
        assert!(parsed.abilities[0].rule.is_none());
    }
}
