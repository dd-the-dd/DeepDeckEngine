use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

fn parse_spell(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Zone Change Search".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{B}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn destroy_then_controller_search_is_entirely_native_and_primitive() {
    let parsed = parse_spell(
        "Destroy target creature or planeswalker. Its controller may search their library for a basic land card, put it onto the battlefield tapped, then shuffle.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("the zone-change/search grammar should emit primitive IR");
    let effects = rule["effects"]
        .as_array()
        .expect("ordered primitive effects");
    assert_eq!(effects[0]["kind"], "bind");
    assert_eq!(effects[0]["value"]["kind"], "controllerOf");
    assert_eq!(effects[1]["kind"], "destroyPermanent");
    assert_eq!(effects[2]["kind"], "optionalEffects");
    let search = &effects[2]["effects"];
    assert_eq!(search[0]["kind"], "chooseCards");
    assert_eq!(
        search[0]["candidates"]["where"],
        json!({
            "kind": "and",
            "operands": [
                { "kind": "typeLineContains", "value": "Basic" },
                { "kind": "cardTypeContains", "value": "Land" },
            ],
        })
    );
    assert_eq!(search[1]["kind"], "moveCards");
    assert_eq!(search[1]["to"]["tapped"], true);
    assert_eq!(search[2]["kind"], "shuffleZone");
    assert_eq!(search[3]["kind"], "recordLibrarySearch");
    assert!(!rule.to_string().contains("resolveSpellInstruction"));
}

#[test]
fn exile_optional_target_and_that_card_reuse_the_same_local_automata() {
    let parsed = parse_spell(
        "Exile up to one target artifact. Its controller may search their library for a basic land card, put that card onto the battlefield, then shuffle.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().expect("sibling parses");
    assert_eq!(rule["declaration"]["decisions"][0]["minimum"], 0);
    assert_eq!(rule["effects"][1]["kind"], "exilePermanent");
    assert_eq!(rule["effects"][2]["effects"][1]["to"]["tapped"], false);
}

#[test]
fn claimed_near_misses_fail_in_the_responsible_child_automaton() {
    for (oracle_text, state, found) in [
        (
            "Destroy target hero. Its controller may search their library for a basic land card, put it onto the battlefield tapped, then shuffle.",
            "selector.predicate.start",
            "hero",
        ),
        (
            "Destroy target creature. Its controller may search their library for a basic hero card, put it onto the battlefield tapped, then shuffle.",
            "selector.search-card.card-noun",
            "hero",
        ),
        (
            "Destroy target creature. Its controller may search their library for a basic land card, put it into their hand, then shuffle.",
            "effect.library-search.destination",
            "into",
        ),
    ] {
        let parsed = parse_spell(oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(diagnostic.code, "oracle_machine_failure");
        assert_eq!(
            diagnostic.automaton.as_deref(),
            Some("zone-change-library-search")
        );
        assert_eq!(diagnostic.state.as_deref(), Some(state));
        assert_eq!(diagnostic.found.as_deref(), Some(found));
    }
}

#[test]
fn owner_search_is_not_approximated_as_the_target_controller() {
    let parsed = parse_spell(
        "Destroy target creature. Its owner may search their library for a basic land card, put it onto the battlefield tapped, then shuffle.",
    );

    assert_ne!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
}
