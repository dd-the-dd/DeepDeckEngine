use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "Destroy target permanent an opponent controls. Its controller may search their library for a basic land card, put it onto the battlefield, then shuffle.";

fn parse_spell(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Trophy".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{B}{G}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn assassins_trophy_is_native_and_composes_only_primitive_effects() {
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
    let rule = ability.rule.as_ref().expect("primitive rule");
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"],
        json!({
            "kind": "permanents",
            "controller": {
                "kind": "opponentsOf",
                "player": {
                    "kind": "controllerOf",
                    "object": { "kind": "self" },
                },
            },
            "where": null,
        })
    );
    let effects = rule["effects"].as_array().expect("ordered effects");
    assert_eq!(effects[0]["kind"], "bind");
    assert_eq!(effects[0]["value"]["kind"], "controllerOf");
    assert_eq!(effects[1]["kind"], "destroyPermanent");
    assert_eq!(effects[2]["kind"], "optionalEffects");
    assert_eq!(effects[2]["effects"][0]["kind"], "chooseCards");
    assert_eq!(effects[2]["effects"][1]["kind"], "moveCards");
    assert_eq!(effects[2]["effects"][1]["to"]["tapped"], false);
    assert_eq!(effects[2]["effects"][2]["kind"], "shuffleZone");
    assert_eq!(effects[2]["effects"][3]["kind"], "recordLibrarySearch");
}

#[test]
fn controller_qualifier_is_shared_across_object_types_and_player_scopes() {
    for (oracle, expected_controller_kind, expected_type) in [
        (
            "Destroy target artifact an opponent controls. Its controller may search their library for a basic land card, put it onto the battlefield, then shuffle.",
            "opponentsOf",
            "Artifact",
        ),
        (
            "Destroy target creature you control. Its controller may search their library for a basic land card, put it onto the battlefield, then shuffle.",
            "controllerOf",
            "Creature",
        ),
    ] {
        let parsed = parse_spell(oracle);
        assert_eq!(parsed.status, "canonical");
        assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
        let candidates = &parsed.abilities[0].rule.as_ref().unwrap()["declaration"]["decisions"][0]
            ["candidates"];
        assert_eq!(candidates["controller"]["kind"], expected_controller_kind);
        assert_eq!(candidates["where"]["value"], expected_type);
    }
}

#[test]
fn malformed_controller_qualifiers_fail_in_the_selector_child() {
    for (oracle, state, found) in [
        (
            "Destroy target permanent an rival controls. Its controller may search their library for a basic land card, put it onto the battlefield, then shuffle.",
            "selector.controller-qualifier.opponent",
            "rival",
        ),
        (
            "Destroy target permanent an opponent owns. Its controller may search their library for a basic land card, put it onto the battlefield, then shuffle.",
            "selector.controller-qualifier.relation",
            "owns",
        ),
    ] {
        let parsed = parse_spell(oracle);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(ability.status, "unsupported");
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(
            diagnostic.automaton.as_deref(),
            Some("zone-change-library-search")
        );
        assert_eq!(diagnostic.state.as_deref(), Some(state));
        assert_eq!(diagnostic.found.as_deref(), Some(found));
    }
}
