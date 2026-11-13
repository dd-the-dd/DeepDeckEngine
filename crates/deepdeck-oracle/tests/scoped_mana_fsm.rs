use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

fn parse_scoped_mana(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Mana Rock".to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn commander_identity_mana_is_native_and_primitive() {
    let parsed =
        parse_scoped_mana("{T}: Add one mana of any color in your commander's color identity.");
    assert_eq!(parsed.status, "canonical");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(
        ability
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown"),
        "promoted vocabulary must not hide lexical UNKNOWN: {:?}",
        ability.entities
    );
    assert_eq!(
        ability.rule.as_ref(),
        Some(&json!({
            "kind": "manaAbility",
            "source": { "kind": "self" },
            "costs": [{
                "kind": "tap",
                "object": { "kind": "self" },
            }],
            "effects": [{
                "kind": "addMana",
                "player": {
                    "kind": "controllerOf",
                    "object": { "kind": "self" },
                },
                "mana": {
                    "kind": "chooseCommanderColor",
                    "amount": 1,
                },
            }],
        }))
    );
}

#[test]
fn literal_amount_reuses_numeric_automaton() {
    let parsed =
        parse_scoped_mana("{T}: Add two mana of any color in your commander's color identity.");
    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["effects"][0]["mana"]["amount"],
        2
    );
}

#[test]
fn malformed_color_identity_is_a_committed_native_failure() {
    let parsed =
        parse_scoped_mana("{T}: Add one mana of any color in your commander's mana identity.");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    let diagnostic = &ability.diagnostics[0];
    assert_eq!(
        diagnostic.automaton.as_deref(),
        Some("simple-tap-scoped-mana-ability")
    );
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("effect.add-scoped-mana.color-identity")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("mana"));
    assert_eq!(diagnostic.expected.as_deref(), Some("colorNoun"));
}

#[test]
fn trailing_text_cannot_be_silently_accepted_or_fall_back() {
    let parsed = parse_scoped_mana(
        "{T}: Add one mana of any color in your commander's color identity plus one.",
    );
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    let diagnostic = &ability.diagnostics[0];
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("activation.scoped-mana.terminator")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("plus"));
}

#[test]
fn unrestricted_any_color_is_owned_by_the_general_mana_automaton() {
    let parsed = parse_scoped_mana("{T}: Add one mana of any color.");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule.as_ref().expect("general mana rule")["effects"][0]["mana"],
        serde_json::json!({ "kind": "chooseColor", "amount": 1 })
    );
}
