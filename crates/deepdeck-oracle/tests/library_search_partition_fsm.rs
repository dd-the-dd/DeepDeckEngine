use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "Search your library for up to two basic land cards, reveal those cards, put one onto the battlefield tapped and the other into your hand, then shuffle.";

fn parse_spell(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Search Partition".to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some("{2}{G}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn exact_search_reveal_partition_is_native_and_preserves_the_primitive_ir() {
    let parsed = parse_spell(EXACT_ORACLE);

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
        ability.rule.as_ref(),
        Some(&json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": [
                {
                    "kind": "chooseCards",
                    "id": "searchedCards",
                    "player": {
                        "kind": "controllerOf",
                        "object": { "kind": "self" },
                    },
                    "minimum": 0,
                    "maximum": 2,
                    "candidates": {
                        "kind": "cards",
                        "zone": {
                            "kind": "library",
                            "player": {
                                "kind": "controllerOf",
                                "object": { "kind": "self" },
                            },
                        },
                        "where": {
                            "kind": "and",
                            "operands": [
                                { "kind": "typeLineContains", "value": "Basic" },
                                { "kind": "cardTypeContains", "value": "Land" },
                            ],
                        },
                    },
                },
                {
                    "kind": "revealCards",
                    "cards": { "kind": "decisionResult", "decisionId": "searchedCards" },
                },
                {
                    "kind": "chooseCards",
                    "id": "battlefieldCard",
                    "player": {
                        "kind": "controllerOf",
                        "object": { "kind": "self" },
                    },
                    "from": { "kind": "decisionResult", "decisionId": "searchedCards" },
                    "count": { "kind": "integer", "value": 1 },
                },
                {
                    "kind": "moveCards",
                    "cards": { "kind": "decisionResult", "decisionId": "battlefieldCard" },
                    "to": {
                        "kind": "battlefield",
                        "player": {
                            "kind": "controllerOf",
                            "object": { "kind": "self" },
                        },
                        "tapped": true,
                    },
                },
                {
                    "kind": "moveCards",
                    "cards": {
                        "kind": "setDifference",
                        "left": { "kind": "decisionResult", "decisionId": "searchedCards" },
                        "right": { "kind": "decisionResult", "decisionId": "battlefieldCard" },
                    },
                    "to": {
                        "kind": "hand",
                        "player": {
                            "kind": "controllerOf",
                            "object": { "kind": "self" },
                        },
                    },
                },
                {
                    "kind": "shuffleZone",
                    "zone": {
                        "kind": "library",
                        "player": {
                            "kind": "controllerOf",
                            "object": { "kind": "self" },
                        },
                    },
                },
                {
                    "kind": "recordLibrarySearch",
                    "player": {
                        "kind": "controllerOf",
                        "object": { "kind": "self" },
                    },
                    "cards": { "kind": "decisionResult", "decisionId": "searchedCards" },
                },
            ],
        }))
    );
}

#[test]
fn collection_pronoun_filter_and_tapped_modifier_are_shared_grammar_choices() {
    let parsed = parse_spell(
        "Search your library for up to two land cards, reveal them, put one onto the battlefield and the other into your hand, then shuffle.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().expect("sibling rule");
    assert_eq!(
        rule["effects"][0]["candidates"]["where"],
        json!({ "kind": "cardTypeContains", "value": "Land" })
    );
    assert_eq!(rule["effects"][3]["to"]["tapped"], false);
}

#[test]
fn claimed_near_misses_fail_in_the_responsible_local_state() {
    for (oracle_text, state, found) in [
        (
            "Search your library for up to two basic land cards, reveal that card, put one onto the battlefield tapped and the other into your hand, then shuffle.",
            "effect.reveal-collection.reference",
            Some("that"),
        ),
        (
            "Search your library for up to three basic land cards, reveal those cards, put one onto the battlefield tapped and the other into your hand, then shuffle.",
            "effect.collection-partition.remainder-reference",
            Some("the"),
        ),
        (
            "Search your library for up to two basic land cards, reveal those cards, put one onto the battlefield tapped and the other into your hand, then shuffle",
            "sequence.library-search-partition.terminator",
            None,
        ),
    ] {
        let parsed = parse_spell(oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(ability.status, "unsupported");
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(
            diagnostic.automaton.as_deref(),
            Some("library-search-partition")
        );
        assert_eq!(diagnostic.state.as_deref(), Some(state));
        assert_eq!(diagnostic.found.as_deref(), found);
    }
}
