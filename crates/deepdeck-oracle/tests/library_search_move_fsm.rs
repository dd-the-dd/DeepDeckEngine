use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const FARSEEK_ORACLE: &str = "Search your library for a Plains, Island, Swamp, or Mountain card, put it onto the battlefield tapped, then shuffle.";

fn parse_spell(card_name: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some("{1}{G}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn farseek_is_native_and_preserves_its_primitive_ir() {
    let parsed = parse_spell("Farseek", FARSEEK_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule.as_ref(),
        Some(&json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "effects": [
                {
                    "kind": "chooseCards",
                    "id": "searchedCards",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "minimum": 0,
                    "maximum": 1,
                    "candidates": {
                        "kind": "cards",
                        "zone": {
                            "kind": "library",
                            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                        },
                        "where": {
                            "kind": "or",
                            "operands": [
                                { "kind": "subtypeContains", "value": "Plains" },
                                { "kind": "subtypeContains", "value": "Island" },
                                { "kind": "subtypeContains", "value": "Swamp" },
                                { "kind": "subtypeContains", "value": "Mountain" },
                            ],
                        },
                    },
                },
                {
                    "kind": "moveCards",
                    "cards": { "kind": "decisionResult", "decisionId": "searchedCards" },
                    "to": {
                        "kind": "battlefield",
                        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                        "tapped": true,
                    },
                },
                {
                    "kind": "shuffleZone",
                    "zone": {
                        "kind": "library",
                        "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    },
                },
                {
                    "kind": "recordLibrarySearch",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "cards": { "kind": "decisionResult", "decisionId": "searchedCards" },
                },
            ],
        }))
    );
    assert!(
        !ability
            .rule
            .as_ref()
            .unwrap()
            .to_string()
            .contains("resolveSpellInstruction")
    );
}

#[test]
fn subtype_union_supports_different_and_multiword_subtypes() {
    let parsed = parse_spell(
        "Test Search",
        "Search your library for a Time Lord or Desert card, put that card onto the battlefield, then shuffle.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["effects"][0]["candidates"]["where"],
        json!({
            "kind": "or",
            "operands": [
                { "kind": "subtypeContains", "value": "Time Lord" },
                { "kind": "subtypeContains", "value": "Desert" },
            ],
        })
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["effects"][1]["to"]["tapped"],
        false
    );
}

#[test]
fn claimed_near_misses_fail_in_the_responsible_local_state() {
    for (oracle_text, state, found) in [
        (
            "Search your library for a Plains or card, put it onto the battlefield tapped, then shuffle.",
            "selector.search-card.subtype",
            Some("card"),
        ),
        (
            "Search your library for a hero card, put it onto the battlefield tapped, then shuffle.",
            "selector.search-card.predicate",
            Some("hero"),
        ),
        // The search ends at its selection; its ordinary later actions own
        // the number agreement of `them` with one found card.
        (
            "Search your library for a Forest card, put them onto the battlefield tapped, then shuffle.",
            "lower.card-action.bindings",
            None,
        ),
    ] {
        let parsed = parse_spell("Test Search", oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(ability.status, "unsupported");
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(diagnostic.code, "oracle_machine_failure");
        assert_eq!(
            diagnostic.automaton.as_deref(),
            Some("contextual-effect-program")
        );
        assert_eq!(diagnostic.state.as_deref(), Some(state));
        assert_eq!(diagnostic.found.as_deref(), found);
    }
    // A comma joins ordinary actions as `and` does (`Draw a card, gain 1
    // life.`): the bound search's comma-joined shuffle is that coordination.
    let coordinated = parse_spell(
        "Test Search",
        "Search your library for a Forest card, put it onto the battlefield tapped, shuffle.",
    );
    assert_eq!(coordinated.abilities[0].status, "canonical");
}
