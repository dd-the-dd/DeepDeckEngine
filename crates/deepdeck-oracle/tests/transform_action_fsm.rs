use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

fn parse(
    card_name: &str,
    type_line: &str,
    oracle_text: &str,
) -> deepdeck_oracle::oracle::OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("transform".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn incubator_style_transform_action_is_native_and_atomic() {
    let parsed = parse(
        "Incubator Token",
        "Token Artifact — Incubator",
        "{2}: Transform this token.",
    );

    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule,
        Some(json!({
            "kind": "activatedAbility",
            "source": { "kind": "self" },
            "costs": [{ "kind": "payMana", "manaCost": "{2}" }],
            "declaration": { "kind": "castingDeclaration", "decisions": [] },
            "effects": [{
                "kind": "transformPermanent",
                "object": { "kind": "self" }
            }]
        }))
    );
}

#[test]
fn current_card_name_uses_the_same_source_reference_automaton() {
    let parsed = parse(
        "Test Shifter",
        "Creature — Shapeshifter",
        "{3}: Transform Test Shifter.",
    );

    assert_eq!(parsed.status, "canonical", "{parsed:#?}");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["effects"][0]["object"],
        json!({ "kind": "self" })
    );
}

#[test]
fn target_transform_remains_a_committed_unknown_at_the_source_boundary() {
    let parsed = parse(
        "Test Shifter",
        "Creature — Shapeshifter",
        "{3}: Transform target permanent.",
    );

    assert_eq!(parsed.status, "unsupported", "{parsed:#?}");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("activated-targetless-effects")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("reference.source.start")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("target"));
}
