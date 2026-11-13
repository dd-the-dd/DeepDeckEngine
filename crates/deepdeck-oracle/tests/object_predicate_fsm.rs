use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::{Value, json};

fn parse_card(
    name: &str,
    type_line: &str,
    mana_cost: &str,
    oracle_text: &str,
) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

fn only_rule(parsed: &OracleCardParseResult) -> &Value {
    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    parsed.abilities[0]
        .rule
        .as_ref()
        .expect("native primitive rule")
}

#[test]
fn card_type_lowering_is_shared_across_hand_permanent_and_spell_domains() {
    let hand = parse_card(
        "Test Hand Choice",
        "Sorcery",
        "{B}",
        "Target opponent reveals their hand. You choose a creature card from it. That player discards that card.",
    );
    let permanent = parse_card(
        "Test Removal",
        "Instant",
        "{1}{B}",
        "Destroy target creature.",
    );
    let spell = parse_card(
        "Test Counter",
        "Instant",
        "{1}{U}",
        "Counter target creature spell.",
    );

    let expected = json!({ "kind": "cardTypeContains", "value": "Creature" });
    assert_eq!(
        only_rule(&hand)["effects"][1]["candidates"]["where"],
        expected
    );
    assert_eq!(
        only_rule(&permanent)["declaration"]["decisions"][0]["candidates"]["where"],
        expected
    );
    assert_eq!(
        only_rule(&spell)["declaration"]["decisions"][0]["candidates"]["where"],
        expected
    );

    let negative_hand = parse_card(
        "Test Hand Choice",
        "Sorcery",
        "{B}",
        "Target opponent reveals their hand. You choose a nonland card from it. That player discards that card.",
    );
    let negative_spell = parse_card(
        "Test Counter",
        "Instant",
        "{1}{U}",
        "Counter target nonland spell.",
    );
    let expected_negative = json!({
        "kind": "not",
        "operand": { "kind": "cardTypeContains", "value": "Land" },
    });
    assert_eq!(
        only_rule(&negative_hand)["effects"][1]["candidates"]["where"],
        expected_negative
    );
    assert_eq!(
        only_rule(&negative_spell)["declaration"]["decisions"][0]["candidates"]["where"],
        expected_negative
    );
}

#[test]
fn domain_identities_preserve_the_existing_canonical_ir_shape() {
    let permanent = parse_card(
        "Test Removal",
        "Instant",
        "{1}{B}",
        "Destroy target permanent.",
    );
    let spell = parse_card("Test Counter", "Instant", "{1}{U}", "Counter target spell.");

    assert_eq!(
        only_rule(&permanent)["declaration"]["decisions"][0]["candidates"]["where"],
        Value::Null
    );
    assert!(
        only_rule(&spell)["declaration"]["decisions"][0]["candidates"]
            .get("where")
            .is_none()
    );
}

#[test]
fn library_search_keeps_supertype_type_and_subtype_boolean_structure() {
    let basic_land = parse_card(
        "Test Ramp",
        "Sorcery",
        "{1}{G}",
        "Search your library for a basic land card, put it onto the battlefield tapped, then shuffle.",
    );
    let subtype_union = parse_card(
        "Test Search",
        "Sorcery",
        "{1}{G}",
        "Search your library for a Plains or Island card, put it onto the battlefield, then shuffle.",
    );

    assert_eq!(
        only_rule(&basic_land)["effects"][0]["candidates"]["where"],
        json!({
            "kind": "and",
            "operands": [
                { "kind": "typeLineContains", "value": "Basic" },
                { "kind": "cardTypeContains", "value": "Land" },
            ],
        })
    );
    assert_eq!(
        only_rule(&subtype_union)["effects"][0]["candidates"]["where"],
        json!({
            "kind": "or",
            "operands": [
                { "kind": "subtypeContains", "value": "Plains" },
                { "kind": "subtypeContains", "value": "Island" },
            ],
        })
    );
}

#[test]
fn ast_unification_does_not_broaden_the_search_lexicon() {
    let parsed = parse_card(
        "Test Search",
        "Sorcery",
        "{1}{G}",
        "Search your library for a hero card, put it onto the battlefield tapped, then shuffle.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("selector.search-card.predicate")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("hero"));
}
