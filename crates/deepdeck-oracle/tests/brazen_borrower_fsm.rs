use deepdeck_oracle::oracle::{
    OracleCardFace, OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance,
    parse_oracle_card,
};
use serde_json::json;

const FRONT_ORACLE: &str = "Flash\nFlying\nBrazen Borrower can block only creatures with flying.";
const ADVENTURE_ORACLE: &str =
    "Return target nonland permanent an opponent controls to its owner's hand.";

fn parse_adventure(front_oracle: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Brazen Borrower // Petty Theft".to_string(),
        type_line: "Creature \u{2014} Faerie Rogue // Instant \u{2014} Adventure".to_string(),
        mana_cost: Some("{1}{U}{U} // {1}{U}".to_string()),
        oracle_text: None,
        layout: Some("adventure".to_string()),
        faces: vec![
            OracleCardFace {
                id: "brazen-borrower".to_string(),
                name: "Brazen Borrower".to_string(),
                type_line: "Creature \u{2014} Faerie Rogue".to_string(),
                mana_cost: Some("{1}{U}{U}".to_string()),
                oracle_text: front_oracle.to_string(),
                power: Some("3".to_string()),
                toughness: Some("1".to_string()),
                loyalty: None,
            },
            OracleCardFace {
                id: "petty-theft".to_string(),
                name: "Petty Theft".to_string(),
                type_line: "Instant \u{2014} Adventure".to_string(),
                mana_cost: Some("{1}{U}".to_string()),
                oracle_text: ADVENTURE_ORACLE.to_string(),
                power: None,
                toughness: None,
                loyalty: None,
            },
        ],
    })
}

fn parse_single(name: &str, type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn expected_restriction() -> serde_json::Value {
    json!({
        "kind": "staticAbility",
        "source": { "kind": "self" },
        "activeWhile": {
            "kind": "inZone",
            "object": { "kind": "self" },
            "zone": { "kind": "battlefield" },
        },
        "modifiers": [{
            "kind": "blockRestriction",
            "attackers": {
                "kind": "permanents",
                "where": {
                    "kind": "and",
                    "operands": [
                        { "kind": "cardTypeContains", "value": "Creature" },
                        {
                            "kind": "not",
                            "operand": { "kind": "hasKeyword", "value": "flying" },
                        },
                    ],
                },
            },
            "blockers": { "kind": "self" },
        }],
    })
}

#[test]
fn exact_adventure_is_fully_native_and_preserves_all_four_abilities() {
    let parsed = parse_adventure(FRONT_ORACLE);

    // CR 715: the Adventure part is linked to the adventurer card and cast as
    // its Adventure, so the whole card is native canonical coverage.
    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    assert_eq!(parsed.abilities.len(), 4);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native
                && ability.status == "canonical")
    );
    assert_eq!(
        parsed.abilities[3].source.face_id.as_deref(),
        Some("petty-theft")
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["ability"]["kind"],
        "flash"
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref().unwrap()["ability"]["kind"],
        "flying"
    );
    assert_eq!(parsed.abilities[2].rule, Some(expected_restriction()));

    // The Adventure part compiles exactly as the same text on a lone instant.
    let petty_theft = parse_single("Petty Theft", "Instant", ADVENTURE_ORACLE);
    assert_eq!(petty_theft.status, "canonical", "{petty_theft:#?}");
    assert_eq!(
        petty_theft.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    let spell = petty_theft.abilities[0].rule.as_ref().unwrap();
    assert_eq!(parsed.abilities[3].rule.as_ref(), Some(spell));
    assert_eq!(spell["kind"], "spellAbility");
    assert_eq!(
        spell["declaration"]["decisions"][0]["candidates"],
        json!({
            "kind": "permanents",
            "controller": {
                "kind": "opponentsOf",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            },
            "where": {
                "kind": "not",
                "operand": { "kind": "cardTypeContains", "value": "Land" },
            },
        })
    );
    assert_eq!(
        spell["effects"],
        json!([{
            "kind": "returnToOwnersHand",
            "object": { "kind": "chosenTarget", "id": "targetPermanent" },
        }])
    );
    assert!(
        !serde_json::to_string(&parsed)
            .unwrap()
            .contains("canBlockOnlyFlying")
    );
}

#[test]
fn this_token_sibling_uses_the_same_native_restriction() {
    let parsed = parse_single(
        "Sky Token",
        "Token Creature \u{2014} Bird",
        "This token can block only creatures with flying.",
    );

    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(parsed.abilities[0].rule, Some(expected_restriction()));
}

#[test]
fn unknown_selector_keyword_reports_the_native_selector_state() {
    let parsed = parse_single(
        "Sky Watcher",
        "Creature \u{2014} Bird",
        "This creature can block only creatures with skybound.",
    );

    assert_eq!(parsed.status, "unsupported", "{parsed:#?}");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let diagnostic = &parsed.abilities[0].diagnostics[0];
    assert_eq!(diagnostic.automaton.as_deref(), Some("block-restriction"));
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("selector.keyword-qualifier.keyword")
    );
}

#[test]
fn attack_near_miss_is_rejected_by_the_native_action_state() {
    let parsed = parse_single(
        "Sky Watcher",
        "Creature \u{2014} Bird",
        "This creature can attack only creatures with flying.",
    );

    assert_eq!(parsed.status, "unsupported", "{parsed:#?}");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let diagnostic = &parsed.abilities[0].diagnostics[0];
    assert_eq!(diagnostic.automaton.as_deref(), Some("block-restriction"));
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("block-restriction.action")
    );
}

#[test]
fn omen_part_still_refuses_with_its_own_layout_reason() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Omen Drake // Fair Wind".to_string(),
        type_line: "Creature \u{2014} Drake // Sorcery \u{2014} Omen".to_string(),
        mana_cost: Some("{3}{U} // {1}{U}".to_string()),
        oracle_text: None,
        layout: Some("omen".to_string()),
        faces: vec![
            OracleCardFace {
                id: "omen-drake".to_string(),
                name: "Omen Drake".to_string(),
                type_line: "Creature \u{2014} Drake".to_string(),
                mana_cost: Some("{3}{U}".to_string()),
                oracle_text: "Flying".to_string(),
                power: Some("3".to_string()),
                toughness: Some("3".to_string()),
                loyalty: None,
            },
            OracleCardFace {
                id: "fair-wind".to_string(),
                name: "Fair Wind".to_string(),
                type_line: "Sorcery \u{2014} Omen".to_string(),
                mana_cost: Some("{1}{U}".to_string()),
                oracle_text: "Draw a card.".to_string(),
                power: None,
                toughness: None,
                loyalty: None,
            },
        ],
    });

    assert_eq!(parsed.status, "unsupported", "{parsed:#?}");
    let omen = parsed
        .abilities
        .iter()
        .find(|ability| ability.source.face_id.as_deref() == Some("fair-wind"))
        .expect("the Omen part is preserved");
    assert_eq!(omen.status, "unsupported");
    assert_eq!(omen.rule, None);
    assert_eq!(omen.diagnostics[0].code, "card_layout_runtime_missing");
    assert_eq!(
        omen.diagnostics[0].state.as_deref(),
        Some("card-structure.omen-runtime")
    );
}
