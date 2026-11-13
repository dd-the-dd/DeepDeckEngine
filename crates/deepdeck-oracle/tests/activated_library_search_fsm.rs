use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const FLOODED_STRAND_ORACLE: &str = "{T}, Pay 1 life, Sacrifice this land: Search your library for a Plains or Island card, put it onto the battlefield, then shuffle.";
const POLLUTED_DELTA_ORACLE: &str = "{T}, Pay 1 life, Sacrifice this land: Search your library for an Island or Swamp card, put it onto the battlefield, then shuffle.";
const MISTY_RAINFOREST_ORACLE: &str = "{T}, Pay 1 life, Sacrifice this land: Search your library for a Forest or Island card, put it onto the battlefield, then shuffle.";

fn parse_permanent(card_name: &str, type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(String::new()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn flooded_strand_is_native_and_lowers_only_to_primitive_costs_and_effects() {
    let parsed = parse_permanent("Flooded Strand", "Land", FLOODED_STRAND_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
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
                                { "kind": "subtypeContains", "value": "Plains" },
                                { "kind": "subtypeContains", "value": "Island" },
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
fn polluted_delta_reuses_the_native_fetch_land_automaton() {
    let parsed = parse_permanent("Polluted Delta", "Land", POLLUTED_DELTA_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("Polluted Delta rule");
    assert_eq!(
        rule["costs"],
        json!([
            { "kind": "tap", "object": { "kind": "self" } },
            {
                "kind": "payLife",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "amount": { "kind": "integer", "value": 1 },
            },
            { "kind": "sacrificePermanent", "permanent": { "kind": "self" } },
        ])
    );
    assert_eq!(
        rule["effects"][0]["candidates"]["where"],
        json!({
            "kind": "or",
            "operands": [
                { "kind": "subtypeContains", "value": "Island" },
                { "kind": "subtypeContains", "value": "Swamp" },
            ],
        })
    );
    assert_eq!(rule["effects"][1]["kind"], "moveCards");
    assert_eq!(rule["effects"][1]["to"]["tapped"], false);
    assert_eq!(rule["effects"][2]["kind"], "shuffleZone");
    assert_eq!(rule["effects"][3]["kind"], "recordLibrarySearch");
    assert!(!rule.to_string().contains("resolve"));
}

#[test]
fn misty_rainforest_reuses_the_native_fetch_land_automaton() {
    let parsed = parse_permanent("Misty Rainforest", "Land", MISTY_RAINFOREST_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("Misty Rainforest rule");
    assert_eq!(
        rule["costs"],
        json!([
            { "kind": "tap", "object": { "kind": "self" } },
            {
                "kind": "payLife",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "amount": { "kind": "integer", "value": 1 },
            },
            { "kind": "sacrificePermanent", "permanent": { "kind": "self" } },
        ])
    );
    assert_eq!(
        rule["effects"][0]["candidates"]["where"],
        json!({
            "kind": "or",
            "operands": [
                { "kind": "subtypeContains", "value": "Forest" },
                { "kind": "subtypeContains", "value": "Island" },
            ],
        })
    );
    assert_eq!(rule["effects"][1]["kind"], "moveCards");
    assert_eq!(rule["effects"][1]["to"]["tapped"], false);
    assert_eq!(rule["effects"][2]["kind"], "shuffleZone");
    assert_eq!(rule["effects"][3]["kind"], "recordLibrarySearch");
    assert!(!rule.to_string().contains("resolve"));
}

#[test]
fn costs_source_type_articles_subtypes_and_destination_state_are_reusable() {
    for (type_line, oracle_text, life, tapped) in [
        (
            "Artifact",
            "{T}, Pay 2 life, Sacrifice this artifact: Search your library for a Time Lord or a Desert card, put that card onto the battlefield tapped, then shuffle.",
            2,
            true,
        ),
        (
            "Land",
            "{T}, Sacrifice this land: Search your library for an Island or Swamp card, put it onto the battlefield, then shuffle.",
            0,
            false,
        ),
    ] {
        let parsed = parse_permanent("Test Search Permanent", type_line, oracle_text);
        assert_eq!(parsed.status, "canonical", "{oracle_text}");
        assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
        let rule = parsed.abilities[0].rule.as_ref().expect("sibling rule");
        let life_cost = rule["costs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|cost| cost["kind"] == "payLife");
        assert_eq!(
            life_cost.map(|cost| cost["amount"]["value"].as_i64().unwrap()),
            (life > 0).then_some(life)
        );
        assert_eq!(rule["effects"][1]["to"]["tapped"], tapped);
        assert_eq!(rule["effects"][0]["candidates"]["where"]["kind"], "or");
    }
}

#[test]
fn claimed_near_misses_fail_at_the_responsible_local_state() {
    for (oracle_text, state, found) in [
        (
            "{T}, Pay 1 life Sacrifice this land: Search your library for a Plains or Island card, put it onto the battlefield, then shuffle.",
            "activation.library-search.effect-separator",
            "Sacrifice",
        ),
        (
            "{T}, Pay X life, Sacrifice this land: Search your library for a Plains or Island card, put it onto the battlefield, then shuffle.",
            "cost.pay-life.amount-supported",
            "X",
        ),
        (
            "{T}, Pay 1 life, Sacrifice this spell: Search your library for a Plains or Island card, put it onto the battlefield, then shuffle.",
            "cost.sacrifice-self.source-type",
            "spell",
        ),
        (
            "{T}, Pay 1 life, Sacrifice this land: Search your library for a Plains or card, put it onto the battlefield, then shuffle.",
            "selector.search-card.subtype",
            "card",
        ),
    ] {
        let parsed = parse_permanent("Test Search Land", "Land", oracle_text);
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
