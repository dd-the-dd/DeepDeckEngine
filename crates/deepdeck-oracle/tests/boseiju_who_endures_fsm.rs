use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const BOSEIJU_ORACLE: &str = "Channel — {1}{G}, Discard this card: Destroy target artifact, enchantment, or nonbasic land an opponent controls. That player may search their library for a land card with a basic land type, put it onto the battlefield, then shuffle. This ability costs {1} less to activate for each legendary creature you control.";

fn parse(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Boseiju, Who Endures".to_string(),
        type_line: "Legendary Land".to_string(),
        mana_cost: None,
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn channel_zone_change_search_and_reduction_are_native_primitives() {
    let parsed = parse(BOSEIJU_ORACLE);
    let ability = &parsed.abilities[0];
    let rule = ability.rule.as_ref().expect("primitive activated rule");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(rule["activationZone"], "hand");
    assert_eq!(
        rule["costs"],
        json!([
            { "kind": "payMana", "manaCost": "{1}{G}" },
            { "kind": "discardCard", "card": { "kind": "self" } },
        ])
    );
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"],
        json!({
            "kind": "permanents",
            "controller": {
                "kind": "opponentsOf",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            },
            "where": {
                "kind": "or",
                "operands": [
                    { "kind": "cardTypeContains", "value": "Artifact" },
                    { "kind": "cardTypeContains", "value": "Enchantment" },
                    {
                        "kind": "and",
                        "operands": [
                            {
                                "kind": "not",
                                "operand": { "kind": "typeLineContains", "value": "Basic" },
                            },
                            { "kind": "cardTypeContains", "value": "Land" },
                        ],
                    },
                ],
            },
        })
    );
    assert_eq!(rule["effects"][0]["kind"], "bind");
    assert_eq!(rule["effects"][1]["kind"], "destroyPermanent");
    assert_eq!(rule["effects"][2]["kind"], "optionalEffects");
    assert_eq!(
        rule["effects"][2]["effects"]
            .as_array()
            .expect("optional search children")
            .iter()
            .map(|effect| effect["kind"].as_str().expect("effect kind"))
            .collect::<Vec<_>>(),
        [
            "chooseCards",
            "moveCards",
            "shuffleZone",
            "recordLibrarySearch"
        ]
    );
    assert_eq!(
        rule["effects"][2]["effects"][0]["candidates"]["where"],
        json!({
            "kind": "and",
            "operands": [
                { "kind": "cardTypeContains", "value": "Land" },
                {
                    "kind": "or",
                    "operands": [
                        { "kind": "subtypeContains", "value": "Plains" },
                        { "kind": "subtypeContains", "value": "Island" },
                        { "kind": "subtypeContains", "value": "Swamp" },
                        { "kind": "subtypeContains", "value": "Mountain" },
                        { "kind": "subtypeContains", "value": "Forest" },
                    ],
                },
            ],
        })
    );
    assert_eq!(
        rule["manaCostReduction"],
        json!({
            "kind": "countPermanents",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            "where": {
                "kind": "and",
                "operands": [
                    { "kind": "isLegendary" },
                    { "kind": "cardTypeContains", "value": "Creature" },
                ],
            },
        })
    );
    assert!(!rule.to_string().contains("resolveTriggeredInstruction"));
}

#[test]
fn otawara_reuses_label_cost_union_return_and_reduction_without_search() {
    let parsed = parse(
        "Channel — {3}{U}, Discard this card: Return target artifact, creature, enchantment, or planeswalker to its owner's hand. This ability costs {1} less to activate for each legendary creature you control.",
    );
    let ability = &parsed.abilities[0];
    let rule = ability
        .rule
        .as_ref()
        .unwrap_or_else(|| panic!("Otawara sibling primitive rule: {ability:#?}"));

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(rule["activationZone"], "hand");
    assert_eq!(rule["costs"][0]["manaCost"], "{3}{U}");
    assert_eq!(rule["effects"].as_array().map(Vec::len), Some(1));
    assert_eq!(rule["effects"][0]["kind"], "returnToOwnersHand");
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"]["kind"],
        "or"
    );
}

#[test]
fn dangling_oxford_target_is_an_explicit_failure() {
    let parsed = parse(
        "Channel — {1}{G}, Discard this card: Destroy target artifact, enchantment, or an opponent controls. This ability costs {1} less to activate for each legendary creature you control.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    // `, or an opponent controls` does not continue the object list, so the
    // selector returns the comma (roadmap #12), and the effect parent failing
    // at it reports the list's own reason: the dangling `, or`.
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("selector.predicate.after-comma"),
        "{:#?}",
        ability.diagnostics
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("an"));
}

#[test]
fn that_player_without_target_binding_is_a_structured_reference_failure() {
    let parsed = parse(
        "Channel — {1}{G}, Discard this card: Draw a card. That player may search their library for a land card with a basic land type, put it onto the battlefield, then shuffle. This ability costs {1} less to activate for each legendary creature you control.",
    );
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    // The shared binding environment resolves `that player`: no player was
    // declared and no event records one, so the reference itself fails.
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("player-subject.bound.resolve")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("That"));
}

#[test]
fn trailing_sentence_fails_full_consumption_after_reduction_metadata() {
    let parsed = parse(&format!("{BOSEIJU_ORACLE} Draw a card."));
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    // The labeled zone-change family claims only a complete reading; the
    // shared activated-ability parser then owns the unconsumed sentence.
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("activation.effect-sequence.complete"),
        "{:#?}",
        ability.diagnostics
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("Draw"));
}

#[test]
fn boseiju_promoted_vocabulary_has_no_unknown_tokens() {
    let parsed = parse(BOSEIJU_ORACLE);
    let unknown = parsed.abilities[0]
        .entities
        .iter()
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();

    assert!(unknown.is_empty(), "unknown tokens: {unknown:?}");
}
