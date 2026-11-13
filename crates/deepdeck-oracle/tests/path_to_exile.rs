use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

fn parse_spell(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Zone Change Search".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn path_to_exile_compiles_to_an_exact_primitive_sequence() {
    let parsed = parse_spell(
        "Exile target creature. Its controller may search their library for a basic land card, put that card onto the battlefield tapped, then shuffle.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("Path to Exile should produce primitive IR");
    assert_eq!(
        rule,
        &json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [{
                    "kind": "chooseTargets",
                    "id": "targetPermanent",
                    "minimum": 1,
                    "maximum": 1,
                    "candidates": {
                        "kind": "permanents",
                        "where": { "kind": "cardTypeContains", "value": "Creature" },
                    },
                }],
            },
            "effects": [
                {
                    "kind": "bind",
                    "id": "targetController",
                    "value": {
                        "kind": "controllerOf",
                        "object": { "kind": "chosenTarget", "id": "targetPermanent" },
                    },
                },
                {
                    "kind": "exilePermanent",
                    "permanent": { "kind": "chosenTarget", "id": "targetPermanent" },
                },
                {
                    "kind": "optionalEffects",
                    "player": { "kind": "boundValue", "id": "targetController" },
                    "effects": [
                        {
                            "kind": "chooseCards",
                            "id": "searchedCards",
                            "player": { "kind": "boundValue", "id": "targetController" },
                            "minimum": 0,
                            "maximum": 1,
                            "candidates": {
                                "kind": "cards",
                                "zone": {
                                    "kind": "library",
                                    "player": { "kind": "boundValue", "id": "targetController" },
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
                                "player": { "kind": "boundValue", "id": "targetController" },
                                "tapped": true,
                            },
                        },
                        {
                            "kind": "shuffleZone",
                            "zone": {
                                "kind": "library",
                                "player": { "kind": "boundValue", "id": "targetController" },
                            },
                        },
                        {
                            "kind": "recordLibrarySearch",
                            "player": { "kind": "boundValue", "id": "targetController" },
                            "cards": { "kind": "decisionResult", "decisionId": "searchedCards" },
                        },
                    ],
                },
            ],
        })
    );
    assert!(!rule.to_string().contains("resolveSpellInstruction"));
}

#[test]
fn zone_change_search_pattern_supports_destroy_optional_targets_and_other_filters() {
    let parsed = parse_spell(
        "Destroy up to one target artifact. That player may search their library for an Equipment card, put it onto the battlefield, then shuffle.",
    );

    assert_eq!(parsed.status, "canonical");
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("the generic sibling should produce primitive IR");
    assert_eq!(rule["declaration"]["decisions"][0]["minimum"], 0);
    assert_eq!(rule["effects"][0]["kind"], "bind");
    assert_eq!(rule["effects"][1]["kind"], "destroyPermanent");
    assert_eq!(rule["effects"][2]["kind"], "optionalEffects");
    assert_eq!(
        rule["effects"][2]["effects"][0]["candidates"]["where"],
        json!({ "kind": "subtypeContains", "value": "Equipment" })
    );
    assert_eq!(rule["effects"][2]["effects"][1]["to"]["tapped"], false);
}

#[test]
fn owner_search_near_miss_is_not_approximated_as_a_controller_search() {
    let parsed = parse_spell(
        "Exile target creature. Its owner may search their library for a basic land card, put that card onto the battlefield tapped, then shuffle.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert!(parsed.abilities[0].rule.is_none());
}
