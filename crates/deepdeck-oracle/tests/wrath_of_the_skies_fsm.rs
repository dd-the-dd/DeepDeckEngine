use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const WRATH_ORACLE: &str = "You get X {E} (energy counters), then you may pay any amount of {E}. Destroy each artifact, creature, and enchantment with mana value less than or equal to the amount of {E} paid this way.";

fn parse(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Wrath of the Skies".to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some("{X}{W}{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn variable_energy_gain_payment_and_sweep_are_native_and_linked() {
    let parsed = parse(WRATH_ORACLE);
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("primitive rule");
    assert_eq!(
        rule["declaration"]["decisions"][0],
        json!({ "id": "xValue", "kind": "chooseNumber", "minimum": 0 })
    );
    assert_eq!(
        rule["effects"],
        json!([
            {
                "kind": "addPlayerCounters",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "counter": "energy",
                "count": { "kind": "decisionResult", "decisionId": "xValue" },
            },
            {
                "kind": "chooseNumber",
                "id": "paidPlayerCounters",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "minimum": { "kind": "integer", "value": 0 },
                "maximum": {
                    "kind": "countPlayerCounters",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "counter": "energy",
                },
            },
            {
                "kind": "removePlayerCounters",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "counter": "energy",
                "count": { "kind": "decisionResult", "decisionId": "paidPlayerCounters" },
            },
            {
                "kind": "destroyPermanent",
                "permanent": { "kind": "eachPermanent", "where": {
                    "kind": "and", "operands": [
                        { "kind": "or", "operands": [
                            { "kind": "cardTypeContains", "value": "Artifact" },
                            { "kind": "cardTypeContains", "value": "Creature" },
                            { "kind": "cardTypeContains", "value": "Enchantment" },
                        ] },
                        { "kind": "compare", "left": { "kind": "manaValueOf", "object": { "kind": "candidate" } },
                          "operator": "<=", "right": { "kind": "decisionResult", "decisionId": "paidPlayerCounters" } },
                    ],
                } },
            },
        ])
    );
    let serialized = rule.to_string();
    assert!(!serialized.contains("resolveSpellInstruction"));
    assert!(!serialized.contains("operation"));
}

#[test]
fn literal_gain_and_single_permanent_type_reuse_the_same_submachines() {
    let parsed = parse(
        "You get three {E} (energy counters), then you may pay any amount of {E}. Destroy each creature with mana value less than or equal to the amount of {E} paid this way.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("sibling primitive rule");
    assert!(rule.get("declaration").is_none());
    assert_eq!(rule["effects"][0]["count"]["value"], 3);
    assert_eq!(
        rule["effects"][3]["permanent"]["where"]["operands"][0],
        json!({ "kind": "cardTypeContains", "value": "Creature" })
    );
}

#[test]
fn wrong_counter_symbol_is_a_committed_typed_failure() {
    let parsed = parse(
        "You get X {C} (energy counters), then you may pay any amount of {E}. Destroy each creature with mana value less than or equal to the amount of {E} paid this way.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("player-counter-payment-sweep")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("player-counter.symbol")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("{C}"));
}

#[test]
fn dangling_oxford_union_fails_in_the_shared_selector() {
    let parsed = parse(
        "You get X {E} (energy counters), then you may pay any amount of {E}. Destroy each artifact, creature, and with mana value less than or equal to the amount of {E} paid this way.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("selector.type-union-list.branch")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("with"));
}

#[test]
fn wrong_payment_anaphor_and_trailing_text_are_structured_failures() {
    for (text, state, found) in [
        (
            "You get X {E} (energy counters), then you may pay any amount of {E}. Destroy each creature with mana value less than or equal to the amount of {E} paid that way.",
            "effect.counter-payment-sweep.reference",
            "that",
        ),
        (
            "You get X {E} (energy counters), then you may pay any amount of {E}. Destroy each creature with mana value less than or equal to the amount of {E} paid this way. Draw a card.",
            "sequence.player-counter-sweep.complete",
            "Draw",
        ),
    ] {
        let parsed = parse(text);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(ability.status, "unsupported");
        assert_eq!(ability.diagnostics[0].state.as_deref(), Some(state));
        assert_eq!(ability.diagnostics[0].found.as_deref(), Some(found));
    }
}

#[test]
fn wrath_promoted_vocabulary_has_no_unknown_tokens() {
    let parsed = parse(WRATH_ORACLE);
    let entities = &parsed.abilities[0].entities;
    let unknown = entities
        .iter()
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();

    assert!(unknown.is_empty(), "unknown tokens: {unknown:?}");
    assert!(
        entities
            .iter()
            .filter(|entity| entity.raw == "{E}")
            .all(|entity| entity.token_type == "counterSymbol")
    );
    assert!(entities.iter().any(|entity| {
        entity.raw.eq_ignore_ascii_case("energy") && entity.token_type == "counterTypeWord"
    }));
    assert!(entities.iter().any(|entity| {
        entity.raw.eq_ignore_ascii_case("amount") && entity.token_type == "quantityNoun"
    }));
}
