use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const VERDANT_CATACOMBS_ORACLE: &str = "{T}, Pay 1 life, Sacrifice this land: Search your library for a Swamp or Forest card, put it onto the battlefield, then shuffle.";
const BLOODSTAINED_MIRE_ORACLE: &str = "{T}, Pay 1 life, Sacrifice this land: Search your library for a Swamp or Mountain card, put it onto the battlefield, then shuffle.";

fn parse_land(card_name: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Land".to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn verdant_catacombs_is_native_and_lowers_to_independent_primitives() {
    let parsed = parse_land("Verdant Catacombs", VERDANT_CATACOMBS_ORACLE);

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
            "kind": "activatedAbility",
            "source": { "kind": "self" },
            "costs": [
                { "kind": "tap", "object": { "kind": "self" } },
                {
                    "kind": "payLife",
                    "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                    "amount": { "kind": "integer", "value": 1 },
                },
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
                            "kind": "or",
                            "operands": [
                                { "kind": "subtypeContains", "value": "Swamp" },
                                { "kind": "subtypeContains", "value": "Forest" },
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
                        "tapped": false,
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
            .contains("resolve")
    );
}

#[test]
fn bloodstained_mire_reuses_the_same_cost_selector_and_search_automata() {
    let parsed = parse_land("Bloodstained Mire", BLOODSTAINED_MIRE_ORACLE);

    assert_eq!(parsed.status, "canonical");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("primitive fetch-land rule");
    assert_eq!(
        rule["effects"][0]["candidates"]["where"],
        json!({
            "kind": "or",
            "operands": [
                { "kind": "subtypeContains", "value": "Swamp" },
                { "kind": "subtypeContains", "value": "Mountain" },
            ],
        })
    );
    assert_eq!(rule["effects"][1]["to"]["tapped"], false);
    assert_eq!(rule["effects"][2]["kind"], "shuffleZone");
}

#[test]
fn promoted_fetch_land_near_misses_fail_at_the_responsible_state() {
    for (oracle_text, state, found) in [
        (
            "{T}, Pay 1 life, Sacrifice this land: Search your library for a Swamp and Forest card, put it onto the battlefield, then shuffle.",
            "selector.search-card.card-noun",
            "and",
        ),
        (
            "{T}, Pay 1 life, Sacrifice this land: Search your library for a Swamp or Forest card, put it onto the battlefield, shuffle.",
            "effect.library-search.post-move-shuffle-separator",
            "shuffle",
        ),
        (
            "{T}, Pay 1 life, Sacrifice this land: Search your library for a Swamp or Forest card, put it onto the battlefield, then shuffle twice.",
            "activation.library-search.terminator",
            "twice",
        ),
    ] {
        let parsed = parse_land("Test Fetch Land", oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(
            ability.recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
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
