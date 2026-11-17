use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const FLOW_STATE_ORACLE: &str = "Look at the top three cards of your library. Put one of them into your hand and the rest on the bottom of your library in any order. If there is an instant card and a sorcery card in your graveyard, instead put two of them into your hand and the rest on the bottom of your library in any order.";

fn parse(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Conditional Partition".to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some("{2}{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn flow_state_is_one_native_partition_with_a_conditional_count() {
    let parsed = parse(FLOW_STATE_ORACLE);
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("primitive rule");
    let effects = rule["effects"].as_array().expect("effect list");
    assert_eq!(effects.len(), 5);
    assert_eq!(effects[0]["kind"], "lookAtTopCards");
    assert_eq!(effects[1]["kind"], "chooseCards");
    assert_eq!(effects[2]["kind"], "moveCards");
    assert_eq!(effects[3]["kind"], "chooseOrder");
    assert_eq!(effects[4]["kind"], "moveCards");

    let count = &effects[1]["count"];
    assert_eq!(count["kind"], "conditionalValue");
    assert_eq!(count["ifTrue"]["operands"][0]["value"], 2);
    assert_eq!(count["ifFalse"]["operands"][0]["value"], 1);
    assert_eq!(
        count["condition"],
        json!({
            "kind": "and",
            "operands": [
                {
                    "kind": "compare",
                    "operator": ">=",
                    "left": {
                        "kind": "countCards",
                        "zone": {
                            "kind": "graveyard",
                            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                        },
                        "where": { "kind": "cardTypeContains", "value": "Instant" },
                    },
                    "right": { "kind": "integer", "value": 1 },
                },
                {
                    "kind": "compare",
                    "operator": ">=",
                    "left": {
                        "kind": "countCards",
                        "zone": {
                            "kind": "graveyard",
                            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                        },
                        "where": { "kind": "cardTypeContains", "value": "Sorcery" },
                    },
                    "right": { "kind": "integer", "value": 1 },
                },
            ],
        })
    );
    let serialized = rule.to_string();
    assert!(!serialized.contains("resolveSpellInstruction"));
    assert!(!serialized.contains("conditionalEffect"));
}

#[test]
fn counts_and_card_type_conjunctions_are_generic_siblings() {
    let parsed = parse(
        "Look at the top four cards of your library. Put two of them into your hand and the rest on the bottom of your library in a random order. If there is a creature card and a land card in your graveyard, instead put three of them into your hand and the rest on the bottom of your library in a random order.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("sibling primitive rule");
    let count = &rule["effects"][1]["count"];
    assert_eq!(count["ifTrue"]["operands"][0]["value"], 3);
    assert_eq!(count["ifFalse"]["operands"][0]["value"], 2);
    assert_eq!(
        count["condition"]["operands"][0]["left"]["where"]["value"],
        "Creature"
    );
    assert_eq!(
        count["condition"]["operands"][1]["left"]["where"]["value"],
        "Land"
    );
    assert_eq!(
        rule["effects"].as_array().unwrap().last().unwrap()["order"]["kind"],
        "random"
    );
}

#[test]
fn unsupported_card_predicate_fails_in_the_shared_selector() {
    let parsed = parse(
        "Look at the top three cards of your library. Put one of them into your hand and the rest on the bottom of your library in any order. If there is a mystery card and a sorcery card in your graveyard, instead put two of them into your hand and the rest on the bottom of your library in any order.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("conditional-top-cards-partition")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("selector.card-type-union.branch")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("mystery"));
}

#[test]
fn trailing_unconsumed_sentence_is_a_structured_native_failure() {
    let parsed = parse(&format!("{FLOW_STATE_ORACLE} Draw a card."));
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("sequence.conditional-top-cards.complete")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("Draw"));
}

#[test]
fn flow_state_promoted_vocabulary_has_no_unknown_tokens() {
    let parsed = parse(FLOW_STATE_ORACLE);
    let unknown = parsed.abilities[0]
        .entities
        .iter()
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();

    assert!(unknown.is_empty(), "unknown tokens: {unknown:?}");
}
