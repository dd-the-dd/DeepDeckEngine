use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const ORACLE: &str = "{T}, Sacrifice this land: Search your library for a basic land card, put it onto the battlefield tapped, then shuffle. Then if you control four or more lands, untap that land.";

fn parse_permanent(card_name: &str, type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn fabled_passage_is_native_and_lowers_to_independent_primitives() {
    let parsed = parse_permanent("Fabled Passage", "Land", ORACLE);

    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(
        ability
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown")
    );
    let rule = ability.rule.as_ref().expect("primitive activated rule");
    assert_eq!(
        rule,
        &json!({
            "kind": "activatedAbility",
            "source": { "kind": "self" },
            "costs": [
                { "kind": "tap", "object": { "kind": "self" } },
                { "kind": "sacrificePermanent", "permanent": { "kind": "self" } },
            ],
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
                            "kind": "and",
                            "operands": [
                                { "kind": "typeLineContains", "value": "Basic" },
                                { "kind": "cardTypeContains", "value": "Land" },
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
                {
                    "kind": "conditionalEffect",
                    "condition": {
                        "kind": "compare",
                        "operator": ">=",
                        "left": {
                            "kind": "countPermanents",
                            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                            "where": { "kind": "cardTypeContains", "value": "Land" },
                            "excludeSource": false,
                        },
                        "right": { "kind": "integer", "value": 4 },
                    },
                    "then": [{
                        "kind": "untapPermanents",
                        "objects": { "kind": "decisionResult", "decisionId": "searchedCards" },
                    }],
                    "else": [],
                },
            ],
        })
    );
    assert!(!rule.to_string().contains("resolveFabledPassage"));
    assert!(!rule.to_string().contains("resolveTriggeredInstruction"));
}

#[test]
fn fabled_passage_concepts_generalize_to_other_permanent_types() {
    let parsed = parse_permanent(
        "Test Search Relic",
        "Artifact",
        "{T}, Sacrifice this artifact: Search your library for an artifact card, put it onto the battlefield tapped, then shuffle. Then if you control three or more artifacts, untap that artifact.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().expect("sibling rule");
    assert_eq!(
        rule["effects"][4]["condition"]["left"]["where"],
        json!({ "kind": "cardTypeContains", "value": "Artifact" })
    );
    assert_eq!(rule["effects"][4]["condition"]["right"]["value"], 3);
    assert_eq!(rule["effects"][4]["then"][0]["kind"], "untapPermanents");
}

#[test]
fn fabled_passage_followup_near_misses_fail_in_the_responsible_local_state() {
    for (oracle_text, state, found) in [
        (
            "{T}, Sacrifice this land: Search your library for a basic land card, put it onto the battlefield tapped, then shuffle. Then if you control four and more lands, untap that land.",
            "condition.controlled-permanent-count.comparator-connector",
            "and",
        ),
        (
            "{T}, Sacrifice this land: Search your library for a basic land card, put it onto the battlefield tapped, then shuffle. Then if you control four or more lands untap that land.",
            "condition.envelope.separator",
            "untap",
        ),
        (
            "{T}, Sacrifice this land: Search your library for a basic land card, put it onto the battlefield tapped, then shuffle. Then if you control four or more lands, untap a land.",
            "effect.untap-bound-collection.determiner",
            "a",
        ),
        (
            "{T}, Sacrifice this land: Search your library for a basic land card, put it onto the battlefield tapped, then shuffle. Then if you control four or more lands, untap that creature.",
            "effect.untap-bound-collection.complete",
            ".",
        ),
    ] {
        let parsed = parse_permanent("Test Search Land", "Land", oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(ability.status, "unsupported", "{oracle_text}");
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(
            diagnostic.automaton.as_deref(),
            Some("activated-library-search")
        );
        assert_eq!(diagnostic.state.as_deref(), Some(state), "{oracle_text}");
        assert_eq!(diagnostic.found.as_deref(), Some(found), "{oracle_text}");
    }
}
